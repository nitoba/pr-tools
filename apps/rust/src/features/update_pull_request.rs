//! Preparação e escrita segura de um Pull Request existente.
//!
//! Este módulo mantém a fronteira de update separada do publisher de criação:
//! a leitura inicial fixa a autoridade remota, a releitura protege contra
//! concorrência e o PATCH só recebe título e descrição aprovados.

use std::future::Future;
use std::pin::Pin;

use tracing::info;

use crate::ai::{self, PrDescription};
use crate::azure;
use crate::azure::pull_requests::{
    PullRequest, UpdatePullRequestInput, get_pull_request, update_pull_request,
};
use crate::cli::CliOptions;
use crate::config::{self, Config};
use crate::error::{AppError, Result};
use crate::git::{self, ChangeContext, RepositoryRemote};

/// Contexto remoto e Git congelado antes da geração da proposta.
#[derive(Debug, Clone)]
pub struct UpdatePrep {
    /// Configuração resolvida, incluindo PAT e provider.
    pub config: Config,
    /// Remote Azure do clone atual.
    pub remote: RepositoryRemote,
    /// ID numérico do PR informado na CLI.
    pub pr_id: i64,
    /// Snapshot remoto usado para revisão e comparação concorrente.
    pub current: PullRequest,
    /// Contexto Git coletado com as refs remotas do PR.
    pub context: ChangeContext,
    /// Prompt completo da proposta.
    pub prompt: String,
}

/// Resultado da tentativa de atualização.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateOutcome {
    /// Conteúdo já era idêntico; nenhum PATCH foi necessário.
    NoOp,
    /// PATCH confirmado por GET posterior.
    Updated {
        /// Snapshot remoto confirmado.
        remote: PullRequest,
    },
    /// Snapshot mudou ou o resultado não pode ser confirmado.
    Conflict {
        /// Estado remoto observado.
        remote: PullRequest,
        /// Motivo apresentado na revisão.
        reason: String,
    },
    /// Resultado de transporte incerto sem reconciliação conclusiva.
    Unknown {
        /// Motivo apresentado ao usuário.
        reason: String,
    },
}

/// Cliente mínimo usado pela máquina de atualização.
pub(crate) trait UpdateGateway {
    /// Relê o mesmo PR.
    fn get<'a>(&'a self) -> Pin<Box<dyn Future<Output = Result<PullRequest>> + Send + 'a>>;

    /// Envia somente título e descrição ao mesmo PR.
    fn patch<'a>(
        &'a self,
        input: &'a UpdatePullRequestInput,
    ) -> Pin<Box<dyn Future<Output = Result<PullRequest>> + Send + 'a>>;
}

/// Implementação Azure do gateway de update.
#[derive(Clone)]
pub(crate) struct AzureUpdateGateway {
    client: azure::AzureClient,
    project: String,
    repository: String,
    id: i64,
}

impl UpdateGateway for AzureUpdateGateway {
    fn get<'a>(&'a self) -> Pin<Box<dyn Future<Output = Result<PullRequest>> + Send + 'a>> {
        Box::pin(async move {
            get_pull_request(&self.client, &self.project, &self.repository, self.id).await
        })
    }

    fn patch<'a>(
        &'a self,
        input: &'a UpdatePullRequestInput,
    ) -> Pin<Box<dyn Future<Output = Result<PullRequest>> + Send + 'a>> {
        Box::pin(async move {
            update_pull_request(
                &self.client,
                &self.project,
                &self.repository,
                self.id,
                input,
            )
            .await
        })
    }
}

/// Timeout usado nas chamadas remotas iniciadas pela TUI de update.
const UPDATE_AZURE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// Monta o gateway autenticado para a operação de update.
pub(crate) fn gateway_for(prep: &UpdatePrep) -> Result<AzureUpdateGateway> {
    azure::client_for_with_timeout(
        Some(&prep.remote),
        prep.config.azure_pat.trim(),
        UPDATE_AZURE_TIMEOUT,
    )
    .map(|client| AzureUpdateGateway {
        client,
        project: prep.remote.project.clone(),
        repository: prep.remote.repository.clone(),
        id: prep.pr_id,
    })
    .map_err(actionable_update_error)
}

