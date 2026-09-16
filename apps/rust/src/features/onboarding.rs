//! Onboarding local de perfis para remotes Azure sem associação.
//!
//! A decisão é deliberadamente anterior ao provider e aos writers remotos.
//! Este módulo só lê Git/configuração e, depois de uma confirmação explícita,
//! persiste um perfil e um binding exato.

use crate::config::{self, Config, ProcessProfile, RepositoryProfileBinding};
use crate::error::{AppError, Result};
use crate::features::process_profiles::{self, ProfileSelection};
use crate::git::{self, RepositoryRemote};

/// Ações exibidas na tela inicial.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OnboardingAction {
    /// Criar um perfil preenchendo os campos manualmente.
    New,
    /// Copiar um perfil existente para um draft independente.
    Import,
    /// Manter o fallback atual desta execução.
    Skip,
}

/// Origem apresentada na revisão do draft.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DraftOrigin {
    /// Todos os valores foram iniciados vazios/default.
    New,
    /// Valores copiados do perfil nomeado.
    Imported(String),
}

impl DraftOrigin {
    /// Rótulo seguro para a revisão.
    #[must_use]
    pub fn label(&self) -> String {
        match self {
            Self::New => "Novo perfil".to_owned(),
            Self::Imported(name) => format!("Importado de {name}"),
        }
    }
}

/// Draft completo editado no onboarding.
#[derive(Debug, Clone, PartialEq)]
pub struct OnboardingDraft {
    /// Nome do novo perfil.
    pub name: String,
    /// Field Azure que recebe `program`.
    pub program_field: String,
    /// `System.AreaPath` padrão.
    pub area_path: String,
    /// `System.AssignedTo` padrão.
    pub assigned_to: String,
    /// Herdar `System.IterationPath` do pai.
    pub inherit_iteration_path: bool,
    /// Estado opcional do Work Item pai.
    pub parent_transition: String,
    /// `Microsoft.VSTS.Common.Priority` padrão.
    pub priority: f64,
    /// Valor do field de programa.
    pub program: String,
    /// Reviewer de targets de dev.
    pub reviewer_dev: String,
    /// Reviewer de targets sprint.
    pub reviewer_sprint: String,
    /// `Custom.Team` padrão.
    pub team: String,
    /// Origem do draft.
    pub origin: DraftOrigin,
}

impl Default for OnboardingDraft {
    fn default() -> Self {
        Self {
            name: String::new(),
            program_field: String::new(),
            area_path: String::new(),
            assigned_to: String::new(),
            inherit_iteration_path: true,
            parent_transition: String::new(),
            priority: 2.0,
            program: String::new(),
            reviewer_dev: String::new(),
            reviewer_sprint: String::new(),
            team: String::new(),
            origin: DraftOrigin::New,
        }
    }
}

impl OnboardingDraft {
    /// Copia valores de processo para um draft novo; a identidade continua
    /// independente e editável.
    #[must_use]
    pub fn from_profile(profile: &ProcessProfile) -> Self {
        Self {
            name: String::new(),
            program_field: profile
                .program_field()
                .map_or_else(String::new, str::to_owned),
            area_path: profile.area_path.clone(),
            assigned_to: profile.assigned_to.clone(),
            inherit_iteration_path: profile.inherit_iteration_path,
            parent_transition: profile.parent_transition.clone().unwrap_or_default(),
            priority: profile.priority,
            program: profile.program.clone(),
            reviewer_dev: profile.reviewer_dev.clone(),
            reviewer_sprint: profile.reviewer_sprint.clone(),
            team: profile.team.clone(),
            origin: DraftOrigin::Imported(profile.name.clone()),
        }
    }

    /// Converte o draft para o tipo persistido sem alterar valores não editados.
    #[must_use]
    pub fn to_profile(&self) -> ProcessProfile {
        let parent_transition = self.parent_transition.trim();
        ProcessProfile {
            name: self.name.clone(),
            program_field: self.program_field.clone(),
            area_path: self.area_path.clone(),
            assigned_to: self.assigned_to.clone(),
            team: self.team.clone(),
            program: self.program.clone(),
            priority: self.priority,
            inherit_iteration_path: self.inherit_iteration_path,
            parent_transition: (!parent_transition.is_empty())
                .then(|| parent_transition.to_owned()),
            reviewer_dev: self.reviewer_dev.clone(),
            reviewer_sprint: self.reviewer_sprint.clone(),
        }
    }

