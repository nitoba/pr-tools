//! `prt doctor` — diagnóstico do ambiente.
//!
//! Espelha `doctor_service.dart`: Git, configuração, providers e Azure.

use std::sync::OnceLock;
use std::time::Duration;

use base64::Engine as _;
use regex::Regex;
use tokio::time::timeout;

use crate::config::{Config, config_paths, load_config};
use crate::features::process_profiles;
use crate::git::{RepositoryRemote, parse_azure_remote};

/// Timeout por comando externo (espelha `doctorCommandTimeout` do Dart).
const CMD_TIMEOUT: Duration = Duration::from_secs(5);
/// Timeout por requisição HTTP.
const HTTP_TIMEOUT: Duration = Duration::from_secs(10);

/// Check individual.
#[derive(Debug, Clone)]
pub struct Check {
    /// Componente (`git`, `configuração`, ...).
    pub component: &'static str,
    /// Passou?
    pub ok: bool,
    /// É apenas aviso?
    pub warning: bool,
    /// Detalhe.
    pub detail: String,
    /// Como resolver.
    pub fix: String,
}

/// Relatório agregado.
#[derive(Debug, Default)]
pub struct DoctorReport {
    /// Checks executados.
    pub checks: Vec<Check>,
}

impl DoctorReport {
    /// Conta (falhas, avisos).
    #[must_use]
    pub fn summary(&self) -> (usize, usize) {
        let failures = self.checks.iter().filter(|c| !c.ok && !c.warning).count();
        let warnings = self.checks.iter().filter(|c| !c.ok && c.warning).count();
        (failures, warnings)
    }

    /// Exit code (0 se sem falhas).
    #[must_use]
    pub fn exit_code(&self) -> i32 {
        i32::from(self.summary().0 != 0)
    }
}

/// Monta um check de sucesso (sem `fix`).
fn ok_check(component: &'static str, detail: String) -> Check {
    Check {
        component,
        ok: true,
        warning: false,
        detail,
        fix: String::new(),
    }
}

/// Monta um check de aviso.
fn warn_check(component: &'static str, detail: String, fix: String) -> Check {
    Check {
        component,
        ok: false,
        warning: true,
        detail,
        fix,
    }
}

/// Monta um check de falha.
fn fail_check(component: &'static str, detail: String, fix: String) -> Check {
    Check {
        component,
        ok: false,
        warning: false,
        detail,
        fix,
    }
}

/// Inspeciona Git, configuração, providers e Azure DevOps, sem nunca falhar.
///
/// Cada problema vira um [`Check`] com `fix` em PT-BR (espelha
/// `DoctorServiceLive` do Dart). `source` é a branch de origem para o
/// contexto de PR (`--source`); `None` usa a branch atual.
pub async fn inspect(source: Option<&str>) -> DoctorReport {
    let mut checks = Vec::new();
    let git = inspect_git(source, &mut checks).await;
    let config = match load_config() {
        Ok(config) => config,
        Err(err) => {
            let detail = err.to_string();
            checks.push(fail_check(
                "Configuração",
                detail,
                "Execute `prt init` ou corrija o arquivo de configuração indicado.".to_owned(),
            ));
            return DoctorReport { checks };
        }
    };
    inspect_configuration(&config, &mut checks).await;
    inspect_process_profiles(
        &config,
        git.remote.as_ref(),
        git.work_item_id.as_deref(),
        &mut checks,
    )
    .await;
    inspect_providers(&config, &mut checks).await;
    inspect_azure(&config, git.remote.as_ref(), &mut checks).await;
    DoctorReport { checks }
}

#[derive(Debug, Default)]
struct GitInspection {
    remote: Option<RepositoryRemote>,
    work_item_id: Option<String>,
}

