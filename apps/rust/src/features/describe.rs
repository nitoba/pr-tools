//! `prt desc` — gera descrição de PR e publica nos targets.
//!
//! Fluxo (espelha `describe_command.dart`):
//! prepare → dry-run? → generate (+rewrite se > 4000) → mostra + copia →
//! confirma criação + reviewers → publica.

use std::time::Duration;

use tracing::info;

use crate::ai::{self, PrDescription};
use crate::azure::pull_requests::{self, PullRequestCandidate};
use crate::azure::{self, work_items::FunctionalWorkItemContext};
use crate::cli::CliOptions;
use crate::config::{self, Config};
use crate::error::{AppError, Result};
use crate::git::{self, ChangeContext, GitContextFingerprint};

/// Limite das operações Azure acionadas pela recuperação da TUI.
pub const TUI_AZURE_TIMEOUT: Duration = Duration::from_secs(30);

/// Estado da leitura do contexto funcional usado por desc.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FunctionalContextStatus {
    /// Nenhum Work Item foi resolvido; o fluxo é Git-only.
    NotRequested,
    /// O Work Item foi lido e projetado com sucesso.
    Loaded(FunctionalWorkItemContext),
    /// A leitura falhou; contém somente uma mensagem segura para exibição.
    Unavailable(String),
}

impl FunctionalContextStatus {
    /// Retorna a projeção quando a leitura foi concluída.
    #[must_use]
    pub fn context(&self) -> Option<&FunctionalWorkItemContext> {
        match self {
            Self::Loaded(context) => Some(context),
            Self::NotRequested | Self::Unavailable(_) => None,
        }
    }

    /// Retorna se a TUI precisa de confirmação para seguir Git-only.
    #[must_use]
    pub fn requires_confirmation(&self) -> bool {
        matches!(self, Self::Unavailable(_))
    }

    /// Rótulo seguro para preparação, revisão, logs e receipts.
    #[must_use]
    pub fn display_label(&self) -> String {
        match self {
            Self::NotRequested => "somente contexto Git (sem Work Item)".to_owned(),
            Self::Loaded(context) => {
                format!("Work Item #{} — {}", context.id, context.title)
            }
            Self::Unavailable(message) => format!("indisponível: {message}"),
        }
    }
}

/// Contexto preparado para geração.
#[derive(Debug, Clone)]
pub struct DescribePrep {
    /// Config resolvida (com overrides CLI).
    pub config: Config,
    /// Contexto Git.
    pub context: ChangeContext,
    /// Targets resolvidos.
    pub targets: Vec<String>,
    /// Work Item (CLI ou branch).
    pub work_item_id: String,
    /// Estado e projeção do contexto funcional.
    pub functional_context: FunctionalContextStatus,
    /// Snapshot bruto do Work Item carregado para reutilização no handoff.
    pub work_item: Option<azure::WorkItem>,
    /// Fingerprint do checkout capturado antes da publicação.
    pub fingerprint: GitContextFingerprint,
    /// Prompt de usuário.
    pub prompt: String,
}

/// Prepara config + contexto + prompt.
///
/// # Errors
///
/// Retorna erro de Git/config se coleta falhar.
pub async fn prepare(options: &CliOptions) -> Result<DescribePrep> {
    let mut config = config::load_config()?;
    config::apply_cli_overrides(
        &mut config,
        options.provider.as_deref(),
        options.model.as_deref(),
        options.base_url.as_deref(),
        options.api_key.as_deref(),
    );
    let context = git::collect(options.source.as_deref())?;
    let targets = git::resolve_targets(&context, &options.targets);
    let uses_default_targets = options.targets.is_empty();
    if (uses_default_targets || options.targets.iter().any(|t| t == "sprint"))
        && context.sprint_branch.is_empty()
    {
        return Err(AppError::Git {
            message: "target sprint solicitado mas nenhuma branch sprint/* encontrada".to_owned(),
        });
    }
    let work_item_id = options
        .work_item
        .as_ref()
        .map_or_else(|| context.work_item_id.clone(), |w| w.as_str().to_owned());
    let (functional_context, work_item) = load_functional_context_with_snapshot(
        context.remote.as_ref(),
        &config.azure_pat,
        &work_item_id,
    )
    .await;
    let fingerprint =
        git::GitContextFingerprint::capture(&context.branch, &targets).unwrap_or_default();
    let prompt = ai::build_describe_prompt(
        &context.branch,
        &targets,
        &work_item_id,
        functional_context.context(),
        &context.log,
        &context.diff,
    );
    Ok(DescribePrep {
        config,
        context,
        targets,
        work_item_id,
        functional_context,
        work_item,
        fingerprint,
        prompt,
    })
}

