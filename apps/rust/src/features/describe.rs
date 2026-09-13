//! `prt desc` — gera descrição de PR e publica nos targets.
//!
//! Fluxo (espelha `describe_command.dart`):
//! prepare → dry-run? → generate (+rewrite se > 4000) → mostra + copia →
//! confirma criação + reviewers → publica.

use std::time::Duration;

use tracing::info;

use crate::ai::{self, PrDescription};
use crate::azure;
use crate::azure::pull_requests::{self, PullRequestCandidate};
use crate::cli::CliOptions;
use crate::config::{self, Config};
use crate::error::{AppError, Result};
use crate::git::{self, ChangeContext};

/// Limite das operações Azure acionadas pela recuperação da TUI.
pub const TUI_AZURE_TIMEOUT: Duration = Duration::from_secs(30);

/// Contexto preparado para geração.
#[derive(Debug)]
pub struct DescribePrep {
    /// Config resolvida (com overrides CLI).
    pub config: Config,
    /// Contexto Git.
    pub context: ChangeContext,
    /// Targets resolvidos.
    pub targets: Vec<String>,
    /// Work Item (CLI ou branch).
    pub work_item_id: String,
    /// Prompt de usuário.
    pub prompt: String,
}

/// Prepara config + contexto + prompt.
///
/// # Errors
///
/// Retorna erro de Git/config se coleta falhar.
pub fn prepare(options: &CliOptions) -> Result<DescribePrep> {
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
    let prompt = ai::build_describe_prompt(
        &context.branch,
        &targets,
        &work_item_id,
        &context.log,
        &context.diff,
    );
    Ok(DescribePrep {
        config,
        context,
        targets,
        work_item_id,
        prompt,
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
