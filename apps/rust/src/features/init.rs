//! `prt init` — wizard de configuração.
//!
//! Espelha `config_service_live.dart#initialize`: pré-preenche a partir do
//! `config.json` + `.env` + env existentes, valida o provider, e salva
//! `config.json` (0600) + merge `.env` + `pr-template.md` (se ausente).
//! A TUI vive em [`crate::tui::init_wizard`]; aqui fica só lógica testável.

use std::path::Path;

use crate::config::{
    COMPATIBLE_REASONING, Config, DEFAULT_BASE_URL, DEFAULT_COMPATIBLE_MODEL, DEFAULT_TEMPLATE,
    OPENCODE_MODEL, OPENCODE_REASONING, config_paths,
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

/// Rascunho editável do wizard (pré-preenchido com o existente).
#[derive(Debug, Clone)]
pub struct InitDraft {
    /// Novo PAT digitado (vazio = manter o atual).
    pub pat_input: String,
    /// PAT atual existe?
    pub has_existing_pat: bool,
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
}

impl InitDraft {
    /// Carrega rascunho a partir dos arquivos + env existentes.
    #[must_use]
    pub fn load_existing() -> Self {
        let cfg = crate::config::load_config().unwrap_or_default();
        Self {
            pat_input: String::new(),
            has_existing_pat: !cfg.azure_pat.is_empty(),
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
            // Process values are intentionally absent from the global draft.
            reviewer_dev: String::new(),
            reviewer_sprint: String::new(),
            test_area_path: String::new(),
            test_assigned_to: String::new(),
            test_team: String::new(),
            test_program: String::new(),
            template: current_template(),
            profiles: Vec::new(),
            bindings: Vec::new(),
            default_profile: String::new(),
        }
    }

    /// Valida todos os campos; retorna a primeira mensagem de erro.
    #[must_use]
    pub fn validate_all(&self) -> Option<String> {
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

fn preserve_process_profiles(config: &mut Config, previous: &Config) {
    config.profiles.clone_from(&previous.profiles);
    config.bindings.clone_from(&previous.bindings);
    config.default_profile.clone_from(&previous.default_profile);
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
    /// Arquivo `.env` usado para segredos globais.
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
    let paths = config_paths();
    std::fs::create_dir_all(&paths.directory)?;
    let previous = crate::config::load_config().unwrap_or_default();
    let mut cfg = draft.to_config(&previous.azure_pat, &previous.api_key);
    // Init edita somente configuração global. As coleções de processo são
    // transferidas sem reconstrução, incluindo perfis genéricos e bindings.
    preserve_process_profiles(&mut cfg, &previous);
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

    // .env: merge preservando chaves desconhecidas e removendo os overrides
    // de processo que deixaram de ser fontes canônicas.
    let mut dotenv_vals = Vec::new();
    if !cfg.azure_pat.is_empty() {
        dotenv_vals.push(("AZURE_PAT", cfg.azure_pat.as_str()));
    }
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
    lines.retain(|line| !is_legacy_process_env_line(line));
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

fn is_legacy_process_env_line(line: &str) -> bool {
    let Some((key, _)) = line.trim_start().split_once('=') else {
        return false;
    };
    matches!(
        key.trim(),
        "PR_REVIEWER_DEV"
            | "PR_REVIEWER_SPRINT"
            | "TEST_CARD_ASSIGNED_TO"
            | "TEST_CARD_AREA_PATH"
            | "TEST_CARD_TEAM"
            | "TEST_CARD_PROGRAM"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ProcessProfile, RepositoryProfileBinding};

    fn draft() -> InitDraft {
        InitDraft {
            pat_input: String::new(),
            has_existing_pat: false,
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
        }
    }

    #[test]
    fn global_init_draft_should_default_global_values() {
        let d = draft();
        let cfg = d.to_config("", "");
        assert_eq!(cfg.codex_model, crate::config::CODEX_MODEL);
        assert!(cfg.reviewer_dev.is_empty());
        assert!(cfg.test_program.is_empty());
    }

    #[test]
    fn global_init_should_preserve_profiles_bindings_and_default() {
        let generic = ProcessProfile {
            name: "IBS Novo".to_owned(),
            program_field: "Custom.ProgramasNovo".to_owned(),
            area_path: "Projeto\\QA".to_owned(),
            assigned_to: "qa@example.com".to_owned(),
            team: "QA".to_owned(),
            program: "Produto".to_owned(),
            priority: 2.0,
            inherit_iteration_path: true,
            parent_transition: None,
            reviewer_dev: "dev@example.com".to_owned(),
            reviewer_sprint: "sprint@example.com".to_owned(),
        };
        let previous = Config {
            profiles: vec![generic],
            bindings: vec![RepositoryProfileBinding {
                profile: "IBS Novo".to_owned(),
                organization: "ibsbiosistemico".to_owned(),
                project: "Projeto".to_owned(),
                repository: "repo".to_owned(),
            }],
            default_profile: "IBS Novo".to_owned(),
            ..Config::default()
        };
        let draft = draft();
        let mut next = draft.to_config("", "");
        preserve_process_profiles(&mut next, &previous);

        assert_eq!(next.profiles, previous.profiles);
        assert_eq!(next.bindings, previous.bindings);
        assert_eq!(next.default_profile, "IBS Novo");
        assert_eq!(next.profiles[0].program_field, "Custom.ProgramasNovo");
    }

    #[test]
    fn global_init_without_remote_should_not_create_process_association() {
        let cfg = draft().to_config("", "");
        assert!(cfg.profiles.is_empty());
        assert!(cfg.bindings.is_empty());
        assert!(cfg.default_profile.is_empty());
    }

    #[test]
    fn draft_should_preserve_provider_executable_paths() {
        let mut d = draft();
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
        std::fs::write(
            &path,
            "OUTRA=1\nAZURE_PAT=\"antigo\"\nPR_REVIEWER_DEV=legacy@example.com\n",
        )
        .unwrap();
        merge_dotenv(&path, &[("AZURE_PAT", "novo")]).unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("OUTRA=1"));
        assert!(content.contains("AZURE_PAT=\"novo\""));
        assert!(!content.contains("PR_REVIEWER_DEV"));
    }
}