/// Prepara leitura remota, elegibilidade, refs exatas e prompt.
///
/// # Errors
///
/// Retorna erro acionável quando o PAT/remote está ausente, o PR não existe,
/// não está ativo, pertence a outro repositório ou suas refs não existem no
/// clone local/`origin`.
pub async fn prepare(options: &CliOptions) -> Result<UpdatePrep> {
    let pr_id = options
        .pr
        .as_ref()
        .and_then(|id| id.as_str().parse::<i64>().ok())
        .ok_or_else(|| AppError::cli("--pr inválido: use um ID numérico."))?;
    let mut config = config::load_config()?;
    config::apply_cli_overrides(
        &mut config,
        options.provider.as_deref(),
        options.model.as_deref(),
        options.base_url.as_deref(),
        options.api_key.as_deref(),
    );
    let remote = git::origin_remote()?.ok_or_else(|| AppError::Git {
        message: "o comando desc --pr requer um remote Git do Azure DevOps".to_owned(),
    })?;
    let client = azure::client_for(Some(&remote), config.azure_pat.trim())
        .map_err(actionable_update_error)?;
    let current = get_pull_request(&client, &remote.project, &remote.repository, pr_id)
        .await
        .map_err(|error| initial_read_error(pr_id, error))?;
    validate_eligibility(&current, &remote)?;
    let context = git::collect_for_refs(&current.source_ref_name, &current.target_ref_name)?;
    let prompt = ai::build_update_prompt(
        &current.source_ref_name,
        &current.target_ref_name,
        &current.title,
        &current.description,
        &context.log,
        &context.diff,
    );
    Ok(UpdatePrep {
        config,
        remote,
        pr_id,
        current,
        context,
        prompt,
    })
}

/// Gera e valida uma proposta usando o prompt de update.
///
/// # Errors
///
/// Propaga falha de provider ou de validação do body gerado.
pub async fn generate(prep: &UpdatePrep) -> Result<PrDescription> {
    let report = |provider: &str, model: &str| {
        info!(provider, model, "tentando gerar proposta de atualização");
    };
    crate::features::describe::generate_from_prompt(
        &prep.config,
        &prep.prompt,
        &prep.context.branch,
        report,
    )
    .await
}

/// Compara todos os campos que protegem a atualização concorrente.
#[must_use]
pub fn same_snapshot(left: &PullRequest, right: &PullRequest) -> bool {
    left.status == right.status
        && left.repository == right.repository
        && left.source_ref_name == right.source_ref_name
        && left.target_ref_name == right.target_ref_name
        && left.title == right.title
        && left.description == right.description
}

/// Verifica elegibilidade contra o remote do clone atual.
///
/// # Errors
///
/// Retorna [`AppError::Git`] se o PR não estiver ativo ou não pertencer ao
/// projeto/repositório do clone atual.
pub fn validate_eligibility(pr: &PullRequest, remote: &RepositoryRemote) -> Result<()> {
    if !pr.status.eq_ignore_ascii_case("active") {
        return Err(AppError::Git {
            message: format!(
                "PR #{} não está ativo (status: {}); somente PRs active podem ser atualizados",
                pr.pull_request_id,
                if pr.status.is_empty() {
                    "desconhecido"
                } else {
                    &pr.status
                }
            ),
        });
    }
    if pr.repository.name.trim().is_empty()
        || !pr.repository.name.eq_ignore_ascii_case(&remote.repository)
        || (!pr.repository.project.name.trim().is_empty()
            && !pr
                .repository
                .project
                .name
                .eq_ignore_ascii_case(&remote.project))
    {
        return Err(AppError::Git {
            message: format!(
                "PR #{} pertence a outro repositório; execute no clone correspondente ({}/{})",
                pr.pull_request_id, remote.project, remote.repository
            ),
        });
    }
    Ok(())
}

