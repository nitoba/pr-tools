//! Configuração — espelha `config_models` + `config_defaults` + `config_service` do Dart.
//!
//! Precedência: CLI > env (`PR_AI_*`, `AZURE_PAT`, `PR_REVIEWER_*`, `TEST_CARD_*`)
//! > dotenv (`.env`) > `config.json` > defaults.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Nome do provider (`codex`, `opencode`, `openai-compatible`).
pub type ProviderName = String;
/// Nível de thinking (`provider-default`, `minimal`, `low`, ...).
pub type ReasoningLevel = String;

/// Modelo padrão do Codex (espelha o Dart).
pub const CODEX_MODEL: &str = "gpt-5.6-luna";
/// Thinking padrão do Codex.
pub const CODEX_REASONING: &str = "high";
/// Modelo padrão do `OpenCode` (`provider/modelo`).
pub const OPENCODE_MODEL: &str = "openai/gpt-5.5";
/// Thinking padrão do `OpenCode`.
pub const OPENCODE_REASONING: &str = "provider-default";
/// Base URL padrão do endpoint OpenAI-compatible.
pub const DEFAULT_BASE_URL: &str = "https://api.openai.com/v1";
/// Modelo padrão do endpoint OpenAI-compatible.
pub const DEFAULT_COMPATIBLE_MODEL: &str = "gpt-4o-mini";
/// Thinking padrão do endpoint OpenAI-compatible.
pub const COMPATIBLE_REASONING: &str = "provider-default";

/// Template padrão (PT-BR) — espelha `defaultTemplate` do Dart.
pub const DEFAULT_TEMPLATE: &str = r#"Analise o diff e o log do git fornecidos e gere uma descrição de pull request em português brasileiro.

Retorne um objeto JSON com exatamente estes campos:
- "title": título curto, técnico e descritivo, com no máximo 80 caracteres.
- "body": descrição em Markdown.

O body deve seguir este formato:

## Descrição

Resumo conciso em 1 ou 2 frases do que mudou e por quê.

## Alterações

Liste componentes ou arquivos relevantes e descreva a mudança funcional.

## Tipo de mudança

- [ ] Bug fix
- [ ] Nova feature
- [ ] Breaking change
- [ ] Refactoring

Não invente alterações que não estejam no diff.

Responda somente com o objeto JSON. Não inclua o prompt, o contexto Git, o log, o diff ou qualquer texto adicional fora desse objeto.
"#;

/// Configuração resolvida.
///
/// Serializada em `camelCase` para compat com o `config.json` da versão Dart.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    /// Providers em ordem de tentativa.
    #[serde(default = "default_providers")]
    pub providers: Vec<ProviderName>,
    /// Base URL do endpoint OpenAI-compatible.
    #[serde(default = "default_base_url")]
    pub base_url: String,
    /// Modelo do endpoint compatible.
    #[serde(default = "default_compatible_model")]
    pub compatible_model: String,
    /// Reasoning do endpoint compatible.
    #[serde(default = "default_reasoning")]
    pub compatible_reasoning: ReasoningLevel,
    /// Modelo do Codex.
    #[serde(default = "default_codex_model")]
    pub codex_model: String,
    /// Thinking do Codex.
    #[serde(default = "default_codex_reasoning")]
    pub codex_reasoning: ReasoningLevel,
    /// Modelo do `OpenCode` (`provider/modelo`).
    #[serde(default = "default_opencode_model")]
    pub opencode_model: String,
    /// Thinking do `OpenCode`.
    #[serde(default = "default_opencode_reasoning")]
    pub opencode_reasoning: ReasoningLevel,
    /// PAT do Azure DevOps (nunca logar — `obs-no-sensitive-data`).
    #[serde(default)]
    pub azure_pat: String,
    /// Email de review para `dev`.
    #[serde(default)]
    pub reviewer_dev: String,
    /// Email de review para `sprint`.
    #[serde(default)]
    pub reviewer_sprint: String,
    /// `AreaPath` padrão de Test Cases.
    #[serde(default)]
    pub test_area_path: String,
    /// Responsável padrão.
    #[serde(default)]
    pub test_assigned_to: String,
    /// `Custom.Team`.
    #[serde(default)]
    pub test_team: String,
    /// `Custom.ProgramasAgrotrace`.
    #[serde(default)]
    pub test_program: String,
    /// API key do endpoint compatible.
    #[serde(default)]
    pub api_key: String,
    /// Template do prompt de sistema.
    #[serde(default = "default_template")]
    pub template: String,
}