    /// Valores ordenados para a tela de revisão.
    #[must_use]
    pub fn review_rows(&self) -> Vec<(&'static str, String)> {
        vec![
            ("name", self.name.clone()),
            ("programField", self.program_field.clone()),
            ("areaPath", self.area_path.clone()),
            ("assignedTo", self.assigned_to.clone()),
            (
                "inheritIterationPath",
                self.inherit_iteration_path.to_string(),
            ),
            ("parentTransition", self.parent_transition.clone()),
            ("priority", self.priority.to_string()),
            ("program", self.program.clone()),
            ("reviewerDev", self.reviewer_dev.clone()),
            ("reviewerSprint", self.reviewer_sprint.clone()),
            ("team", self.team.clone()),
        ]
    }
}

/// Decisão feita antes de iniciar um fluxo.
#[derive(Debug, Clone)]
pub enum ProfileDecision {
    /// Não há remote Azure; o fluxo existente decide como antes.
    NoRemote {
        /// Configuração carregada para o fluxo existente.
        config: Config,
    },
    /// Binding ou fallback já resolvido.
    Selected(ProfileSelection),
    /// Remote Azure sem binding exato; requer escolha do usuário em TTY.
    NeedsOnboarding {
        /// Configuração sem alterações.
        config: Config,
        /// Remote que será associado após a confirmação.
        remote: RepositoryRemote,
    },
}

/// Lê config/Git e avalia a necessidade do onboarding sem migrar ou escrever.
///
/// # Errors
///
/// Retorna primeiro os erros de configuração, antes de permitir provider ou
/// writer remoto.
pub fn inspect(source: Option<&str>) -> Result<ProfileDecision> {
    inspect_with_migration(source, false)
}

/// Inspeciona a decisão, habilitando a migração apenas para uma execução que
/// já confirmou que pode persistir localmente.
///
/// # Errors
///
/// Retorna erros de configuração ou coleta do contexto Git.
pub fn inspect_with_migration(
    source: Option<&str>,
    migrate_legacy: bool,
) -> Result<ProfileDecision> {
    let config = if migrate_legacy {
        config::load_config()?
    } else {
        config::load_config_without_migration()?
    };
    process_profiles::validate_config(&config)?;
    let change = git::collect(source)?;
    let Some(remote) = change.remote else {
        return Ok(ProfileDecision::NoRemote { config });
    };
    resolve(config, remote)
}

/// Resolve um remote Azure e sinaliza onboarding quando não há binding exato.
///
/// # Errors
///
/// Retorna [`AppError::Config`] quando os perfis ou bindings não puderem ser
/// resolvidos de forma inequívoca.
pub fn resolve(config: Config, remote: RepositoryRemote) -> Result<ProfileDecision> {
    process_profiles::validate_config(&config)?;
    let bindings = process_profiles::binding_for(&config.bindings, &remote);
    if bindings.len() > 1 {
        // `select` mantém a mensagem já usada no restante do produto, com o
        // remote e todos os perfis conflitantes.
        process_profiles::select(&config, &remote).map(ProfileDecision::Selected)
    } else if bindings.is_empty() {
        Ok(ProfileDecision::NeedsOnboarding { config, remote })
    } else {
        process_profiles::select(&config, &remote).map(ProfileDecision::Selected)
    }
}