#[cfg(test)]
async fn load_functional_context(
    remote: Option<&git::RepositoryRemote>,
    pat: &str,
    work_item_id: &str,
) -> FunctionalContextStatus {
    load_functional_context_with_snapshot(remote, pat, work_item_id)
        .await
        .0
}

async fn load_functional_context_with_snapshot(
    remote: Option<&git::RepositoryRemote>,
    pat: &str,
    work_item_id: &str,
) -> (FunctionalContextStatus, Option<azure::WorkItem>) {
    if work_item_id.trim().is_empty() {
        return (FunctionalContextStatus::NotRequested, None);
    }
    let client = match azure::client_for(remote, pat) {
        Ok(client) => client,
        Err(error) => {
            return (
                FunctionalContextStatus::Unavailable(safe_azure_error(&error)),
                None,
            );
        }
    };
    match azure::get_work_item(&client, work_item_id.trim()).await {
        Ok(item) => (
            FunctionalContextStatus::Loaded(FunctionalWorkItemContext::from_work_item(&item)),
            Some(item),
        ),
        Err(error) => (
            FunctionalContextStatus::Unavailable(safe_azure_error(&error)),
            None,
        ),
    }
}

fn safe_azure_error(error: &AppError) -> String {
    match error {
        AppError::Azure { status, .. } if *status == 401 || *status == 403 => format!(
            "Azure DevOps recusou a leitura do Work Item (HTTP {status}); verifique o PAT e a permissão de leitura"
        ),
        AppError::Azure { status, .. } => {
            format!("Azure DevOps não pôde carregar o Work Item (HTTP {status})")
        }
        AppError::Http(_) => {
            "não foi possível comunicar com Azure DevOps; verifique a rede e o PAT".to_owned()
        }
        AppError::Config { .. } => {
            "Azure DevOps não está configurado para carregar o Work Item".to_owned()
        }
        AppError::Git { .. } => "remote Azure DevOps não encontrado".to_owned(),
        _ => "não foi possível carregar o contexto funcional do Work Item".to_owned(),
    }
}

/// Mensagem para modos sem canal seguro de confirmação.
#[must_use]
pub fn non_interactive_functional_context_error(
    status: &FunctionalContextStatus,
) -> Option<AppError> {
    let FunctionalContextStatus::Unavailable(detail) = status else {
        return None;
    };
    Some(AppError::FunctionalContext {
        message: format!(
            "contexto funcional não pôde ser carregado; modo não interativo não permite fallback Git-only: {detail}. Execute em um terminal interativo para confirmar Git-only ou corrija o acesso ao Azure"
        ),
    })
}

/// Gera descrição (com rewrite se exceder 4000).
///
/// # Errors
///
/// Propaga [`AppError::Ai`].
pub async fn generate(prep: &DescribePrep) -> Result<PrDescription> {
    let report = |provider: &str, model: &str| {
        info!(provider, model, "tentando gerar descrição");
    };
    generate_from_prompt(&prep.config, &prep.prompt, &prep.context.branch, report).await
}