fn default_providers() -> Vec<String> {
    vec![
        "codex".to_owned(),
        "opencode".to_owned(),
        "openai-compatible".to_owned(),
    ]
}
fn default_base_url() -> String {
    DEFAULT_BASE_URL.to_owned()
}
fn default_compatible_model() -> String {
    DEFAULT_COMPATIBLE_MODEL.to_owned()
}
fn default_reasoning() -> String {
    "provider-default".to_owned()
}
fn default_codex_model() -> String {
    CODEX_MODEL.to_owned()
}
fn default_codex_reasoning() -> String {
    CODEX_REASONING.to_owned()
}
fn default_opencode_model() -> String {
    OPENCODE_MODEL.to_owned()
}
fn default_opencode_reasoning() -> String {
    OPENCODE_REASONING.to_owned()
}
fn default_template() -> String {
    DEFAULT_TEMPLATE.to_owned()
}

impl Default for Config {
    fn default() -> Self {
        Self {
            providers: default_providers(),
            base_url: default_base_url(),
            compatible_model: default_compatible_model(),
            compatible_reasoning: default_reasoning(),
            codex_model: default_codex_model(),
            codex_reasoning: default_codex_reasoning(),
            opencode_model: default_opencode_model(),
            opencode_reasoning: default_opencode_reasoning(),
            azure_pat: String::new(),
            reviewer_dev: String::new(),
            reviewer_sprint: String::new(),
            test_area_path: String::new(),
            test_assigned_to: String::new(),
            test_team: String::new(),
            test_program: String::new(),
            api_key: String::new(),
            template: default_template(),
        }
    }
}

/// Caminhos de configuração (`~/.config/pr-tools/...` ou `$XDG_CONFIG_HOME`).
#[derive(Debug, Clone)]
pub struct ConfigPaths {
    /// Diretório base.
    pub directory: PathBuf,
    /// `config.json`.
    pub config_file: PathBuf,
    /// `.env`.
    pub env_file: PathBuf,
    /// `pr-template.md`.
    pub template_file: PathBuf,
}

/// Resolve os caminhos de configuração.
#[must_use]
pub fn config_paths() -> ConfigPaths {
    let base = std::env::var("XDG_CONFIG_HOME").map_or_else(
        |_| {
            directories::BaseDirs::new()
                .map_or_else(|| PathBuf::from("."), |d| d.home_dir().join(".config"))
        },
        PathBuf::from,
    );
    let dir = base.join("pr-tools");
    ConfigPaths {
        config_file: dir.join("config.json"),
        env_file: dir.join(".env"),
        template_file: dir.join("pr-template.md"),
        directory: dir,
    }
}

/// Aplica overrides da CLI sobre a config (precedência máxima).
pub fn apply_cli_overrides(
    config: &mut Config,
    provider: Option<&str>,
    model: Option<&str>,
    base_url: Option<&str>,
    api_key: Option<&str>,
) {
    if let Some(p) = provider {
        config.providers = vec![p.to_owned()];
    }
    if let Some(m) = model {
        // O modelo override aplica-se ao provider ativo (primeiro da lista).
        if let Some(first) = config.providers.first() {
            match first.as_str() {
                "codex" => m.clone_into(&mut config.codex_model),
                "opencode" => m.clone_into(&mut config.opencode_model),
                _ => m.clone_into(&mut config.compatible_model),
            }
        }
    }
    if let Some(u) = base_url {
        u.clone_into(&mut config.base_url);
    }
    if let Some(k) = api_key {
        k.clone_into(&mut config.api_key);
    }
}