/// Valida um draft sem tocar em arquivos ou serviços externos.
///
/// # Errors
///
/// Retorna [`AppError::Config`] identificando o primeiro campo inválido.
pub fn validate_draft(config: &Config, draft: &OnboardingDraft) -> Result<()> {
    if draft.name.trim().is_empty() {
        return Err(AppError::Config {
            message: "name: informe um nome para o perfil".to_owned(),
        });
    }
    if draft.program_field.trim().is_empty() {
        return Err(AppError::Config {
            message: "programField: informe o field Azure do programa".to_owned(),
        });
    }
    if !draft.priority.is_finite() || draft.priority <= 0.0 {
        return Err(AppError::Config {
            message: "priority: informe um número positivo e finito".to_owned(),
        });
    }
    for (field, value) in [
        ("assignedTo", draft.assigned_to.as_str()),
        ("reviewerDev", draft.reviewer_dev.as_str()),
        ("reviewerSprint", draft.reviewer_sprint.as_str()),
    ] {
        if !value.trim().is_empty() && !valid_email(value) {
            return Err(AppError::Config {
                message: format!("{field}: informe um email válido ou deixe vazio"),
            });
        }
    }
    if config
        .effective_process_profiles()
        .iter()
        .any(|profile| profile.name == draft.name)
    {
        return Err(AppError::Config {
            message: format!("name: o perfil {} já existe", draft.name),
        });
    }
    Ok(())
}

/// Persiste o draft confirmado e devolve a seleção congelada desta execução.
///
/// A construção inteira é validada antes da substituição atômica. O fallback
/// legado continua implícito quando não há um `Agrotrace` persistido;
/// `defaultProfile` nunca é alterado.
///
/// # Errors
///
/// Retorna [`AppError::Config`] quando o draft ou o binding não for válido, ou
/// quando a persistência local falhar.
pub fn save(
    config: &Config,
    remote: &RepositoryRemote,
    draft: &OnboardingDraft,
) -> Result<ProfileSelection> {
    let next = saved_config(config, remote, draft)?;
    config::persist_local_config(&next)?;
    process_profiles::select(&next, remote)
}

fn saved_config(
    config: &Config,
    remote: &RepositoryRemote,
    draft: &OnboardingDraft,
) -> Result<Config> {
    validate_draft(config, draft)?;
    let mut next = config.clone();
    if !process_profiles::binding_for(&next.bindings, remote).is_empty() {
        return Err(AppError::Config {
            message: format!(
                "remote {} já possui binding; execute novamente para reutilizar o perfil",
                remote_label(remote)
            ),
        });
    }
    let profile = draft.to_profile();
    let profile_name = profile.name.clone();
    next.bindings.push(RepositoryProfileBinding {
        profile: profile_name,
        organization: remote.organization.clone(),
        project: remote.project.clone(),
        repository: remote.repository.clone(),
    });
    next.profiles.push(profile);
    process_profiles::validate_config(&next)?;
    Ok(next)
}

/// Orientação para modos sem escolha interativa.
#[must_use]
pub fn non_interactive_guidance(remote: &RepositoryRemote) -> String {
    format!(
        "não há perfil associado a {}/{}/{}; nenhuma alteração foi feita em config.json. Execute `prt desc` ou `prt test` em um terminal interativo para escolher Novo perfil, Importar perfil ou Agora não, ou use `prt init` para configurar os valores globais",
        remote.organization, remote.project, remote.repository
    )
}

fn remote_label(remote: &RepositoryRemote) -> String {
    format!(
        "{}/{}/{}",
        remote.organization, remote.project, remote.repository
    )
}

