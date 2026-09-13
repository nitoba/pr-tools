//! `prt test` — gera card de Test Case.
//!
//! Espelha `test_card_command.dart` + `test_card_service.dart` +
//! `test_card_models.dart` + `test_card_validation.dart`: resolve Work Item
//! pai, busca PR + exemplos, gera JSON `{title, body}`, monta criação e
//! atualiza o pai para Test QA. As decisões interativas (confirmações,
//! prompts) ficam na TUI; aqui só preparo de dados, geração e escrita.

use serde_json::Value;
use std::fmt::Write as _;
use std::time::Duration;
use tracing::info;

use crate::ai::{self, PrDescription};
use crate::azure::pull_requests::{PublishedPr, PullRequest};
use crate::azure::work_items::TestCaseInput;
use crate::azure::{self, WorkItem, pull_requests, work_items};
use crate::cli::CliOptions;
use crate::config::{self, Config};
use crate::error::{AppError, Result};
use crate::features::describe::DescribePrep;
use crate::git::{self, ChangeContext};

/// Limite das operações Azure acionadas pela recuperação da TUI.
const TUI_AZURE_TIMEOUT: Duration = Duration::from_secs(30);

/// Entrada de preparação do Test Case.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone)]
pub enum TestCardRequest {
    /// Entrada pública do comando `prt test`.
    Cli(CliOptions),
    /// Entrada interna continuando uma publicação completa de `prt desc`.
    PublishedPr(TestCardLaunchContext),
}

/// Contexto congelado na fronteira entre a publicação do PR e o Test Case.
#[derive(Debug, Clone)]
pub struct TestCardLaunchContext {
    /// Receipt mínima do PR escolhido.
    pub published_pr: PublishedPr,
    /// Remote Azure usado pela publicação.
    pub remote: git::RepositoryRemote,
    /// ID do Work Item confirmado por `desc`, quando havia um.
    pub work_item_id: Option<i64>,
    /// Snapshot do Work Item confirmado por `desc`, quando havia um.
    pub work_item: Option<WorkItem>,
    /// Ref source retornada/esperada pelo PR.
    pub source_ref_name: String,
    /// Ref target retornada/esperada pelo PR.
    pub target_ref_name: String,
    /// Configuração resolvida para a preparação.
    pub config: Config,
    /// Os seis valores de configuração de Test Case resolvidos na origem.
    pub settings: TestSettings,
    /// Fingerprint do checkout no momento da publicação.
    pub fingerprint: git::GitContextFingerprint,
}

/// Valida `examples` (0-5, default 2).
///
/// # Errors
///
/// Retorna erro de CLI se fora do intervalo.
pub fn parse_examples_count(raw: Option<&str>) -> Result<usize> {
    let Some(v) = raw else { return Ok(2) };
    let n: i64 = v
        .trim()
        .parse()
        .map_err(|_| crate::error::AppError::cli("--examples inválido: use 0-5."))?;
    if !(0..=5).contains(&n) {
        return Err(crate::error::AppError::cli("--examples inválido: use 0-5."));
    }
    Ok(usize::try_from(n).unwrap_or_default())
}

/// System prompt do card (texto EXATO de `testCardSystemPrompt` do Dart).
pub const TEST_CARD_SYSTEM: &str = r#"Você é um analista de QA técnico.

Gere um card de teste em português brasileiro para Azure DevOps com base no Work Item pai, no PR, nas alterações e nos exemplos fornecidos.

Retorne um objeto JSON com exatamente estes campos:
- "title": título curto, objetivo e testável.
- "body": Markdown com estas seções, nesta ordem:
  - ## Objetivo
  - ## Cenário base
  - ## Checklist de testes
  - ## Resultado esperado

Regras:
- Não invente comportamento que não esteja sustentado pelo contexto.
- Foque em cobertura funcional, validações e regressão.
- O checklist deve ser uma lista de passos executáveis, com um item por passo.
- O resultado esperado deve ser explícito e verificável; ele será associado aos passos no Test Case.
- Não cite nomes de arquivos, classes, funções, APIs internas ou detalhes de implementação.
- Descreva apenas cenários observáveis e validáveis pelo usuário final ou pelo analista de QA."#;

/// Dados preparados para gerar/criar o card (espelha `TestCardPreparation`).
#[derive(Debug, Clone)]
pub struct TestCardPrep {
    /// Config resolvida (com overrides da CLI).
    pub config: Config,
    /// Contexto Git (branch, diff, log, remote).
    pub context: ChangeContext,
    /// Work Item pai.
    pub parent: WorkItem,
    /// ID do PR pedido (`--pr`, se informado).
    pub pr_id: Option<String>,
    /// Configuração resolvida para a entrada publicada, quando disponível.
    pub settings: Option<TestSettings>,
    /// Alterações do PR já resumidas em texto.
    pub pr_changes: String,
    /// Exemplos de Test Case (`- #id título` por linha).
    pub examples_text: String,
    /// Prompt de usuário montado.
    pub prompt: String,
}

/// Busca o PR pedido em `--pr` (ou `None` sem a flag).
///
/// # Errors
///
/// Retorna [`AppError::Cli`] se o ID não for numérico; propaga
/// [`AppError::Azure`] em falha de rede.
async fn fetch_requested_pr(
    client: &azure::AzureClient,
    remote: &git::RepositoryRemote,
    options: &CliOptions,
) -> Result<Option<PullRequest>> {
    let pr_number: Option<i64> = options
        .pr
        .as_ref()
        .map(|id| id.as_str().parse::<i64>())
        .transpose()
        .map_err(|_| AppError::cli("--pr inválido: use um ID numérico."))?;
    match pr_number {
        Some(id) => Ok(Some(
            pull_requests::get_pull_request(client, &remote.project, &remote.repository, id)
                .await?,
        )),
        None => Ok(None),
    }
}

/// Resolve o Work Item pai: `--work-item` → branch → vinculados do PR.
///
/// # Errors
///
/// Retorna [`AppError::Cli`] com ID inválido ou pai indeterminável; propaga
/// [`AppError::Azure`] ao listar vinculados do PR.
async fn resolve_parent_id(
    client: &azure::AzureClient,
    remote: &git::RepositoryRemote,
    options: &CliOptions,
    change: &ChangeContext,
    pr: Option<&PullRequest>,
) -> Result<i64> {
    let mut parent_id: Option<i64> = options
        .work_item
        .as_ref()
        .map(|id| id.as_str().parse::<i64>())
        .transpose()
        .map_err(|_| AppError::cli("--work-item inválido: use um ID numérico."))?;
    if parent_id.is_none() && !change.work_item_id.trim().is_empty() {
        // ID vindo da branch (só dígitos pelo regex do git); se falhar,
        // cai para a resolução via PR abaixo.
        parent_id = change.work_item_id.trim().parse::<i64>().ok();
    }
    if parent_id.is_none() {
        if let Some(pulled) = pr {
            let linked = pull_requests::get_pull_request_work_item_ids(
                client,
                &remote.project,
                &remote.repository,
                pulled.pull_request_id,
            )
            .await?;
            let mut linked_items = Vec::new();
            for id in linked {
                if let Ok(item) = azure::get_work_item(client, &id.to_string()).await {
                    linked_items.push(item);
                }
            }
            parent_id = select_parent_work_item(&linked_items);
        }
    }
    parent_id.ok_or_else(|| {
        AppError::cli("não foi possível resolver o work item pai; use --work-item explicitamente")
    })
}

/// Resumo best-effort das alterações do PR (vazio sem PR ou em falha).
async fn fetch_pr_changes_text(
    client: &azure::AzureClient,
    remote: &git::RepositoryRemote,
    pr: Option<&PullRequest>,
) -> String {
    match pr {
        Some(pulled) => pull_requests::get_pull_request_changes(
            client,
            &remote.project,
            &remote.repository,
            pulled.pull_request_id,
        )
        .await
        .unwrap_or_default(),
        None => String::new(),
    }
}

/// Exemplos best-effort de Test Cases (`- #id título` por linha).
///
/// # Errors
///
/// Retorna erro de CLI se `--examples` estiver fora de 0-5.
async fn fetch_examples_text(
    client: &azure::AzureClient,
    remote: &git::RepositoryRemote,
    options: &CliOptions,
) -> Result<String> {
    let count = parse_examples_count(options.examples.as_deref())?;
    Ok(fetch_examples_text_count(client, remote, count).await)
}

