//! `prt init` — wizard de configuração.
//!
//! Espelha `config_service_live.dart#initialize`: pré-preenche a partir do
//! `config.json` + `.env` + env existentes, valida emails, e salva
//! `config.json` (0600) + merge `.env` + `pr-template.md` (se ausente).
//! A TUI vive em [`crate::tui::init_wizard`]; aqui fica só lógica testável.

use std::path::Path;

use crate::config::{
    AGROTRACE_PROFILE, COMPATIBLE_REASONING, Config, DEFAULT_BASE_URL, DEFAULT_COMPATIBLE_MODEL,
    DEFAULT_TEMPLATE, OPENCODE_MODEL, OPENCODE_REASONING, ProcessProfile, RepositoryProfileBinding,
    config_paths,
};
use crate::error::{AppError, Result};

/// Providers oferecidos (valor + rótulo), como no Dart.
pub const PROVIDERS: &[(&str, &str)] = &[
    ("codex", "Codex local"),
    ("opencode", "OpenCode local"),
    ("openai-compatible", "OpenAI-compatible"),
];

/// Níveis de thinking (valor + dica), como no Dart.
pub const REASONING_LEVELS: &[(&str, &str)] = &[
    ("provider-default", "padrão do provider"),
    ("none", "sem reasoning adicional"),
    ("minimal", "resposta mais rápida"),
    ("low", "reasoning leve"),
    ("medium", "equilíbrio custo/profundidade"),
    ("high", "reasoning aprofundado"),
    ("xhigh", "máxima profundidade"),
];

/// Valida o conjunto fechado de schemas expostos pelo wizard.
#[must_use]
pub fn validate_process_profile_name(name: &str) -> Option<String> {
    if ProcessProfile::named(name).is_some() {
        None
    } else {
        Some("schema de perfil inválido; use Agrotrace ou CheckMilk".to_owned())
    }
}

/// Valida email opcional — espelha `validateOptionalEmail`.
///
/// Retorna `None` quando válido (vazio ok) ou a mensagem de erro.
#[must_use]
pub fn validate_optional_email(value: &str) -> Option<String> {
    let normalized = value.trim();
    if normalized.is_empty() {
        return None;
    }
    let mut parts = normalized.split('@');
    let ok = match (parts.next(), parts.next(), parts.next()) {
        (Some(user), Some(domain), None) => !user.is_empty() && domain.contains('.'),
        _ => false,
    };
    if ok && !normalized.contains(' ') {
        None
    } else {
        Some("informe um email válido ou deixe vazio.".to_owned())
    }
}

/// Rascunho editável do wizard (pré-preenchido com o existente).
#[derive(Debug, Clone)]
pub struct InitDraft {
    /// Novo PAT digitado (vazio = manter o atual).
    pub pat_input: String,
    /// PAT atual existe?
    pub has_existing_pat: bool,
    /// PAT efetivo (existente; só exibido como `••••`).
    pub reviewer_sprint: String,
    /// Email de review de dev.
    pub reviewer_dev: String,
    /// Responsável do card de teste.
    pub test_assigned_to: String,
    /// Provider padrão.
    pub provider: String,
    /// Modelo do Codex.
    pub codex_model: String,
    /// Caminho opcional do executável do Codex (vazio = PATH).
    pub codex_path: String,
    /// Thinking do Codex.
    pub codex_reasoning: String,
    /// Modelo do `OpenCode`.
    pub opencode_model: String,
    /// Caminho opcional do executável do `OpenCode` (vazio = PATH).
    pub opencode_path: String,
    /// Thinking do `OpenCode`.
    pub opencode_reasoning: String,
    /// Base URL compatible.
    pub base_url: String,
    /// Modelo compatible.
    pub compatible_model: String,
    /// Thinking compatible.
    pub compatible_reasoning: String,
    /// Nova API key digitada (vazio = manter).
    pub api_key_input: String,
    /// API key atual existe?
    pub has_existing_api_key: bool,
    /// `AreaPath` padrão.
    pub test_area_path: String,
    /// Team (default `DevOps`).
    pub test_team: String,
    /// Program (default `Agrotrace`).
    pub test_program: String,
}

