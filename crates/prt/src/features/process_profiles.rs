//! Seleção e validação local dos perfis de processo Azure DevOps.
//!
//! A seleção depende exclusivamente da identidade do remote Azure. O caminho
//! do checkout nunca entra na chave, e bindings ambíguos são rejeitados antes
//! de qualquer operação remota de escrita.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::azure::{self, work_items};
use crate::config::{AGROTRACE_PROFILE, Config, ProcessProfile, RepositoryProfileBinding};
use crate::error::{AppError, Result};
use crate::git::RepositoryRemote;

/// Resultado congelado da seleção de um perfil para uma execução.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProfileSelection {
    /// Perfil escolhido, incluindo seus defaults não secretos.
    pub profile: ProcessProfile,
    /// Field Azure usado para o valor de programa.
    pub program_field: String,
    /// Identidade do remote que determinou a seleção.
    pub remote: RepositoryRemote,
}

/// Cria o snapshot compatível para consumidores locais que ainda não têm um
/// remote disponível (por exemplo, fixtures e a camada de settings).
///
/// # Panics
///
/// Entra em pânico apenas se os defaults compilados não puderem criar o perfil
/// Agrotrace canônico, o que indica um erro interno de configuração.
#[must_use]
pub fn legacy_selection(config: &Config, remote: RepositoryRemote) -> ProfileSelection {
    let profile_name = if config.default_profile.trim().is_empty() {
        AGROTRACE_PROFILE
    } else {
        config.default_profile.as_str()
    };
    let profile = config
        .effective_process_profiles()
        .into_iter()
        .find(|profile| profile.name == profile_name)
        .or_else(|| ProcessProfile::named(AGROTRACE_PROFILE))
        .expect("perfil Agrotrace canônico disponível");
    let program_field = profile
        .program_field()
        .unwrap_or(crate::config::AGROTRACE_PROGRAM_FIELD)
        .to_owned();
    ProfileSelection {
        profile,
        program_field,
        remote,
    }
}

/// Snapshot dos metadados remotos usados por uma tentativa de Test Case.
#[derive(Debug, Clone, PartialEq)]
pub struct ProfileMetadata {
    /// Tipo remoto confirmado como `Test Case`.
    pub test_case_type: work_items::WorkItemTypeMetadata,
    /// Fields retornados com `$expand=all`.
    pub test_case_fields: Vec<work_items::WorkItemFieldMetadata>,
    /// Tipo do Work Item pai consultado.
    pub parent_type: String,
    /// Estados permitidos pelo tipo do pai.
    pub parent_states: Vec<work_items::WorkItemStateMetadata>,
}

/// Consulta metadata necessária para validar um perfil antes da escrita.
///
/// # Errors
///
/// Retorna erro de validação quando o tipo, field ou estado necessário não
/// existe ou é incompatível; propaga falhas remotas com a etapa identificada.
pub async fn load_metadata(
    client: &azure::AzureClient,
    project: &str,
    selection: &ProfileSelection,
    parent_type: &str,
) -> Result<ProfileMetadata> {
    let types = work_items::list_work_item_types(client, project)
        .await
        .map_err(|error| metadata_error("listar Work Item Types", error))?;
    let Some(test_case_type) = types.into_iter().find(|item| item.name == "Test Case") else {
        return Err(AppError::Config {
            message: format!(
                "validação de metadata: o projeto não disponibiliza o Work Item Type Test Case para o perfil {}",
                selection.name()
            ),
        });
    };
    let test_case_fields =
        work_items::list_work_item_type_fields(client, project, &test_case_type.name)
            .await
            .map_err(|error| metadata_error("listar fields do Work Item Type Test Case", error))?;
    validate_schema_fields(selection, &test_case_fields)?;

    let parent_type = parent_type.trim().to_owned();
    let parent_states = if let Some(transition) = selection.profile.parent_transition() {
        if parent_type.is_empty() {
            return Err(AppError::Config {
                message: format!(
                    "validação de metadata: o tipo do Work Item pai é necessário para validar o estado {transition}"
                ),
            });
        }
        work_items::list_work_item_type_states(client, project, &parent_type)
            .await
            .map_err(|error| metadata_error("listar estados do Work Item pai", error))?
    } else {
        Vec::new()
    };
    if let Some(transition) = selection.profile.parent_transition()
        && !parent_states.iter().any(|state| state.name == transition)
    {
        return Err(AppError::Config {
            message: format!(
                "validação de metadata: o estado do Work Item pai {transition} não existe para o tipo {parent_type}"
            ),
        });
    }
    Ok(ProfileMetadata {
        test_case_type,
        test_case_fields,
        parent_type,
        parent_states,
    })
}