/// Gera uma descrição a partir de um prompt já montado.
///
/// Compartilha a validação e a reescrita de limite entre criação e
/// atualização, sem compartilhar seus publishers remotos.
///
/// # Errors
///
/// Propaga [`AppError::Ai`] ou [`AppError::DescriptionTooLong`].
pub async fn generate_from_prompt(
    config: &Config,
    prompt: &str,
    branch: &str,
    report: impl Fn(&str, &str),
) -> Result<PrDescription> {
    let raw = ai::generate_with_fallback(config, &config.template, prompt, &report).await?;
    let mut desc = ai::normalize_description(&raw, branch);
    if !ai::is_within_limit(&desc.body) {
        let rewrite_system = format!("{}\n\n{}", config.template, ai::REWRITE_INSTRUCTIONS);
        let rewrite_prompt = format!("## Descrição original\n\n# {}\n\n{}", desc.title, desc.body);
        let raw2 =
            ai::generate_with_fallback(config, &rewrite_system, &rewrite_prompt, &report).await?;
        desc = ai::normalize_description(&raw2, branch);
    }
    ai::validate_description(&desc)?;
    Ok(desc)
}

/// Copia body para o clipboard (best-effort).
#[must_use]
pub fn copy_to_clipboard(body: &str) -> bool {
    arboard::Clipboard::new()
        .and_then(|mut c| c.set_text(body.to_owned()))
        .is_ok()
}

/// Classificação de uma falha ocorrida ao publicar um PR.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PublishFailureKind {
    /// O Azure respondeu recusando a operação; o PR não deve ter sido criado.
    Confirmed,
    /// A resposta não permite saber se o Azure criou o PR.
    OutcomeUnknown,
}

/// Falha de publicação pronta para a TUI apresentar e recuperar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublishFailure {
    /// Mensagem acionável sem payload JSON bruto.
    pub message: String,
    /// Se é seguro assumir que não houve criação.
    pub kind: PublishFailureKind,
    /// Target em processamento quando a falha ocorreu, se conhecido.
    pub target: Option<String>,
}

/// Classifica e explica uma falha ocorrida ao criar um Pull Request.
///
/// Falhas de transporte, timeout, limitação e respostas 5xx/2xx sem payload
/// válido são tratadas como resultado incerto: o Azure pode ter criado o PR
/// antes de a resposta ser perdida.
#[must_use]
pub fn classify_publish_error(error: &AppError, target: Option<&str>) -> PublishFailure {
    let (kind, message) = match error {
        AppError::Http(_) => (
            PublishFailureKind::OutcomeUnknown,
            "não foi possível confirmar se o Azure DevOps criou o PR por causa de uma falha de rede"
                .to_owned(),
        ),
        AppError::Azure { status, message } => {
            let detail = azure_error_detail(message);
            let kind = if *status < 300 || *status == 408 || *status == 429 || *status >= 500 {
                PublishFailureKind::OutcomeUnknown
            } else {
                PublishFailureKind::Confirmed
            };
            let text = match *status {
                401 | 403 => format!(
                    "Azure DevOps recusou a criação por falta de permissão ou PAT inválido (HTTP {status}); verifique o PAT, o projeto e a permissão de criação de Pull Requests"
                ),
                408 | 429 => format!(
                    "não foi possível confirmar a criação do PR (HTTP {status}); verifique o Azure DevOps antes de reenviar"
                ),
                500..=599 => format!(
                    "não foi possível confirmar a criação do PR porque o Azure DevOps falhou (HTTP {status}); verifique o Azure DevOps antes de reenviar"
                ),
                200..=299 => format!(
                    "o Azure DevOps respondeu sucesso, mas não foi possível confirmar a criação do PR (HTTP {status}); verifique antes de reenviar"
                ),
                _ => format!("Azure DevOps recusou a criação do PR (HTTP {status})"),
            };
            (kind, append_detail(text, detail.as_str()))
        }
        _ => (PublishFailureKind::Confirmed, error.to_string()),
    };
    let target = target.map(str::to_owned);
    let message = match target.as_deref() {
        Some(target) => format!("target {target}: {message}"),
        None => message,
    };
    PublishFailure {
        message,
        kind,
        target,
    }
}