/// Carrega `config.json` + `.env` + env atual (sem bloquear async).
///
/// # Errors
///
/// Retorna [`crate::error::AppError::Config`] se o JSON for inválido.
pub fn load_config() -> crate::error::Result<Config> {
    let paths = config_paths();
    let mut config = Config::default();

    if let Ok(raw) = std::fs::read_to_string(&paths.config_file) {
        let file_cfg: Config =
            serde_json::from_str(&raw).map_err(|e| crate::error::AppError::Config {
                message: format!("{}: {e}", paths.config_file.display()),
            })?;
        config = file_cfg;
    }
    // `.env` opcional (merge simples `KEY=VAL`).
    if let Ok(raw) = std::fs::read_to_string(&paths.env_file) {
        for line in raw.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some((k, v)) = line.split_once('=') {
                let value = parse_dotenv_value(v);
                apply_env_kv(&mut config, k.trim(), &value);
            }
        }
    }
    // Env atual tem precedência sobre arquivos.
    for (k, v) in std::env::vars() {
        apply_env_kv(&mut config, &k, &v);
    }
    // Template em arquivo tem precedência sobre o JSON.
    if let Ok(t) = std::fs::read_to_string(&paths.template_file) {
        if !t.trim().is_empty() {
            config.template = t;
        }
    }
    Ok(config)
}

/// Remove as aspas externas usadas pelo `.env` salvo pelo `prt init`.
///
/// O Dart aceita `AZURE_PAT="token"` como `token`; sem esta normalização o
/// Rust enviava literalmente `"token"` no Basic Auth e o Azure respondia com
/// a página HTML de login (HTTP 203).
fn parse_dotenv_value(raw: &str) -> String {
    let value = raw.trim();
    let quoted = value.len() >= 2
        && matches!(
            (value.as_bytes().first(), value.as_bytes().last()),
            (Some(b'"'), Some(b'"')) | (Some(b'\''), Some(b'\''))
        );
    if quoted {
        value[1..value.len() - 1].to_owned()
    } else {
        value.to_owned()
    }
}

fn apply_env_kv(config: &mut Config, key: &str, value: &str) {
    match key {
        "AZURE_PAT" | "AZURE_DEVOPS_PAT" => value.clone_into(&mut config.azure_pat),
        "PR_REVIEWER_DEV" => value.clone_into(&mut config.reviewer_dev),
        "PR_REVIEWER_SPRINT" => value.clone_into(&mut config.reviewer_sprint),
        "TEST_CARD_ASSIGNED_TO" => value.clone_into(&mut config.test_assigned_to),
        "TEST_CARD_AREA_PATH" => value.clone_into(&mut config.test_area_path),
        "TEST_CARD_TEAM" => value.clone_into(&mut config.test_team),
        "TEST_CARD_PROGRAM" => value.clone_into(&mut config.test_program),
        "PR_AI_BASE_URL" => value.clone_into(&mut config.base_url),
        "PR_AI_API_KEY" => value.clone_into(&mut config.api_key),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_should_match_dart_values() {
        let cfg = Config::default();
        assert_eq!(cfg.codex_model, "gpt-5.6-luna");
        assert_eq!(cfg.opencode_model, "openai/gpt-5.5");
        assert_eq!(cfg.compatible_model, "gpt-4o-mini");
        assert!(cfg.template.contains("## Descrição"));
    }

    #[test]
    fn cli_overrides_should_win_over_defaults() {
        let mut cfg = Config::default();
        apply_cli_overrides(&mut cfg, Some("codex"), Some("gpt-x"), None, None);
        assert_eq!(cfg.providers, vec!["codex"]);
        assert_eq!(cfg.codex_model, "gpt-x");
    }

    #[test]
    fn dotenv_value_should_remove_outer_quotes() {
        assert_eq!(parse_dotenv_value("  \"token\"  "), "token");
        assert_eq!(parse_dotenv_value("'token'"), "token");
        assert_eq!(parse_dotenv_value("token"), "token");
    }
}
