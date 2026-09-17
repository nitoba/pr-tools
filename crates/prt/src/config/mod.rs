//! Configuração — espelha `config_models` + `config_defaults` + `config_service` do Dart.
//!
//! Precedência: CLI > env (`PR_AI_*`, `AZURE_PAT`) > dotenv (`.env`) >
//! `config.json` > defaults. Processo, reviewers e defaults de Test Case vivem
//! exclusivamente em [`ProcessProfile`]; as chaves antigas da raiz são lidas
//! apenas pela migração de JSON legado.

use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Deserializer, Serialize, Serializer};
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

const LEGACY_ROOT_KEYS: [&str; 6] = [
    "reviewerDev",
    "reviewerSprint",
    "testAreaPath",
    "testAssignedTo",
    "testProgram",
    "testTeam",
];

const LEGACY_PROVIDER_ROOT_KEYS: [&str; 9] = [
    "baseUrl",
    "compatibleModel",
    "compatibleReasoning",
    "codexModel",
    "codexPath",
    "codexReasoning",
    "opencodeModel",
    "opencodePath",
    "opencodeReasoning",
];

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

/// Perfil local de um processo Azure DevOps usado pelo `prt`.
///
/// O formato persistido agrupa reviewers em `reviewers` e defaults do Test
/// Case em `testCard`. A implementação mantém os campos planos internamente
/// para não espalhar detalhes de serialização pelo domínio; o desserializador
/// também aceita os formatos anteriores.
#[derive(Debug, Clone, PartialEq)]
pub struct ProcessProfile {
    /// Identificador estável do perfil.
    pub name: String,
    /// Field Azure que recebe o programa (`testCard.programField`, `Custom.*`).
    pub program_field: String,
    /// `System.AreaPath` padrão.
    pub area_path: String,
    /// `testCard.assignedTo`: `System.AssignedTo` padrão.
    pub assigned_to: String,
    /// `testCard.team`: `Custom.Team` padrão.
    pub team: String,
    /// Valor do campo de programa fixo do schema.
    pub program: String,
    /// `Microsoft.VSTS.Common.Priority` padrão.
    pub priority: f64,
    /// Se a `IterationPath` do Work Item pai deve ser herdada.
    pub inherit_iteration_path: bool,
    /// Estado opcional aplicado ao Work Item pai após a criação.
    pub parent_transition: Option<String>,
    /// Reviewer padrão para targets `dev`.
    pub reviewer_dev: String,
    /// Reviewer padrão para targets `sprint`.
    pub reviewer_sprint: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProfileReviewersInput {
    #[serde(default)]
    development: Option<String>,
    #[serde(default)]
    sprint: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProfileTestCardInput {
    #[serde(default)]
    assigned_to: Option<String>,
    #[serde(default)]
    program_field: Option<String>,
    #[serde(default)]
    team: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProcessProfileInput {
    #[serde(default)]
    name: String,
    #[serde(default)]
    program: String,
    #[serde(default)]
    area_path: String,
    #[serde(default = "default_profile_priority")]
    priority: f64,
    #[serde(default = "default_inherit_iteration_path")]
    inherit_iteration_path: bool,
    #[serde(default)]
    parent_transition: Option<String>,
    #[serde(default)]
    reviewers: Option<ProfileReviewersInput>,
    #[serde(default)]
    test_card: Option<ProfileTestCardInput>,
    // Flat names from the two previous profile schemas.
    #[serde(default)]
    reviewer_dev: Option<String>,
    #[serde(default)]
    reviewer_sprint: Option<String>,
    #[serde(default, rename = "testCardProgramField")]
    test_card_program_field: Option<String>,
    #[serde(default, rename = "programField")]
    program_field: Option<String>,
    #[serde(default, rename = "testCardAssignedTo")]
    test_card_assigned_to: Option<String>,
    #[serde(default, rename = "assignedTo")]
    assigned_to: Option<String>,
    #[serde(default, rename = "testCardTeam")]
    test_card_team: Option<String>,
    #[serde(default)]
    team: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProfileReviewersOutput<'a> {
    development: &'a str,
    sprint: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProfileTestCardOutput<'a> {
    assigned_to: &'a str,
    program_field: &'a str,
    team: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProcessProfileOutput<'a> {
    name: &'a str,
    program: &'a str,
    area_path: &'a str,
    inherit_iteration_path: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    parent_transition: Option<&'a str>,
    priority: f64,
    reviewers: ProfileReviewersOutput<'a>,
    test_card: ProfileTestCardOutput<'a>,
}

impl Serialize for ProcessProfile {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        ProcessProfileOutput {
            name: &self.name,
            program: &self.program,
            area_path: &self.area_path,
            inherit_iteration_path: self.inherit_iteration_path,
            parent_transition: self.parent_transition.as_deref(),
            priority: self.priority,
            reviewers: ProfileReviewersOutput {
                development: &self.reviewer_dev,
                sprint: &self.reviewer_sprint,
            },
            test_card: ProfileTestCardOutput {
                assigned_to: &self.assigned_to,
                program_field: self.program_field().unwrap_or_default(),
                team: &self.team,
            },
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ProcessProfile {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let input = ProcessProfileInput::deserialize(deserializer)?;
        let reviewers = input.reviewers.unwrap_or_default();
        let test_card = input.test_card.unwrap_or_default();
        Ok(Self {
            name: input.name,
            program_field: test_card
                .program_field
                .or(input.test_card_program_field)
                .or(input.program_field)
                .unwrap_or_default(),
            area_path: input.area_path,
            assigned_to: test_card
                .assigned_to
                .or(input.test_card_assigned_to)
                .or(input.assigned_to)
                .unwrap_or_default(),
            team: test_card
                .team
                .or(input.test_card_team)
                .or(input.team)
                .unwrap_or_default(),
            program: input.program,
            priority: input.priority,
            inherit_iteration_path: input.inherit_iteration_path,
            parent_transition: input.parent_transition,
            reviewer_dev: reviewers
                .development
                .or(input.reviewer_dev)
                .unwrap_or_default(),
            reviewer_sprint: reviewers
                .sprint
                .or(input.reviewer_sprint)
                .unwrap_or_default(),
        })
    }
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
            program_field: AGROTRACE_PROGRAM_FIELD.to_owned(),
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

    /// Cria um perfil com os valores fixos do schema legado selecionado.
    #[must_use]
    pub fn named(name: &str) -> Option<Self> {
        matches!(name, AGROTRACE_PROFILE | CHECKMILK_PROFILE).then(|| Self {
            name: name.to_owned(),
            program_field: match name {
                AGROTRACE_PROFILE => AGROTRACE_PROGRAM_FIELD,
                CHECKMILK_PROFILE => CHECKMILK_PROGRAM_FIELD,
                _ => unreachable!("nome filtrado acima"),
            }
            .to_owned(),
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

    /// Retorna o field de programa efetivo, incluindo a compatibilidade legada.
    #[must_use]
    pub fn program_field(&self) -> Option<&str> {
        if self.program_field.trim().is_empty() {
            match self.name.as_str() {
                AGROTRACE_PROFILE => Some(AGROTRACE_PROGRAM_FIELD),
                CHECKMILK_PROFILE => Some(CHECKMILK_PROGRAM_FIELD),
                _ => None,
            }
        } else {
            Some(self.program_field.as_str())
        }
    }

    /// Retorna a transição aplicável, tratando texto vazio como ausência.
    #[must_use]
    pub fn parent_transition(&self) -> Option<&str> {
        self.parent_transition
            .as_deref()
            .filter(|transition| !transition.trim().is_empty())
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
/// O formato persistido agrupa cada provider em um objeto com `id`, `type`,
/// `model` e `reasoning`. Os campos planos abaixo são uma representação
/// interna compatível com o restante do runtime e não fazem parte do JSON
/// canônico.
#[derive(Debug, Clone)]
pub struct Config {
    /// Providers em ordem de tentativa.
    pub providers: Vec<ProviderName>,
    /// Base URL do endpoint OpenAI-compatible.
    pub base_url: String,
    /// Modelo do endpoint compatible.
    pub compatible_model: String,
    /// Reasoning do endpoint compatible.
    pub compatible_reasoning: ReasoningLevel,
    /// Modelo do Codex.
    pub codex_model: String,
    /// Caminho opcional do executável do Codex (vazio = PATH).
    pub codex_path: String,
    /// Thinking do Codex.
    pub codex_reasoning: ReasoningLevel,
    /// Modelo do `OpenCode` (`provider/modelo`).
    pub opencode_model: String,
    /// Caminho opcional do executável do `OpenCode` (vazio = PATH).
    pub opencode_path: String,
    /// Thinking do `OpenCode`.
    pub opencode_reasoning: ReasoningLevel,
    /// PAT do Azure DevOps (nunca logar — `obs-no-sensitive-data`).
    pub azure_pat: String,
    /// Campo transitório usado somente por testes/compatibilidade de migração.
    pub reviewer_dev: String,
    /// Campo transitório usado somente por testes/compatibilidade de migração.
    pub reviewer_sprint: String,
    /// Campo transitório usado somente por testes/compatibilidade de migração.
    pub test_area_path: String,
    /// Campo transitório usado somente por testes/compatibilidade de migração.
    pub test_assigned_to: String,
    /// Campo transitório usado somente por testes/compatibilidade de migração.
    pub test_team: String,
    /// Campo transitório usado somente por testes/compatibilidade de migração.
    pub test_program: String,
    /// API key do endpoint compatible.
    pub api_key: String,
    /// Template do prompt de sistema.
    pub template: String,
    /// Perfis de processo persistidos no `config.json`.
    pub profiles: Vec<ProcessProfile>,
    /// Bindings por `(organization, project, repository)`.
    pub bindings: Vec<RepositoryProfileBinding>,
    /// Perfil usado quando não existe binding explícito.
    pub default_profile: String,
    /// Provider escolhido como padrão pelo wizard e pelo `defaultProvider`.
    pub default_provider: String,
    /// ID persistido para o provider OpenAI-compatible (por exemplo `openai`).
    /// O runtime usa o `type` canônico `openai-compatible`.
    pub compatible_provider_id: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProviderObjectInput {
    #[serde(default)]
    id: String,
    #[serde(default, rename = "type")]
    provider_type: String,
    #[serde(default)]
    model: String,
    #[serde(default)]
    reasoning: String,
    #[serde(default)]
    base_url: Option<String>,
    #[serde(default)]
    path: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum ProviderInput {
    Name(String),
    Object(ProviderObjectInput),
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConfigInput {
    #[serde(default)]
    default_profile: String,
    #[serde(default)]
    default_provider: Option<String>,
    #[serde(default)]
    providers: Option<Vec<ProviderInput>>,
    // Global names from the previous flat provider schema.
    #[serde(default)]
    base_url: Option<String>,
    #[serde(default)]
    compatible_model: Option<String>,
    #[serde(default)]
    compatible_reasoning: Option<String>,
    #[serde(default)]
    codex_model: Option<String>,
    #[serde(default)]
    codex_path: Option<String>,
    #[serde(default)]
    codex_reasoning: Option<String>,
    #[serde(default)]
    opencode_model: Option<String>,
    #[serde(default)]
    opencode_path: Option<String>,
    #[serde(default)]
    opencode_reasoning: Option<String>,
    #[serde(default)]
    azure_pat: String,
    #[serde(default)]
    api_key: String,
    #[serde(default = "default_template")]
    template: String,
    #[serde(default)]
    profiles: Vec<ProcessProfile>,
    #[serde(default)]
    bindings: Vec<RepositoryProfileBinding>,
    // Root process names are read only to preserve the legacy in-memory
    // migration path; they are never emitted by the canonical serializer.
    #[serde(default)]
    reviewer_dev: String,
    #[serde(default)]
    reviewer_sprint: String,
    #[serde(default)]
    test_area_path: String,
    #[serde(default)]
    test_assigned_to: String,
    #[serde(default)]
    test_team: String,
    #[serde(default)]
    test_program: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProviderObjectOutput {
    id: String,
    #[serde(rename = "type")]
    provider_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    base_url: Option<String>,
    model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    path: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ConfigOutput<'a> {
    default_profile: &'a str,
    default_provider: &'a str,
    providers: Vec<ProviderObjectOutput>,
    profiles: &'a [ProcessProfile],
    bindings: &'a [RepositoryProfileBinding],
    #[serde(skip_serializing_if = "Option::is_none")]
    api_key: Option<&'a str>,
}

impl Serialize for Config {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let providers = self
            .providers
            .iter()
            .map(|provider| {
                let (id, provider_type, model, reasoning, base_url, path) =
                    self.provider_output_values(provider);
                ProviderObjectOutput {
                    id,
                    provider_type,
                    model,
                    reasoning,
                    base_url,
                    path,
                }
            })
            .collect();
        ConfigOutput {
            default_profile: &self.default_profile,
            default_provider: self.default_provider_id(),
            providers,
            profiles: &self.profiles,
            bindings: &self.bindings,
            api_key: (!self.api_key.trim().is_empty()).then_some(self.api_key.as_str()),
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Config {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let ConfigInput {
            default_profile,
            default_provider,
            providers,
            base_url,
            compatible_model,
            compatible_reasoning,
            codex_model,
            codex_path,
            codex_reasoning,
            opencode_model,
            opencode_path,
            opencode_reasoning,
            azure_pat,
            api_key,
            template,
            profiles,
            bindings,
            reviewer_dev,
            reviewer_sprint,
            test_area_path,
            test_assigned_to,
            test_team,
            test_program,
        } = ConfigInput::deserialize(deserializer)?;
        let mut config = Self {
            azure_pat,
            reviewer_dev,
            reviewer_sprint,
            test_area_path,
            test_assigned_to,
            test_team,
            test_program,
            api_key,
            template,
            profiles,
            bindings,
            default_profile,
            ..Self::default()
        };
        apply_optional_setting(&mut config.base_url, base_url);
        apply_optional_setting(&mut config.compatible_model, compatible_model);
        apply_optional_setting(&mut config.compatible_reasoning, compatible_reasoning);
        apply_optional_setting(&mut config.codex_model, codex_model);
        apply_optional_setting(&mut config.codex_path, codex_path);
        apply_optional_setting(&mut config.codex_reasoning, codex_reasoning);
        apply_optional_setting(&mut config.opencode_model, opencode_model);
        apply_optional_setting(&mut config.opencode_path, opencode_path);
        apply_optional_setting(&mut config.opencode_reasoning, opencode_reasoning);
        if let Some(providers) = providers {
            apply_provider_inputs(&mut config, providers, default_provider.as_deref());
        } else if let Some(default_provider) = default_provider {
            config.default_provider = default_provider;
        }
        Ok(config)
    }
}

fn apply_optional_setting(target: &mut String, value: Option<String>) {
    if let Some(value) = value {
        *target = value;
    }
}

fn apply_provider_inputs(
    config: &mut Config,
    providers: Vec<ProviderInput>,
    requested_default: Option<&str>,
) {
    config.providers.clear();
    let mut provider_ids = Vec::with_capacity(providers.len());
    for provider in providers {
        let (id, provider_type, model, reasoning, base_url, path) = match provider {
            ProviderInput::Name(name) => (name.clone(), name, None, None, None, None),
            ProviderInput::Object(object) => {
                let provider_type = if object.provider_type.trim().is_empty() {
                    object.id.clone()
                } else {
                    object.provider_type.clone()
                };
                (
                    object.id,
                    provider_type,
                    Some(object.model),
                    Some(object.reasoning),
                    object.base_url,
                    object.path,
                )
            }
        };
        let provider_type = provider_type.trim().to_owned();
        if provider_type.is_empty() {
            continue;
        }
        let provider_name = provider_type.clone();
        provider_ids.push((id, provider_name.clone()));
        config.providers.push(provider_name);
        match provider_type.as_str() {
            "codex" => {
                if let Some(model) = model.filter(|value| !value.is_empty()) {
                    config.codex_model = model;
                }
                if let Some(reasoning) = reasoning.filter(|value| !value.is_empty()) {
                    config.codex_reasoning = reasoning;
                }
                if let Some(path) = path {
                    config.codex_path = path;
                }
            }
            "opencode" => {
                if let Some(model) = model.filter(|value| !value.is_empty()) {
                    config.opencode_model = model;
                }
                if let Some(reasoning) = reasoning.filter(|value| !value.is_empty()) {
                    config.opencode_reasoning = reasoning;
                }
                if let Some(path) = path {
                    config.opencode_path = path;
                }
            }
            "openai-compatible" => {
                if let Some(model) = model.filter(|value| !value.is_empty()) {
                    config.compatible_model = model;
                }
                if let Some(reasoning) = reasoning.filter(|value| !value.is_empty()) {
                    config.compatible_reasoning = reasoning;
                }
                if let Some(base_url) = base_url {
                    config.base_url = base_url;
                }
                if let Some(id) = provider_ids.last().map(|(id, _)| id)
                    && !id.trim().is_empty()
                {
                    config.compatible_provider_id.clone_from(id);
                }
            }
            _ => {}
        }
    }
    config.default_provider = resolve_provider_name(
        requested_default,
        &provider_ids,
        config.providers.first().map(String::as_str),
    );
}

impl Config {
    fn default_provider_id(&self) -> &str {
        if self.default_provider == "openai-compatible" {
            if self.compatible_provider_id.trim().is_empty() {
                "openai"
            } else {
                self.compatible_provider_id.as_str()
            }
        } else {
            self.default_provider.as_str()
        }
    }

    fn provider_output_values(
        &self,
        provider: &str,
    ) -> (
        String,
        String,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
    ) {
        match provider {
            "codex" => (
                "codex".to_owned(),
                "codex".to_owned(),
                self.codex_model.clone(),
                owned_non_default_reasoning(&self.codex_reasoning),
                None,
                owned_non_empty(&self.codex_path),
            ),
            "opencode" => (
                "opencode".to_owned(),
                "opencode".to_owned(),
                self.opencode_model.clone(),
                owned_non_default_reasoning(&self.opencode_reasoning),
                None,
                owned_non_empty(&self.opencode_path),
            ),
            "openai-compatible" => (
                if self.compatible_provider_id.trim().is_empty() {
                    "openai".to_owned()
                } else {
                    self.compatible_provider_id.clone()
                },
                "openai-compatible".to_owned(),
                self.compatible_model.clone(),
                owned_non_default_reasoning(&self.compatible_reasoning),
                owned_non_empty(&self.base_url),
                None,
            ),
            other => (
                other.to_owned(),
                other.to_owned(),
                self.compatible_model.clone(),
                owned_non_default_reasoning(&self.compatible_reasoning),
                None,
                None,
            ),
        }
    }
}

fn owned_non_empty(value: &str) -> Option<String> {
    (!value.trim().is_empty()).then(|| value.to_owned())
}

fn owned_non_default_reasoning(value: &str) -> Option<String> {
    (!value.trim().is_empty() && value != "provider-default").then(|| value.to_owned())
}

fn resolve_provider_name(
    requested: Option<&str>,
    provider_ids: &[(String, String)],
    first: Option<&str>,
) -> String {
    if let Some(requested) = requested.map(str::trim).filter(|value| !value.is_empty()) {
        if let Some((_, provider_type)) = provider_ids
            .iter()
            .find(|(id, provider_type)| id == requested || provider_type == requested)
        {
            return provider_type.clone();
        }
        return requested.to_owned();
    }
    first.unwrap_or("codex").to_owned()
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
            default_provider: "codex".to_owned(),
            compatible_provider_id: "openai".to_owned(),
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
        if !self.profiles.is_empty()
            || [
                &self.reviewer_dev,
                &self.reviewer_sprint,
                &self.test_area_path,
                &self.test_assigned_to,
                &self.test_program,
                &self.test_team,
            ]
            .iter()
            .all(|value| value.trim().is_empty())
        {
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
        let mut profiles = self.profiles.clone();
        if !profiles
            .iter()
            .any(|profile| profile.name == AGROTRACE_PROFILE)
        {
            if let Some(agrotrace) = ProcessProfile::named(AGROTRACE_PROFILE) {
                profiles.push(agrotrace);
            }
        }
        profiles
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

fn legacy_value(object: &serde_json::Map<String, Value>, key: &str) -> String {
    object
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

fn legacy_profile_from_object(object: &serde_json::Map<String, Value>) -> ProcessProfile {
    ProcessProfile {
        name: AGROTRACE_PROFILE.to_owned(),
        program_field: AGROTRACE_PROGRAM_FIELD.to_owned(),
        area_path: legacy_value(object, "testAreaPath"),
        assigned_to: legacy_value(object, "testAssignedTo"),
        team: legacy_value(object, "testTeam"),
        program: legacy_value(object, "testProgram"),
        priority: 2.0,
        inherit_iteration_path: true,
        parent_transition: Some(DEFAULT_PARENT_TRANSITION.to_owned()),
        reviewer_dev: legacy_value(object, "reviewerDev"),
        reviewer_sprint: legacy_value(object, "reviewerSprint"),
    }
}

fn has_legacy_root_keys(object: &serde_json::Map<String, Value>) -> bool {
    LEGACY_ROOT_KEYS.iter().any(|key| object.contains_key(*key))
}

fn move_profile_key_to_nested(
    profile: &mut serde_json::Map<String, Value>,
    nested: &mut serde_json::Map<String, Value>,
    old_names: &[&str],
    new_name: &str,
) -> bool {
    let mut changed = false;
    let mut value = None;
    for old_name in old_names {
        if let Some(candidate) = profile.remove(*old_name) {
            changed = true;
            if value.is_none() {
                value = Some(candidate);
            }
        }
    }
    if let Some(value) = value {
        // The nested schema is canonical and wins if both formats coexist.
        nested.entry(new_name.to_owned()).or_insert(value);
    }
    changed
}

fn canonicalize_profile_keys(value: &mut Value) -> bool {
    let Some(profiles) = value.get_mut("profiles").and_then(Value::as_array_mut) else {
        return false;
    };
    let mut changed = false;
    for profile in profiles {
        if let Some(profile) = profile.as_object_mut() {
            let mut test_card = profile
                .remove("testCard")
                .and_then(|value| value.as_object().cloned())
                .unwrap_or_default();
            changed |= move_profile_key_to_nested(
                profile,
                &mut test_card,
                &["testCardProgramField", "programField"],
                "programField",
            );
            changed |= move_profile_key_to_nested(
                profile,
                &mut test_card,
                &["testCardAssignedTo", "assignedTo"],
                "assignedTo",
            );
            changed |= move_profile_key_to_nested(
                profile,
                &mut test_card,
                &["testCardTeam", "team"],
                "team",
            );

            let mut reviewers = profile
                .remove("reviewers")
                .and_then(|value| value.as_object().cloned())
                .unwrap_or_default();
            changed |= move_profile_key_to_nested(
                profile,
                &mut reviewers,
                &["reviewerDev"],
                "development",
            );
            changed |=
                move_profile_key_to_nested(profile, &mut reviewers, &["reviewerSprint"], "sprint");

            if !test_card.is_empty() {
                profile.insert("testCard".to_owned(), Value::Object(test_card));
            }
            if !reviewers.is_empty() {
                profile.insert("reviewers".to_owned(), Value::Object(reviewers));
            }
        }
    }
    changed
}

fn has_legacy_profile_keys(value: &Value) -> bool {
    value
        .get("profiles")
        .and_then(Value::as_array)
        .is_some_and(|profiles| {
            profiles.iter().any(|profile| {
                profile.as_object().is_some_and(|profile| {
                    [
                        "programField",
                        "testCardProgramField",
                        "assignedTo",
                        "testCardAssignedTo",
                        "team",
                        "testCardTeam",
                        "reviewerDev",
                        "reviewerSprint",
                    ]
                    .iter()
                    .any(|key| profile.contains_key(*key))
                })
            })
        })
}

fn has_legacy_provider_schema(value: &Value) -> bool {
    let has_legacy_root = value.as_object().is_some_and(|object| {
        LEGACY_PROVIDER_ROOT_KEYS
            .iter()
            .any(|key| object.contains_key(*key))
    });
    let Some(providers) = value.get("providers").and_then(Value::as_array) else {
        return has_legacy_root;
    };
    has_legacy_root
        || providers.iter().any(Value::is_string)
        || (!providers.is_empty() && value.get("defaultProvider").is_none())
}

/// Normaliza chaves legadas no JSON existente sem reserializar segredos.
fn migrate_legacy_json(raw: &str, config: &Config) -> crate::error::Result<String> {
    let mut value: Value =
        serde_json::from_str(raw).map_err(|error| crate::error::AppError::Config {
            message: format!("falha ao migrar config.json: {error}"),
        })?;
    canonicalize_profile_keys(&mut value);
    {
        let object = value
            .as_object_mut()
            .ok_or_else(|| crate::error::AppError::Config {
                message: "config.json deve conter um objeto JSON".to_owned(),
            })?;
        let has_profiles = object
            .get("profiles")
            .and_then(Value::as_array)
            .is_some_and(|profiles| !profiles.is_empty());
        if !has_profiles {
            let profile = config
                .profiles
                .iter()
                .find(|profile| profile.name == AGROTRACE_PROFILE)
                .cloned()
                .unwrap_or_else(|| legacy_profile_from_object(object));
            object.insert(
                "profiles".to_owned(),
                serde_json::to_value([profile]).map_err(|error| {
                    crate::error::AppError::Config {
                        message: format!("falha ao serializar perfil legado: {error}"),
                    }
                })?,
            );
            object.insert(
                "defaultProfile".to_owned(),
                Value::String(AGROTRACE_PROFILE.to_owned()),
            );
        }
        for key in LEGACY_ROOT_KEYS {
            object.remove(key);
        }
    }
    let canonical_config = if has_legacy_provider_schema(&value) {
        let mut canonical_config = config.clone();
        canonical_config.providers = default_providers();
        canonical_config
    } else {
        config.clone()
    };
    let canonical =
        serde_json::to_value(canonical_config).map_err(|error| crate::error::AppError::Config {
            message: format!("falha ao serializar providers canônicos: {error}"),
        })?;
    {
        let object = value.as_object_mut().expect("objeto JSON validado");
        if let Some(providers) = canonical.get("providers") {
            object.insert("providers".to_owned(), providers.clone());
        }
        if let Some(default_provider) = canonical.get("defaultProvider") {
            object.insert("defaultProvider".to_owned(), default_provider.clone());
        }
        for key in LEGACY_PROVIDER_ROOT_KEYS {
            object.remove(key);
        }
        if let Some(api_key) = canonical.get("apiKey") {
            object.insert("apiKey".to_owned(), api_key.clone());
        } else {
            object.remove("apiKey");
        }
    }
    if let Some(profiles) = value
        .as_object_mut()
        .and_then(|object| object.get_mut("profiles"))
        .and_then(Value::as_array_mut)
    {
        for profile in profiles {
            if let Some(profile) = profile.as_object_mut() {
                profile.remove("azurePat");
                profile.remove("apiKey");
            }
        }
    }
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

/// Persiste a configuração local sem copiar o PAT para `config.json`.
///
/// O helper é usado pelo onboarding, que altera somente `profiles` e
/// `bindings`; todas as demais opções permanecem no snapshot fornecido pelo
/// chamador. O template continua no arquivo dedicado e o PAT continua no
/// `.env`/ambiente global.
///
/// # Errors
///
/// Retorna [`crate::error::AppError`] quando a configuração não puder ser
/// serializada ou o arquivo não puder ser substituído atomicamente.
pub fn persist_local_config(config: &Config) -> crate::error::Result<()> {
    let paths = config_paths();
    std::fs::create_dir_all(&paths.directory)?;
    let mut json =
        serde_json::to_value(config).map_err(|error| crate::error::AppError::Config {
            message: format!("falha ao serializar config: {error}"),
        })?;
    if let Some(object) = json.as_object_mut() {
        object.remove("azurePat");
        object.remove("template");
    }
    let contents =
        serde_json::to_string_pretty(&json).map_err(|error| crate::error::AppError::Config {
            message: format!("falha ao formatar config: {error}"),
        })?;
    let original_env = read_optional_file(&paths.env_file)?;
    if let Err(error) = persist_global_env(&paths.env_file, config.azure_pat.as_str()) {
        let _ = restore_optional_file(&paths.env_file, original_env.as_deref());
        return Err(error);
    }
    if let Err(error) = write_atomic(&paths.config_file, &format!("{contents}\n")) {
        let _ = restore_optional_file(&paths.env_file, original_env.as_deref());
        return Err(error);
    }
    Ok(())
}

fn persist_global_env(path: &Path, azure_pat: &str) -> crate::error::Result<()> {
    let current = read_optional_file(path)?.unwrap_or_default();
    let mut lines: Vec<String> = current.lines().map(str::to_owned).collect();
    lines.retain(|line| {
        let Some((key, _)) = line.trim_start().split_once('=') else {
            return true;
        };
        !matches!(
            key.trim(),
            "PR_REVIEWER_DEV"
                | "PR_REVIEWER_SPRINT"
                | "TEST_CARD_ASSIGNED_TO"
                | "TEST_CARD_AREA_PATH"
                | "TEST_CARD_TEAM"
                | "TEST_CARD_PROGRAM"
        )
    });
    if !azure_pat.trim().is_empty() {
        let escaped = azure_pat.replace('\\', r"\\").replace('"', "\\\"");
        let line = format!(r#"AZURE_PAT="{escaped}""#);
        if let Some(existing) = lines.iter_mut().find(|line| {
            let trimmed = line.trim_start();
            trimmed.starts_with("AZURE_PAT")
                && trimmed["AZURE_PAT".len()..].trim_start().starts_with('=')
        }) {
            *existing = line;
        } else {
            lines.push(line);
        }
    }
    if lines.is_empty() && azure_pat.trim().is_empty() && current.trim().is_empty() {
        return Ok(());
    }
    write_atomic(path, &format!("{}\n", lines.join("\n").trim_end()))
}

fn read_optional_file(path: &Path) -> crate::error::Result<Option<String>> {
    match std::fs::read_to_string(path) {
        Ok(contents) => Ok(Some(contents)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn restore_optional_file(path: &Path, contents: Option<&str>) -> crate::error::Result<()> {
    match contents {
        Some(contents) => write_atomic(path, contents),
        None => match std::fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.into()),
        },
    }
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
        p.clone_into(&mut config.default_provider);
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
    load_config_internal(true)
}

/// Carrega a configuração sem executar a migração legada automática.
///
/// É usado antes de uma decisão interativa para que `--dry-run`, `--raw` e
/// execuções sem TTY nunca alterem `config.json` apenas por avaliar o
/// onboarding.
///
/// # Errors
///
/// Retorna [`crate::error::AppError::Config`] quando o JSON configurado for
/// inválido.
pub fn load_config_without_migration() -> crate::error::Result<Config> {
    load_config_internal(false)
}

fn load_config_internal(migrate_legacy: bool) -> crate::error::Result<Config> {
    let paths = config_paths();
    let legacy = legacy_config_paths();
    let mut config = Config::default();

    let config_source = read_config_source(
        &paths.config_file,
        legacy.as_ref().map(|paths| paths.config_file.as_path()),
    );
    if let Some((_, raw)) = &config_source {
        let mut raw_value: Value =
            serde_json::from_str(raw).map_err(|e| crate::error::AppError::Config {
                message: format!("{}: {e}", paths.config_file.display()),
            })?;
        // Alias antigo é aceito para leitura, mas a normalização antes do
        // deserialize também resolve arquivos que contenham os dois nomes.
        canonicalize_profile_keys(&mut raw_value);
        let file_cfg: Config =
            serde_json::from_value(raw_value).map_err(|e| crate::error::AppError::Config {
                message: format!("{}: {e}", paths.config_file.display()),
            })?;
        config = file_cfg;
    }
    // `.env` opcional (merge simples `KEY=VAL`). Apenas credenciais e opções
    // globais são aplicadas; overrides de processo antigos não são fonte.
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
        let has_legacy = raw_value.as_object().is_some_and(has_legacy_root_keys)
            || has_legacy_profile_keys(&raw_value)
            || has_legacy_provider_schema(&raw_value);
        if migrate_legacy && has_legacy {
            let has_profiles = raw_value
                .get("profiles")
                .and_then(Value::as_array)
                .is_some_and(|profiles| !profiles.is_empty());
            if !has_profiles {
                config.profiles.push(legacy_profile_from_object(
                    raw_value.as_object().expect("objeto JSON validado"),
                ));
                AGROTRACE_PROFILE.clone_into(&mut config.default_profile);
            }
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
    fn profile_key_normalization_should_prefer_new_names_when_both_exist() {
        let mut raw = serde_json::json!({
            "profiles": [{
                "name": "Novo",
                "programField": "Custom.Legado",
                "testCardProgramField": "Custom.Novo",
                "assignedTo": "legado@example.com",
                "testCardAssignedTo": "novo@example.com",
                "team": "Legacy Team",
                "testCardTeam": "New Team"
            }]
        });

        assert!(canonicalize_profile_keys(&mut raw));
        let profile: ProcessProfile =
            serde_json::from_value(raw["profiles"][0].clone()).expect("perfil normalizado");
        assert_eq!(profile.program_field, "Custom.Novo");
        assert_eq!(profile.assigned_to, "novo@example.com");
        assert_eq!(profile.team, "New Team");
        assert!(raw["profiles"][0].get("programField").is_none());
        assert!(raw["profiles"][0].get("assignedTo").is_none());
        assert!(raw["profiles"][0].get("team").is_none());
    }

    #[test]
    fn legacy_json_migration_should_materialize_single_agrotrace_profile() {
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
    fn legacy_migration_should_be_idempotent_and_keep_original_on_atomic_failure() {
        let dir = tempfile::tempdir().expect("diretório temporário");
        let path = dir.path().join("config.json");
        let raw = serde_json::json!({
            "testAreaPath": "AGROTRACE\\QA",
            "testAssignedTo": "qa@example.com",
            "testTeam": "DevOps",
            "testProgram": "Agrotrace",
            "reviewerDev": "dev@example.com",
            "reviewerSprint": "sprint@example.com",
            "azurePat": "pat-secret",
            "apiKey": "api-secret"
        });
        std::fs::write(&path, serde_json::to_string_pretty(&raw).unwrap()).unwrap();
        let config: Config = serde_json::from_value(raw).unwrap();
        // The loader reads legacy process values from the raw JSON object;
        // transient compatibility fields are intentionally skipped by serde.
        let migrated = migrate_legacy_json(&std::fs::read_to_string(&path).unwrap(), &config)
            .expect("migração");
        write_atomic(&path, &migrated).expect("substituição atômica");
        let first: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(first["profiles"].as_array().unwrap().len(), 1);
        assert_eq!(first["defaultProfile"], AGROTRACE_PROFILE);
        for key in LEGACY_ROOT_KEYS {
            assert!(first.get(key).is_none(), "chave legada persistida: {key}");
        }
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

        let target = dir.path().join("config-failure");
        std::fs::create_dir(&target).unwrap();
        let marker = target.join("original");
        std::fs::write(&marker, "original").unwrap();
        assert!(write_atomic(&target, "replacement").is_err());
        assert_eq!(std::fs::read_to_string(&marker).unwrap(), "original");
    }

    #[test]
    fn normalization_should_remove_legacy_keys_and_preserve_existing_profiles() {
        let raw = serde_json::json!({
            "providers": ["codex"],
            "defaultProfile": "Existente",
            "reviewerDev": "legacy-dev@example.com",
            "reviewerSprint": "legacy-sprint@example.com",
            "testAreaPath": "Legacy\\QA",
            "testAssignedTo": "legacy@example.com",
            "testProgram": "Legacy",
            "testTeam": "Legacy Team",
            "profiles": [{
                "name": "Existente",
                "programField": "Custom.ProgramasExistente",
                "areaPath": "Atual\\QA",
                "assignedTo": "atual@example.com",
                "team": "Atual Team",
                "program": "Atual",
                "priority": 4,
                "inheritIterationPath": false,
                "parentTransition": "Ready",
                "reviewerDev": "atual-dev@example.com",
                "reviewerSprint": "atual-sprint@example.com",
                "azurePat": "profile-pat",
                "apiKey": "profile-key"
            }],
            "bindings": [{
                "profile": "Existente",
                "organization": "org",
                "project": "project",
                "repository": "repo"
            }]
        });
        let mut config: Config = serde_json::from_value(raw.clone()).unwrap();
        assert_eq!(
            config.profiles[0].program_field,
            "Custom.ProgramasExistente"
        );
        assert_eq!(config.profiles[0].assigned_to, "atual@example.com");
        assert_eq!(config.profiles[0].team, "Atual Team");
        let migrated = migrate_legacy_json(&raw.to_string(), &config).unwrap();
        let value: Value = serde_json::from_str(&migrated).unwrap();
        for key in LEGACY_ROOT_KEYS {
            assert!(value.get(key).is_none(), "chave legada persistida: {key}");
        }
        assert_eq!(value["defaultProfile"], "Existente");
        assert_eq!(value["profiles"][0]["areaPath"], "Atual\\QA");
        assert_eq!(
            value["profiles"][0]["testCard"]["programField"],
            "Custom.ProgramasExistente"
        );
        assert_eq!(
            value["profiles"][0]["testCard"]["assignedTo"],
            "atual@example.com"
        );
        assert_eq!(value["profiles"][0]["testCard"]["team"], "Atual Team");
        assert!(value["profiles"][0].get("programField").is_none());
        assert!(value["profiles"][0].get("assignedTo").is_none());
        assert!(value["profiles"][0].get("team").is_none());
        assert_eq!(
            value["profiles"][0]["reviewers"]["development"],
            "atual-dev@example.com"
        );
        assert_eq!(
            value["profiles"][0]["reviewers"]["sprint"],
            "atual-sprint@example.com"
        );
        assert!(value["profiles"][0].get("reviewerDev").is_none());
        assert!(value["profiles"][0].get("reviewerSprint").is_none());
        assert!(value["profiles"][0].get("azurePat").is_none());
        assert!(value["profiles"][0].get("apiKey").is_none());
        assert_eq!(value["bindings"][0]["profile"], "Existente");
        assert_eq!(value["providers"][0]["id"], "codex");
        assert_eq!(value["providers"][0]["type"], "codex");
        assert_eq!(value["defaultProvider"], "codex");
        config.profiles = serde_json::from_value(value["profiles"].clone()).unwrap();
        assert_eq!(config.profiles.len(), 1);
    }

    #[test]
    fn canonical_serialization_should_omit_legacy_root_keys_and_profile_secrets() {
        let config = Config {
            profiles: vec![ProcessProfile::named(AGROTRACE_PROFILE).unwrap()],
            ..Config::default()
        };
        let value = serde_json::to_value(config).unwrap();
        for key in LEGACY_ROOT_KEYS {
            assert!(value.get(key).is_none(), "chave legada serializada: {key}");
        }
        let profile = &value["profiles"][0];
        assert!(profile["testCard"].get("programField").is_some());
        assert!(profile["testCard"].get("assignedTo").is_some());
        assert!(profile["testCard"].get("team").is_some());
        assert!(profile.get("programField").is_none());
        assert!(profile.get("assignedTo").is_none());
        assert!(profile.get("team").is_none());
        assert!(profile.get("azurePat").is_none());
        assert!(profile.get("apiKey").is_none());
        assert_eq!(value["defaultProvider"], "codex");
        assert_eq!(value["providers"][0]["type"], "codex");
    }

    #[test]
    fn nested_config_should_load_provider_and_profile_values() {
        let raw = serde_json::json!({
            "defaultProfile": "Agrotrace",
            "defaultProvider": "openai",
            "providers": [
                {
                    "id": "codex",
                    "type": "codex",
                    "model": "gpt-5.6-luna",
                    "reasoning": "medium"
                },
                {
                    "id": "openai",
                    "type": "openai-compatible",
                    "baseUrl": "https://api.example/v1",
                    "model": "gpt-4o-mini"
                }
            ],
            "profiles": [{
                "name": "Agrotrace",
                "program": "Agrotrace",
                "areaPath": "AGROTRACE\\Devops",
                "inheritIterationPath": true,
                "parentTransition": "Test QA",
                "priority": 2,
                "reviewers": {
                    "development": "dev@example.com",
                    "sprint": "sprint@example.com"
                },
                "testCard": {
                    "assignedTo": "qa@example.com",
                    "programField": "Custom.ProgramasAgrotrace",
                    "team": "DevOps"
                }
            }]
        });
        let config: Config = serde_json::from_value(raw).expect("configuração aninhada");

        assert_eq!(config.default_provider, "openai-compatible");
        assert_eq!(config.compatible_provider_id, "openai");
        assert_eq!(config.base_url, "https://api.example/v1");
        assert_eq!(config.codex_reasoning, "medium");
        assert_eq!(config.profiles[0].reviewer_dev, "dev@example.com");
        assert_eq!(
            config.profiles[0].program_field,
            "Custom.ProgramasAgrotrace"
        );

        let value = serde_json::to_value(config).expect("configuração serializável");
        assert_eq!(value["defaultProvider"], "openai");
        assert_eq!(value["providers"][1]["id"], "openai");
        assert_eq!(value["providers"][1]["type"], "openai-compatible");
        assert_eq!(
            value["profiles"][0]["reviewers"]["development"],
            "dev@example.com"
        );
        assert_eq!(
            value["profiles"][0]["testCard"]["programField"],
            "Custom.ProgramasAgrotrace"
        );
    }

    #[test]
    fn legacy_provider_list_should_be_migrated_even_with_nested_profiles() {
        let raw = serde_json::json!({
            "providers": ["codex"],
            "profiles": [{
                "name": "Agrotrace",
                "program": "Agrotrace",
                "areaPath": "AGROTRACE\\Devops",
                "testCard": {
                    "assignedTo": "qa@example.com",
                    "programField": "Custom.ProgramasAgrotrace",
                    "team": "DevOps"
                },
                "reviewers": {"development": "", "sprint": ""}
            }]
        });
        let config: Config = serde_json::from_value(raw.clone()).expect("configuração legada");
        let migrated = migrate_legacy_json(&raw.to_string(), &config).expect("migração");
        let value: Value = serde_json::from_str(&migrated).expect("JSON migrado");

        assert_eq!(value["providers"][0]["id"], "codex");
        assert_eq!(value["providers"][0]["type"], "codex");
        assert_eq!(value["defaultProvider"], "codex");
    }

    #[test]
    fn mixed_config_should_remove_flat_provider_settings_during_migration() {
        let raw = serde_json::json!({
            "apiKey": "",
            "baseUrl": "https://api.example/v1",
            "codexModel": "gpt-5.6-luna",
            "codexReasoning": "medium",
            "compatibleModel": "gpt-4o-mini",
            "opencodeModel": "openai/gpt-5.5",
            "defaultProfile": "Agrotrace",
            "defaultProvider": "codex",
            "providers": [{
                "id": "codex",
                "type": "codex",
                "model": "gpt-5.6-luna",
                "reasoning": "medium"
            }],
            "profiles": [{
                "name": "Agrotrace",
                "program": "Agrotrace",
                "areaPath": "AGROTRACE\\Devops",
                "reviewers": {"development": "", "sprint": ""},
                "testCard": {
                    "assignedTo": "qa@example.com",
                    "programField": "Custom.ProgramasAgrotrace",
                    "team": "DevOps"
                }
            }]
        });
        let config: Config = serde_json::from_value(raw.clone()).expect("configuração misturada");
        let migrated = migrate_legacy_json(&raw.to_string(), &config).expect("migração");
        let value: Value = serde_json::from_str(&migrated).expect("JSON migrado");

        for key in LEGACY_PROVIDER_ROOT_KEYS {
            assert!(value.get(key).is_none(), "chave antiga persistida: {key}");
        }
        assert!(value.get("apiKey").is_none());
        assert_eq!(value["providers"].as_array().unwrap().len(), 3);
        assert_eq!(value["providers"][0]["reasoning"], "medium");
        assert_eq!(value["providers"][2]["type"], "openai-compatible");
    }

    #[test]
    fn legacy_process_dotenv_should_be_ignored_while_global_secrets_are_preserved() {
        let mut config = Config::default();
        apply_env_kv(&mut config, "PR_REVIEWER_DEV", "legacy@example.com");
        apply_env_kv(&mut config, "TEST_CARD_PROGRAM", "Legacy");
        apply_env_kv(&mut config, "AZURE_PAT", "pat-secret");
        apply_env_kv(&mut config, "PR_AI_API_KEY", "api-secret");
        assert!(config.reviewer_dev.is_empty());
        assert!(config.test_program.is_empty());
        assert_eq!(config.azure_pat, "pat-secret");
        assert_eq!(config.api_key, "api-secret");
    }

    #[test]
    fn persisted_global_env_should_keep_pat_and_remove_process_overrides() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".env");
        std::fs::write(&path, "OTHER=1\nAZURE_PAT=\"old\"\nTEST_CARD_TEAM=legacy\n").unwrap();

        persist_global_env(&path, "new\\pat").unwrap();

        let contents = std::fs::read_to_string(path).unwrap();
        assert!(contents.contains("OTHER=1"));
        assert!(contents.contains(r#"AZURE_PAT="new\\pat""#));
        assert!(!contents.contains("TEST_CARD_TEAM"));
    }
}