/// Busca a quantidade já validada de exemplos para uma entrada estruturada.
async fn fetch_examples_text_count(
    client: &azure::AzureClient,
    remote: &git::RepositoryRemote,
    count: usize,
) -> String {
    if count == 0 {
        return String::new();
    }
    work_items::get_test_case_examples(client, &remote.project, count)
        .await
        .unwrap_or_default()
        .iter()
        .map(|item| format!("- #{} {}", item.id, item.title()))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Prepara dados do card: config → PAT → Git → remote → PR → pai
/// (`--work-item` → branch → vinculados do PR) → mudanças → exemplos → prompt.
///
/// Decisões documentadas (espelham o service Dart):
/// - É `async` porque faz I/O de rede.
/// - Sem prompt interativo: pai indeterminável vira [`AppError::Cli`] pedindo
///   `--work-item` (a TUI decide confirmações).
/// - Mudanças/exemplos com falha viram texto vazio (best-effort, como o
///   `either`+`fold` do Dart); a listagem de vinculados do PR propaga erro,
///   mas item individual com falha é ignorado.
/// - Seleção do pai: menor ID não-`Test Case` (ver [`select_parent_work_item`]).
///
/// # Errors
///
/// Retorna [`AppError::Config`] sem PAT; [`AppError::Git`] sem remote;
/// [`AppError::Cli`] com IDs inválidos, `--examples` fora de 0-5 ou pai
/// indeterminável; [`AppError::Azure`] em falha de rede.
pub async fn prepare(options: &CliOptions) -> Result<TestCardPrep> {
    prepare_request(TestCardRequest::Cli(options.clone())).await
}

/// Prepara o card a partir do comando standalone ou de um PR publicado.
///
/// # Errors
///
/// Retorna as falhas de configuração, Git, Azure e validação específicas da
/// entrada escolhida.
pub async fn prepare_request(request: TestCardRequest) -> Result<TestCardPrep> {
    match request {
        TestCardRequest::Cli(options) => prepare_cli(&options).await,
        TestCardRequest::PublishedPr(context) => prepare_published_pr(&context).await,
    }
}

async fn prepare_cli(options: &CliOptions) -> Result<TestCardPrep> {
    let mut config = config::load_config()?;
    config::apply_cli_overrides(
        &mut config,
        options.provider.as_deref(),
        options.model.as_deref(),
        options.base_url.as_deref(),
        options.api_key.as_deref(),
    );
    if config.azure_pat.trim().is_empty() {
        return Err(AppError::Config {
            message: "o comando test requer AZURE_PAT ou AZURE_DEVOPS_PAT configurado".to_owned(),
        });
    }
    let change = git::collect(options.source.as_deref())?;
    let Some(remote) = change.remote.as_ref() else {
        return Err(AppError::Git {
            message: "o comando test requer um remote git do azure devops".to_owned(),
        });
    };
    let client = azure::client_for(Some(remote), config.azure_pat.trim())?;
    let pr = fetch_requested_pr(&client, remote, options).await?;
    let parent_id = resolve_parent_id(&client, remote, options, &change, pr.as_ref()).await?;
    let parent = azure::get_work_item(&client, &parent_id.to_string()).await?;
    let pr_changes = fetch_pr_changes_text(&client, remote, pr.as_ref()).await;
    let examples_text = fetch_examples_text(&client, remote, options).await?;
    let prompt = build_test_card_prompt(&parent, &change, pr.as_ref(), &pr_changes, &examples_text);
    Ok(TestCardPrep {
        config,
        context: change,
        parent,
        pr_id: options.pr.as_ref().map(|id| id.as_str().to_owned()),
        settings: None,
        pr_changes,
        examples_text,
        prompt,
    })
}

/// Prepara um Test Case a partir do PR selecionado na receipt publicada.
async fn prepare_published_pr(context: &TestCardLaunchContext) -> Result<TestCardPrep> {
    let config = context.config.clone();
    if config.azure_pat.trim().is_empty() {
        return Err(AppError::Config {
            message: "o handoff do PR requer AZURE_PAT ou AZURE_DEVOPS_PAT configurado".to_owned(),
        });
    }
    if context.settings.team.trim().is_empty() {
        return Err(AppError::cli(
            "Custom.Team é obrigatório para preparar o test case.",
        ));
    }
    if context.settings.program.trim().is_empty() {
        return Err(AppError::cli(
            "Custom.ProgramasAgrotrace é obrigatório para preparar o test case.",
        ));
    }
    if !context.settings.priority.is_finite() || context.settings.priority <= 0.0 {
        return Err(AppError::cli(
            "prioridade deve ser um número positivo para preparar o test case.",
        ));
    }
    let client = azure::client_for(Some(&context.remote), config.azure_pat.trim())?;
    let published = &context.published_pr;
    if published.id <= 0 {
        return Err(AppError::cli("id de PR publicado inválido"));
    }
    let pr = pull_requests::get_pull_request(
        &client,
        &context.remote.project,
        &context.remote.repository,
        published.id,
    )
    .await?;
    validate_published_pr(context, &pr)?;

    let linked_ids = pull_requests::get_pull_request_work_item_ids(
        &client,
        &context.remote.project,
        &context.remote.repository,
        published.id,
    )
    .await?;
    let (parent_id, parent) = if let Some(expected_id) = context.work_item_id {
        (
            expected_id,
            validate_published_work_item(
                published.id,
                expected_id,
                &linked_ids,
                context.work_item.as_ref(),
            )?,
        )
    } else {
        let mut linked_items = Vec::with_capacity(linked_ids.len());
        for id in linked_ids {
            linked_items.push(azure::get_work_item(&client, &id.to_string()).await?);
        }
        let parent_id = select_parent_work_item(&linked_items).ok_or_else(|| {
            AppError::cli(
                "não foi possível resolver o work item pai vinculado ao PR; nenhuma escrita foi iniciada",
            )
        })?;
        let parent = linked_items
            .into_iter()
            .find(|item| item.id == parent_id)
            .ok_or_else(|| AppError::cli("Work Item pai não encontrado após a resolução"))?;
        (parent_id, parent)
    };

    let mut change = git::collect_for_refs(&pr.source_ref_name, &pr.target_ref_name)?;
    if change.remote.as_ref() != Some(&context.remote) {
        return Err(AppError::Git {
            message: "o remote local não corresponde ao repositório Azure do PR selecionado"
                .to_owned(),
        });
    }
    // A autoridade do prompt é o PR remoto, mesmo quando a ref foi resolvida
    // localmente ou sob `origin/` pelo coletor exato.
    change.base_branch = pr.target_ref_name.clone();
    change.source_ref = pr.source_ref_name.clone();
    change.work_item_id = parent_id.to_string();
    let pr_changes = fetch_pr_changes_text(&client, &context.remote, Some(&pr)).await;
    let examples_text = fetch_examples_text_count(&client, &context.remote, 2).await;
    let prompt = build_test_card_prompt(&parent, &change, Some(&pr), &pr_changes, &examples_text);
    Ok(TestCardPrep {
        config,
        context: change,
        parent,
        pr_id: Some(published.id.to_string()),
        settings: Some(context.settings.clone()),
        pr_changes,
        examples_text,
        prompt,
    })
}

fn validate_published_pr(context: &TestCardLaunchContext, pr: &PullRequest) -> Result<()> {
    let published = &context.published_pr;
    if pr.pull_request_id != published.id {
        return Err(AppError::cli(format!(
            "Azure retornou PR #{} em vez do PR #{} selecionado",
            pr.pull_request_id, published.id
        )));
    }
    if !pr.repository.name.trim().is_empty()
        && pr.repository.name.trim() != context.remote.repository.trim()
    {
        return Err(AppError::Git {
            message: format!(
                "o PR #{} pertence ao repositório {}, não a {}",
                published.id, pr.repository.name, context.remote.repository
            ),
        });
    }
    if !pr.repository.project.name.trim().is_empty()
        && pr.repository.project.name.trim() != context.remote.project.trim()
    {
        return Err(AppError::Git {
            message: format!(
                "o PR #{} pertence ao projeto {}, não a {}",
                published.id, pr.repository.project.name, context.remote.project
            ),
        });
    }
    if pr.source_ref_name.trim().is_empty() || pr.target_ref_name.trim().is_empty() {
        return Err(AppError::Git {
            message: format!(
                "o PR #{} não retornou sourceRefName/targetRefName válidos",
                published.id
            ),
        });
    }
    let expected_target = format!("refs/heads/{}", published.target.trim());
    if pr.target_ref_name != expected_target || pr.target_ref_name != context.target_ref_name {
        return Err(AppError::Git {
            message: format!(
                "o target remoto do PR #{} ({}) não corresponde ao target publicado {}",
                published.id, pr.target_ref_name, published.target
            ),
        });
    }
    if !context.source_ref_name.trim().is_empty() && pr.source_ref_name != context.source_ref_name {
        return Err(AppError::Git {
            message: format!(
                "a source ref do PR #{} divergiu do snapshot publicado",
                published.id
            ),
        });
    }
    Ok(())
}

fn validate_published_work_item(
    pr_id: i64,
    expected_id: i64,
    linked_ids: &[i64],
    snapshot: Option<&WorkItem>,
) -> Result<WorkItem> {
    if !linked_ids.contains(&expected_id) {
        return Err(AppError::cli(format!(
            "Work Item #{expected_id} não está vinculado ao PR #{pr_id}; a preparação foi interrompida"
        )));
    }
    let parent = snapshot.ok_or_else(|| {
        AppError::cli(format!(
            "snapshot do Work Item #{expected_id} não está disponível; a preparação foi interrompida"
        ))
    })?;
    if parent.id != expected_id {
        return Err(AppError::cli(format!(
            "snapshot do Work Item diverge do ID esperado #{expected_id}"
        )));
    }
    Ok(parent.clone())
}

/// Lê campo de texto do Work Item (`""` se ausente/não-texto; espelha `workItemText`).
#[must_use]
pub fn work_item_field<'a>(item: &'a WorkItem, field: &str) -> &'a str {
    item.field_text(field)
}

/// Escolhe o Work Item pai: menor ID não-`Test Case`; se todos forem
/// `Test Case`, o menor ID; vazio vira `None` (espelha `selectParentWorkItem`).
#[must_use]
pub fn select_parent_work_item(items: &[WorkItem]) -> Option<i64> {
    let mut ordered: Vec<&WorkItem> = items.iter().collect();
    ordered.sort_by_key(|item| item.id);
    ordered
        .iter()
        .find(|item| item.work_item_type() != "Test Case")
        .or_else(|| ordered.first())
        .copied()
        .map(|item| item.id)
}