impl InitDraft {
    /// Carrega rascunho a partir dos arquivos + env existentes.
    #[must_use]
    pub fn load_existing() -> Self {
        let cfg = crate::config::load_config().unwrap_or_default();
        Self {
            pat_input: String::new(),
            has_existing_pat: !cfg.azure_pat.is_empty(),
            reviewer_sprint: cfg.reviewer_sprint,
            reviewer_dev: cfg.reviewer_dev,
            test_assigned_to: cfg.test_assigned_to,
            provider: cfg
                .providers
                .first()
                .cloned()
                .unwrap_or_else(|| "codex".to_owned()),
            codex_model: cfg.codex_model,
            codex_path: cfg.codex_path,
            codex_reasoning: cfg.codex_reasoning,
            opencode_model: cfg.opencode_model,
            opencode_path: cfg.opencode_path,
            opencode_reasoning: cfg.opencode_reasoning,
            base_url: cfg.base_url,
            compatible_model: cfg.compatible_model,
            compatible_reasoning: cfg.compatible_reasoning,
            api_key_input: String::new(),
            has_existing_api_key: !cfg.api_key.is_empty(),
            test_area_path: cfg.test_area_path,
            test_team: if cfg.test_team.is_empty() {
                "DevOps".to_owned()
            } else {
                cfg.test_team
            },
            test_program: if cfg.test_program.is_empty() {
                "Agrotrace".to_owned()
            } else {
                cfg.test_program
            },
        }
    }

    /// Converte em [`Config`] pronta para salvar.
    ///
    /// `keep_pat`/`keep_key` são os valores atuais lidos do disco.
    #[must_use]
    pub fn to_config(&self, keep_pat: &str, keep_key: &str) -> Config {
        Config {
            providers: vec![self.provider.clone()],
            base_url: or_default(&self.base_url, DEFAULT_BASE_URL),
            compatible_model: or_default(&self.compatible_model, DEFAULT_COMPATIBLE_MODEL),
            compatible_reasoning: or_default(&self.compatible_reasoning, COMPATIBLE_REASONING),
            codex_model: or_default(&self.codex_model, crate::config::CODEX_MODEL),
            codex_path: self.codex_path.trim().to_owned(),
            codex_reasoning: or_default(&self.codex_reasoning, crate::config::CODEX_REASONING),
            opencode_model: or_default(&self.opencode_model, OPENCODE_MODEL),
            opencode_path: self.opencode_path.trim().to_owned(),
            opencode_reasoning: or_default(&self.opencode_reasoning, OPENCODE_REASONING),
            azure_pat: if self.pat_input.trim().is_empty() {
                keep_pat.to_owned()
            } else {
                self.pat_input.trim().to_owned()
            },
            api_key: if self.api_key_input.trim().is_empty() {
                keep_key.to_owned()
            } else {
                self.api_key_input.trim().to_owned()
            },
            reviewer_dev: self.reviewer_dev.trim().to_owned(),
            reviewer_sprint: self.reviewer_sprint.trim().to_owned(),
            test_area_path: self.test_area_path.trim().to_owned(),
            test_assigned_to: self.test_assigned_to.trim().to_owned(),
            test_team: or_default(&self.test_team, "DevOps"),
            test_program: or_default(&self.test_program, "Agrotrace"),
            template: current_template(),
            profiles: Vec::new(),
            bindings: Vec::new(),
            default_profile: String::new(),
        }
    }

    /// Valida todos os campos; retorna a primeira mensagem de erro.
    #[must_use]
    pub fn validate_all(&self) -> Option<String> {
        for (label, value) in [
            ("email da sprint", &self.reviewer_sprint),
            ("email de dev", &self.reviewer_dev),
            ("responsável do card", &self.test_assigned_to),
        ] {
            if let Some(err) = validate_optional_email(value) {
                return Some(format!("{label}: {err}"));
            }
        }
        if !PROVIDERS.iter().any(|(v, _)| *v == self.provider) {
            return Some("provider inválido.".to_owned());
        }
        None
    }
}