/// Valida a associação local antes das sondas remotas. O fallback legado é
/// permitido pelo fluxo, mas o diagnóstico sinaliza a ausência de binding
/// explícito para que clones do mesmo projeto não dependam do default global.
async fn inspect_process_profiles(
    config: &Config,
    remote: Option<&RepositoryRemote>,
    work_item_id: Option<&str>,
    checks: &mut Vec<Check>,
) {
    let Some(remote) = remote else {
        checks.push(fail_check(
            "Perfis de processo",
            "remote Azure não disponível para selecionar um perfil.".to_owned(),
            "Configure um remote Azure DevOps válido e repita `prt doctor`.".to_owned(),
        ));
        return;
    };
    let label = remote_label(remote);
    if let Err(error) = process_profiles::validate_config(config) {
        checks.push(fail_check(
            "Perfis de processo",
            format!("{label}: {error}"),
            "Corrija os perfis/bindings em config.json; informe um programField válido e uma associação por remote.".to_owned(),
        ));
        return;
    }
    let bindings = process_profiles::binding_for(&config.bindings, remote);
    if bindings.is_empty() {
        let fallback = if config.default_profile.trim().is_empty() {
            crate::config::AGROTRACE_PROFILE
        } else {
            config.default_profile.trim()
        };
        checks.push(fail_check(
            "Binding de processo",
            format!("{label}: nenhum binding explícito; fallback ativo: {fallback}."),
            format!("Associe {label} a um perfil em config.json ou execute `prt init`."),
        ));
    } else {
        checks.push(ok_check(
            "Binding de processo",
            format!("{label}: {}.", bindings[0].profile),
        ));
    }
    let selection = match process_profiles::select(config, remote) {
        Ok(selection) => selection,
        Err(error) => {
            checks.push(fail_check(
                "Perfil de processo",
                format!("{label}: {error}"),
                "Corrija o perfil selecionado e repita `prt doctor`.".to_owned(),
            ));
            return;
        }
    };
    checks.push(ok_check(
        "Perfil de processo",
        format!(
            "{label}: {} · {}.",
            selection.name(),
            selection.program_field
        ),
    ));
    if selection.profile.team.trim().is_empty() {
        checks.push(fail_check(
            "Campos do perfil",
            format!("{label} / {}: Custom.Team está vazio.", selection.name()),
            format!(
                "Preencha team no perfil {} em `prt init` ou config.json.",
                selection.name()
            ),
        ));
    }
    if selection.profile.program.trim().is_empty() {
        checks.push(fail_check(
            "Campos do perfil",
            format!(
                "{label} / {}: {} está vazio.",
                selection.name(),
                selection.program_field
            ),
            format!(
                "Preencha program no perfil {} em `prt init` ou config.json.",
                selection.name()
            ),
        ));
    }
    if !selection.profile.priority.is_finite() || selection.profile.priority <= 0.0 {
        checks.push(fail_check(
            "Campos do perfil",
            format!(
                "{label} / {}: priority deve ser um número positivo.",
                selection.name()
            ),
            format!(
                "Corrija priority no perfil {} em config.json.",
                selection.name()
            ),
        ));
    }
    for (target, reviewer) in [
        ("dev", selection.profile.reviewer_dev.as_str()),
        ("sprint", selection.profile.reviewer_sprint.as_str()),
    ] {
        if !reviewer.trim().is_empty() && !is_valid_email(reviewer) {
            checks.push(fail_check(
                "Reviewer do perfil",
                format!(
                    "{label} / {} / {target}: reviewer inválido.",
                    selection.name()
                ),
                format!(
                    "Corrija reviewer{} no perfil {} em config.json.",
                    if target == "dev" { "Dev" } else { "Sprint" },
                    selection.name()
                ),
            ));
        }
    }
    if config.azure_pat.trim().is_empty() {
        return;
    }
    let client = match crate::azure::client_for_with_timeout(
        Some(remote),
        config.azure_pat.trim(),
        HTTP_TIMEOUT,
    ) {
        Ok(client) => client,
        Err(error) => {
            checks.push(fail_check(
                "Metadata do perfil",
                format!("{label} / {}: {error}", selection.name()),
                "Confirme o PAT e repita `prt doctor`.".to_owned(),
            ));
            return;
        }
    };
    let parent_type = match work_item_id {
        Some(id) => match crate::azure::get_work_item(&client, id).await {
            Ok(item) => item.work_item_type().to_owned(),
            Err(error) => {
                checks.push(fail_check(
                    "Metadata do pai",
                    format!("{label}: não foi possível consultar o Work Item pai: {error}"),
                    "Confirme o ID da branch e as permissões de leitura de Work Items.".to_owned(),
                ));
                return;
            }
        },
        None if selection.profile.parent_transition().is_some() => {
            checks.push(warn_check(
                "Metadata do pai",
                format!(
                    "{label} / {}: nenhum Work Item da branch; o estado do pai não foi validado.",
                    selection.name()
                ),
                "Use uma branch com Work Item numérico ou rode `prt test --work-item <id>`."
                    .to_owned(),
            ));
            return;
        }
        None => String::new(),
    };
    match process_profiles::load_metadata(&client, &remote.project, &selection, &parent_type).await
    {
        Ok(metadata) => {
            let values_valid = [
                ("Custom.Team", selection.profile.team.as_str()),
                (&selection.program_field, selection.profile.program.as_str()),
            ]
            .into_iter()
            .filter_map(|(reference_name, value)| {
                metadata
                    .test_case_fields
                    .iter()
                    .find(|field| field.reference_name == reference_name)
                    .map(|field| process_profiles::validate_field_value(field, value))
            })
            .all(|result| result.is_ok());
            if values_valid {
                checks.push(ok_check(
                    "Metadata do perfil",
                    format!(
                        "{label} / {}: Test Case, fields e estado compatíveis.",
                        selection.name()
                    ),
                ));
            } else {
                checks.push(fail_check(
                    "Valores do perfil",
                    format!(
                        "{label} / {}: um valor padrão não é aceito pelo Azure.",
                        selection.name()
                    ),
                    "Ajuste team/program aos allowedValues retornados pelo processo.".to_owned(),
                ));
            }
        }
        Err(error) => checks.push(fail_check(
            "Metadata do perfil",
            format!("{label} / {}: {error}", selection.name()),
            "Confirme o Work Item Type, fields e estado do processo no Azure DevOps.".to_owned(),
        )),
    }
}

/// Executa `prog args` com timeout.
///
/// Retorna `None` em timeout, falha de spawn ou saída com erro. Em sucesso
/// retorna a saída aparada (`stdout`, ou `stderr` quando `stdout` vazio) —
/// pode ser `Some("")` (ex.: `git branch --show-current` em detached HEAD).
async fn run_cmd(prog: &str, args: &[&str], wait: Duration) -> Option<String> {
    let output = timeout(wait, async {
        for candidate in crate::process::command_candidates(prog) {
            let mut command = tokio::process::Command::new(&candidate);
            command.args(args).kill_on_drop(true);
            match command.output().await {
                Ok(output) => return Some(output),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(_) => return None,
            }
        }
        None
    })
    .await
    .ok()??;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if stdout.is_empty() {
        Some(String::from_utf8_lossy(&output.stderr).trim().to_owned())
    } else {
        Some(stdout)
    }
}

/// Remove escapes ANSI, `\r` e espaços das bordas (espelha `_cleanOutput`).
fn clean_output(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(current) = chars.next() {
        if current == '\u{1b}' {
            // Consome a sequência CSI (`ESC [ params final`): pula o `[`
            // e vai até o byte final (`@`-`~`), sem vazar parâmetros.
            let mut first = true;
            for next in chars.by_ref() {
                if first && next == '[' {
                    first = false;
                    continue;
                }
                first = false;
                if ('@'..='~').contains(&next) {
                    break;
                }
            }
        } else if current != '\r' {
            out.push(current);
        }
    }
    out.trim().to_owned()
}

/// Primeira linha não-vazia de `value` já limpa (espelha `_cleanLine`).
fn clean_line(value: &str) -> String {
    clean_output(value)
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or_default()
        .to_owned()
}

/// Rótulo `org/projeto/repo` (espelha `remoteLabel` do Dart).
fn remote_label(remote: &RepositoryRemote) -> String {
    let org = remote.organization.as_str();
    let project = remote.project.as_str();
    let repo = remote.repository.as_str();
    format!("{org}/{project}/{repo}")
}