/// Monta o prompt de usuário (espelha `buildTestCardPrompt` do Dart).
#[must_use]
pub fn build_test_card_prompt(
    parent: &WorkItem,
    change: &ChangeContext,
    pr: Option<&PullRequest>,
    pr_changes: &str,
    examples_text: &str,
) -> String {
    let mut lines = vec![
        "## Contexto do Work Item".to_owned(),
        String::new(),
        format!("ID: {}", parent.id),
        format!("Título: {}", work_item_field(parent, "System.Title")),
        format!("Tipo: {}", work_item_field(parent, "System.WorkItemType")),
    ];
    let area = work_item_field(parent, "System.AreaPath");
    let description = work_item_field(parent, "System.Description");
    if !area.is_empty() {
        lines.push(format!("Área: {area}"));
    }
    if !description.is_empty() {
        lines.push(format!("Descrição: {description}"));
    }
    if let Some(pulled) = pr {
        lines.extend([
            String::new(),
            "## Contexto do PR".to_owned(),
            String::new(),
            format!("PR ID: {}", pulled.pull_request_id),
            format!("Título: {}", pulled.title),
            format!("Branch origem: {}", pulled.source_ref_name),
            format!("Branch destino: {}", pulled.target_ref_name),
        ]);
        if !pulled.description.is_empty() {
            lines.push(format!("Descrição: {}", pulled.description));
        }
    }
    lines.extend([
        String::new(),
        "## Contexto Git".to_owned(),
        String::new(),
        format!("Branch atual: {}", change.branch),
        format!("Base: {}", change.base_branch),
    ]);
    if !pr_changes.trim().is_empty() {
        lines.extend([String::new(), "## Arquivos alterados".to_owned()]);
        lines.extend(pr_changes.lines().map(str::to_owned));
    }
    if !change.diff.is_empty() {
        lines.extend([
            String::new(),
            "## Diff resumido".to_owned(),
            String::new(),
            "```diff".to_owned(),
            change.diff.clone(),
            "```".to_owned(),
        ]);
    }
    if !change.log.is_empty() {
        lines.extend([
            String::new(),
            "## Commits".to_owned(),
            String::new(),
            "```".to_owned(),
            change.log.clone(),
            "```".to_owned(),
        ]);
    }
    if !examples_text.trim().is_empty() {
        lines.extend([
            String::new(),
            "## Exemplos de Test Case".to_owned(),
            String::new(),
        ]);
        lines.extend(examples_text.lines().map(str::to_owned));
    }
    lines.extend([
        String::new(),
        "## Instruções finais".to_owned(),
        String::new(),
        "Gere o card conforme o formato definido no system prompt.".to_owned(),
    ]);
    let mut prompt = lines.join("\n");
    prompt.push('\n');
    prompt
}

/// Gera o card via IA (fallback de providers + normalização).
///
/// Sem limite de 4000 caracteres (o card não tem o limite do PR) e sem
/// validação de seções — o formato é contrato do system prompt; a TUI pode
/// chamar [`validate_card`] antes de criar. A normalização reaproveitada
/// limita o título a 80 de largura e limpa fences/think.
///
/// # Errors
///
/// Propaga [`AppError::Ai`] se todos os providers falharem.
pub async fn generate(prep: &TestCardPrep) -> Result<PrDescription> {
    let report = |provider: &str, model: &str| {
        info!(provider, model, "tentando gerar card de teste");
    };
    let raw =
        ai::generate_with_fallback(&prep.config, TEST_CARD_SYSTEM, &prep.prompt, report).await?;
    Ok(ai::normalize_description(
        &raw,
        &format!("test-card/{}", prep.parent.id),
    ))
}

/// Configurações de criação do Test Case (espelha `TestCardSettings`).
#[derive(Debug, Clone)]
pub struct TestSettings {
    /// `AreaPath` (`--area-path` ou `testAreaPath`).
    pub area_path: String,
    /// Responsável (`--assigned-to` ou `testAssignedTo`).
    pub assigned_to: String,
    /// `IterationPath` (`--iteration-path` ou o do pai).
    pub iteration_path: String,
    /// Prioridade (`--priority`, default 2).
    pub priority: f64,
    /// Time (`Custom.Team`, obrigatório).
    pub team: String,
    /// Programa (`Custom.ProgramasAgrotrace`, obrigatório).
    pub program: String,
}

/// Campo editável da revisão da TUI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestSettingsField {
    /// `System.AreaPath`.
    AreaPath,
    /// `System.AssignedTo`.
    AssignedTo,
    /// `System.IterationPath`.
    IterationPath,
    /// `Microsoft.VSTS.Common.Priority`.
    Priority,
    /// `Custom.Team`.
    Team,
    /// `Custom.ProgramasAgrotrace`.
    Program,
}

impl TestSettingsField {
    /// Índice usado pela ordem dos campos na TUI.
    #[must_use]
    pub const fn index(self) -> usize {
        match self {
            Self::AreaPath => 0,
            Self::AssignedTo => 1,
            Self::IterationPath => 2,
            Self::Priority => 3,
            Self::Team => 4,
            Self::Program => 5,
        }
    }

    /// Nome amigável mostrado ao usuário.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::AreaPath => "AreaPath",
            Self::AssignedTo => "responsável (AssignedTo)",
            Self::IterationPath => "IterationPath",
            Self::Priority => "prioridade",
            Self::Team => "Custom.Team",
            Self::Program => "Custom.ProgramasAgrotrace",
        }
    }
}

/// Classificação de uma falha de criação.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreateFailureKind {
    /// O Azure respondeu recusando a operação; o card não deve ter sido criado.
    Confirmed,
    /// A resposta não permite saber se o Azure criou o card.
    OutcomeUnknown,
}

/// Falha de criação pronta para a TUI apresentar e recuperar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateFailure {
    /// Mensagem acionável e sem o payload JSON bruto.
    pub message: String,
    /// Se é seguro assumir que não houve criação.
    pub kind: CreateFailureKind,
    /// Campo que o Azure apontou, quando identificável.
    pub field: Option<TestSettingsField>,
}

/// Possível Test Case criado antes de uma resposta perdida.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestCaseCandidate {
    /// ID do Work Item.
    pub id: i64,
    /// Link navegável para o Work Item.
    pub url: String,
    /// Título exato retornado pelo Azure.
    pub title: String,
    /// Data de criação, quando o Azure a retornou.
    pub created_at: String,
    /// Se possui a relação esperada com o Work Item pai.
    pub parent_matches: bool,
    /// Quantidade de campos enviados que também coincidem.
    pub matching_fields: usize,
    /// Quantidade de campos não vazios comparáveis.
    pub comparable_fields: usize,
}

impl TestSettings {
    /// Resolve precedência CLI > config, com `IterationPath` herdado do pai e
    /// prioridade default 2 (espelha `_settings` do command Dart; sem os
    /// prompts interativos, que ficam na TUI).
    ///
    /// # Errors
    ///
    /// Retorna [`AppError::Cli`] se `--priority` não for positivo ou se
    /// `Custom.Team` / `Custom.ProgramasAgrotrace` estiverem vazios.
    pub fn from_cli_or_config(
        options: &CliOptions,
        config: &Config,
        parent: &WorkItem,
    ) -> Result<Self> {
        let team = options
            .team
            .clone()
            .unwrap_or_else(|| config.test_team.clone());
        if team.trim().is_empty() {
            return Err(AppError::cli(
                "Custom.Team é obrigatório para criar o test case.",
            ));
        }
        let program = options
            .program
            .clone()
            .unwrap_or_else(|| config.test_program.clone());
        if program.trim().is_empty() {
            return Err(AppError::cli(
                "Custom.ProgramasAgrotrace é obrigatório para criar o test case.",
            ));
        }
        Ok(Self {
            area_path: options
                .area_path
                .clone()
                .unwrap_or_else(|| config.test_area_path.clone()),
            assigned_to: options
                .assigned_to
                .clone()
                .unwrap_or_else(|| config.test_assigned_to.clone()),
            iteration_path: options
                .iteration_path
                .clone()
                .unwrap_or_else(|| work_item_field(parent, "System.IterationPath").to_owned()),
            priority: parse_priority(options.priority.as_deref())?,
            team,
            program,
        })
    }

    /// Resolve os seis campos a partir da configuração já carregada.
    ///
    /// Esta variante é usada pelo handoff publicado, que não possui uma nova
    /// linha de comando para re-resolver os valores.
    ///
    /// # Errors
    ///
    /// Retorna erro quando `Custom.Team` ou `Custom.ProgramasAgrotrace` está
    /// ausente na configuração.
    pub fn from_config(config: &Config, parent: &WorkItem) -> Result<Self> {
        if config.test_team.trim().is_empty() {
            return Err(AppError::cli(
                "Custom.Team é obrigatório para criar o test case.",
            ));
        }
        if config.test_program.trim().is_empty() {
            return Err(AppError::cli(
                "Custom.ProgramasAgrotrace é obrigatório para criar o test case.",
            ));
        }
        Ok(Self {
            area_path: config.test_area_path.clone(),
            assigned_to: config.test_assigned_to.clone(),
            iteration_path: work_item_field(parent, "System.IterationPath").to_owned(),
            priority: 2.0,
            team: config.test_team.clone(),
            program: config.test_program.clone(),
        })
    }
}

impl TestCardLaunchContext {
    /// Monta o contexto interno a partir da preparação de `desc`.
    ///
    /// O Work Item é carregado uma única vez em `desc`; se a leitura falhou,
    /// o ID não é convertido silenciosamente em outro pai no handoff.
    ///
    /// # Errors
    ///
    /// Retorna erro quando o remote Azure ou o ID do Work Item não pode ser
    /// representado no contrato publicado.
    pub fn from_describe(prep: &DescribePrep, published_pr: &PublishedPr) -> Result<Self> {
        let remote = prep.context.remote.clone().ok_or_else(|| AppError::Git {
            message: "remote Azure DevOps não encontrado para continuar ao Test Case".to_owned(),
        })?;
        let work_item_id = if prep.work_item_id.trim().is_empty() {
            None
        } else {
            Some(
                prep.work_item_id
                    .trim()
                    .parse::<i64>()
                    .map_err(|_| AppError::cli("Work Item resolvido por desc não é numérico"))?,
            )
        };
        let parent = prep.work_item.clone();
        let fallback_parent = WorkItem {
            id: 0,
            fields: std::collections::HashMap::new(),
            relations: Vec::new(),
        };
        let settings =
            TestSettings::from_config(&prep.config, parent.as_ref().unwrap_or(&fallback_parent))
                .unwrap_or_else(|_| TestSettings {
                    area_path: prep.config.test_area_path.clone(),
                    assigned_to: prep.config.test_assigned_to.clone(),
                    iteration_path: parent.as_ref().map_or_else(String::new, |item| {
                        work_item_field(item, "System.IterationPath").to_owned()
                    }),
                    priority: 2.0,
                    team: prep.config.test_team.clone(),
                    program: prep.config.test_program.clone(),
                });
        Ok(Self {
            published_pr: published_pr.clone(),
            remote,
            work_item_id,
            work_item: parent,
            source_ref_name: prep.context.source_ref.clone(),
            target_ref_name: format!("refs/heads/{}", published_pr.target),
            config: prep.config.clone(),
            settings,
            fingerprint: prep.fingerprint.clone(),
        })
    }
}