fn or_default(value: &str, default: &str) -> String {
    let t = value.trim();
    if t.is_empty() {
        default.to_owned()
    } else {
        t.to_owned()
    }
}

/// Template atual do disco (ou o padrão).
fn current_template() -> String {
    let paths = config_paths();
    std::fs::read_to_string(&paths.template_file).map_or_else(
        |_| DEFAULT_TEMPLATE.to_owned(),
        |t| {
            if t.trim().is_empty() {
                DEFAULT_TEMPLATE.to_owned()
            } else {
                t
            }
        },
    )
}

/// Resultado do `init`.
#[derive(Debug)]
pub struct InitResult {
    /// Config foi salva.
    pub saved: bool,
    /// PAT configurado.
    pub pat_configured: bool,
    /// Caminhos usados (para exibir).
    pub config_file: String,
    /// Arquivo `.env` usado para segredos e overrides.
    pub env_file: String,
}

/// Salva o rascunho: `config.json` (0600) + merge `.env` + template.
///
/// Espelha o Dart: PAT vai só para o `.env`; `config.json` não guarda PAT.
///
/// # Errors
///
/// Retorna [`AppError`] se não conseguir escrever os arquivos.
pub fn save_draft(draft: &InitDraft) -> Result<InitResult> {
    let previous = crate::config::load_config().unwrap_or_default();
    let profile = if previous.default_profile.trim().is_empty() {
        AGROTRACE_PROFILE
    } else {
        previous.default_profile.as_str()
    };
    let transition = previous
        .profiles
        .iter()
        .find(|candidate| candidate.name == profile)
        .map_or(Some("Test QA"), |candidate| {
            candidate.parent_transition.as_deref()
        });
    save_draft_for_profile_with_transition(draft, profile, transition)
}

/// Salva o draft e atualiza o perfil fixo escolhido pelo wizard.
///
/// # Errors
///
/// Retorna [`AppError`] se o perfil for inválido ou não conseguir escrever os
/// arquivos de configuração.
pub fn save_draft_for_profile(draft: &InitDraft, profile_name: &str) -> Result<InitResult> {
    save_draft_for_profile_with_transition(draft, profile_name, Some("Test QA"))
}