/// Validador manual de email (fallback caso a regex não compile).
fn fallback_email_valid(normalized: &str) -> bool {
    if normalized.contains(' ') {
        return false;
    }
    let mut parts = normalized.split('@');
    match (parts.next(), parts.next(), parts.next()) {
        (Some(local), Some(domain), None) => {
            !local.is_empty()
                && domain.contains('.')
                && !domain.starts_with('.')
                && !domain.ends_with('.')
        }
        _ => false,
    }
}

/// Informa se `value` parece um email válido (espelha `validateOptionalEmail`).
///
/// Vazio retorna `false` — o chamador trata "não configurado" à parte.
fn is_valid_email(value: &str) -> bool {
    static RE: OnceLock<Option<Regex>> = OnceLock::new();
    let normalized = value.trim();
    if normalized.is_empty() {
        return false;
    }
    if let Some(re) = RE
        .get_or_init(|| Regex::new(r"^[^\s@]+@[^\s@]+\.[^\s@]+$").ok())
        .as_ref()
    {
        return re.is_match(normalized);
    }
    fallback_email_valid(normalized)
}

/// Normaliza a base URL: apara, remove `/` finais e o sufixo `/chat/completions`.
fn normalize_base_url(raw: &str) -> String {
    let mut base = raw.trim().to_owned();
    while base.ends_with('/') && base.len() > 1 {
        base.pop();
    }
    if let Some(stripped) = base.strip_suffix("/chat/completions") {
        base = stripped.to_owned();
    }
    while base.ends_with('/') && base.len() > 1 {
        base.pop();
    }
    base
}

/// Inspeciona Git: versão, repositório, branch, origin, remote Azure e contexto.
///
/// Nunca falha: cada problema vira um [`Check`]. Retorna o remote Azure
/// parseado do `origin` (para a sonda do Azure).
async fn inspect_git(source: Option<&str>, checks: &mut Vec<Check>) -> GitInspection {
    let version = run_cmd("git", &["--version"], CMD_TIMEOUT).await;
    let Some(version) = version else {
        checks.push(fail_check(
            "Git",
            "O executável git não está disponível.".to_owned(),
            "Instale o Git e abra um novo terminal antes de executar `prt doctor`.".to_owned(),
        ));
        return GitInspection::default();
    };
    checks.push(ok_check("Git", clean_line(&version)));

    let toplevel = run_cmd("git", &["rev-parse", "--show-toplevel"], CMD_TIMEOUT)
        .await
        .map(|out| out.trim().to_owned())
        .filter(|root| !root.is_empty());
    let Some(root) = toplevel else {
        checks.push(fail_check(
            "Repositório Git",
            "O diretório atual não está dentro de um repositório Git.".to_owned(),
            "Entre no clone do projeto usado para gerar o contexto.".to_owned(),
        ));
        return GitInspection::default();
    };
    checks.push(ok_check(
        "Repositório Git",
        format!("Projeto detectado em {root}."),
    ));

    let current = run_cmd("git", &["branch", "--show-current"], CMD_TIMEOUT)
        .await
        .unwrap_or_default();
    let branch = current.trim();
    if branch.is_empty() {
        checks.push(warn_check(
            "Branch de trabalho",
            "O Git está em detached HEAD.".to_owned(),
            "Mude para uma branch de trabalho ou use `--source <branch>`.".to_owned(),
        ));
    } else if matches!(branch, "dev" | "main" | "master") {
        checks.push(warn_check(
            "Branch de trabalho",
            format!("A branch atual ({branch}) é uma branch base."),
            "Mude para a branch da alteração antes de gerar o contexto.".to_owned(),
        ));
    } else {
        checks.push(ok_check("Branch de trabalho", branch.to_owned()));
    }

    let origin_url = run_cmd("git", &["remote", "get-url", "origin"], CMD_TIMEOUT)
        .await
        .map(|out| out.trim().to_owned())
        .filter(|url| !url.is_empty());
    if let Some(url) = &origin_url {
        checks.push(ok_check("Remote origin", url.clone()));
    } else {
        checks.push(fail_check(
            "Remote origin",
            "Nenhum remote origin foi encontrado.".to_owned(),
            "Configure o remote Azure DevOps com `git remote add origin <url>`.".to_owned(),
        ));
    }

    let origin_text = origin_url.as_deref().unwrap_or_default();
    let azure_remote = parse_azure_remote(origin_text);
    match &azure_remote {
        Some(remote) => {
            checks.push(ok_check("Remote Azure DevOps", remote_label(remote)));
        }
        None if origin_text.is_empty() => {
            checks.push(fail_check(
                "Remote Azure DevOps",
                "Nenhum remote Azure DevOps foi identificado.".to_owned(),
                "Configure o remote Azure DevOps com `git remote add origin <url>`.".to_owned(),
            ));
        }
        None => {
            checks.push(fail_check(
                "Remote Azure DevOps",
                format!("O origin atual não é Azure DevOps ({origin_text})."),
                "Aponte o origin para o repositório Azure DevOps.".to_owned(),
            ));
        }
    }

    let work_item_id = inspect_git_context(source, checks).await;
    GitInspection {
        remote: azure_remote,
        work_item_id,
    }
}