/// Valida os fields fixos e os valores finais antes de iniciar o POST.
///
/// # Errors
///
/// Retorna [`AppError::Config`] quando um field do schema não existe ou tem
/// tipo incompatível.
pub fn validate_schema_fields(
    selection: &ProfileSelection,
    fields: &[work_items::WorkItemFieldMetadata],
) -> Result<()> {
    let required = ["Custom.Team", selection.program_field.as_str()];
    for reference_name in required {
        let Some(field) = fields
            .iter()
            .find(|field| field.reference_name == reference_name)
        else {
            return Err(AppError::Config {
                message: format!(
                    "validação de metadata: o field fixo {reference_name} não existe no Work Item Type Test Case"
                ),
            });
        };
        if let Some(field_type) = field.field_type.as_deref()
            && !is_string_field(field_type)
        {
            return Err(AppError::Config {
                message: format!(
                    "validação de metadata: o field {reference_name} tem tipo incompatível {}",
                    field_type
                ),
            });
        }
    }
    Ok(())
}

/// Valida um valor final contra required/default/allowedValues do Azure.
///
/// # Errors
///
/// Retorna [`AppError::Config`] quando um valor obrigatório está ausente ou
/// não pertence aos valores permitidos pelo processo.
pub fn validate_field_value(field: &work_items::WorkItemFieldMetadata, value: &str) -> Result<()> {
    let value = value.trim();
    if value.is_empty() {
        let has_default = field.default_value.as_ref().is_some_and(|default| {
            !default.is_null() && value_text(default).is_some_and(|v| !v.trim().is_empty())
        });
        if field.required && !has_default {
            return Err(AppError::Config {
                message: format!(
                    "validação de metadata: o field obrigatório {} não possui default e não recebeu valor",
                    field.reference_name
                ),
            });
        }
        return Ok(());
    }
    if !field.allowed_values.is_empty()
        && !field
            .allowed_values
            .iter()
            .filter_map(value_text)
            .any(|allowed| allowed == value)
    {
        return Err(AppError::Config {
            message: format!(
                "validação de metadata: o valor informado não é permitido no field {}",
                field.reference_name
            ),
        });
    }
    Ok(())
}

fn is_string_field(field_type: &str) -> bool {
    matches!(
        field_type.to_ascii_lowercase().as_str(),
        "string" | "plaintext" | "html" | "history" | "treepath"
    )
}

fn value_text(value: &serde_json::Value) -> Option<String> {
    value
        .as_str()
        .map(str::to_owned)
        .or_else(|| value.as_i64().map(|number| number.to_string()))
        .or_else(|| value.as_f64().map(|number| number.to_string()))
        .or_else(|| {
            value.as_object().and_then(|object| {
                ["value", "name", "displayName"]
                    .iter()
                    .find_map(|key| object.get(*key).and_then(serde_json::Value::as_str))
                    .map(str::to_owned)
            })
        })
}

fn metadata_error(stage: &str, error: AppError) -> AppError {
    match error {
        AppError::Azure { status, message } if status < 300 => AppError::Azure {
            status,
            message: format!("validação de metadata falhou ao {stage}: {message}"),
        },
        AppError::Azure { status, .. } => AppError::Azure {
            status,
            message: format!("validação de metadata falhou ao {stage}"),
        },
        AppError::Http(_) => AppError::Config {
            message: format!("validação de metadata falhou ao {stage}: erro de transporte"),
        },
        other => AppError::Config {
            message: format!("validação de metadata falhou ao {stage}: {other}"),
        },
    }
}