/// Classifica e explica uma falha ocorrida ao criar um Test Case.
///
/// Erros de transporte, timeout, limitação e respostas 5xx são tratados como
/// resultado incerto: o Azure pode ter criado o item antes de perder a resposta.
#[must_use]
pub fn classify_create_error(error: &AppError) -> CreateFailure {
    let (kind, detail, field) = match error {
        AppError::Http(_) => (
            CreateFailureKind::OutcomeUnknown,
            "não foi possível confirmar se o Azure DevOps criou o Test Case por causa de uma falha de rede".to_owned(),
            None,
        ),
        AppError::Azure { status, message } => {
            let detail = azure_error_detail(message);
            let field = test_settings_field(message);
            let kind = if *status < 300 || *status == 408 || *status == 429 || *status >= 500 {
                CreateFailureKind::OutcomeUnknown
            } else {
                CreateFailureKind::Confirmed
            };
            let text = match *status {
                401 | 403 => format!(
                    "Azure DevOps recusou a criação por falta de permissão ou PAT inválido (HTTP {status}); verifique o PAT e o acesso de criação de Test Cases no projeto"
                ),
                408 | 429 => format!(
                    "não foi possível confirmar a criação do Test Case (HTTP {status}); verifique o Azure DevOps antes de reenviar"
                ),
                500..=599 => format!(
                    "não foi possível confirmar a criação do Test Case porque o Azure DevOps falhou (HTTP {status}); verifique o Azure DevOps antes de reenviar"
                ),
                200..=299 => format!(
                    "o Azure DevOps respondeu sucesso, mas não foi possível confirmar a criação do Test Case (HTTP {status}); verifique antes de reenviar"
                ),
                _ => format!("Azure DevOps recusou a criação (HTTP {status})"),
            };
            (kind, append_detail(text, detail.as_str()), field)
        }
        _ => (
            CreateFailureKind::Confirmed,
            error.to_string(),
            test_settings_field(&error.to_string()),
        ),
    };
    let message = match field {
        Some(field) if matches!(kind, CreateFailureKind::Confirmed) => {
            format!("campo {}: {detail}", field.label())
        }
        _ => detail,
    };
    CreateFailure {
        message,
        kind,
        field,
    }
}

/// Explica uma falha ao atualizar o Work Item pai.
#[must_use]
pub fn describe_parent_update_error(error: &AppError) -> String {
    match error {
        AppError::Http(_) => {
            "falha de rede ao atualizar o Work Item pai; o Test Case continua criado e é seguro tentar novamente".to_owned()
        }
        AppError::Azure { status, .. } if *status == 401 || *status == 403 => format!(
            "sem permissão para atualizar o Work Item pai (HTTP {status}); verifique o PAT e o acesso de edição no projeto. O Test Case continua criado"
        ),
        AppError::Azure { status, message } => format!(
            "{}; o Test Case continua criado e é seguro tentar novamente",
            append_detail(
                format!("falha ao atualizar o Work Item pai no Azure DevOps (HTTP {status})"),
                azure_error_detail(message).as_str(),
            )
        ),
        _ => format!(
            "falha ao atualizar o Work Item pai: {error}; o Test Case continua criado e é seguro tentar novamente"
        ),
    }
}

