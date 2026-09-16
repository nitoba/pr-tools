//! Configuração — espelha `config_models` + `config_defaults` + `config_service` do Dart.
//!
//! Precedência: CLI > env (`PR_AI_*`, `AZURE_PAT`, `PR_REVIEWER_*`, `TEST_CARD_*`)
//! > dotenv (`.env`) > `config.json` > defaults.

use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tempfile::NamedTempFile;

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

/// Nome do perfil de processo legado.
pub const AGROTRACE_PROFILE: &str = "Agrotrace";
/// Nome do perfil de processo `CheckMilk`.
pub const CHECKMILK_PROFILE: &str = "CheckMilk";
/// Campo fixo de programa do perfil Agrotrace.
pub const AGROTRACE_PROGRAM_FIELD: &str = "Custom.ProgramasAgrotrace";
/// Campo fixo de programa do perfil `CheckMilk`.
pub const CHECKMILK_PROGRAM_FIELD: &str = "Custom.ProgramasCheckmilk";
/// Estado padrão usado pelo perfil legado.
pub const DEFAULT_PARENT_TRANSITION: &str = "Test QA";

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

/// Perfil local de um processo Azure DevOps suportado pelo `prt`.
///
/// O nome também identifica o schema fechado da V1: somente `Agrotrace` e
/// `CheckMilk` são aceitos. Credenciais deliberadamente não fazem parte deste
/// tipo; elas continuam no `.env`/ambiente global existente.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessProfile {
    /// Identificador estável do perfil (`Agrotrace` ou `CheckMilk`).
    pub name: String,
    /// `System.AreaPath` padrão.
    #[serde(default)]
    pub area_path: String,
    /// `System.AssignedTo` padrão.
    #[serde(default)]
    pub assigned_to: String,
    /// `Custom.Team` padrão.
    #[serde(default)]
    pub team: String,
    /// Valor do campo de programa fixo do schema.
    #[serde(default)]
    pub program: String,
    /// `Microsoft.VSTS.Common.Priority` padrão.
    #[serde(default = "default_profile_priority")]
    pub priority: f64,
    /// Se a `IterationPath` do Work Item pai deve ser herdada.
    #[serde(default = "default_inherit_iteration_path")]
    pub inherit_iteration_path: bool,
    /// Estado opcional aplicado ao Work Item pai após a criação.
    #[serde(default)]
    pub parent_transition: Option<String>,
    /// Reviewer padrão para targets `dev`.
    #[serde(default)]
    pub reviewer_dev: String,
    /// Reviewer padrão para targets `sprint`.
    #[serde(default)]
    pub reviewer_sprint: String,
}

fn default_profile_priority() -> f64 {
    2.0
}

fn default_inherit_iteration_path() -> bool {
    true
}

impl ProcessProfile {
    /// Cria o perfil Agrotrace a partir das chaves legadas não secretas.
    #[must_use]
    pub fn from_legacy(config: &Config) -> Self {
        Self {
            name: AGROTRACE_PROFILE.to_owned(),
            area_path: config.test_area_path.clone(),
            assigned_to: config.test_assigned_to.clone(),
            team: config.test_team.clone(),
            program: config.test_program.clone(),
            priority: 2.0,
            inherit_iteration_path: true,
            parent_transition: Some(DEFAULT_PARENT_TRANSITION.to_owned()),
            reviewer_dev: config.reviewer_dev.clone(),
            reviewer_sprint: config.reviewer_sprint.clone(),
        }
    }

    /// Cria um perfil com os valores fixos do schema selecionado.
    #[must_use]
    pub fn named(name: &str) -> Option<Self> {
        matches!(name, AGROTRACE_PROFILE | CHECKMILK_PROFILE).then(|| Self {
            name: name.to_owned(),
            area_path: String::new(),
            assigned_to: String::new(),
            team: String::new(),
            program: String::new(),
            priority: 2.0,
            inherit_iteration_path: true,
            parent_transition: Some(DEFAULT_PARENT_TRANSITION.to_owned()),
            reviewer_dev: String::new(),
            reviewer_sprint: String::new(),
        })
    }

