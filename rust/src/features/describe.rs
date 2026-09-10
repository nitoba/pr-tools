//! `prt desc` — gera descrição de PR e publica nos targets.
//!
//! Fluxo (espelha `describe_command.dart`):
//! prepare → dry-run? → generate (+rewrite se > 4000) → mostra + copia →
//! confirma criação + reviewers → publica.

use tracing::info;

use crate::ai::{self, PrDescription};
use crate::cli::CliOptions;
use crate::config::{self, Config};
use crate::error::{AppError, Result};
use crate::git::{self, ChangeContext};

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
    if options.targets.iter().any(|t| t == "sprint") && context.sprint_branch.is_empty() {
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
    let raw = ai::generate_with_fallback(&prep.config, &prep.config.template, &prep.prompt, report)
        .await?;
    let mut desc = ai::normalize_description(&raw, &prep.context.branch);
    if !ai::is_within_limit(&desc.body) {
        let rewrite_system = format!("{}\n\n{}", prep.config.template, ai::REWRITE_INSTRUCTIONS);
        let rewrite_prompt = format!("## Descrição original\n\n# {}\n\n{}", desc.title, desc.body);
        let raw2 =
            ai::generate_with_fallback(&prep.config, &rewrite_system, &rewrite_prompt, report)
                .await?;
        desc = ai::normalize_description(&raw2, &prep.context.branch);
        ai::validate_description(&desc)?;
    }
    Ok(desc)
}

/// Copia body para o clipboard (best-effort).
#[must_use]
pub fn copy_to_clipboard(body: &str) -> bool {
    arboard::Clipboard::new()
        .and_then(|mut c| c.set_text(body.to_owned()))
        .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copy_should_not_panic_on_empty() {
        // Apenas garante que a função existe e retorna bool (pode falhar sem display).
        let _ = copy_to_clipboard("teste");
    }
}