/// Executa releitura, no-op, PATCH e GET de confirmação.
///
/// A função não oferece retry implícito. Em resultados incertos, o único
/// caminho automático é a releitura de reconciliação definida pelo contrato.
pub(crate) async fn execute_update<G: UpdateGateway>(
    gateway: &G,
    initial: &PullRequest,
    approved: &PrDescription,
) -> Result<UpdateOutcome> {
    if approved.title.trim().is_empty() {
        return Err(AppError::cli("título é obrigatório"));
    }
    if !ai::is_within_limit(&approved.body) {
        return Err(AppError::DescriptionTooLong {
            length: approved.body.chars().count(),
        });
    }

    let current = gateway.get().await.map_err(actionable_update_error)?;
    if !same_snapshot(initial, &current) {
        return Ok(UpdateOutcome::Conflict {
            remote: current,
            reason: "o PR mudou desde a revisão; releia e revise a proposta antes de atualizar"
                .to_owned(),
        });
    }
    if current.title == approved.title && current.description == approved.body {
        return Ok(UpdateOutcome::NoOp);
    }

    let input = UpdatePullRequestInput {
        title: approved.title.clone(),
        description: approved.body.clone(),
    };
    match gateway.patch(&input).await {
        Ok(_) => reconcile_after_success(gateway, approved).await,
        Err(error) if is_uncertain_patch_error(&error) => {
            reconcile_after_uncertain(gateway, approved, &error).await
        }
        Err(error) => Err(actionable_update_error(error)),
    }
}

async fn reconcile_after_success<G: UpdateGateway>(
    gateway: &G,
    approved: &PrDescription,
) -> Result<UpdateOutcome> {
    match gateway.get().await {
        Ok(remote) if remote.title == approved.title && remote.description == approved.body => {
            Ok(UpdateOutcome::Updated { remote })
        }
        Ok(remote) => Ok(UpdateOutcome::Conflict {
            remote,
            reason: "o PATCH respondeu sucesso, mas o GET confirmatório divergiu".to_owned(),
        }),
        Err(error) => Ok(UpdateOutcome::Unknown {
            reason: format!(
                "PATCH enviado, mas não foi possível confirmar o resultado por GET: {}",
                actionable_update_error(error)
            ),
        }),
    }
}

async fn reconcile_after_uncertain<G: UpdateGateway>(
    gateway: &G,
    approved: &PrDescription,
    error: &AppError,
) -> Result<UpdateOutcome> {
    match gateway.get().await {
        Ok(remote) if remote.title == approved.title && remote.description == approved.body => {
            Ok(UpdateOutcome::Updated { remote })
        }
        Ok(remote) => Ok(UpdateOutcome::Conflict {
            remote,
            reason: format!(
                "resultado incerto reconciliado sem confirmar o conteúdo aprovado: {error}"
            ),
        }),
        Err(reconciliation_error) => Ok(UpdateOutcome::Unknown {
            reason: format!(
                "resultado incerto; a reconciliação também falhou: {}",
                actionable_update_error(reconciliation_error)
            ),
        }),
    }
}

fn is_uncertain_patch_error(error: &AppError) -> bool {
    match error {
        AppError::Http(_) => true,
        AppError::Azure { status, .. } => {
            *status < 300 || *status == 408 || *status == 429 || *status >= 500
        }
        _ => false,
    }
}

fn initial_read_error(id: i64, error: AppError) -> AppError {
    match actionable_update_error(error) {
        AppError::Azure { status: 404, .. } => AppError::Azure {
            status: 404,
            message: format!(
                "PR #{id} não encontrado no Azure DevOps; confira o ID e o clone correspondente"
            ),
        },
        error => error,
    }
}