    /// Retorna o campo de programa permitido pelo schema.
    #[must_use]
    pub fn program_field(&self) -> Option<&'static str> {
        match self.name.as_str() {
            AGROTRACE_PROFILE => Some(AGROTRACE_PROGRAM_FIELD),
            CHECKMILK_PROFILE => Some(CHECKMILK_PROGRAM_FIELD),
            _ => None,
        }
    }
}

/// Associação de um perfil à identidade Azure exata do remote Git.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepositoryProfileBinding {
    /// Nome do perfil associado.
    pub profile: String,
    /// Organização Azure (`dev.azure.com/{organization}`).
    pub organization: String,
    /// Projeto Azure.
    pub project: String,
    /// Repositório Azure.
    pub repository: String,
}

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
    /// Caminho opcional do executável do Codex (vazio = PATH).
    #[serde(default)]
    pub codex_path: String,
    /// Thinking do Codex.
    #[serde(default = "default_codex_reasoning")]
    pub codex_reasoning: ReasoningLevel,
    /// Modelo do `OpenCode` (`provider/modelo`).
    #[serde(default = "default_opencode_model")]
    pub opencode_model: String,
    /// Caminho opcional do executável do `OpenCode` (vazio = PATH).
    #[serde(default)]
    pub opencode_path: String,
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
    /// Perfis de processo persistidos no `config.json`.
    #[serde(default)]
    pub profiles: Vec<ProcessProfile>,
    /// Bindings por `(organization, project, repository)`.
    #[serde(default)]
    pub bindings: Vec<RepositoryProfileBinding>,
    /// Perfil usado quando não existe binding explícito.
    #[serde(default)]
    pub default_profile: String,
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
            codex_path: String::new(),
            codex_reasoning: default_codex_reasoning(),
            opencode_model: default_opencode_model(),
            opencode_path: String::new(),
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
            profiles: Vec::new(),
            bindings: Vec::new(),
            default_profile: String::new(),
        }
    }
}

impl Config {
    /// Materializa a configuração legada em um perfil Agrotrace.
    ///
    /// A operação é somente em memória. O loader persiste o JSON original
    /// através de [`migrate_legacy_json`] para que a troca seja atômica e não
    /// serialize PAT/API key dentro do perfil.
    pub fn migrate_legacy_profiles(&mut self) -> bool {
        if !self.profiles.is_empty() {
            if self.default_profile.trim().is_empty()
                && self
                    .profiles
                    .iter()
                    .any(|profile| profile.name == AGROTRACE_PROFILE)
            {
                AGROTRACE_PROFILE.clone_into(&mut self.default_profile);
                return true;
            }
            return false;
        }
        let legacy = ProcessProfile::from_legacy(self);
        self.profiles.push(legacy);
        AGROTRACE_PROFILE.clone_into(&mut self.default_profile);
        true
    }

    /// Perfis efetivos, incluindo o fallback legado para configurações
    /// construídas em memória sem passar pelo loader.
    #[must_use]
    pub fn effective_process_profiles(&self) -> Vec<ProcessProfile> {
        if self.profiles.is_empty() {
            vec![ProcessProfile::from_legacy(self)]
        } else {
            self.profiles.clone()
        }
    }
}

/// Caminhos de configuração (`BaseDirs::config_dir()/pr-tools` ou `$XDG_CONFIG_HOME`).
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
    let base = config_base_dir(
        std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from),
        directories::BaseDirs::new().map(|dirs| dirs.config_dir().to_path_buf()),
    );
    let dir = base.join("pr-tools");
    ConfigPaths {
        config_file: dir.join("config.json"),
        env_file: dir.join(".env"),
        template_file: dir.join("pr-template.md"),
        directory: dir,
    }
}

#[cfg(windows)]
fn legacy_config_paths() -> Option<ConfigPaths> {
    let dir = directories::BaseDirs::new()?
        .home_dir()
        .join(".config")
        .join("pr-tools");
    Some(ConfigPaths {
        config_file: dir.join("config.json"),
        env_file: dir.join(".env"),
        template_file: dir.join("pr-template.md"),
        directory: dir,
    })
}