/// Tenta coletar o contexto de PR via `git::collect` sem propagar erro.
///
/// Sucesso gera `Contexto de PR` + `Work Item da branch`; erro vira aviso
/// em `Contexto de PR/Test Case` (não aborta o `doctor`).
async fn inspect_git_context(source: Option<&str>, checks: &mut Vec<Check>) -> Option<String> {
    let owned = source.map(str::to_owned);
    let collected =
        tokio::task::spawn_blocking(move || crate::git::collect(owned.as_deref())).await;
    match collected {
        Ok(Ok(ctx)) => {
            let lines = ctx.diff_original_lines;
            let base = ctx.base_branch.as_str();
            checks.push(ok_check(
                "Contexto de PR",
                format!("{lines} linhas de diff contra {base}."),
            ));
            if ctx.work_item_id.is_empty() {
                checks.push(warn_check(
                    "Work Item da branch",
                    "Nenhum ID numérico foi encontrado no nome da branch.".to_owned(),
                    "Use `--work-item <id>` ao gerar o PR ou Test Case.".to_owned(),
                ));
            } else {
                let id = ctx.work_item_id.as_str();
                checks.push(ok_check("Work Item da branch", format!("#{id}.")));
                return Some(ctx.work_item_id);
            }
            None
        }
        Ok(Err(err)) => {
            let detail = err.to_string();
            checks.push(warn_check(
                "Contexto de PR/Test Case",
                detail,
                "Entre na branch da alteração ou use `--source <branch>`.".to_owned(),
            ));
            None
        }
        Err(join_err) => {
            checks.push(warn_check(
                "Contexto de PR/Test Case",
                format!("falha interna ao coletar contexto Git: {join_err}"),
                "Entre na branch da alteração ou use `--source <branch>`.".to_owned(),
            ));
            None
        }
    }
}

/// Informa se `path` existe, sem falhar (`false` em qualquer erro).
async fn path_exists(path: &std::path::Path) -> bool {
    tokio::fs::try_exists(path).await.unwrap_or(false)
}

/// Checa um email opcional de reviewer/responsável.
fn inspect_email(component: &'static str, value: &str, checks: &mut Vec<Check>) {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        checks.push(warn_check(
            component,
            "Email não configurado; a confirmação solicitará o reviewer/responsável.".to_owned(),
            "Execute `prt init` ou informe o email durante o fluxo interativo.".to_owned(),
        ));
    } else if !is_valid_email(trimmed) {
        checks.push(warn_check(
            component,
            "O valor configurado não parece ser um email válido.".to_owned(),
            "Execute `prt init` e informe um email Azure DevOps válido.".to_owned(),
        ));
    } else {
        checks.push(ok_check(component, "Email configurado.".to_owned()));
    }
}

/// Inspeciona a configuração carregada (arquivos, template, providers, emails).
async fn inspect_configuration(config: &Config, checks: &mut Vec<Check>) {
    let paths = config_paths();
    let (config_exists, env_exists, template_exists) = tokio::join!(
        path_exists(&paths.config_file),
        path_exists(&paths.env_file),
        path_exists(&paths.template_file),
    );
    if config_exists || env_exists {
        let shown = if config_exists {
            paths.config_file.display().to_string()
        } else {
            paths.env_file.display().to_string()
        };
        checks.push(ok_check(
            "Configuração local",
            format!("{shown} carregado."),
        ));
    } else {
        checks.push(warn_check(
            "Configuração local",
            "Nenhum arquivo de configuração foi criado; defaults estão sendo usados.".to_owned(),
            "Execute `prt init` para salvar PAT, reviewers, provider e modelos.".to_owned(),
        ));
    }
    if template_exists {
        checks.push(ok_check(
            "Template de prompt",
            "Template personalizado encontrado.".to_owned(),
        ));
    } else {
        checks.push(warn_check(
            "Template de prompt",
            "O template padrão embutido será usado.".to_owned(),
            "Execute `prt init` para criar o template editável.".to_owned(),
        ));
    }
    if config.providers.is_empty() {
        checks.push(fail_check(
            "Providers configurados",
            "A lista de providers está vazia.".to_owned(),
            "Execute `prt init` e escolha pelo menos um provider.".to_owned(),
        ));
    } else {
        let names = config.providers.join(", ");
        checks.push(ok_check("Providers configurados", names));
    }
    inspect_email("Reviewer de dev", &config.reviewer_dev, checks);
    inspect_email("Reviewer de sprint", &config.reviewer_sprint, checks);
    inspect_email("Reviewer do Test Case", &config.test_assigned_to, checks);
    if config.test_area_path.trim().is_empty() {
        checks.push(warn_check(
            "Defaults do Test Case",
            "AreaPath não configurado.".to_owned(),
            "Informe o AreaPath durante a criação ou configure-o em `prt init`.".to_owned(),
        ));
    } else {
        let area = config.test_area_path.trim().to_owned();
        checks.push(ok_check(
            "Defaults do Test Case",
            format!("AreaPath: {area}."),
        ));
    }
}

/// Checks de um provider + se está pronto para gerar conteúdo.
struct ProviderOutcome {
    /// Checks emitidos.
    checks: Vec<Check>,
    /// `true` se o provider pode gerar conteúdo agora.
    ready: bool,
}

/// Sinais (minúsculos) de que o Codex NÃO está autenticado.
const CODEX_LOGGED_OUT_MARKERS: &[&str] = &[
    "not logged",
    "not authenticated",
    "not signed in",
    "please log",
    "login required",
    "no credentials",
    "unauthenticated",
    "não autenticado",
    "não está autenticado",
];

/// Interpreta `codex login status` de forma tolerante.
///
/// O comando já retornou sucesso; basta ausência de marcadores de logout.
/// Saída vazia conta como autenticado (espelha o Dart, que confia no exit).
fn codex_logged_in(output: &str) -> bool {
    let lower = output.to_lowercase();
    !CODEX_LOGGED_OUT_MARKERS
        .iter()
        .any(|marker| lower.contains(*marker))
}

/// Resultado da checagem de `opencode auth list`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OpencodeAuth {
    /// Credencial compatível encontrada.
    Ready,
    /// Nenhuma credencial utilizável.
    NoCredentials,
    /// Há credenciais, mas nenhuma para o provider do modelo.
    WrongProvider,
}

/// Classifica a saída de `opencode auth list` para o provider do modelo.
///
/// `provider` é o prefixo do modelo (`provider/modelo`), já em minúsculas.
fn classify_opencode_auth(output: &str, provider: &str) -> OpencodeAuth {
    let lower = clean_output(output).to_lowercase();
    if lower.trim().is_empty()
        || lower.contains("0 credentials")
        || lower.contains("no credentials")
    {
        return OpencodeAuth::NoCredentials;
    }
    if !provider.is_empty() && !lower.contains(provider) {
        return OpencodeAuth::WrongProvider;
    }
    OpencodeAuth::Ready
}