/// Torna autenticação e permissão acionáveis sem expor o PAT.
#[must_use]
pub fn actionable_update_error(error: AppError) -> AppError {
    match error {
        AppError::Config { message }
            if message.to_ascii_lowercase().contains("pat")
                && (message.to_ascii_lowercase().contains("não configurado")
                    || message.to_ascii_lowercase().contains("nao configurado")) =>
        {
            AppError::Config {
                message: "PAT do Azure DevOps não configurado; verifique PAT/permissão e rode `prt doctor`"
                    .to_owned(),
            }
        }
        AppError::Config { message } => AppError::Config { message },
        AppError::Azure { status, .. } if status == 401 || status == 403 => AppError::Azure {
            status,
            message: format!(
                "Azure DevOps recusou a atualização; verifique PAT e permissão de Pull Requests e rode `prt doctor` (HTTP {status})"
            ),
        },
        error => error,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};

    use super::*;
    use crate::azure::pull_requests::{PullRequestProject, PullRequestRepository};

    fn pull_request() -> PullRequest {
        PullRequest {
            pull_request_id: 42,
            title: "Título atual".to_owned(),
            description: "Descrição atual".to_owned(),
            source_ref_name: "refs/heads/feature/42".to_owned(),
            target_ref_name: "refs/heads/dev".to_owned(),
            status: "active".to_owned(),
            repository: PullRequestRepository {
                id: "repo-id".to_owned(),
                name: "repo".to_owned(),
                project: PullRequestProject {
                    name: "project".to_owned(),
                },
            },
        }
    }

    fn remote() -> RepositoryRemote {
        RepositoryRemote {
            organization: "org".to_owned(),
            project: "project".to_owned(),
            repository: "repo".to_owned(),
        }
    }

    fn approved() -> PrDescription {
        PrDescription {
            title: "Título aprovado".to_owned(),
            body: "Body aprovado\n- [ ] validar".to_owned(),
        }
    }

    #[derive(Clone)]
    struct FakeGateway {
        gets: Arc<Mutex<VecDeque<Result<PullRequest>>>>,
        patch_result: Arc<Mutex<Option<Result<PullRequest>>>>,
        patches: Arc<Mutex<Vec<UpdatePullRequestInput>>>,
    }

    impl FakeGateway {
        fn new(gets: Vec<Result<PullRequest>>, patch_result: Result<PullRequest>) -> Self {
            Self {
                gets: Arc::new(Mutex::new(gets.into_iter().collect())),
                patch_result: Arc::new(Mutex::new(Some(patch_result))),
                patches: Arc::new(Mutex::new(Vec::new())),
            }
        }

        fn patch_count(&self) -> usize {
            self.patches.lock().expect("mutex não envenenado").len()
        }
    }

    impl UpdateGateway for FakeGateway {
        fn get<'a>(&'a self) -> Pin<Box<dyn Future<Output = Result<PullRequest>> + Send + 'a>> {
            Box::pin(async move {
                self.gets
                    .lock()
                    .expect("mutex não envenenado")
                    .pop_front()
                    .unwrap_or_else(|| {
                        Err(AppError::Git {
                            message: "GET não roteado".to_owned(),
                        })
                    })
            })
        }

        fn patch<'a>(
            &'a self,
            input: &'a UpdatePullRequestInput,
        ) -> Pin<Box<dyn Future<Output = Result<PullRequest>> + Send + 'a>> {
            let input = input.clone();
            Box::pin(async move {
                self.patches
                    .lock()
                    .expect("mutex não envenenado")
                    .push(input);
                self.patch_result
                    .lock()
                    .expect("mutex não envenenado")
                    .take()
                    .unwrap_or_else(|| {
                        Err(AppError::Git {
                            message: "PATCH não roteado".to_owned(),
                        })
                    })
            })
        }
    }

    #[test]
    fn update_should_reject_non_active_pull_requests_before_generation() {
        let mut pr = pull_request();
        pr.status = "completed".to_owned();
        let error = validate_eligibility(&pr, &remote()).unwrap_err();
        assert!(error.to_string().contains("somente PRs active"));
    }

    #[test]
    fn update_should_reject_pull_request_from_another_repository() {
        let mut pr = pull_request();
        pr.repository.name = "outro-repo".to_owned();
        let error = validate_eligibility(&pr, &remote()).unwrap_err();
        assert!(error.to_string().contains("clone correspondente"));
    }

    #[test]
    fn initial_update_read_failure_should_not_create_proposal_or_write() {
        let error = initial_read_error(
            42,
            AppError::Azure {
                status: 404,
                message: "missing".to_owned(),
            },
        );
        assert!(error.to_string().contains("PR #42 não encontrado"));
    }

    #[test]
    fn update_prompt_and_generation_should_include_remote_snapshot_and_validate_proposal() {
        let prompt = ai::build_update_prompt(
            "refs/heads/feature/42",
            "refs/heads/dev",
            "Título atual",
            "Descrição atual",
            "abc123 commit",
            "diff --git a/file b/file",
        );
        assert!(prompt.contains("refs/heads/feature/42"));
        assert!(prompt.contains("refs/heads/dev"));
        assert!(prompt.contains("Título atual"));
        assert!(prompt.contains("Descrição atual"));
        assert!(prompt.contains("abc123 commit"));
        assert!(prompt.contains("diff --git"));
    }

    #[tokio::test]
    async fn update_should_block_patch_when_any_remote_snapshot_field_changed() {
        let initial = pull_request();
        let mut changed = initial.clone();
        changed.title.push('!');
        let gateway = FakeGateway::new(vec![Ok(changed)], Ok(initial.clone()));
        let result = execute_update(&gateway, &initial, &approved())
            .await
            .unwrap();
        assert!(matches!(result, UpdateOutcome::Conflict { .. }));
        assert_eq!(gateway.patch_count(), 0);
    }

    #[tokio::test]
    async fn update_should_confirm_noop_without_patch() {
        let initial = pull_request();
        let proposed = PrDescription {
            title: initial.title.clone(),
            body: initial.description.clone(),
        };
        let gateway = FakeGateway::new(vec![Ok(initial.clone())], Ok(initial.clone()));
        let result = execute_update(&gateway, &initial, &proposed).await.unwrap();
        assert_eq!(result, UpdateOutcome::NoOp);
        assert_eq!(gateway.patch_count(), 0);
    }

    #[tokio::test]
    async fn update_should_send_minimal_json_patch_to_same_pull_request() {
        let initial = pull_request();
        let mut confirmed = initial.clone();
        confirmed.title = approved().title;
        confirmed.description = approved().body;
        let gateway = FakeGateway::new(vec![Ok(initial), Ok(confirmed.clone())], Ok(confirmed));
        let result = execute_update(&gateway, &pull_request(), &approved())
            .await
            .unwrap();
        assert!(matches!(result, UpdateOutcome::Updated { .. }));
        let patches = gateway.patches.lock().expect("mutex não envenenado");
        assert_eq!(patches.len(), 1);
        assert_eq!(patches[0].title, "Título aprovado");
        assert_eq!(patches[0].description, "Body aprovado\n- [ ] validar");
    }

    #[tokio::test]
    async fn update_should_confirm_only_after_exact_post_patch_get() {
        let initial = pull_request();
        let mut remote_after = initial.clone();
        remote_after.title = approved().title;
        remote_after.description = "divergente".to_owned();
        let gateway = FakeGateway::new(vec![Ok(initial.clone()), Ok(remote_after)], Ok(initial));
        let result = execute_update(&gateway, &pull_request(), &approved())
            .await
            .unwrap();
        assert!(matches!(result, UpdateOutcome::Conflict { .. }));
    }

    #[tokio::test]
    async fn update_should_reconcile_success_timeout_transport_and_invalid_response() {
        let initial = pull_request();
        let timeout = AppError::Azure {
            status: 504,
            message: "timeout".to_owned(),
        };
        let mut confirmed = initial.clone();
        confirmed.title = approved().title;
        confirmed.description = approved().body;
        let gateway = FakeGateway::new(
            vec![Ok(initial.clone()), Ok(confirmed.clone())],
            Err(timeout),
        );
        let result = execute_update(&gateway, &initial, &approved())
            .await
            .unwrap();
        assert!(matches!(result, UpdateOutcome::Updated { .. }));
        assert_eq!(gateway.patch_count(), 1);

        let invalid = AppError::Azure {
            status: 200,
            message: "resposta inválida".to_owned(),
        };
        assert!(is_uncertain_patch_error(&invalid));
        assert!(!is_uncertain_patch_error(&AppError::Azure {
            status: 400,
            message: "bad request".to_owned(),
        }));
    }

    #[test]
    fn update_authorization_failures_should_be_actionable_and_write_nothing() {
        let error = actionable_update_error(AppError::Azure {
            status: 403,
            message: "forbidden".to_owned(),
        });
        let message = error.to_string();
        assert!(message.contains("PAT"));
        assert!(message.contains("permissão"));
        assert!(message.contains("prt doctor"));
    }

    #[tokio::test]
    async fn update_confirmation_should_have_no_creation_publisher_path() {
        let initial = pull_request();
        let mut confirmed = initial.clone();
        confirmed.title = approved().title;
        confirmed.description = approved().body;
        let gateway = FakeGateway::new(
            vec![Ok(initial.clone()), Ok(confirmed.clone())],
            Ok(confirmed),
        );

        let result = execute_update(&gateway, &initial, &approved())
            .await
            .expect("operação de update deve concluir");

        assert!(matches!(result, UpdateOutcome::Updated { .. }));
        assert_eq!(gateway.patch_count(), 1);
    }
}