#[cfg(not(windows))]
fn legacy_config_paths() -> Option<ConfigPaths> {
    None
}

fn read_config_file(primary: &Path, legacy: Option<&Path>) -> Option<String> {
    std::fs::read_to_string(primary)
        .ok()
        .or_else(|| legacy.and_then(|path| std::fs::read_to_string(path).ok()))
}

fn read_config_source(primary: &Path, legacy: Option<&Path>) -> Option<(PathBuf, String)> {
    if let Ok(raw) = std::fs::read_to_string(primary) {
        return Some((primary.to_path_buf(), raw));
    }
    legacy.and_then(|path| {
        std::fs::read_to_string(path)
            .ok()
            .map(|raw| (path.to_path_buf(), raw))
    })
}

/// Acrescenta a migração legada ao JSON existente sem reserializar segredos.
fn migrate_legacy_json(raw: &str, config: &Config) -> crate::error::Result<String> {
    let mut value: Value =
        serde_json::from_str(raw).map_err(|error| crate::error::AppError::Config {
            message: format!("falha ao migrar config.json: {error}"),
        })?;
    let object = value
        .as_object_mut()
        .ok_or_else(|| crate::error::AppError::Config {
            message: "config.json deve conter um objeto JSON".to_owned(),
        })?;
    let profile = config
        .profiles
        .iter()
        .find(|profile| profile.name == AGROTRACE_PROFILE)
        .cloned()
        .unwrap_or_else(|| ProcessProfile::from_legacy(config));
    object.insert(
        "profiles".to_owned(),
        serde_json::to_value([profile]).map_err(|error| crate::error::AppError::Config {
            message: format!("falha ao serializar perfil legado: {error}"),
        })?,
    );
    object.insert(
        "defaultProfile".to_owned(),
        Value::String(AGROTRACE_PROFILE.to_owned()),
    );
    serde_json::to_string_pretty(&value)
        .map(|json| format!("{json}\n"))
        .map_err(|error| crate::error::AppError::Config {
            message: format!("falha ao serializar migração: {error}"),
        })
}

/// Persiste a migração em uma substituição atômica do arquivo original.
fn write_atomic(path: &Path, contents: &str) -> crate::error::Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent)?;
    let mut temp = NamedTempFile::new_in(parent)?;
    temp.as_file_mut().write_all(contents.as_bytes())?;
    temp.as_file().sync_all()?;
    temp.persist(path)
        .map_err(|error| crate::error::AppError::Io(error.error))?;
    Ok(())
}