impl ProfileSelection {
    /// Nome estável mostrado nas telas de revisão e diagnóstico.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.profile.name
    }

    /// Reviewer padrão para o target informado.
    #[must_use]
    pub fn reviewer_for(&self, target: &str) -> String {
        if target.contains("sprint") && !self.profile.reviewer_sprint.trim().is_empty() {
            self.profile.reviewer_sprint.trim().to_owned()
        } else {
            self.profile.reviewer_dev.trim().to_owned()
        }
    }
}

/// Valida a configuração dos perfis e bindings sem acessar o Azure.
///
/// # Errors
///
/// Retorna erro quando há field ausente, perfil repetido, binding sem perfil ou
/// mais de um binding para a mesma identidade Azure.
pub fn validate_config(config: &Config) -> Result<()> {
    let profiles = config.effective_process_profiles();
    let mut names = HashSet::with_capacity(profiles.len());
    for profile in &profiles {
        if profile.name.trim().is_empty() {
            return Err(AppError::Config {
                message: "perfil sem nome na configuração".to_owned(),
            });
        }
        if profile.program_field().is_none() {
            return Err(AppError::Config {
                message: format!(
                    "perfil {} não informa testCard.programField; preencha testCard.programField no perfil",
                    profile.name
                ),
            });
        }
        if let Some(program_field) = profile.program_field()
            && !is_valid_field_reference_name(program_field)
        {
            return Err(AppError::Config {
                message: format!(
                    "perfil {} informa testCard.programField inválido `{program_field}`; use o referenceName do campo Azure (ex.: Custom.ProgramasAgrotrace), não o valor do programa em `program`",
                    profile.name
                ),
            });
        }
        if !names.insert(profile.name.clone()) {
            return Err(AppError::Config {
                message: format!("perfil {} está duplicado na configuração", profile.name),
            });
        }
    }
    let mut bindings = HashMap::<(String, &str, &str), Vec<&str>>::new();
    for binding in &config.bindings {
        if !profiles
            .iter()
            .any(|profile| profile.name == binding.profile)
        {
            return Err(AppError::Config {
                message: format!(
                    "binding {} aponta para o perfil inexistente {}",
                    binding_label(binding),
                    binding.profile
                ),
            });
        }
        bindings
            .entry((
                binding.organization.to_ascii_lowercase(),
                binding.project.as_str(),
                binding.repository.as_str(),
            ))
            .or_default()
            .push(binding.profile.as_str());
    }
    if let Some((remote, profiles)) = bindings
        .into_iter()
        .find(|(_, profiles)| profiles.len() > 1)
    {
        return Err(AppError::Config {
            message: format!(
                "remote {} possui bindings conflitantes para os perfis {}",
                remote_label_parts(&remote.0, remote.1, remote.2),
                profiles.join(", ")
            ),
        });
    }
    if !config.default_profile.trim().is_empty()
        && !profiles
            .iter()
            .any(|profile| profile.name == config.default_profile)
    {
        return Err(AppError::Config {
            message: format!(
                "defaultProfile {} não existe nos perfis configurados",
                config.default_profile
            ),
        });
    }
    Ok(())
}