/// Procura PRs recentes que possam ter sido criados antes de uma resposta
/// perdida.
///
/// A comparação exige título, branch origem e target exatos. Quando há Work
/// Item no contexto, a relação também é consultada e exibida como critério
/// adicional. A adoção continua sendo uma decisão exclusiva da TUI.
///
/// # Errors
///
/// Propaga falhas da consulta Azure principal; relações individuais são
/// best-effort para que um candidato ainda possa ser adotado visualmente.
pub async fn find_publish_candidates(
    remote: &git::RepositoryRemote,
    branch: &str,
    work_item_id: &str,
    title: &str,
    target: &str,
) -> Result<Vec<PullRequestCandidate>> {
    let config = config::load_config()?;
    let client =
        azure::client_for_with_timeout(Some(remote), config.azure_pat.trim(), TUI_AZURE_TIMEOUT)?;
    let mut candidates = pull_requests::find_recent_pull_request_candidates(
        &client,
        &remote.project,
        &remote.repository,
        title,
        &format!("refs/heads/{branch}"),
        &format!("refs/heads/{target}"),
        work_item_id.trim(),
    )
    .await?;
    for candidate in &mut candidates {
        if candidate.url.trim().is_empty() || candidate.url.contains("/_apis/") {
            candidate.url = format!(
                "https://dev.azure.com/{}/{}/_git/{}/pullrequest/{}",
                remote.organization, remote.project, remote.repository, candidate.id
            );
        }
    }
    Ok(candidates)
}

fn append_detail(message: String, detail: &str) -> String {
    if detail.is_empty() {
        message
    } else {
        format!("{message}: {detail}")
    }
}

fn azure_error_detail(raw: &str) -> String {
    let detail = serde_json::from_str::<serde_json::Value>(raw)
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

fn first_error_message(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::Object(object) => {
            for key in ["message", "Message", "errorMessage"] {
                if let Some(message) = object.get(key).and_then(serde_json::Value::as_str) {
                    if !message.trim().is_empty() {
                        return Some(message.to_owned());
                    }
                }
            }
            object.values().find_map(first_error_message)
        }
        serde_json::Value::Array(values) => values.iter().find_map(first_error_message),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn preparation_without_work_item_should_not_request_functional_context() {
        let remote = None;
        let status = load_functional_context(remote, "", "").await;
        assert_eq!(status, FunctionalContextStatus::NotRequested);
    }

    #[test]
    fn non_interactive_functional_context_failure_should_be_actionable() {
        let status = FunctionalContextStatus::Unavailable(
            "Azure DevOps recusou a leitura do Work Item (HTTP 403)".to_owned(),
        );
        let error =
            non_interactive_functional_context_error(&status).expect("falha não interativa");
        assert_eq!(error.exit_code(), 1);
        let message = error.to_string();
        assert!(message.contains("contexto funcional não pôde ser carregado"));
        assert!(message.contains("fallback Git-only"));
        assert!(message.contains("HTTP 403"));
    }

    #[test]
    fn functional_context_error_should_not_expose_azure_response_body() {
        let error = AppError::Azure {
            status: 401,
            message: r#"{"message":"secret work item description"}"#.to_owned(),
        };
        let safe = safe_azure_error(&error);
        assert!(safe.contains("HTTP 401"));
        assert!(!safe.contains("secret work item description"));
    }

    #[test]
    fn copy_should_not_panic_on_empty() {
        // Apenas garante que a função existe e retorna bool (pode falhar sem display).
        let _ = copy_to_clipboard("teste");
    }

    #[test]
    fn network_publish_failure_should_be_outcome_unknown() {
        let error = AppError::Azure {
            status: 504,
            message: "gateway timeout".to_owned(),
        };
        let failure = classify_publish_error(&error, Some("dev"));
        assert_eq!(failure.kind, PublishFailureKind::OutcomeUnknown);
        assert_eq!(failure.target.as_deref(), Some("dev"));
        assert!(failure.message.contains("target dev"));
        assert!(failure.message.contains("confirmar"));
    }

    #[test]
    fn unauthorized_publish_failure_should_explain_pat_and_permissions() {
        let error = AppError::Azure {
            status: 403,
            message: r#"{"message":"not allowed"}"#.to_owned(),
        };
        let failure = classify_publish_error(&error, None);
        assert_eq!(failure.kind, PublishFailureKind::Confirmed);
        assert!(failure.message.contains("PAT"));
        assert!(failure.message.contains("permissão"));
        assert!(failure.message.contains("not allowed"));
    }
}