/// Procura candidatos que possam ter sido criados antes de um timeout.
///
/// A comparação combina título exato, relação com o pai e os campos enviados.
/// A exclusão continua sendo uma decisão exclusiva da TUI, nunca desta função.
///
/// # Errors
///
/// Propaga a falha da consulta WIQL; os carregamentos individuais são
/// best-effort em [`work_items::find_test_case_candidates`].
pub async fn find_create_candidates(
    prep: &TestCardPrep,
    title: &str,
    settings: &TestSettings,
) -> Result<Vec<TestCaseCandidate>> {
    let client = current_azure_client(prep)?;
    let Some(remote) = prep.context.remote.as_ref() else {
        return Err(AppError::Git {
            message: "o comando test requer um remote git do azure devops".to_owned(),
        });
    };
    let items = work_items::find_test_case_candidates(&client, &remote.project, title).await?;
    let mut candidates = items
        .into_iter()
        .map(|item| {
            let parent_matches = item.relations.iter().any(|relation| {
                relation.rel == "System.LinkTypes.Related"
                    && relation_id(&relation.url) == Some(prep.parent.id)
            });
            let (matching_fields, comparable_fields) = matching_settings_fields(&item, settings);
            TestCaseCandidate {
                id: item.id,
                url: candidate_url(prep, item.id),
                title: item.title().to_owned(),
                created_at: work_item_field(&item, "System.CreatedDate").to_owned(),
                parent_matches,
                matching_fields,
                comparable_fields,
            }
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| {
        right
            .parent_matches
            .cmp(&left.parent_matches)
            .then_with(|| right.matching_fields.cmp(&left.matching_fields))
            .then_with(|| right.created_at.cmp(&left.created_at))
            .then_with(|| left.id.cmp(&right.id))
    });
    Ok(candidates)
}

/// Exclui um candidato pela API reversível da lixeira.
///
/// A confirmação visual e a escolha do ID acontecem na TUI antes desta função.
///
/// # Errors
///
/// Propaga falhas de configuração, autenticação e transporte.
pub async fn delete_candidate(prep: &TestCardPrep, id: i64) -> Result<()> {
    if id <= 0 {
        return Err(AppError::cli("id de Test Case inválido para exclusão"));
    }
    let client = current_azure_client(prep)?;
    let Some(remote) = prep.context.remote.as_ref() else {
        return Err(AppError::Git {
            message: "o comando test requer um remote git do azure devops".to_owned(),
        });
    };
    work_items::delete_work_item(&client, &remote.project, id).await
}

/// Cria um cliente Azure com o PAT atualmente salvo, não com o snapshot inicial.
fn current_azure_client(prep: &TestCardPrep) -> Result<azure::AzureClient> {
    let config = config::load_config()?;
    azure::client_for_with_timeout(
        prep.context.remote.as_ref(),
        config.azure_pat.trim(),
        TUI_AZURE_TIMEOUT,
    )
}

fn relation_id(url: &str) -> Option<i64> {
    url.split('?')
        .next()
        .unwrap_or(url)
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .and_then(|value| value.parse::<i64>().ok())
}

fn candidate_url(prep: &TestCardPrep, id: i64) -> String {
    match prep.context.remote.as_ref() {
        Some(remote) => format!(
            "https://dev.azure.com/{}/{}/_workitems/edit/{id}",
            remote.organization, remote.project
        ),
        None => format!("workitem:{id}"),
    }
}

fn matching_settings_fields(item: &WorkItem, settings: &TestSettings) -> (usize, usize) {
    let priority = settings.priority.to_string();
    let expected = [
        ("System.AreaPath", settings.area_path.as_str()),
        ("System.AssignedTo", settings.assigned_to.as_str()),
        ("System.IterationPath", settings.iteration_path.as_str()),
        ("Microsoft.VSTS.Common.Priority", priority.as_str()),
        ("Custom.Team", settings.team.as_str()),
        ("Custom.ProgramasAgrotrace", settings.program.as_str()),
    ];
    let mut matching = 0;
    let mut comparable = 0;
    for (field, expected) in expected {
        if expected.trim().is_empty() {
            continue;
        }
        comparable += 1;
        if item.fields.get(field).is_some_and(|actual| {
            work_item_value_text(actual)
                .is_some_and(|actual| values_match(expected, actual.as_str()))
        }) {
            matching += 1;
        }
    }
    (matching, comparable)
}

fn work_item_value_text(value: &Value) -> Option<String> {
    if let Some(text) = value.as_str() {
        return Some(text.to_owned());
    }
    if let Some(number) = value.as_f64() {
        return Some(if number.fract() == 0.0 {
            format!("{number:.0}")
        } else {
            number.to_string()
        });
    }
    value.as_object().and_then(|object| {
        ["uniqueName", "displayName"]
            .iter()
            .find_map(|key| object.get(*key).and_then(Value::as_str).map(str::to_owned))
    })
}

fn values_match(expected: &str, actual: &str) -> bool {
    let expected = expected.trim().replace(',', ".");
    let actual = actual.trim().replace(',', ".");
    expected == actual
        || expected
            .parse::<f64>()
            .ok()
            .zip(actual.parse::<f64>().ok())
            .is_some_and(|(left, right)| (left - right).abs() < f64::EPSILON)
}

fn append_detail(message: String, detail: &str) -> String {
    if detail.is_empty() {
        message
    } else {
        format!("{message}: {detail}")
    }
}

fn azure_error_detail(raw: &str) -> String {
    let detail = serde_json::from_str::<Value>(raw)
        .ok()
        .and_then(|value| first_error_message(&value))
        .unwrap_or_else(|| raw.to_owned());
    detail
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(240)
        .collect()
}

fn first_error_message(value: &Value) -> Option<String> {
    match value {
        Value::Object(object) => {
            for key in ["message", "Message", "errorMessage"] {
                if let Some(message) = object.get(key).and_then(Value::as_str) {
                    if !message.trim().is_empty() {
                        return Some(message.to_owned());
                    }
                }
            }
            object.values().find_map(first_error_message)
        }
        Value::Array(values) => values.iter().find_map(first_error_message),
        _ => None,
    }
}

fn test_settings_field(raw: &str) -> Option<TestSettingsField> {
    let lower = raw.to_ascii_lowercase();
    if lower.contains("custom.programasagrotrace")
        || lower.contains("programasagrotrace")
        || lower.contains("programa")
    {
        Some(TestSettingsField::Program)
    } else if lower.contains("custom.team") || lower.contains("custom_team") {
        Some(TestSettingsField::Team)
    } else if lower.contains("system.areapath") || lower.contains("area path") {
        Some(TestSettingsField::AreaPath)
    } else if lower.contains("system.assignedto") || lower.contains("assigned to") {
        Some(TestSettingsField::AssignedTo)
    } else if lower.contains("system.iterationpath") || lower.contains("iteration path") {
        Some(TestSettingsField::IterationPath)
    } else if lower.contains("microsoft.vsts.common.priority") || lower.contains("priority") {
        Some(TestSettingsField::Priority)
    } else {
        None
    }
}

/// Interpreta `--priority` (default 2; vírgula vira ponto, como `_decimal`).
///
/// # Errors
///
/// Retorna [`AppError::Cli`] se não for número positivo.
fn parse_priority(raw: Option<&str>) -> Result<f64> {
    let Some(text) = raw.map(str::trim).filter(|t| !t.is_empty()) else {
        return Ok(2.0);
    };
    let number: f64 = text.replace(',', ".").parse().unwrap_or(f64::NAN);
    if number.is_finite() && number > 0.0 {
        Ok(number)
    } else {
        Err(AppError::cli("--priority deve ser um número positivo."))
    }
}

/// Valida título/corpo antes de criar (espelha `parseRequiredText`).
///
/// # Errors
///
/// Retorna [`AppError::Cli`] se algum estiver vazio.
pub fn validate_card(title: &str, body: &str) -> Result<()> {
    if title.trim().is_empty() {
        return Err(AppError::cli(
            "título é obrigatório para criar o test case.",
        ));
    }
    if body.trim().is_empty() {
        return Err(AppError::cli("corpo é obrigatório para criar o test case."));
    }
    Ok(())
}

/// Monta a entrada de criação (pura; espelha `buildCreateTestCaseInput`).
///
/// Usa o [`build_test_case_steps_xml`] existente para os steps e
/// [`markdown_to_html`] para a descrição; opcionais vazios viram `None`.
#[must_use]
pub fn build_test_case_input(
    settings: &TestSettings,
    organization: &str,
    parent_id: i64,
    title: &str,
    body: &str,
) -> TestCaseInput {
    TestCaseInput {
        title: title.to_owned(),
        description_html: Some(markdown_to_html(body)),
        steps_xml: Some(build_test_case_steps_xml(body)),
        area_path: empty_to_none(&settings.area_path),
        parent_id: (parent_id > 0).then_some(parent_id),
        iteration_path: empty_to_none(&settings.iteration_path),
        priority: Some(settings.priority),
        team: empty_to_none(&settings.team),
        program: empty_to_none(&settings.program),
        assigned_to: {
            let trimmed = settings.assigned_to.trim();
            (!trimmed.is_empty()).then(|| trimmed.to_owned())
        },
        organization: (!organization.is_empty()).then(|| organization.to_owned()),
    }
}

/// Texto vazio vira `None` (opcionais do patch, como no Dart).
fn empty_to_none(value: &str) -> Option<String> {
    (!value.is_empty()).then(|| value.to_owned())
}

/// Cria o Test Case no Azure DevOps.
///
/// # Errors
///
/// Retorna [`AppError::Cli`] se título/corpo vazios; [`AppError::Git`] sem
/// remote; propaga [`AppError::Azure`] em falha HTTP.
pub async fn create(
    prep: &TestCardPrep,
    settings: &TestSettings,
    title: &str,
    body: &str,
) -> Result<WorkItem> {
    create_with_pat_and_timeout(
        prep,
        settings,
        title,
        body,
        prep.config.azure_pat.as_str(),
        None,
    )
    .await
}

/// Cria o Test Case usando a configuração atual, para a recuperação da TUI.
///
/// Diferentemente de [`create`], relê o PAT antes de cada envio. A separação
/// preserva o comportamento dos consumidores não interativos, que continuam
/// usando o snapshot de configuração da preparação.
///
/// # Errors
///
/// Propaga os mesmos erros de [`create`].
pub async fn create_with_current_config(
    prep: &TestCardPrep,
    settings: &TestSettings,
    title: &str,
    body: &str,
) -> Result<WorkItem> {
    validate_card(title, body)?;
    let config = config::load_config()?;
    create_with_pat_and_timeout(
        prep,
        settings,
        title,
        body,
        config.azure_pat.as_str(),
        Some(TUI_AZURE_TIMEOUT),
    )
    .await
}

async fn create_with_pat_and_timeout(
    prep: &TestCardPrep,
    settings: &TestSettings,
    title: &str,
    body: &str,
    pat: &str,
    timeout: Option<Duration>,
) -> Result<WorkItem> {
    validate_card(title, body)?;
    let Some(remote) = prep.context.remote.as_ref() else {
        return Err(AppError::Git {
            message: "o comando test requer um remote git do azure devops".to_owned(),
        });
    };
    let client = match timeout {
        Some(timeout) => azure::client_for_with_timeout(Some(remote), pat, timeout)?,
        None => azure::client_for(Some(remote), pat)?,
    };
    let input = build_test_case_input(settings, &remote.organization, prep.parent.id, title, body);
    work_items::create_test_case(&client, &remote.project, &input).await
}

/// Atualiza o pai para Test QA (+ esforços opcionais em horas decimais).
///
/// # Errors
///
/// Retorna [`AppError::Cli`] se algum esforço for inválido; propaga
/// [`AppError::Azure`] em falha HTTP.
pub async fn update_parent(
    prep: &TestCardPrep,
    effort: Option<&str>,
    real_effort: Option<&str>,
) -> Result<()> {
    let client = azure::client_for(prep.context.remote.as_ref(), prep.config.azure_pat.trim())?;
    work_items::update_parent_to_test_qa(&client, prep.parent.id, effort, real_effort).await
}

/// Atualiza o pai usando o PAT atualmente salvo, para a recuperação da TUI.
///
/// # Errors
///
/// Propaga os mesmos erros de [`update_parent`].
pub async fn update_parent_with_current_config(
    prep: &TestCardPrep,
    effort: Option<&str>,
    real_effort: Option<&str>,
) -> Result<()> {
    let client = current_azure_client(prep)?;
    work_items::update_parent_to_test_qa(&client, prep.parent.id, effort, real_effort).await
}

/// Texto de `## Título` (só `##`, como o Dart; `#` sozinho é parágrafo).
fn heading_text(line: &str) -> Option<&str> {
    let rest = line.strip_prefix("##")?;
    if !rest.starts_with(is_sep) {
        return None;
    }
    let content = rest.trim_start_matches(is_sep);
    (!content.is_empty()).then_some(content)
}

/// Separador entre marcador de lista e conteúdo (espelha `\s` nos casos reais).
fn is_sep(c: char) -> bool {
    c == ' ' || c == '\t'
}

/// Item de lista não ordenada (`-`, `*`, `+`, `•` + espaço).
fn unordered_item(line: &str) -> Option<&str> {
    let marker = line.chars().next()?;
    if !matches!(marker, '-' | '*' | '+' | '•') {
        return None;
    }
    let rest = &line[marker.len_utf8()..];
    if !rest.starts_with(is_sep) {
        return None;
    }
    let content = rest.trim_start_matches(is_sep);
    (!content.is_empty()).then_some(content)
}

/// Item de lista ordenada (`1.`/`1)` + espaço).
fn ordered_item(line: &str) -> Option<&str> {
    let end = line.find(|c: char| !c.is_ascii_digit())?;
    if end == 0 {
        return None;
    }
    let rest = line[end..]
        .strip_prefix('.')
        .or_else(|| line[end..].strip_prefix(')'))?;
    if !rest.starts_with(is_sep) {
        return None;
    }
    let content = rest.trim_start_matches(is_sep);
    (!content.is_empty()).then_some(content)
}

/// Escapa `&`, `<`, `>` (sempre antes do inline, como no Dart).
fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// `` `código` `` → `<code>` via placeholders (protege o conteúdo do bold).
fn replace_code_spans(text: &str) -> String {
    if text.matches('`').count() % 2 != 0 {
        return text.to_owned();
    }
    let mut out = String::new();
    let mut spans: Vec<&str> = Vec::new();
    let mut rest = text;
    while let Some(open) = rest.find('`') {
        let after_open = &rest[open + 1..];
        let Some(close) = after_open.find('`') else {
            break;
        };
        out.push_str(&rest[..open]);
        let _ = write!(out, "\0{}\0", spans.len());
        spans.push(&after_open[..close]);
        rest = &after_open[close + 1..];
    }
    out.push_str(rest);
    let mut result = out;
    for (index, span) in spans.iter().enumerate() {
        result = result.replace(&format!("\0{index}\0"), &format!("<code>{span}</code>"));
    }
    result
}

/// `**negrito**` → `<b>` (espelha `\*\*([^*][^*]*?)\*\*` do Dart).
fn replace_bold(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(start) = rest.find("**") {
        let after = &rest[start + 2..];
        let inner = after
            .find("**")
            .map(|end| &after[..end])
            .filter(|inner| !inner.is_empty() && !inner.contains('*'));
        let Some(inner) = inner else {
            out.push_str(&rest[..=start]);
            rest = &rest[start + 1..];
            continue;
        };
        out.push_str(&rest[..start]);
        let _ = write!(out, "<b>{inner}</b>");
        rest = &after[inner.len() + 2..];
    }
    out.push_str(rest);
    out
}

/// Inline: escape → code → bold.
fn markdown_inline_to_html(value: &str) -> String {
    replace_bold(&replace_code_spans(&escape_html(value)))
}

/// Converte Markdown mínimo em HTML (espelha `markdownToHtml` do Dart).
///
/// Suporta `## ` → `<h2>`, listas `-`/`*`/`+`/`•` → `<ul>`, `1.`/`1)` →
/// `<ol>`, `**negrito**`, `` `código` `` e parágrafos, com escape de HTML.
#[must_use]
pub fn markdown_to_html(body: &str) -> String {
    let mut html = String::new();
    let mut list: Option<&str> = None;
    for line in body.split('\n') {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            close_open_list(&mut html, &mut list);
            continue;
        }
        if let Some(heading) = heading_text(trimmed) {
            close_open_list(&mut html, &mut list);
            let _ = writeln!(html, "<h2>{}</h2>", markdown_inline_to_html(heading));
            continue;
        }
        let item = unordered_item(trimmed)
            .map(|content| ("ul", content))
            .or_else(|| ordered_item(trimmed).map(|content| ("ol", content)));
        if let Some((tag, content)) = item {
            if list != Some(tag) {
                if let Some(open) = list.take() {
                    let _ = writeln!(html, "</{open}>");
                }
                let _ = writeln!(html, "<{tag}>");
                list = Some(tag);
            }
            let _ = writeln!(html, "<li>{}</li>", markdown_inline_to_html(content));
            continue;
        }
        close_open_list(&mut html, &mut list);
        let _ = writeln!(html, "<p>{}</p>", markdown_inline_to_html(trimmed));
    }
    close_open_list(&mut html, &mut list);
    html
}