/// Classificação do status de `GET {base}/models`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CompatibleStatus {
    /// Acessível (inclui 4xx fora auth/404, como no Dart).
    Reachable,
    /// 401/403 — autenticação rejeitada.
    AuthRejected,
    /// 5xx — serviço com problema.
    ServerError,
    /// 404 — responde, mas sem `/models`.
    RouteMissing,
}

/// Classifica o status HTTP do endpoint OpenAI-compatible.
fn classify_compatible_status(status: u16) -> CompatibleStatus {
    if status == 401 || status == 403 {
        CompatibleStatus::AuthRejected
    } else if status >= 500 {
        CompatibleStatus::ServerError
    } else if status == 404 {
        CompatibleStatus::RouteMissing
    } else {
        CompatibleStatus::Reachable
    }
}

/// Resultado da sonda `GET {base}/models`.
#[derive(Debug, Clone, PartialEq, Eq)]
enum CompatibleProbe {
    /// Acessível.
    Reachable(u16),
    /// Autenticação rejeitada.
    AuthRejected(u16),
    /// Serviço com problema.
    ServerError(u16),
    /// Responde, mas sem `/models` (a geração pode funcionar).
    RouteMissing,
    /// Inalcançável (timeout, DNS, conexão).
    Unreachable(String),
}

/// Mensagem curta e sem segredos para erro de transporte HTTP.
fn friendly_http_error(err: &reqwest::Error) -> String {
    if err.is_timeout() {
        "tempo esgotado (timeout)".to_owned()
    } else if err.is_connect() {
        "falha de conexão".to_owned()
    } else {
        "erro de transporte".to_owned()
    }
}

/// Faz `GET {base}/models` com `Authorization` quando há key (timeout 10s).
async fn probe_compatible_models(base: &str, api_key: &str, has_key: bool) -> CompatibleProbe {
    let url = format!("{base}/models");
    let client = match reqwest::Client::builder().timeout(HTTP_TIMEOUT).build() {
        Ok(client) => client,
        Err(err) => return CompatibleProbe::Unreachable(err.to_string()),
    };
    let mut request = client.get(url);
    if has_key {
        request = request.header(reqwest::header::AUTHORIZATION, format!("Bearer {api_key}"));
    }
    let response = match request.send().await {
        Ok(response) => response,
        Err(err) => return CompatibleProbe::Unreachable(friendly_http_error(&err)),
    };
    let status = response.status().as_u16();
    match classify_compatible_status(status) {
        CompatibleStatus::Reachable => CompatibleProbe::Reachable(status),
        CompatibleStatus::AuthRejected => CompatibleProbe::AuthRejected(status),
        CompatibleStatus::ServerError => CompatibleProbe::ServerError(status),
        CompatibleStatus::RouteMissing => CompatibleProbe::RouteMissing,
    }
}

/// Inspeciona o Codex CLI + autenticação.
async fn inspect_codex(config: &Config) -> ProviderOutcome {
    let mut out = Vec::new();
    let executable = if config.codex_path.trim().is_empty() {
        "codex"
    } else {
        config.codex_path.trim()
    };
    let version = run_cmd(executable, &["--version"], CMD_TIMEOUT).await;
    let Some(version) = version else {
        out.push(fail_check(
            "Codex CLI",
            "O executável `codex` não foi encontrado ou não iniciou.".to_owned(),
            "Instale o Codex CLI e confirme que `codex` está no PATH.".to_owned(),
        ));
        return ProviderOutcome {
            checks: out,
            ready: false,
        };
    };
    let model = config.codex_model.as_str();
    let reasoning = config.codex_reasoning.as_str();
    let line = clean_line(&version);
    out.push(ok_check(
        "Codex CLI",
        format!("{line} · modelo {model} · thinking {reasoning}."),
    ));
    let login = run_cmd(executable, &["login", "status"], CMD_TIMEOUT).await;
    match login {
        Some(text) if codex_logged_in(&text) => {
            let detail = clean_line(&text);
            let detail = if detail.is_empty() {
                "Codex autenticado.".to_owned()
            } else {
                detail
            };
            out.push(ok_check("Autenticação Codex", detail));
            ProviderOutcome {
                checks: out,
                ready: true,
            }
        }
        Some(text) => {
            let detail = clean_line(&text);
            let detail = if detail.is_empty() {
                "O Codex não está autenticado.".to_owned()
            } else {
                detail
            };
            out.push(warn_check(
                "Autenticação Codex",
                detail,
                "Execute `codex login` e repita `prt doctor`.".to_owned(),
            ));
            ProviderOutcome {
                checks: out,
                ready: false,
            }
        }
        None => {
            out.push(warn_check(
                "Autenticação Codex",
                "O Codex não está autenticado.".to_owned(),
                "Execute `codex login` e repita `prt doctor`.".to_owned(),
            ));
            ProviderOutcome {
                checks: out,
                ready: false,
            }
        }
    }
}

