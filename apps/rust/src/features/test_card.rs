//! `prt test` — gera card de Test Case.
//!
//! Espelha `test_card_command.dart` + `test_card_service.dart` +
//! `test_card_models.dart` + `test_card_validation.dart`: resolve Work Item
//! pai, busca PR + exemplos, gera JSON `{title, body}`, monta criação e
//! atualiza o pai para Test QA. As decisões interativas (confirmações,
//! prompts) ficam na TUI; aqui só preparo de dados, geração e escrita.

use serde_json::Value;
use std::fmt::Write as _;
use tracing::info;

use crate::ai::{self, PrDescription};
use crate::azure::pull_requests::PullRequest;
use crate::azure::work_items::TestCaseInput;
use crate::azure::{self, WorkItem, pull_requests, work_items};
use crate::cli::CliOptions;
use crate::config::{self, Config};
use crate::error::{AppError, Result};
use crate::git::{self, ChangeContext};

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
    if count == 0 {
        return Ok(String::new());
    }
    Ok(
        work_items::get_test_case_examples(client, &remote.project, count)
            .await
            .unwrap_or_default()
            .iter()
            .map(|item| format!("- #{} {}", item.id, item.title()))
            .collect::<Vec<_>>()
            .join("\n"),
    )
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
        pr_changes,
        examples_text,
        prompt,
    })
}

/// Lê campo de texto do Work Item (`""` se ausente/não-texto; espelha `workItemText`).
#[must_use]
pub fn work_item_field<'a>(item: &'a WorkItem, field: &str) -> &'a str {
    item.fields.get(field).and_then(Value::as_str).unwrap_or("")
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
/// remote; propaga [`AppError::Azure`] em falha HTTP (ver pendência de
/// transporte em `azure::work_items`).
pub async fn create(
    prep: &TestCardPrep,
    settings: &TestSettings,
    title: &str,
    body: &str,
) -> Result<WorkItem> {
    validate_card(title, body)?;
    let Some(remote) = prep.context.remote.as_ref() else {
        return Err(AppError::Git {
            message: "o comando test requer um remote git do azure devops".to_owned(),
        });
    };
    let client = azure::client_for(Some(remote), prep.config.azure_pat.trim())?;
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
    use super::*;
    use crate::cli::Command;
    use crate::git::RepositoryRemote;

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