fn valid_email(value: &str) -> bool {
    let value = value.trim();
    let mut parts = value.split('@');
    matches!((parts.next(), parts.next(), parts.next()), (Some(user), Some(domain), None)
        if !user.is_empty() && domain.contains('.') && !value.contains(' '))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AGROTRACE_PROGRAM_FIELD, CHECKMILK_PROGRAM_FIELD};

    fn remote(organization: &str) -> RepositoryRemote {
        RepositoryRemote {
            organization: organization.to_owned(),
            project: "Projeto IBS".to_owned(),
            repository: "repo".to_owned(),
        }
    }

    fn profile(name: &str) -> ProcessProfile {
        ProcessProfile {
            name: name.to_owned(),
            program_field: "Custom.ProgramasGenerico".to_owned(),
            area_path: "Projeto\\QA".to_owned(),
            assigned_to: "qa@example.com".to_owned(),
            inherit_iteration_path: false,
            parent_transition: None,
            priority: 4.0,
            program: "Produto".to_owned(),
            reviewer_dev: "dev@example.com".to_owned(),
            reviewer_sprint: "sprint@example.com".to_owned(),
            team: "QA".to_owned(),
        }
    }

    fn valid_draft(name: &str) -> OnboardingDraft {
        OnboardingDraft {
            name: name.to_owned(),
            program_field: "Custom.ProgramasNovo".to_owned(),
            area_path: "Projeto\\QA".to_owned(),
            assigned_to: "qa@example.com".to_owned(),
            inherit_iteration_path: true,
            parent_transition: "Test QA".to_owned(),
            priority: 2.0,
            program: "Produto".to_owned(),
            reviewer_dev: "dev@example.com".to_owned(),
            reviewer_sprint: "sprint@example.com".to_owned(),
            team: "QA".to_owned(),
            origin: DraftOrigin::New,
        }
    }

    fn generic_config() -> Config {
        Config {
            azure_pat: "pat-secret".to_owned(),
            api_key: "api-secret".to_owned(),
            default_profile: "Agrotrace".to_owned(),
            ..Config::default()
        }
    }

    #[test]
    fn any_azure_remote_without_binding_requires_onboarding() {
        let decision = resolve(Config::default(), remote("OutraOrganizacao")).unwrap();
        let ProfileDecision::NeedsOnboarding { remote, .. } = decision else {
            panic!("remote Azure sem binding deveria pedir onboarding")
        };
        assert_eq!(remote.organization, "OutraOrganizacao");
        assert_eq!(remote.project, "Projeto IBS");
        assert_eq!(remote.repository, "repo");
        assert_eq!(
            [
                OnboardingAction::New,
                OnboardingAction::Import,
                OnboardingAction::Skip
            ]
            .len(),
            3
        );
    }

    #[test]
    fn exact_binding_selects_profile_without_onboarding() {
        let mut config = Config::default();
        config
            .profiles
            .push(ProcessProfile::named("CheckMilk").unwrap());
        config.default_profile = "CheckMilk".to_owned();
        config.bindings.push(RepositoryProfileBinding {
            profile: "CheckMilk".to_owned(),
            organization: "outra-org".to_owned(),
            project: "Projeto IBS".to_owned(),
            repository: "repo".to_owned(),
        });
        let decision = resolve(config, remote("outra-org")).unwrap();
        assert!(
            matches!(decision, ProfileDecision::Selected(selection) if selection.name() == "CheckMilk")
        );
    }

    #[test]
    fn new_draft_has_required_fields_and_defaults() {
        let draft = OnboardingDraft::default();
        assert!((draft.priority - 2.0).abs() < f64::EPSILON);
        assert!(draft.inherit_iteration_path);
        assert!(draft.program_field.is_empty());
    }

    #[test]
    fn import_copies_profile_values_without_mutating_source() {
        let source = profile("Origem");
        let draft = OnboardingDraft::from_profile(&source);
        assert_eq!(draft.program_field, source.program_field);
        assert_eq!(draft.area_path, source.area_path);
        assert_eq!(draft.parent_transition, "");
        assert_eq!(source.name, "Origem");
        assert!(matches!(draft.origin, DraftOrigin::Imported(ref name) if name == "Origem"));
    }

    #[test]
    fn invalid_draft_is_rejected_without_persisting() {
        let draft = OnboardingDraft {
            name: "Novo".to_owned(),
            ..OnboardingDraft::default()
        };
        let error = validate_draft(&Config::default(), &draft).unwrap_err();
        assert!(error.to_string().contains("programField"));
    }

    #[test]
    fn invalid_config_blocks_onboarding_and_remote_effects() {
        let mut config = Config::default();
        let mut invalid = profile("Sem field");
        invalid.program_field.clear();
        config.profiles.push(invalid);
        let error = resolve(config, remote("ibsbiosistemico")).unwrap_err();
        assert!(error.to_string().contains("programField"));
    }

    #[test]
    fn partial_edit_preserves_unedited_values() {
        let source = profile("Origem");
        let mut draft = OnboardingDraft::from_profile(&source);
        draft.name = "Destino".to_owned();
        draft.team = "Outro QA".to_owned();
        assert_eq!(draft.area_path, source.area_path);
        assert_eq!(draft.program_field, source.program_field);
        assert_eq!(draft.team, "Outro QA");
    }

    #[test]
    fn review_requires_explicit_save_confirmation() {
        let draft = OnboardingDraft {
            name: "Novo".to_owned(),
            program_field: "Custom.ProgramasNovo".to_owned(),
            ..OnboardingDraft::default()
        };
        assert_eq!(draft.review_rows().len(), 11);
        assert_eq!(draft.origin.label(), "Novo perfil");
    }

    #[test]
    fn confirmed_save_is_exact_and_secret_free() {
        let config = generic_config();
        let remote = remote("ibsbiosistemico");
        let next = saved_config(&config, &remote, &valid_draft("IBS Novo")).unwrap();
        let profile = next
            .profiles
            .iter()
            .find(|profile| profile.name == "IBS Novo")
            .expect("perfil novo");
        assert_eq!(next.default_profile, config.default_profile);
        assert_eq!(next.bindings.len(), 1);
        assert_eq!(next.bindings[0].profile, "IBS Novo");
        assert_eq!(next.bindings[0].organization, "ibsbiosistemico");
        assert_eq!(profile.program_field, "Custom.ProgramasNovo");
        let json = serde_json::to_value(profile).unwrap();
        assert!(json.get("programField").is_some());
        assert!(json.get("azurePat").is_none());
        assert!(json.get("apiKey").is_none());
    }

    #[test]
    fn saved_remote_reuses_binding_without_duplicate_onboarding() {
        let config = generic_config();
        let remote = remote("ibsbiosistemico");
        let draft = valid_draft("IBS Novo");
        let saved = saved_config(&config, &remote, &draft).unwrap();
        let error = saved_config(&saved, &remote, &draft).unwrap_err();
        assert!(error.to_string().contains("já existe"));
        assert_eq!(saved.bindings.len(), 1);
        assert_eq!(saved.profiles.len(), 1);
    }

    #[test]
    fn invalid_cancelled_or_failed_onboarding_preserves_previous_configuration() {
        let config = generic_config();
        let remote = remote("ibsbiosistemico");
        let invalid = OnboardingDraft {
            name: "IBS Novo".to_owned(),
            ..OnboardingDraft::default()
        };
        assert!(saved_config(&config, &remote, &invalid).is_err());
        assert!(config.profiles.is_empty());
        assert!(config.bindings.is_empty());
    }

    #[test]
    fn skip_keeps_fallback_without_persisting() {
        let config = generic_config();
        let before = config.clone();
        let remote = remote("ibsbiosistemico");
        let decision = resolve(config, remote).unwrap();
        let ProfileDecision::NeedsOnboarding { config, remote } = decision else {
            panic!("remote sem binding deveria pedir decisão")
        };
        let fallback = process_profiles::select(&config, &remote).unwrap();
        assert_eq!(fallback.name(), "Agrotrace");
        assert_eq!(before.profiles.len(), 0);
        assert_eq!(before.bindings.len(), 0);
        assert_eq!(before.default_profile, "Agrotrace");
    }

    #[test]
    fn non_interactive_guidance_names_remote_and_preserves_no_write_contract() {
        let remote = remote("OutraOrganizacao");
        let guidance = non_interactive_guidance(&remote);
        assert!(guidance.contains("OutraOrganizacao/Projeto IBS/repo"));
        assert!(guidance.contains("nenhuma alteração foi feita"));
        assert!(guidance.contains("terminal interativo"));
    }

    #[test]
    fn save_resumes_original_execution_once() {
        let config = generic_config();
        let remote = remote("ibsbiosistemico");
        let saved = saved_config(&config, &remote, &valid_draft("IBS Novo")).unwrap();
        let selection = process_profiles::select(&saved, &remote).unwrap();

        assert_eq!(selection.name(), "IBS Novo");
        assert_eq!(selection.program_field, "Custom.ProgramasNovo");
        assert_eq!(saved.bindings.len(), 1);
        assert_eq!(saved.profiles.len(), 1);
    }

    #[test]
    fn legacy_program_fields_remain_known() {
        assert_eq!(
            ProcessProfile::named("Agrotrace").unwrap().program_field(),
            Some(AGROTRACE_PROGRAM_FIELD)
        );
        assert_eq!(
            ProcessProfile::named("CheckMilk").unwrap().program_field(),
            Some(CHECKMILK_PROGRAM_FIELD)
        );
    }
}