fn config_base_dir(
    xdg_config_home: Option<PathBuf>,
    platform_config_dir: Option<PathBuf>,
) -> PathBuf {
    xdg_config_home
        .or(platform_config_dir)
        .unwrap_or_else(|| PathBuf::from("."))
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
    let legacy = legacy_config_paths();
    let mut config = Config::default();

    let config_source = read_config_source(
        &paths.config_file,
        legacy.as_ref().map(|paths| paths.config_file.as_path()),
    );
    if let Some((_, raw)) = &config_source {
        let file_cfg: Config =
            serde_json::from_str(raw).map_err(|e| crate::error::AppError::Config {
                message: format!("{}: {e}", paths.config_file.display()),
            })?;
        config = file_cfg;
    }
    // `.env` opcional (merge simples `KEY=VAL`).
    if let Some(raw) = read_config_file(
        &paths.env_file,
        legacy.as_ref().map(|paths| paths.env_file.as_path()),
    ) {
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
    if let Some(t) = read_config_file(
        &paths.template_file,
        legacy.as_ref().map(|paths| paths.template_file.as_path()),
    ) {
        if !t.trim().is_empty() {
            config.template = t;
        }
    }
    if let Some((source_path, raw)) = config_source {
        let raw_value: Value =
            serde_json::from_str(&raw).map_err(|error| crate::error::AppError::Config {
                message: format!("falha ao ler config.json para migração: {error}"),
            })?;
        let has_profiles = raw_value
            .get("profiles")
            .and_then(Value::as_array)
            .is_some_and(|profiles| !profiles.is_empty());
        if !has_profiles && config.migrate_legacy_profiles() {
            let migrated = migrate_legacy_json(&raw, &config)?;
            write_atomic(&source_path, &migrated)?;
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

    #[test]
    fn config_base_should_prefer_explicit_xdg_directory() {
        let base = config_base_dir(
            Some(PathBuf::from("/custom/config")),
            Some(PathBuf::from("/platform/config")),
        );
        assert_eq!(base, PathBuf::from("/custom/config"));
    }

    #[test]
    fn config_base_should_use_platform_directory_without_xdg_override() {
        let base = config_base_dir(None, Some(PathBuf::from("C:/Users/test/AppData/Roaming")));
        assert_eq!(base, PathBuf::from("C:/Users/test/AppData/Roaming"));
    }

    #[test]
    fn config_paths_should_use_platform_config_directory() {
        let expected = directories::BaseDirs::new()
            .expect("diretórios base disponíveis")
            .config_dir()
            .join("pr-tools");
        assert_eq!(config_paths().directory, expected);
    }

    #[test]
    fn legacy_config_should_materialize_agrotrace_and_default() {
        let mut config = Config {
            test_area_path: "AGROTRACE\\QA".to_owned(),
            test_assigned_to: "qa@example.com".to_owned(),
            test_team: "DevOps".to_owned(),
            test_program: "Agrotrace".to_owned(),
            reviewer_dev: "dev@example.com".to_owned(),
            reviewer_sprint: "sprint@example.com".to_owned(),
            ..Config::default()
        };
        assert!(config.migrate_legacy_profiles());
        assert_eq!(config.default_profile, AGROTRACE_PROFILE);
        assert_eq!(config.profiles.len(), 1);
        let profile = &config.profiles[0];
        assert_eq!(profile.name, AGROTRACE_PROFILE);
        assert_eq!(profile.priority, 2.0);
        assert!(profile.inherit_iteration_path);
        assert_eq!(
            profile.parent_transition.as_deref(),
            Some(DEFAULT_PARENT_TRANSITION)
        );
        assert_eq!(profile.reviewer_dev, "dev@example.com");
        assert_eq!(profile.reviewer_sprint, "sprint@example.com");
    }

    #[test]
    fn legacy_migration_should_be_atomic_idempotent_and_secret_free() {
        let dir = tempfile::tempdir().expect("diretório temporário");
        let path = dir.path().join("config.json");
        let raw = serde_json::json!({
            "testAreaPath": "AGROTRACE\\QA",
            "testAssignedTo": "qa@example.com",
            "testTeam": "DevOps",
            "testProgram": "Agrotrace",
            "azurePat": "pat-secret",
            "apiKey": "api-secret"
        });
        std::fs::write(&path, serde_json::to_string_pretty(&raw).unwrap()).unwrap();
        let mut config: Config = serde_json::from_value(raw).unwrap();
        config.azure_pat = "pat-secret".to_owned();
        config.api_key = "api-secret".to_owned();
        assert!(config.migrate_legacy_profiles());
        let migrated = migrate_legacy_json(&std::fs::read_to_string(&path).unwrap(), &config)
            .expect("migração");
        write_atomic(&path, &migrated).expect("substituição atômica");
        let first: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(first["profiles"].as_array().unwrap().len(), 1);
        assert_eq!(first["defaultProfile"], AGROTRACE_PROFILE);
        assert!(!first["profiles"].to_string().contains("pat-secret"));
        assert!(!first["profiles"].to_string().contains("api-secret"));

        let mut loaded: Config = serde_json::from_value(first).unwrap();
        assert!(!loaded.migrate_legacy_profiles());
        let second = migrate_legacy_json(&std::fs::read_to_string(&path).unwrap(), &loaded)
            .expect("migração repetida");
        write_atomic(&path, &second).expect("substituição repetida");
        let repeated: Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(repeated["profiles"].as_array().unwrap().len(), 1);
    }
}