/// Inspeciona o `OpenCode` CLI + credencial do provider do modelo.
async fn inspect_opencode(config: &Config) -> ProviderOutcome {
    let mut out = Vec::new();
    let executable = if config.opencode_path.trim().is_empty() {
        "opencode"
    } else {
        config.opencode_path.trim()
    };
    let version = run_cmd(executable, &["--version"], CMD_TIMEOUT).await;
    let Some(version) = version else {
        out.push(fail_check(
            "OpenCode CLI",
            "O executável `opencode` não foi encontrado ou não iniciou.".to_owned(),
            "Instale o OpenCode CLI e confirme que `opencode` está no PATH.".to_owned(),
        ));
        return ProviderOutcome {
            checks: out,
            ready: false,
        };
    };
    let model = config.opencode_model.as_str();
    let reasoning = config.opencode_reasoning.as_str();
    let line = clean_line(&version);
    out.push(ok_check(
        "OpenCode CLI",
        format!("{line} · modelo {model} · thinking {reasoning}."),
    ));
    let provider = config
        .opencode_model
        .split('/')
        .next()
        .unwrap_or_default()
        .trim()
        .to_lowercase();
    let raw = run_cmd(executable, &["auth", "list"], CMD_TIMEOUT)
        .await
        .unwrap_or_default();
    match classify_opencode_auth(&raw, &provider) {
        OpencodeAuth::Ready => {
            out.push(ok_check(
                "Autenticação OpenCode",
                "Credencial compatível encontrada.".to_owned(),
            ));
            ProviderOutcome {
                checks: out,
                ready: true,
            }
        }
        OpencodeAuth::NoCredentials => {
            out.push(warn_check(
                "Autenticação OpenCode",
                "Nenhuma credencial utilizável foi encontrada.".to_owned(),
                "Execute `opencode auth login` para o provider usado pelo modelo.".to_owned(),
            ));
            ProviderOutcome {
                checks: out,
                ready: false,
            }
        }
        OpencodeAuth::WrongProvider => {
            out.push(warn_check(
                "Autenticação OpenCode",
                format!("Não foi encontrada uma credencial clara para {provider}."),
                format!("Execute `opencode auth login {provider}` ou escolha outro modelo em `prt init`."),
            ));
            ProviderOutcome {
                checks: out,
                ready: false,
            }
        }
    }
}

/// Inspeciona o endpoint OpenAI-compatible (URL, key e `GET /models`).
async fn inspect_compatible(config: &Config) -> ProviderOutcome {
    let mut out = Vec::new();
    let base = normalize_base_url(&config.base_url);
    let valid = reqwest::Url::parse(&base)
        .is_ok_and(|url| matches!(url.scheme(), "http" | "https") && url.host_str().is_some());
    if !valid {
        let raw = config.base_url.as_str();
        out.push(warn_check(
            "OpenAI-compatible URL",
            format!("Base URL inválida: {raw}."),
            "Execute `prt init` e informe uma URL HTTP/HTTPS compatível.".to_owned(),
        ));
        return ProviderOutcome {
            checks: out,
            ready: false,
        };
    }
    let has_key = !config.api_key.trim().is_empty();
    if has_key {
        out.push(ok_check(
            "OpenAI-compatible API key",
            "API key configurada (valor oculto).".to_owned(),
        ));
    } else {
        out.push(warn_check(
            "OpenAI-compatible API key",
            "API key não configurada; alguns endpoints locais não exigem uma.".to_owned(),
            "Configure a API key em `prt init` se o endpoint exigir autenticação.".to_owned(),
        ));
    }
    match probe_compatible_models(&base, &config.api_key, has_key).await {
        CompatibleProbe::Reachable(status) => {
            out.push(ok_check(
                "OpenAI-compatible endpoint",
                format!("Endpoint acessível (HTTP {status})."),
            ));
            ProviderOutcome {
                checks: out,
                ready: true,
            }
        }
        CompatibleProbe::AuthRejected(status) => {
            out.push(warn_check(
                "OpenAI-compatible endpoint",
                format!("O endpoint respondeu HTTP {status}; a autenticação foi rejeitada."),
                "Revise a API key e a Base URL em `prt init`.".to_owned(),
            ));
            ProviderOutcome {
                checks: out,
                ready: false,
            }
        }
        CompatibleProbe::ServerError(status) => {
            out.push(warn_check(
                "OpenAI-compatible endpoint",
                format!("O endpoint está acessível, mas respondeu HTTP {status}."),
                "Verifique se o serviço está ativo e tente novamente.".to_owned(),
            ));
            ProviderOutcome {
                checks: out,
                ready: false,
            }
        }
        CompatibleProbe::RouteMissing => {
            out.push(warn_check(
                "OpenAI-compatible endpoint",
                "A URL respondeu, mas não expõe /models; a rota de geração pode ainda funcionar."
                    .to_owned(),
                "Confirme se a Base URL termina na raiz compatível, normalmente /v1.".to_owned(),
            ));
            ProviderOutcome {
                checks: out,
                ready: true,
            }
        }
        CompatibleProbe::Unreachable(detail) => {
            out.push(warn_check(
                "OpenAI-compatible endpoint",
                format!("Não foi possível alcançar {base}. {detail}"),
                "Confirme a Base URL, a rede e se o serviço está em execução.".to_owned(),
            ));
            ProviderOutcome {
                checks: out,
                ready: false,
            }
        }
    }
}

/// Inspeciona os providers configurados em paralelo, mais o resumo agregado.
async fn inspect_providers(config: &Config, checks: &mut Vec<Check>) -> usize {
    let want_codex = config.providers.iter().any(|p| p == "codex");
    let want_opencode = config.providers.iter().any(|p| p == "opencode");
    let want_compatible = config.providers.iter().any(|p| p == "openai-compatible");
    let (codex, opencode, compatible) = tokio::join!(
        async {
            if want_codex {
                inspect_codex(config).await
            } else {
                ProviderOutcome {
                    checks: Vec::new(),
                    ready: false,
                }
            }
        },
        async {
            if want_opencode {
                inspect_opencode(config).await
            } else {
                ProviderOutcome {
                    checks: Vec::new(),
                    ready: false,
                }
            }
        },
        async {
            if want_compatible {
                inspect_compatible(config).await
            } else {
                ProviderOutcome {
                    checks: Vec::new(),
                    ready: false,
                }
            }
        },
    );
    let mut ready = 0;
    for outcome in [codex, opencode, compatible] {
        if outcome.ready {
            ready += 1;
        }
        checks.extend(outcome.checks);
    }
    if ready == 0 {
        checks.push(fail_check(
            "Providers de IA",
            "Nenhum provider configurado está pronto para gerar conteúdo.".to_owned(),
            "Instale/autentique um provider e execute `prt init` para selecioná-lo.".to_owned(),
        ));
    } else {
        checks.push(ok_check(
            "Providers de IA",
            format!("{ready} provider(s) pronto(s) para geração."),
        ));
    }
    ready
}