/// Seleciona exatamente um perfil para um remote Azure.
///
/// Bindings explícitos vencem o fallback `defaultProfile`. A comparação é
/// textual e exata nos três segmentos do remote.
///
/// # Errors
///
/// Retorna [`AppError::Config`] quando a configuração é inválida, há bindings
/// conflitantes ou o perfil selecionado não possui field de programa.
pub fn select(config: &Config, remote: &RepositoryRemote) -> Result<ProfileSelection> {
    validate_config(config)?;
    let profiles = config.effective_process_profiles();
    let matches: Vec<&RepositoryProfileBinding> = config
        .bindings
        .iter()
        .filter(|binding| binding_matches(binding, remote))
        .collect();
    if matches.len() > 1 {
        return Err(AppError::Config {
            message: format!(
                "remote {} possui bindings conflitantes para os perfis {}",
                remote_label(remote),
                matches
                    .iter()
                    .map(|binding| binding.profile.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        });
    }
    let profile_name = matches
        .first()
        .map(|binding| binding.profile.as_str())
        .or_else(|| non_empty(config.default_profile.as_str()))
        .unwrap_or(AGROTRACE_PROFILE);
    let profile = profiles
        .into_iter()
        .find(|profile| profile.name == profile_name)
        .ok_or_else(|| AppError::Config {
            message: format!(
                "perfil {profile_name} não encontrado para o remote {}; associe um perfil no onboarding ou em config.json",
                remote_label(remote)
            ),
        })?;
    let Some(program_field) = profile.program_field().map(str::to_owned) else {
        return Err(AppError::Config {
            message: format!("perfil {} não informa testCard.programField", profile.name),
        });
    };
    Ok(ProfileSelection {
        profile,
        program_field,
        remote: remote.clone(),
    })
}

/// Verifica a forma mínima de um `referenceName` de field Azure.
///
/// Os endpoints de Work Item usam namespaces como `System`, `Custom` ou
/// `Microsoft`; um valor sem namespace normalmente é o valor do campo, não
/// sua referência. A existência real do field continua sendo validada contra
/// os metadados do projeto.
#[must_use]
pub fn is_valid_field_reference_name(value: &str) -> bool {
    let value = value.trim();
    let Some((namespace, name)) = value.split_once('.') else {
        return false;
    };
    !namespace.trim().is_empty()
        && !name.trim().is_empty()
        && !value.chars().any(char::is_whitespace)
}

/// Retorna um binding para um remote, sem aplicar fallback.
#[must_use]
pub fn binding_for<'a>(
    bindings: &'a [RepositoryProfileBinding],
    remote: &RepositoryRemote,
) -> Vec<&'a RepositoryProfileBinding> {
    bindings
        .iter()
        .filter(|binding| binding_matches(binding, remote))
        .collect()
}

fn binding_matches(binding: &RepositoryProfileBinding, remote: &RepositoryRemote) -> bool {
    binding
        .organization
        .eq_ignore_ascii_case(&remote.organization)
        && binding.project == remote.project
        && binding.repository == remote.repository
}

fn non_empty(value: &str) -> Option<&str> {
    (!value.trim().is_empty()).then_some(value)
}

fn binding_label(binding: &RepositoryProfileBinding) -> String {
    remote_label_parts(&binding.organization, &binding.project, &binding.repository)
}

fn remote_label(remote: &RepositoryRemote) -> String {
    remote_label_parts(&remote.organization, &remote.project, &remote.repository)
}