/// Salva o perfil escolhido incluindo a transição opcional do Work Item pai.
///
/// # Errors
///
/// Retorna [`AppError`] se o perfil for inválido, a configuração não passar na
/// validação ou não conseguir escrever os arquivos.
pub fn save_draft_for_profile_with_transition(
    draft: &InitDraft,
    profile_name: &str,
    parent_transition: Option<&str>,
) -> Result<InitResult> {
    let paths = config_paths();
    std::fs::create_dir_all(&paths.directory)?;
    let previous = crate::config::load_config().unwrap_or_default();
    let mut cfg = draft.to_config(&previous.azure_pat, &previous.api_key);
    // O wizard antigo edita somente defaults globais; nunca pode apagar
    // perfis/bindings já configurados. Configurações sem a seção nova recebem
    // a migração legada no mesmo salvamento.
    cfg.profiles = previous.profiles;
    cfg.bindings = previous.bindings;
    cfg.default_profile = previous.default_profile;
    cfg.migrate_legacy_profiles();
    if let Some(message) = validate_process_profile_name(profile_name) {
        return Err(AppError::Config { message });
    }
    let mut profile = ProcessProfile::named(profile_name).ok_or_else(|| AppError::Config {
        message: "schema de perfil inválido; use Agrotrace ou CheckMilk".to_owned(),
    })?;
    profile.area_path.clone_from(&cfg.test_area_path);
    profile.assigned_to.clone_from(&cfg.test_assigned_to);
    profile.team.clone_from(&cfg.test_team);
    profile.program.clone_from(&cfg.test_program);
    profile.reviewer_dev.clone_from(&cfg.reviewer_dev);
    profile.reviewer_sprint.clone_from(&cfg.reviewer_sprint);
    profile.parent_transition = parent_transition
        .map(str::trim)
        .filter(|transition| !transition.is_empty())
        .map(str::to_owned);
    if let Some(existing) = cfg
        .profiles
        .iter_mut()
        .find(|existing| existing.name == profile.name)
    {
        *existing = profile;
    } else {
        cfg.profiles.push(profile);
    }
    profile_name.clone_into(&mut cfg.default_profile);
    if let Some(remote) = crate::git::collect(None)
        .ok()
        .and_then(|context| context.remote)
    {
        if let Some(binding) = cfg.bindings.iter_mut().find(|binding| {
            binding.organization == remote.organization
                && binding.project == remote.project
                && binding.repository == remote.repository
        }) {
            profile_name.clone_into(&mut binding.profile);
        } else {
            cfg.bindings.push(RepositoryProfileBinding {
                profile: profile_name.to_owned(),
                organization: remote.organization,
                project: remote.project,
                repository: remote.repository,
            });
        }
    }
    crate::features::process_profiles::validate_config(&cfg)?;

    // config.json sem o PAT (fica só no .env), como no Dart.
    let mut json = serde_json::to_value(&cfg).map_err(|e| AppError::Config {
        message: format!("falha ao serializar config: {e}"),
    })?;
    if let Some(obj) = json.as_object_mut() {
        obj.remove("azurePat");
        obj.remove("template");
    }
    let pretty = serde_json::to_string_pretty(&json).map_err(|e| AppError::Config {
        message: format!("falha ao serializar config: {e}"),
    })?;
    write_secure(&paths.config_file, format!("{pretty}\n"))?;

    // .env: merge preservando as demais linhas.
    let mut dotenv_vals = Vec::new();
    if !cfg.azure_pat.is_empty() {
        dotenv_vals.push(("AZURE_PAT", cfg.azure_pat.as_str()));
    }
    dotenv_vals.push(("PR_REVIEWER_DEV", cfg.reviewer_dev.as_str()));
    dotenv_vals.push(("PR_REVIEWER_SPRINT", cfg.reviewer_sprint.as_str()));
    dotenv_vals.push(("TEST_CARD_ASSIGNED_TO", cfg.test_assigned_to.as_str()));
    merge_dotenv(&paths.env_file, &dotenv_vals)?;

    if !paths.template_file.exists() {
        write_secure(&paths.template_file, format!("{DEFAULT_TEMPLATE}\n"))?;
    }
    Ok(InitResult {
        saved: true,
        pat_configured: !cfg.azure_pat.is_empty(),
        config_file: paths.config_file.display().to_string(),
        env_file: paths.env_file.display().to_string(),
    })
}

/// Garante arquivos mínimos no modo não-interativo (preserva o existente).
///
/// # Errors
///
/// Retorna [`AppError`] se não conseguir escrever.
pub fn ensure_defaults() -> Result<InitResult> {
    let draft = InitDraft::load_existing();
    if draft.validate_all().is_some() {
        // Arquivo existente inválido: não sobrescreve às cegas.
        return Err(AppError::Config {
            message: "config existente inválida; rode `prt init` interativo".to_owned(),
        });
    }
    save_draft(&draft)
}

/// Escreve arquivo com permissão restrita (0600 no Unix).
fn write_secure(path: &Path, contents: String) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, contents)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