/// Informa se o status Azure é sucesso (2xx).
fn azure_status_ok(status: u16) -> bool {
    (200..300).contains(&status)
}

/// Resultado bruto da sonda Azure (espelha `DoctorProbeResult` do Dart).
#[derive(Debug, Clone, PartialEq, Eq)]
enum AzureProbe {
    /// Consulta com sucesso.
    Ok,
    /// Resposta HTTP fora de 2xx.
    Http(u16),
    /// Erro de transporte (timeout, rede).
    Transport(String),
}

/// Faz `GET` autenticado no Azure DevOps (timeout 10s, sem falhar).
async fn probe_azure(url: &str, auth: &str) -> AzureProbe {
    let client = match reqwest::Client::builder().timeout(HTTP_TIMEOUT).build() {
        Ok(client) => client,
        Err(err) => return AzureProbe::Transport(err.to_string()),
    };
    let response = match client
        .get(url)
        .header(reqwest::header::AUTHORIZATION, auth)
        .send()
        .await
    {
        Ok(response) => response,
        Err(err) => return AzureProbe::Transport(friendly_http_error(&err)),
    };
    let status = response.status().as_u16();
    if azure_status_ok(status) {
        AzureProbe::Ok
    } else {
        AzureProbe::Http(status)
    }
}

/// Converte uma sonda Azure em [`Check`] (escopo usado no `fix` de 401/403).
fn push_azure_probe(
    checks: &mut Vec<Check>,
    component: &'static str,
    probe: AzureProbe,
    scope: &str,
) {
    match probe {
        AzureProbe::Ok => {
            checks.push(ok_check(
                component,
                "PAT consegue consultar o Azure DevOps; escrita não foi testada.".to_owned(),
            ));
        }
        AzureProbe::Http(status) => {
            let detail = format!("Azure DevOps respondeu HTTP {status}.");
            let fix = if status == 401 || status == 403 {
                format!("Revise o PAT e conceda o escopo Azure DevOps “{scope}”.")
            } else {
                "Confirme a rede, a organização/projeto do remote e repita `prt doctor`.".to_owned()
            };
            checks.push(fail_check(component, detail, fix));
        }
        AzureProbe::Transport(err) => {
            checks.push(fail_check(
                component,
                format!("Não foi possível consultar o Azure DevOps: {err}"),
                "Confirme a rede, a organização/projeto do remote e repita `prt doctor`."
                    .to_owned(),
            ));
        }
    }
}