/// Fecha a lista aberta (`</ul>`/`</ol>`), se houver.
fn close_open_list(html: &mut String, list: &mut Option<&str>) {
    if let Some(tag) = list.take() {
        let _ = writeln!(html, "</{tag}>");
    }
}

/// Converte checklist Markdown em XML de steps do Azure.
#[must_use]
pub fn build_test_case_steps_xml(body: &str) -> String {
    let mut steps = String::from("<steps id=\"0\" last=\"0\">");
    let mut id = 1;
    for line in body.lines() {
        let t = line.trim();
        let action = t
            .strip_prefix("- [ ]")
            .or_else(|| t.strip_prefix("- [x]"))
            .or_else(|| t.strip_prefix('-'))
            .map_or(t, str::trim);
        if action.is_empty() {
            continue;
        }
        let action = escape_html(action);
        let _ = write!(
            steps,
            "<step id=\"{id}\" type=\"ActionStep\"><parameterizedString isformatted=\"true\">{action}</parameterizedString><parameterizedString isformatted=\"true\">Resultado esperado: conforme especificado.</parameterizedString></step>"
        );
        id += 1;
    }
    steps.push_str("</steps>");
    steps
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::mpsc::{Receiver, channel};
    use std::thread::{JoinHandle, spawn};

    use super::*;
    use crate::cli::Command;
    use crate::git::RepositoryRemote;

    #[derive(Debug)]
    struct CapturedRequest {
        method: String,
        target: String,
    }

    fn spawn_json_server(
        responses: Vec<String>,
    ) -> (String, Receiver<CapturedRequest>, JoinHandle<()>) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("listener");
        let address = listener.local_addr().expect("endereço");
        let (sender, receiver) = channel();
        let handle = spawn(move || {
            for body in responses {
                let (mut stream, _) = listener.accept().expect("conexão");
                let mut bytes = Vec::new();
                loop {
                    let mut chunk = [0_u8; 4096];
                    let read = stream.read(&mut chunk).expect("leitura");
                    assert!(read > 0, "cliente encerrou antes dos cabeçalhos");
                    bytes.extend_from_slice(&chunk[..read]);
                    if let Some(end) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
                        let headers = String::from_utf8_lossy(&bytes[..end]);
                        let mut parts = headers
                            .lines()
                            .next()
                            .expect("request line")
                            .split_whitespace();
                        sender
                            .send(CapturedRequest {
                                method: parts.next().expect("método").to_owned(),
                                target: parts.next().expect("target").to_owned(),
                            })
                            .expect("captura");
                        break;
                    }
                }
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                stream.write_all(response.as_bytes()).expect("resposta");
            }
        });
        (format!("http://{address}/org"), receiver, handle)
    }

    #[test]
    fn examples_should_default_to_2() {
        assert_eq!(parse_examples_count(None).unwrap(), 2);
    }

    #[test]
    fn examples_should_reject_out_of_range() {
        assert!(parse_examples_count(Some("9")).is_err());
    }

    #[test]
    fn steps_xml_should_wrap_checklist() {
        let xml = build_test_case_steps_xml("- [ ] Abrir tela\n- [ ] Confirmar");
        assert!(xml.contains("ActionStep"));
        assert!(xml.contains("Abrir tela"));
    }

    #[test]
    fn steps_xml_should_escape_xml_metacharacters() {
        let xml = build_test_case_steps_xml("- [ ] Comparar A & B < C > D");
        assert!(xml.contains("Comparar A &amp; B &lt; C &gt; D"));
        assert!(!xml.contains("Comparar A & B < C > D"));
    }

    fn test_options() -> CliOptions {
        CliOptions {
            command: Command::Test,
            source: None,
            targets: Vec::new(),
            work_item: None,
            provider: None,
            model: None,
            base_url: None,
            api_key: None,
            create: false,
            no_create: false,
            pr: None,
            area_path: None,
            assigned_to: None,
            iteration_path: None,
            priority: None,
            team: None,
            program: None,
            examples: None,
            output: crate::cli::OutputFlags {
                dry_run: false,
                raw: false,
                copy: true,
            },
            completion_shell: None,
        }
    }

    fn test_work_item(id: i64, work_type: &str, title: &str) -> WorkItem {
        serde_json::from_value(serde_json::json!({
            "id": id,
            "fields": {"System.Title": title, "System.WorkItemType": work_type},
        }))
        .unwrap()
    }

    fn test_change() -> ChangeContext {
        ChangeContext {
            branch: "feature/11763-x".to_owned(),
            source_ref: "refs/heads/feature/11763-x".to_owned(),
            base_branch: "dev".to_owned(),
            sprint_branch: String::new(),
            diff: "diff --git a".to_owned(),
            diff_original_lines: 1,
            log: "abc123 msg".to_owned(),
            work_item_id: "11763".to_owned(),
            remote: Some(RepositoryRemote {
                organization: "minhaorg".to_owned(),
                project: "MeuProj".to_owned(),
                repository: "meurepo".to_owned(),
            }),
        }
    }

    fn test_pr() -> PullRequest {
        PullRequest {
            pull_request_id: 99,
            title: "PR T".to_owned(),
            description: "PR desc".to_owned(),
            source_ref_name: "refs/heads/feat".to_owned(),
            target_ref_name: "refs/heads/dev".to_owned(),
            status: "active".to_owned(),
            repository: crate::azure::pull_requests::PullRequestRepository {
                id: "repo-id".to_owned(),
                name: "repo".to_owned(),
                project: crate::azure::pull_requests::PullRequestProject {
                    name: "project".to_owned(),
                },
            },
        }
    }

    fn published_context() -> TestCardLaunchContext {
        let remote = RepositoryRemote {
            organization: "org".to_owned(),
            project: "project".to_owned(),
            repository: "repo".to_owned(),
        };
        let parent = test_work_item(11763, "User Story", "Mudança funcional");
        TestCardLaunchContext {
            published_pr: PublishedPr {
                target: "dev".to_owned(),
                id: 99,
                url: "https://dev.azure.com/org/project/_git/repo/pullrequest/99".to_owned(),
            },
            remote,
            work_item_id: Some(parent.id),
            work_item: Some(parent),
            source_ref_name: "refs/heads/feat".to_owned(),
            target_ref_name: "refs/heads/dev".to_owned(),
            config: Config {
                azure_pat: "pat".to_owned(),
                test_area_path: "project\\QA".to_owned(),
                test_assigned_to: "qa@example.com".to_owned(),
                test_team: "DevOps".to_owned(),
                test_program: "Agrotrace".to_owned(),
                ..Config::default()
            },
            settings: TestSettings {
                area_path: "project\\QA".to_owned(),
                assigned_to: "qa@example.com".to_owned(),
                iteration_path: "project\\Sprint 12".to_owned(),
                priority: 2.0,
                team: "DevOps".to_owned(),
                program: "Agrotrace".to_owned(),
            },
            fingerprint: crate::git::GitContextFingerprint::default(),
        }
    }

    #[test]
    fn system_prompt_should_define_card_sections() {
        assert!(TEST_CARD_SYSTEM.contains("analista de QA"));
        assert!(TEST_CARD_SYSTEM.contains("\"title\""));
        assert!(TEST_CARD_SYSTEM.contains("## Objetivo"));
        assert!(TEST_CARD_SYSTEM.contains("## Cenário base"));
        assert!(TEST_CARD_SYSTEM.contains("## Checklist de testes"));
        assert!(TEST_CARD_SYSTEM.contains("## Resultado esperado"));
    }

    #[test]
    fn prompt_should_contain_all_sections() {
        let parent: WorkItem = serde_json::from_value(serde_json::json!({
            "id": 11763,
            "fields": {
                "System.Title": "Minha task",
                "System.WorkItemType": "User Story",
                "System.AreaPath": "Proj\\Time",
                "System.Description": "Fazer X",
            },
        }))
        .unwrap();
        let prompt = build_test_card_prompt(
            &parent,
            &test_change(),
            Some(&test_pr()),
            "- [edit] /src/a.ts",
            "- #5 Exemplo antigo",
        );
        for section in [
            "## Contexto do Work Item",
            "ID: 11763",
            "Título: Minha task",
            "Tipo: User Story",
            "Área: Proj\\Time",
            "Descrição: Fazer X",
            "## Contexto do PR",
            "PR ID: 99",
            "Título: PR T",
            "Branch origem: refs/heads/feat",
            "Branch destino: refs/heads/dev",
            "Descrição: PR desc",
            "## Contexto Git",
            "Branch atual: feature/11763-x",
            "Base: dev",
            "## Arquivos alterados",
            "- [edit] /src/a.ts",
            "## Diff resumido",
            "```diff",
            "## Commits",
            "## Exemplos de Test Case",
            "- #5 Exemplo antigo",
            "## Instruções finais",
            "Gere o card conforme o formato definido no system prompt.",
        ] {
            assert!(prompt.contains(section), "faltou: {section}");
        }
        assert!(prompt.ends_with('\n'));
    }

    #[test]
    fn prompt_should_skip_empty_sections_without_pr() {
        let parent = test_work_item(1, "Task", "T");
        let mut change = test_change();
        change.diff = String::new();
        change.log = String::new();
        let prompt = build_test_card_prompt(&parent, &change, None, "", "");
        assert!(!prompt.contains("Contexto do PR"));
        assert!(!prompt.contains("Arquivos alterados"));
        assert!(!prompt.contains("Diff resumido"));
        assert!(!prompt.contains("Exemplos de Test Case"));
        assert!(prompt.contains("## Contexto Git"));
    }

    #[test]
    fn markdown_should_convert_headings_lists_bold_and_code() {
        let html = markdown_to_html("## Objetivo\nTexto simples");
        assert!(html.contains("<h2>Objetivo</h2>"));
        assert!(html.contains("<p>Texto simples</p>"));

        let html = markdown_to_html("- [ ] Abrir tela\n- [x] Confirmar");
        assert!(html.contains("<ul>"));
        assert!(html.contains("<li>[ ] Abrir tela</li>"));
        assert!(html.contains("</ul>"));

        let html = markdown_to_html("1. Primeiro\n2. Segundo");
        assert!(html.contains("<ol>"));
        assert!(html.contains("<li>Primeiro</li>"));

        let html = markdown_to_html("**negrito** e `código`");
        assert!(html.contains("<b>negrito</b>"));
        assert!(html.contains("<code>código</code>"));

        let html = markdown_to_html("# Não é h2");
        assert!(!html.contains("<h2>"));
        assert!(html.contains("<p># Não é h2</p>"));
    }

    #[test]
    fn markdown_should_escape_html_before_formatting() {
        let html = markdown_to_html("<script>&</script>");
        assert!(!html.contains("<script>"));
        assert!(html.contains("&lt;script&gt;&amp;"));
    }

    #[test]
    fn validate_card_should_reject_empty_title_or_body() {
        assert!(validate_card("", "corpo").is_err());
        assert!(validate_card("título", "  ").is_err());
        assert!(validate_card("título", "corpo").is_ok());
    }

    #[test]
    fn examples_should_accept_zero_and_five() {
        assert_eq!(parse_examples_count(Some("0")).unwrap(), 0);
        assert_eq!(parse_examples_count(Some("5")).unwrap(), 5);
        assert!(parse_examples_count(Some("6")).is_err());
        assert!(parse_examples_count(Some("abc")).is_err());
    }

    #[test]
    fn settings_should_resolve_defaults_from_config() {
        let config = Config {
            test_area_path: "Proj\\Time".to_owned(),
            test_assigned_to: "qa@x.com".to_owned(),
            test_team: "DevOps".to_owned(),
            test_program: "Agrotrace".to_owned(),
            ..Config::default()
        };
        let parent: WorkItem = serde_json::from_value(serde_json::json!({
            "id": 1,
            "fields": {"System.IterationPath": "Proj\\Sprint 12"},
        }))
        .unwrap();
        let settings = TestSettings::from_cli_or_config(&test_options(), &config, &parent).unwrap();
        assert_eq!(settings.area_path, "Proj\\Time");
        assert_eq!(settings.assigned_to, "qa@x.com");
        assert_eq!(settings.iteration_path, "Proj\\Sprint 12");
        assert!((settings.priority - 2.0).abs() < f64::EPSILON);
        assert_eq!(settings.team, "DevOps");
        assert_eq!(settings.program, "Agrotrace");
    }

    #[test]
    fn settings_should_prefer_cli_over_config() {
        let config = Config {
            test_area_path: "Cfg".to_owned(),
            ..Config::default()
        };
        let mut options = test_options();
        options.area_path = Some("Cli".to_owned());
        options.iteration_path = Some("Cli\\S1".to_owned());
        options.priority = Some("1,5".to_owned());
        options.team = Some("DevOps".to_owned());
        options.program = Some("Agrotrace".to_owned());
        let parent = test_work_item(1, "Task", "T");
        let settings = TestSettings::from_cli_or_config(&options, &config, &parent).unwrap();
        assert_eq!(settings.area_path, "Cli");
        assert_eq!(settings.iteration_path, "Cli\\S1");
        assert!((settings.priority - 1.5).abs() < f64::EPSILON);
    }

    #[test]
    fn settings_should_require_team_and_program() {
        let config = Config::default();
        let parent = test_work_item(1, "Task", "T");
        let err = TestSettings::from_cli_or_config(&test_options(), &config, &parent).unwrap_err();
        assert!(err.to_string().contains("Custom.Team"));

        let config = Config {
            test_team: "DevOps".to_owned(),
            ..Config::default()
        };
        let err = TestSettings::from_cli_or_config(&test_options(), &config, &parent).unwrap_err();
        assert!(err.to_string().contains("Custom.ProgramasAgrotrace"));
    }

    #[test]
    fn settings_should_reject_non_positive_priority() {
        let parent = test_work_item(1, "Task", "T");
        let mut options = test_options();
        options.team = Some("DevOps".to_owned());
        options.program = Some("Agrotrace".to_owned());
        options.priority = Some("0".to_owned());
        let err =
            TestSettings::from_cli_or_config(&options, &Config::default(), &parent).unwrap_err();
        assert!(err.to_string().contains("--priority"));
    }

    #[test]
    fn cli_request_should_preserve_standalone_preparation() {
        let mut options = test_options();
        options.create = true;
        options.no_create = false;
        options.source = Some("refs/heads/feature/11763-x".to_owned());
        options.work_item = Some(crate::cli::WorkItemId::parse("--work-item", "11763").unwrap());
        options.pr = Some(crate::cli::WorkItemId::parse("--pr", "99").unwrap());
        options.area_path = Some("Proj\\QA".to_owned());
        options.assigned_to = Some("qa@example.com".to_owned());
        options.iteration_path = Some("Proj\\Sprint 12".to_owned());
        options.priority = Some("1,5".to_owned());
        options.team = Some("DevOps".to_owned());
        options.program = Some("Agrotrace".to_owned());
        options.examples = Some("5".to_owned());
        let request = TestCardRequest::Cli(options);
        match request {
            TestCardRequest::Cli(actual) => {
                assert!(actual.create);
                assert!(!actual.no_create);
                assert_eq!(actual.source.as_deref(), Some("refs/heads/feature/11763-x"));
                assert_eq!(
                    actual
                        .work_item
                        .as_ref()
                        .map(crate::cli::WorkItemId::as_str),
                    Some("11763")
                );
                assert_eq!(
                    actual.pr.as_ref().map(crate::cli::WorkItemId::as_str),
                    Some("99")
                );
                assert_eq!(actual.area_path.as_deref(), Some("Proj\\QA"));
                assert_eq!(actual.assigned_to.as_deref(), Some("qa@example.com"));
                assert_eq!(actual.iteration_path.as_deref(), Some("Proj\\Sprint 12"));
                assert_eq!(actual.priority.as_deref(), Some("1,5"));
                assert_eq!(actual.team.as_deref(), Some("DevOps"));
                assert_eq!(actual.program.as_deref(), Some("Agrotrace"));
                assert_eq!(actual.examples.as_deref(), Some("5"));
                assert_eq!(actual.command, Command::Test);
            }
            TestCardRequest::PublishedPr(_) => panic!("request standalone foi convertido"),
        }

        let parent = test_work_item(11763, "User Story", "Pai");
        let options = CliOptions {
            area_path: Some("Cli\\Area".to_owned()),
            assigned_to: Some("cli@example.com".to_owned()),
            iteration_path: Some("Cli\\Sprint".to_owned()),
            priority: Some("1,5".to_owned()),
            team: Some("CliTeam".to_owned()),
            program: Some("CliProgram".to_owned()),
            ..test_options()
        };
        let settings = TestSettings::from_cli_or_config(&options, &Config::default(), &parent)
            .expect("settings standalone");
        assert_eq!(settings.area_path, "Cli\\Area");
        assert_eq!(settings.assigned_to, "cli@example.com");
        assert_eq!(settings.iteration_path, "Cli\\Sprint");
        assert!((settings.priority - 1.5).abs() < f64::EPSILON);
        assert_eq!(settings.team, "CliTeam");
        assert_eq!(settings.program, "CliProgram");
    }

    #[tokio::test]
    async fn published_request_should_lookup_and_validate_selected_pr() {
        let context = published_context();
        let (base_url, requests, server) = spawn_json_server(vec![
            serde_json::to_string(&serde_json::json!({
                "pullRequestId": 99,
                "repository": {"name": "repo", "project": {"name": "project"}},
                "sourceRefName": "refs/heads/feat",
                "targetRefName": "refs/heads/dev"
            }))
            .unwrap(),
        ]);
        let client = crate::azure::AzureClient::new_for_test(&base_url, "pat");
        let fetched = pull_requests::get_pull_request(
            &client,
            &context.remote.project,
            &context.remote.repository,
            context.published_pr.id,
        )
        .await
        .expect("lookup do PR publicado");
        assert_eq!(fetched.pull_request_id, context.published_pr.id);
        let request = requests.recv().expect("requisição do lookup");
        assert_eq!(
            request.target,
            "/org/project/_apis/git/repositories/repo/pullRequests/99?api-version=7.1"
        );
        assert_eq!(request.method, "GET");
        server.join().expect("servidor do lookup");
        assert!(validate_published_pr(&context, &test_pr()).is_ok());

        let mut wrong_repository = test_pr();
        wrong_repository.repository.name = "outro-repo".to_owned();
        assert!(validate_published_pr(&context, &wrong_repository).is_err());

        let mut missing_target = test_pr();
        missing_target.target_ref_name.clear();
        assert!(validate_published_pr(&context, &missing_target).is_err());

        let mut missing_source = test_pr();
        missing_source.source_ref_name.clear();
        assert!(validate_published_pr(&context, &missing_source).is_err());

        let mut wrong_id = test_pr();
        wrong_id.pull_request_id = 100;
        assert!(validate_published_pr(&context, &wrong_id).is_err());
    }

    #[test]
    fn published_request_should_use_exact_remote_source_and_target_context() {
        let context = published_context();
        let change = test_change();
        let mut calls = Vec::new();
        let collected = crate::git::collect_for_refs_with(
            "refs/heads/feat",
            "refs/heads/dev",
            |args| {
                calls.push(args.iter().map(|arg| (*arg).to_owned()).collect::<Vec<_>>());
                if args.first() == Some(&"diff") {
                    return Ok("diff remoto".to_owned());
                }
                if args.first() == Some(&"log") {
                    return Ok("log remoto".to_owned());
                }
                Ok("oid".to_owned())
            },
            Some(context.remote.clone()),
        )
        .expect("coleta por refs do PR");
        assert_eq!(collected.source_ref, "refs/heads/feat");
        assert_eq!(collected.base_branch, "refs/heads/dev");
        assert!(calls.iter().any(|call| {
            call == &[
                "diff".to_owned(),
                "refs/heads/dev...refs/heads/feat".to_owned(),
            ]
        }));
        assert!(calls.iter().any(|call| {
            call == &[
                "log".to_owned(),
                "--oneline".to_owned(),
                "-50".to_owned(),
                "refs/heads/dev..refs/heads/feat".to_owned(),
            ]
        }));
        let prompt = build_test_card_prompt(
            context.work_item.as_ref().expect("snapshot do pai"),
            &ChangeContext {
                source_ref: context.source_ref_name.clone(),
                base_branch: context.target_ref_name.clone(),
                ..change
            },
            Some(&test_pr()),
            "- [edit] /src/checkout.rs",
            "- #5 Exemplo",
        );
        assert!(prompt.contains("PR ID: 99"));
        assert!(prompt.contains("Branch origem: refs/heads/feat"));
        assert!(prompt.contains("Branch destino: refs/heads/dev"));
        assert!(!prompt.contains("Base: sprint/"));
        assert!(!prompt.contains("Base: main"));
    }

    #[test]
    fn published_request_should_reject_incompatible_work_item_without_fallback() {
        let context = published_context();
        let snapshot = context.work_item.as_ref();
        let error = validate_published_work_item(99, 11763, &[42], snapshot).unwrap_err();
        assert!(error.to_string().contains("não está vinculado"));

        let valid = validate_published_work_item(99, 11763, &[11763], snapshot)
            .expect("Work Item do desc compatível");
        assert_eq!(valid.id, 11763);

        let wrong_snapshot = test_work_item(42, "Task", "Outro pai");
        let error =
            validate_published_work_item(99, 11763, &[11763], Some(&wrong_snapshot)).unwrap_err();
        assert!(error.to_string().contains("diverge"));
        assert!(!error.to_string().contains("fallback"));
    }

    #[test]
    fn published_request_should_resolve_parent_from_pr_links_only_when_needed() {
        let items = vec![
            test_work_item(11763, "User Story", "Pai"),
            test_work_item(12000, "Test Case", "Caso"),
        ];
        assert_eq!(select_parent_work_item(&items), Some(11763));
        assert_eq!(select_parent_work_item(&[]), None);
        let no_parent = AppError::cli(
            "não foi possível resolver o work item pai vinculado ao PR; nenhuma escrita foi iniciada",
        );
        assert!(
            no_parent
                .to_string()
                .contains("nenhuma escrita foi iniciada")
        );
    }

    #[test]
    fn published_request_should_expose_complete_launch_context_and_prompt() {
        let context = published_context();
        let parent = context.work_item.as_ref().expect("Work Item preservado");
        let prompt = build_test_card_prompt(
            parent,
            &test_change(),
            Some(&test_pr()),
            "- [edit] /src/checkout.rs",
            "- #5 Exemplo",
        );
        assert_eq!(context.published_pr.id, 99);
        assert_eq!(context.remote.repository, "repo");
        assert_eq!(context.remote.organization, "org");
        assert_eq!(context.remote.project, "project");
        assert_eq!(context.source_ref_name, "refs/heads/feat");
        assert_eq!(context.target_ref_name, "refs/heads/dev");
        assert_eq!(context.work_item_id, Some(11763));
        assert_eq!(parent.id, 11763);
        assert_eq!(context.settings.area_path, "project\\QA");
        assert_eq!(context.settings.assigned_to, "qa@example.com");
        assert_eq!(context.settings.iteration_path, "project\\Sprint 12");
        assert!((context.settings.priority - 2.0).abs() < f64::EPSILON);
        assert_eq!(context.settings.team, "DevOps");
        assert_eq!(context.settings.program, "Agrotrace");
        assert!(prompt.contains("PR ID: 99"));
        assert!(prompt.contains("- #5 Exemplo"));
    }

    #[test]
    fn create_error_should_focus_remote_validation_field() {
        let error = AppError::Azure {
            status: 400,
            message: serde_json::json!({
                "message": "The field Custom.Team is required."
            })
            .to_string(),
        };
        let failure = classify_create_error(&error);
        assert_eq!(failure.kind, CreateFailureKind::Confirmed);
        assert_eq!(failure.field, Some(TestSettingsField::Team));
        assert!(failure.message.contains("Custom.Team"));
    }

    #[test]
    fn create_auth_error_should_explain_pat_and_project_permission() {
        let failure = classify_create_error(&AppError::Azure {
            status: 401,
            message: "Unauthorized".to_owned(),
        });
        assert_eq!(failure.kind, CreateFailureKind::Confirmed);
        assert!(failure.message.contains("PAT"));
        assert!(failure.message.contains("projeto"));
    }

    #[test]
    fn create_server_error_should_require_duplicate_check_before_retry() {
        let failure = classify_create_error(&AppError::Azure {
            status: 504,
            message: "gateway timeout".to_owned(),
        });
        assert_eq!(failure.kind, CreateFailureKind::OutcomeUnknown);
        assert!(failure.message.contains("confirmar"));
        assert!(failure.message.contains("reenviar"));
    }

    #[test]
    fn malformed_success_response_should_require_duplicate_check_before_retry() {
        let failure = classify_create_error(&AppError::Azure {
            status: 201,
            message: "resposta vazia do Azure DevOps".to_owned(),
        });
        assert_eq!(failure.kind, CreateFailureKind::OutcomeUnknown);
        assert!(failure.message.contains("respondeu sucesso"));
    }

    #[test]
    fn parent_update_error_should_preserve_created_test_case_message() {
        let message = describe_parent_update_error(&AppError::Azure {
            status: 403,
            message: "forbidden".to_owned(),
        });
        assert!(message.contains("permissão"));
        assert!(message.contains("continua criado"));
    }

    #[test]
    fn candidate_field_values_should_compare_numbers_and_identity_objects() {
        let item: WorkItem = serde_json::from_value(serde_json::json!({
            "id": 10,
            "fields": {
                "System.AreaPath": "Proj\\Time",
                "System.AssignedTo": {"uniqueName": "qa@example.com"},
                "Microsoft.VSTS.Common.Priority": 2,
                "Custom.Team": "QA"
            }
        }))
        .unwrap();
        let settings = TestSettings {
            area_path: "Proj\\Time".to_owned(),
            assigned_to: "qa@example.com".to_owned(),
            iteration_path: String::new(),
            priority: 2.0,
            team: "QA".to_owned(),
            program: String::new(),
        };
        assert_eq!(matching_settings_fields(&item, &settings), (4, 4));
    }

    #[test]
    fn build_input_should_map_settings_to_test_case() {
        let settings = TestSettings {
            area_path: "Proj\\T".to_owned(),
            assigned_to: "  a@b.c  ".to_owned(),
            iteration_path: String::new(),
            priority: 2.0,
            team: "DevOps".to_owned(),
            program: "Agrotrace".to_owned(),
        };
        let input =
            build_test_case_input(&settings, "minhaorg", 7, "Título", "## Objetivo\nValidar X");
        assert_eq!(input.title, "Título");
        assert!(
            input
                .description_html
                .is_some_and(|h| h.contains("<h2>Objetivo</h2>"))
        );
        assert!(input.steps_xml.is_some_and(|x| x.contains("<steps")));
        assert_eq!(input.area_path.as_deref(), Some("Proj\\T"));
        assert_eq!(input.iteration_path, None);
        assert_eq!(input.parent_id, Some(7));
        assert_eq!(input.organization.as_deref(), Some("minhaorg"));
        assert_eq!(input.assigned_to.as_deref(), Some("a@b.c"));
    }

    #[test]
    fn select_parent_should_prefer_smallest_non_test_case() {
        let items = vec![
            test_work_item(5, "Test Case", "TC"),
            test_work_item(9, "Task", "T"),
            test_work_item(3, "Bug", "B"),
        ];
        assert_eq!(select_parent_work_item(&items), Some(3));
        let only_cases = vec![
            test_work_item(8, "Test Case", "A"),
            test_work_item(4, "Test Case", "B"),
        ];
        assert_eq!(select_parent_work_item(&only_cases), Some(4));
        assert_eq!(select_parent_work_item(&[]), None);
    }
}