/// Merge `KEY="valor"` no `.env` preservando as demais linhas.
fn merge_dotenv(path: &Path, values: &[(&str, &str)]) -> Result<()> {
    let current = std::fs::read_to_string(path).unwrap_or_default();
    let mut lines: Vec<String> = if current.is_empty() {
        Vec::new()
    } else {
        current.lines().map(str::to_owned).collect()
    };
    for (key, value) in values {
        let escaped = value.replace('\\', r"\\").replace('"', r#"\""#);
        let line = format!(r#"{key}="{escaped}""#);
        let mut replaced = false;
        for existing in &mut lines {
            let t = existing.trim_start();
            if t.starts_with(key) && t[key.len()..].trim_start().starts_with('=') {
                existing.clone_from(&line);
                replaced = true;
                break;
            }
        }
        if !replaced {
            lines.push(line);
        }
    }
    write_secure(path, format!("{}\n", lines.join("\n").trim_end()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn email_should_accept_empty_and_valid() {
        assert_eq!(validate_optional_email(""), None);
        assert_eq!(validate_optional_email("  "), None);
        assert_eq!(validate_optional_email("dev@empresa.com"), None);
    }

    #[test]
    fn email_should_reject_invalid() {
        assert!(validate_optional_email("sem-arroba").is_some());
        assert!(validate_optional_email("a@b").is_some());
        assert!(validate_optional_email("a @b.com").is_some());
    }

    #[test]
    fn draft_should_default_team_and_program() {
        let d = InitDraft {
            pat_input: String::new(),
            has_existing_pat: false,
            reviewer_sprint: String::new(),
            reviewer_dev: String::new(),
            test_assigned_to: String::new(),
            provider: "codex".to_owned(),
            codex_model: String::new(),
            codex_path: String::new(),
            codex_reasoning: String::new(),
            opencode_model: String::new(),
            opencode_path: String::new(),
            opencode_reasoning: String::new(),
            base_url: String::new(),
            compatible_model: String::new(),
            compatible_reasoning: String::new(),
            api_key_input: String::new(),
            has_existing_api_key: false,
            test_area_path: String::new(),
            test_team: String::new(),
            test_program: String::new(),
        };
        let cfg = d.to_config("", "");
        assert_eq!(cfg.test_team, "DevOps");
        assert_eq!(cfg.test_program, "Agrotrace");
        assert_eq!(cfg.codex_model, crate::config::CODEX_MODEL);
    }

    #[test]
    fn unsupported_profile_schema_should_fail_before_save() {
        let error = validate_process_profile_name("OutroProcesso")
            .expect("schema fora do conjunto deveria falhar");
        assert!(error.contains("Agrotrace"));
        assert!(error.contains("CheckMilk"));
    }

    #[test]
    fn draft_should_preserve_provider_executable_paths() {
        let mut d = InitDraft {
            pat_input: String::new(),
            has_existing_pat: false,
            reviewer_sprint: String::new(),
            reviewer_dev: String::new(),
            test_assigned_to: String::new(),
            provider: "codex".to_owned(),
            codex_model: String::new(),
            codex_path: String::new(),
            codex_reasoning: String::new(),
            opencode_model: String::new(),
            opencode_path: String::new(),
            opencode_reasoning: String::new(),
            base_url: String::new(),
            compatible_model: String::new(),
            compatible_reasoning: String::new(),
            api_key_input: String::new(),
            has_existing_api_key: false,
            test_area_path: String::new(),
            test_team: String::new(),
            test_program: String::new(),
        };
        d.codex_path = " C:/Tools/codex.cmd ".to_owned();
        d.opencode_path = "C:/Tools/opencode.exe".to_owned();

        let cfg = d.to_config("", "");

        assert_eq!(cfg.codex_path, "C:/Tools/codex.cmd");
        assert_eq!(cfg.opencode_path, "C:/Tools/opencode.exe");
    }

    #[test]
    fn dotenv_merge_should_preserve_other_keys() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".env");
        std::fs::write(&path, "OUTRA=1\nAZURE_PAT=\"antigo\"\n").unwrap();
        merge_dotenv(&path, &[("AZURE_PAT", "novo")]).unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("OUTRA=1"));
        assert!(content.contains("AZURE_PAT=\"novo\""));
    }
}