fn remote_label_parts(organization: &str, project: &str, repository: &str) -> String {
    format!("{organization}/{project}/{repository}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{CHECKMILK_PROFILE, CHECKMILK_PROGRAM_FIELD, ProcessProfile};

    fn remote(project: &str, repository: &str) -> RepositoryRemote {
        RepositoryRemote {
            organization: "org".to_owned(),
            project: project.to_owned(),
            repository: repository.to_owned(),
        }
    }

    fn profile(name: &str) -> ProcessProfile {
        let lower_name = name.to_ascii_lowercase();
        ProcessProfile {
            name: name.to_owned(),
            program_field: match name {
                AGROTRACE_PROFILE => crate::config::AGROTRACE_PROGRAM_FIELD.to_owned(),
                CHECKMILK_PROFILE => crate::config::CHECKMILK_PROGRAM_FIELD.to_owned(),
                _ => format!("Custom.Programas{name}"),
            },
            area_path: format!("{name}\\QA"),
            assigned_to: format!("{lower_name}@example.com"),
            team: name.to_owned(),
            program: name.to_owned(),
            priority: 2.0,
            inherit_iteration_path: true,
            parent_transition: Some("Test QA".to_owned()),
            reviewer_dev: format!("{lower_name}.dev@example.com"),
            reviewer_sprint: format!("{lower_name}.sprint@example.com"),
        }
    }

    fn config_with_bindings() -> Config {
        Config {
            profiles: vec![profile(AGROTRACE_PROFILE), profile(CHECKMILK_PROFILE)],
            bindings: vec![
                RepositoryProfileBinding {
                    profile: AGROTRACE_PROFILE.to_owned(),
                    organization: "org".to_owned(),
                    project: "AGROTRACE".to_owned(),
                    repository: "agrotrace".to_owned(),
                },
                RepositoryProfileBinding {
                    profile: CHECKMILK_PROFILE.to_owned(),
                    organization: "org".to_owned(),
                    project: "CHECKMILK".to_owned(),
                    repository: "checkmilk".to_owned(),
                },
            ],
            default_profile: AGROTRACE_PROFILE.to_owned(),
            ..Config::default()
        }
    }

    #[test]
    fn selection_should_match_each_bound_remote() {
        let config = config_with_bindings();
        let agrotrace = select(&config, &remote("AGROTRACE", "agrotrace")).unwrap();
        let checkmilk = select(&config, &remote("CHECKMILK", "checkmilk")).unwrap();
        assert_eq!(agrotrace.name(), AGROTRACE_PROFILE);
        assert_eq!(checkmilk.name(), CHECKMILK_PROFILE);
        assert_eq!(checkmilk.program_field, CHECKMILK_PROGRAM_FIELD);
    }

    #[test]
    fn bound_ibs_remote_selects_profile_without_onboarding() {
        let mut config = config_with_bindings();
        config.bindings[0].organization = "ibsbiosistemico".to_owned();
        let selected = select(
            &config,
            &RepositoryRemote {
                organization: "IBSBioSistemico".to_owned(),
                project: "AGROTRACE".to_owned(),
                repository: "agrotrace".to_owned(),
            },
        )
        .unwrap();
        assert_eq!(selected.name(), AGROTRACE_PROFILE);
        assert_eq!(
            selected.program_field,
            crate::config::AGROTRACE_PROGRAM_FIELD
        );
    }

    #[test]
    fn selection_is_independent_of_local_checkout_path() {
        let config = config_with_bindings();
        let first = select(&config, &remote("CHECKMILK", "checkmilk")).unwrap();
        let second = select(&config, &remote("CHECKMILK", "checkmilk")).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.remote, second.remote);
    }

    #[test]
    fn duplicate_remote_bindings_should_fail_with_profiles() {
        let mut config = config_with_bindings();
        config.bindings.push(RepositoryProfileBinding {
            profile: CHECKMILK_PROFILE.to_owned(),
            organization: "org".to_owned(),
            project: "AGROTRACE".to_owned(),
            repository: "agrotrace".to_owned(),
        });
        let error = select(&config, &remote("AGROTRACE", "agrotrace")).unwrap_err();
        let message = error.to_string();
        assert!(message.contains("org/AGROTRACE/agrotrace"), "{message}");
        assert!(message.contains(AGROTRACE_PROFILE), "{message}");
        assert!(message.contains(CHECKMILK_PROFILE), "{message}");
    }

    #[test]
    fn invalid_program_field_should_explain_reference_name_and_program_value() {
        let mut config = config_with_bindings();
        config.profiles[0].program_field = "Agrotrace".to_owned();

        let error = validate_config(&config).expect_err("field sem namespace deveria falhar");
        let message = error.to_string();
        assert!(message.contains("referenceName"), "{message}");
        assert!(message.contains("`program`"), "{message}");
    }

    #[test]
    fn selection_should_ignore_local_checkout_path() {
        let config = config_with_bindings();
        let first = select(&config, &remote("CHECKMILK", "checkmilk")).unwrap();
        let second = select(&config, &remote("CHECKMILK", "checkmilk")).unwrap();
        assert_eq!(first, second);
    }
}