/// Inspeciona PAT + permissões das APIs de Code e Work Items (sondas em paralelo).
async fn inspect_azure(
    config: &Config,
    remote: Option<&RepositoryRemote>,
    checks: &mut Vec<Check>,
) {
    if config.azure_pat.trim().is_empty() {
        checks.push(fail_check(
            "Azure DevOps PAT",
            "PAT não configurado; PRs e Test Cases não poderão ser publicados.".to_owned(),
            "Execute `prt init` ou defina AZURE_PAT/AZURE_DEVOPS_PAT.".to_owned(),
        ));
        return;
    }
    checks.push(ok_check(
        "Azure DevOps PAT",
        "PAT configurado (valor oculto).".to_owned(),
    ));
    let Some(remote) = remote else {
        checks.push(warn_check(
            "Azure DevOps APIs",
            "PAT disponível, mas o remote Azure DevOps não pôde ser identificado; as permissões não foram testadas.".to_owned(),
            "Configure um remote Azure DevOps válido e repita `prt doctor`.".to_owned(),
        ));
        return;
    };
    let org = remote.organization.as_str();
    let project = remote.project.as_str();
    let repos_url =
        format!("https://dev.azure.com/{org}/{project}/_apis/git/repositories?api-version=7.1");
    let wit_url =
        format!("https://dev.azure.com/{org}/{project}/_apis/wit/workitemtypes?api-version=7.1");
    let pat = config.azure_pat.trim();
    let auth = base64::engine::general_purpose::STANDARD.encode(format!(":{pat}"));
    let (repos, work_items) =
        tokio::join!(probe_azure(&repos_url, &auth), probe_azure(&wit_url, &auth),);
    push_azure_probe(checks, "Azure Code API", repos, "Code Read");
    push_azure_probe(
        checks,
        "Azure Work Items API",
        work_items,
        "Work Items Read",
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_line_should_strip_ansi_and_pick_first_line() {
        let raw = "\u{1b}[32mcodex-cli 1.2.3\u{1b}[0m\r\nsegunda linha\n";
        assert_eq!(clean_line(raw), "codex-cli 1.2.3");
    }

    #[test]
    fn clean_line_should_return_empty_when_blank() {
        assert_eq!(clean_line("  \r\n  "), "");
    }

    #[test]
    fn email_should_accept_valid_and_reject_invalid() {
        assert!(is_valid_email("dev@example.com"));
        assert!(!is_valid_email(""));
        assert!(!is_valid_email("sem-arroba"));
        assert!(!is_valid_email("sem-dominio@"));
        assert!(!is_valid_email("nome@sem-ponto"));
        assert!(is_valid_email("  dev@example.com  "));
    }

    #[tokio::test]
    async fn profile_binding_validation_should_report_actionable_failure() {
        let remote = RepositoryRemote {
            organization: "org".to_owned(),
            project: "CHECKMILK".to_owned(),
            repository: "checkmilk".to_owned(),
        };
        let config = Config {
            default_profile: crate::config::AGROTRACE_PROFILE.to_owned(),
            profiles: vec![
                crate::config::ProcessProfile::named(crate::config::AGROTRACE_PROFILE)
                    .expect("perfil suportado"),
            ],
            ..Config::default()
        };
        let mut checks = Vec::new();
        inspect_process_profiles(&config, Some(&remote), None, &mut checks).await;
        let binding = checks
            .iter()
            .find(|check| check.component == "Binding de processo")
            .expect("check de binding");
        assert!(!binding.ok);
        assert!(binding.detail.contains("org/CHECKMILK/checkmilk"));
        assert!(binding.fix.contains("prt init"));
        assert_eq!(DoctorReport { checks }.exit_code(), 1);
    }

    #[tokio::test]
    async fn doctor_accepts_generic_profile_and_reports_invalid_values() {
        let remote = RepositoryRemote {
            organization: "ibsbiosistemico".to_owned(),
            project: "Projeto".to_owned(),
            repository: "repo".to_owned(),
        };
        let profile = crate::config::ProcessProfile {
            name: "IBS Novo".to_owned(),
            program_field: "Custom.ProgramasNovo".to_owned(),
            team: "QA".to_owned(),
            program: "Produto".to_owned(),
            reviewer_dev: "invalido".to_owned(),
            ..crate::config::ProcessProfile::named("Agrotrace").unwrap()
        };
        let config = Config {
            profiles: vec![profile],
            default_profile: "IBS Novo".to_owned(),
            azure_pat: String::new(),
            ..Config::default()
        };
        let mut checks = Vec::new();
        inspect_process_profiles(&config, Some(&remote), None, &mut checks).await;

        let profile_check = checks
            .iter()
            .find(|check| check.component == "Perfil de processo")
            .expect("check do perfil");
        assert!(profile_check.ok);
        assert!(profile_check.detail.contains("Custom.ProgramasNovo"));
        let reviewer_check = checks
            .iter()
            .find(|check| check.component == "Reviewer do perfil")
            .expect("check de reviewer");
        assert!(!reviewer_check.ok);
        assert!(!reviewer_check.detail.contains("unsupported"));
    }

    #[test]
    fn base_url_should_normalize_slashes_and_chat_completions() {
        assert_eq!(
            normalize_base_url("https://api.example.com/v1/"),
            "https://api.example.com/v1"
        );
        assert_eq!(
            normalize_base_url("https://api.example.com/v1/chat/completions"),
            "https://api.example.com/v1"
        );
        assert_eq!(
            normalize_base_url("https://api.example.com/v1/chat/completions/"),
            "https://api.example.com/v1"
        );
        assert_eq!(
            normalize_base_url("  https://api.example.com/v1//  "),
            "https://api.example.com/v1"
        );
    }

    #[test]
    fn codex_login_should_detect_logged_out_markers() {
        assert!(codex_logged_in("Logged in as dev@example.com"));
        assert!(codex_logged_in(""));
        assert!(!codex_logged_in("Not logged in. Run `codex login`."));
        assert!(!codex_logged_in("error: no credentials found"));
        assert!(!codex_logged_in("login required"));
    }

    #[test]
    fn opencode_auth_should_classify_credentials() {
        assert_eq!(
            classify_opencode_auth("", "openai"),
            OpencodeAuth::NoCredentials
        );
        assert_eq!(
            classify_opencode_auth("0 credentials configured", "openai"),
            OpencodeAuth::NoCredentials
        );
        assert_eq!(
            classify_opencode_auth("anthropic: logged in", "openai"),
            OpencodeAuth::WrongProvider
        );
        assert_eq!(
            classify_opencode_auth("openai: logged in", "openai"),
            OpencodeAuth::Ready
        );
        assert_eq!(
            classify_opencode_auth("alguma credencial", ""),
            OpencodeAuth::Ready
        );
    }

    #[test]
    fn compatible_status_should_classify_http_codes() {
        assert_eq!(classify_compatible_status(200), CompatibleStatus::Reachable);
        assert_eq!(classify_compatible_status(201), CompatibleStatus::Reachable);
        assert_eq!(classify_compatible_status(400), CompatibleStatus::Reachable);
        assert_eq!(
            classify_compatible_status(401),
            CompatibleStatus::AuthRejected
        );
        assert_eq!(
            classify_compatible_status(403),
            CompatibleStatus::AuthRejected
        );
        assert_eq!(
            classify_compatible_status(404),
            CompatibleStatus::RouteMissing
        );
        assert_eq!(
            classify_compatible_status(500),
            CompatibleStatus::ServerError
        );
        assert_eq!(
            classify_compatible_status(503),
            CompatibleStatus::ServerError
        );
    }

    #[test]
    fn azure_status_should_accept_only_2xx() {
        assert!(azure_status_ok(200));
        assert!(azure_status_ok(201));
        assert!(!azure_status_ok(301));
        assert!(!azure_status_ok(401));
        assert!(!azure_status_ok(404));
        assert!(!azure_status_ok(500));
    }

    #[test]
    fn remote_label_should_join_segments() {
        let remote = RepositoryRemote {
            organization: "minhaorg".to_owned(),
            project: "meuproj".to_owned(),
            repository: "meurepo".to_owned(),
        };
        assert_eq!(remote_label(&remote), "minhaorg/meuproj/meurepo");
    }

    #[tokio::test]
    async fn command_timeout_should_terminate_child() {
        let dir = tempfile::tempdir().unwrap();
        let completed = dir.path().join("completed");

        #[cfg(windows)]
        let (command, args) = {
            let completed = completed.to_string_lossy().replace('\'', "''");
            let script = format!(
                "Start-Sleep -Milliseconds 1000; Set-Content -LiteralPath '{completed}' -Value completed"
            );
            (
                "powershell.exe".to_owned(),
                vec![
                    "-NoProfile".to_owned(),
                    "-NonInteractive".to_owned(),
                    "-Command".to_owned(),
                    script,
                ],
            )
        };

        #[cfg(not(windows))]
        let (command, args) = (
            "sh".to_owned(),
            vec![
                "-c".to_owned(),
                "sleep 1; printf completed > \"$1\"".to_owned(),
                "prt-test".to_owned(),
                completed.to_string_lossy().into_owned(),
            ],
        );

        let args = args.iter().map(String::as_str).collect::<Vec<_>>();
        assert_eq!(
            run_cmd(&command, &args, Duration::from_millis(300)).await,
            None
        );
        tokio::time::sleep(Duration::from_millis(1800)).await;
        assert!(
            !completed.exists(),
            "o comando continuou executando após o timeout"
        );
    }
}
