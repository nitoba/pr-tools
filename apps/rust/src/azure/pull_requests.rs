//! Pull Requests do Azure DevOps — espelha `infrastructure/azure/pull_requests.dart`.
//!
//! Leituras usadas pelo `prt test` e `prt desc`: busca do PR, IDs dos Work
//! Items vinculados, resumo textual das alterações e criação/publicação de PRs
//! (espelha `pull_requests.dart`, `pull_request_publisher.dart` e
//! `identities.dart`).

use serde::{Deserialize, Serialize};

use crate::azure::{AzureClient, encode_segment};
use crate::error::Result;

/// Número máximo de alterações resumidas (espelha `$top=200` do Dart).
pub const MAX_CHANGES: usize = 200;

/// Pull Request mínimo (espelha `AzurePullRequest` do Dart).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct PullRequest {
    /// ID do PR (`pullRequestId`).
    #[serde(rename = "pullRequestId")]
    pub pull_request_id: i64,
    /// Título.
    #[serde(default, deserialize_with = "string_or_empty")]
    pub title: String,
    /// Descrição (pode vir `null`).
    #[serde(default, deserialize_with = "string_or_empty")]
    pub description: String,
    /// Branch de origem (`sourceRefName`).
    #[serde(
        default,
        deserialize_with = "string_or_empty",
        rename = "sourceRefName"
    )]
    pub source_ref_name: String,
    /// Branch de destino (`targetRefName`).
    #[serde(
        default,
        deserialize_with = "string_or_empty",
        rename = "targetRefName"
    )]
    pub target_ref_name: String,
    /// Estado remoto (`active`, `completed` ou `abandoned`).
    #[serde(default, deserialize_with = "string_or_empty")]
    pub status: String,
    /// Repositório que contém o target do PR.
    #[serde(default)]
    pub repository: PullRequestRepository,
}

/// Identidade mínima do repositório retornado no snapshot do PR.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct PullRequestRepository {
    /// ID do repositório.
    #[serde(default, deserialize_with = "string_or_empty")]
    pub id: String,
    /// Nome do repositório.
    #[serde(default, deserialize_with = "string_or_empty")]
    pub name: String,
    /// Projeto do repositório, quando retornado pelo Azure.
    #[serde(default)]
    pub project: PullRequestProject,
}

/// Identidade mínima do projeto retornado no snapshot do PR.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct PullRequestProject {
    /// Nome do projeto.
    #[serde(default, deserialize_with = "string_or_empty")]
    pub name: String,
}

/// Alteração de arquivo (`changeType` + `item.path`).
#[derive(Debug, Clone, Deserialize)]
struct PullRequestChange {
    /// Tipo (`edit`, `add`, `delete`, ...).
    #[serde(default, deserialize_with = "string_or_empty", rename = "changeType")]
    change_type: String,
    /// Item alterado (ausente vira path vazio, como no Dart).
    #[serde(default)]
    item: ChangeItem,
}

/// Item alterado dentro de uma change entry.
#[derive(Debug, Clone, Default, Deserialize)]
struct ChangeItem {
    /// Caminho do arquivo.
    #[serde(default, deserialize_with = "string_or_empty")]
    path: String,
}

/// Iteração do PR (só o `id` interessa).
#[derive(Debug, Clone, Deserialize)]
struct Iteration {
    /// ID da iteração.
    id: i64,
}

/// Resposta `{"value": [...]}` das listagens de iterações e work items.
#[derive(Debug, Deserialize)]
#[serde(bound(deserialize = "T: Deserialize<'de>"))]
struct ValueList<T> {
    /// Itens.
    #[serde(default)]
    value: Vec<T>,
}

/// Referência de Work Item vinculado (`{"id": ...}`).
#[derive(Debug, Deserialize)]
struct LinkedRef {
    /// ID.
    id: i64,
}

/// Resposta `{"changeEntries": [...]}` do endpoint de changes.
#[derive(Debug, Deserialize)]
struct ChangesList {
    /// Alterações.
    #[serde(default, rename = "changeEntries")]
    entries: Vec<PullRequestChange>,
}

/// Item resumido retornado pela listagem de Pull Requests.
#[derive(Debug, Clone, Deserialize)]
struct PullRequestListItem {
    /// ID do PR.
    #[serde(default, rename = "pullRequestId")]
    pull_request_id: i64,
    /// Título.
    #[serde(default, deserialize_with = "string_or_empty")]
    title: String,
    /// Branch de origem.
    #[serde(
        default,
        deserialize_with = "string_or_empty",
        rename = "sourceRefName"
    )]
    source_ref_name: String,
    /// Branch de destino.
    #[serde(
        default,
        deserialize_with = "string_or_empty",
        rename = "targetRefName"
    )]
    target_ref_name: String,
    /// URL da API.
    #[serde(default, deserialize_with = "string_or_empty")]
    url: String,
    /// URL navegável.
    #[serde(default, deserialize_with = "string_or_empty", rename = "webUrl")]
    web_url: String,
    /// Data de criação.
    #[serde(default, deserialize_with = "string_or_empty", rename = "creationDate")]
    creation_date: String,
    /// Links retornados pelo Azure, incluindo o link web navegável.
    #[serde(default, rename = "_links")]
    links: PullRequestLinks,
}

/// Links parciais de um Pull Request.
#[derive(Debug, Clone, Default, Deserialize)]
struct PullRequestLinks {
    /// Link web.
    #[serde(default)]
    web: PullRequestLink,
}

/// Um link HTTP retornado pelo Azure.
#[derive(Debug, Clone, Default, Deserialize)]
struct PullRequestLink {
    /// URL do link.
    #[serde(default, deserialize_with = "string_or_empty")]
    href: String,
}

/// Resposta da listagem de Pull Requests.
#[derive(Debug, Deserialize)]
struct PullRequestList {
    /// PRs encontrados.
    #[serde(default)]
    value: Vec<PullRequestListItem>,
}

/// Desserializa texto opcional tratando `null`/ausente como `""`.
///
/// Espelha `_string` do Dart (que retorna `""` para `null`).
fn string_or_empty<'de, D>(deserializer: D) -> std::result::Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct StringOrEmpty;

    impl<'de> serde::de::Visitor<'de> for StringOrEmpty {
        type Value = String;

        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("texto ou nulo")
        }

        fn visit_none<E: serde::de::Error>(self) -> std::result::Result<String, E> {
            Ok(String::new())
        }

        fn visit_unit<E: serde::de::Error>(self) -> std::result::Result<String, E> {
            Ok(String::new())
        }

        fn visit_some<D2>(self, deserializer: D2) -> std::result::Result<String, D2::Error>
        where
            D2: serde::Deserializer<'de>,
        {
            deserializer.deserialize_string(StringOrEmpty)
        }

        fn visit_str<E: serde::de::Error>(self, v: &str) -> std::result::Result<String, E> {
            Ok(v.to_owned())
        }

        fn visit_string<E: serde::de::Error>(self, v: String) -> std::result::Result<String, E> {
            Ok(v)
        }
    }

    deserializer.deserialize_option(StringOrEmpty)
}

/// Repositório Azure (só o `id` interessa para criar PRs).
#[derive(Debug, Clone, Deserialize)]
pub struct Repository {
    /// ID do repositório.
    #[serde(default, deserialize_with = "string_or_empty")]
    pub id: String,
}

/// Busca o repositório (`{project}/_apis/git/repositories/{repo}`).
///
/// # Errors
///
/// Propaga [`crate::error::AppError::Azure`] em falha HTTP ou payload inválido.
pub async fn get_repository(
    client: &AzureClient,
    project: &str,
    repository: &str,
) -> Result<Repository> {
    client
        .get(&format!(
            "{}/_apis/git/repositories/{}",
            encode_segment(project),
            encode_segment(repository),
        ))
        .await
}

/// Resolve reviewer (email ou ID) para o ID de identidade do Azure.
///
/// Espelha `AzureIdentityClientLive.resolve`: vazio é erro, UUID busca por
/// `identityIds`, resto busca por `searchFilter=General`. Retorna o primeiro
/// `id` não-vazio.
///
/// # Errors
///
/// Retorna [`crate::error::AppError::Azure`] (status 0 = sem identidade
/// encontrada) ou propaga falha HTTP.
pub async fn resolve_identity(
    client: &AzureClient,
    organization: &str,
    value: &str,
) -> Result<String> {
    use crate::error::AppError;
    let normalized = value.trim();
    if normalized.is_empty() {
        return Err(AppError::Azure {
            status: 0,
            message: "reviewer não informado.".to_owned(),
        });
    }
    let is_id = normalized.len() == 36
        && normalized.bytes().enumerate().all(|(i, b)| {
            b.is_ascii_hexdigit() || ((i == 8 || i == 13 || i == 18 || i == 23) && b == b'-')
        });
    let query = if is_id {
        format!("identityIds={normalized}&queryMembership=None")
    } else {
        format!(
            "searchFilter=General&filterValue={}&queryMembership=None",
            encode_segment(normalized)
        )
    };
    let url = format!(
        "https://vssps.dev.azure.com/{}/_apis/identities?{query}&api-version=7.1",
        encode_segment(organization)
    );
    let response: ValueList<IdentityItem> = client.get_abs(&url).await?;
    response
        .value
        .into_iter()
        .map(|item| item.id.trim().to_owned())
        .find(|id| !id.is_empty())
        .ok_or(AppError::Azure {
            status: 0,
            message: format!("reviewer não encontrado no Azure DevOps: {normalized}."),
        })
}

/// Item de identidade (`{"id": ...}`).
#[derive(Debug, Deserialize)]
struct IdentityItem {
    /// ID da identidade.
    #[serde(default, deserialize_with = "string_or_empty")]
    id: String,
}

/// PR criado (espelha `AzurePullRequest` mínimo: id + URL).
#[derive(Debug, Clone, Deserialize)]
pub struct CreatedPullRequest {
    /// ID (`pullRequestId`).
    #[serde(rename = "pullRequestId")]
    pub pull_request_id: i64,
    /// URL (`url`).
    #[serde(default, deserialize_with = "string_or_empty")]
    pub url: String,
    /// URL web (`webUrl`, preferida quando presente).
    #[serde(default, deserialize_with = "string_or_empty", rename = "webUrl")]
    pub web_url: String,
}

impl CreatedPullRequest {
    /// URL pública do PR (`webUrl ?? url`, como no Dart).
    #[must_use]
    pub fn web_link(&self) -> &str {
        if self.web_url.is_empty() {
            &self.url
        } else {
            &self.web_url
        }
    }
}

/// Possível PR criado antes de uma resposta perdida.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PullRequestCandidate {
    /// Target do PR.
    pub target: String,
    /// ID do PR.
    pub id: i64,
    /// Link navegável.
    pub url: String,
    /// Título exato retornado pelo Azure.
    pub title: String,
    /// Branch de origem retornada pelo Azure.
    pub source_ref: String,
    /// Branch de destino retornada pelo Azure.
    pub target_ref: String,
    /// Data de criação, quando retornada.
    pub created_at: String,
    /// Se o Work Item esperado está vinculado.
    pub work_item_matches: bool,
}

/// Busca PRs recentes com origem, destino e título exatos.
///
/// A listagem é limitada a 50 itens pelo servidor. A validação final é feita
/// localmente porque a API não oferece um filtro consistente por título em
/// todas as versões. A relação com o Work Item é best-effort por candidato.
///
/// # Errors
///
/// Propaga falhas da listagem principal; falhas ao consultar relações
/// individuais deixam o candidato disponível com `work_item_matches = false`.
pub async fn find_recent_pull_request_candidates(
    client: &AzureClient,
    project: &str,
    repository: &str,
    title: &str,
    source_ref: &str,
    target_ref: &str,
    work_item_id: &str,
) -> Result<Vec<PullRequestCandidate>> {
    let response: PullRequestList = client
        .get(&format!(
            "{}/_apis/git/repositories/{}/pullrequests?searchCriteria.sourceRefName={}&searchCriteria.targetRefName={}&searchCriteria.status=all&$top=50",
            encode_segment(project),
            encode_segment(repository),
            encode_segment(source_ref),
            encode_segment(target_ref),
        ))
        .await?;
    let mut candidates = Vec::new();
    for item in response.value {
        if item.pull_request_id <= 0
            || item.title != title
            || item.source_ref_name != source_ref
            || item.target_ref_name != target_ref
        {
            continue;
        }
        let work_item_matches = if work_item_id.trim().is_empty() {
            true
        } else {
            get_pull_request_work_item_ids(client, project, repository, item.pull_request_id)
                .await
                .is_ok_and(|ids| {
                    work_item_id
                        .trim()
                        .parse::<i64>()
                        .is_ok_and(|expected| ids.contains(&expected))
                })
        };
        let url = if !item.links.web.href.is_empty() {
            item.links.web.href.clone()
        } else if item.web_url.is_empty() {
            item.url.clone()
        } else {
            item.web_url.clone()
        };
        candidates.push(PullRequestCandidate {
            target: target_ref
                .strip_prefix("refs/heads/")
                .unwrap_or(target_ref)
                .to_owned(),
            id: item.pull_request_id,
            url,
            title: item.title,
            source_ref: item.source_ref_name,
            target_ref: item.target_ref_name,
            created_at: item.creation_date,
            work_item_matches,
        });
    }
    candidates.sort_by(|left, right| {
        right
            .work_item_matches
            .cmp(&left.work_item_matches)
            .then_with(|| right.created_at.cmp(&left.created_at))
            .then_with(|| left.id.cmp(&right.id))
    });
    Ok(candidates)
}

/// Entrada de criação de PR (espelha `CreatePullRequestInput` do Dart).
#[derive(Debug, Clone)]
pub struct CreatePrInput {
    /// Título.
    pub title: String,
    /// Descrição em Markdown (< 4000 chars).
    pub description: String,
    /// `refs/heads/<origem>`.
    pub source_ref: String,
    /// `refs/heads/<target>`.
    pub target_ref: String,
    /// IDs de identidade dos reviewers (todos `isRequired`).
    pub reviewer_ids: Vec<String>,
    /// IDs de Work Items vinculados.
    pub work_item_ids: Vec<String>,
}

/// Allowlist do PATCH de um PR existente.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UpdatePullRequestInput {
    /// Título aprovado pelo usuário.
    pub title: String,
    /// Descrição aprovada pelo usuário.
    pub description: String,
}

/// Monta o payload mínimo do update, sem campos de criação ou merge.
#[must_use]
pub fn update_pr_body(input: &UpdatePullRequestInput) -> serde_json::Value {
    let mut body = serde_json::Map::new();
    body.insert(
        "title".to_owned(),
        serde_json::Value::String(input.title.clone()),
    );
    body.insert(
        "description".to_owned(),
        serde_json::Value::String(input.description.clone()),
    );
    serde_json::Value::Object(body)
}

/// Monta o corpo JSON de criação (puro; espelha `CreatePullRequestInput.toJson`).
#[must_use]
pub fn create_pr_body(input: &CreatePrInput) -> serde_json::Value {
    use serde_json::{Value, json};
    let mut body = serde_json::Map::new();
    body.insert("title".to_owned(), Value::String(input.title.clone()));
    body.insert(
        "description".to_owned(),
        Value::String(input.description.clone()),
    );
    body.insert(
        "sourceRefName".to_owned(),
        Value::String(input.source_ref.clone()),
    );
    body.insert(
        "targetRefName".to_owned(),
        Value::String(input.target_ref.clone()),
    );
    if !input.reviewer_ids.is_empty() {
        body.insert(
            "reviewers".to_owned(),
            Value::Array(
                input
                    .reviewer_ids
                    .iter()
                    .map(|id| json!({"id": id, "isRequired": true}))
                    .collect(),
            ),
        );
    }
    if !input.work_item_ids.is_empty() {
        body.insert(
            "workItemRefs".to_owned(),
            Value::Array(
                input
                    .work_item_ids
                    .iter()
                    .map(|id| json!({"id": id}))
                    .collect(),
            ),
        );
    }
    Value::Object(body)
}

/// Cria um PR (`POST .../pullrequests`, com `r` minúsculo como no Dart).
///
/// # Errors
///
/// Propaga [`crate::error::AppError::Azure`] em falha HTTP ou payload inválido.
pub async fn create_pull_request(
    client: &AzureClient,
    project: &str,
    repository_id: &str,
    input: &CreatePrInput,
) -> Result<CreatedPullRequest> {
    client
        .post(
            &format!(
                "{}/_apis/git/repositories/{}/pullrequests",
                encode_segment(project),
                encode_segment(repository_id),
            ),
            &create_pr_body(input),
        )
        .await
}

/// PR publicado num target.
#[derive(Debug, Clone)]
pub struct PublishedPr {
    /// Target (`dev`, `sprint/12`, …).
    pub target: String,
    /// ID do PR criado.
    pub id: i64,
    /// URL (`webUrl ?? url`).
    pub url: String,
}

/// Entrada da publicação (evita lista longa de parâmetros).
pub struct PublishInput<'a> {
    /// Remote Azure.
    pub remote: &'a crate::git::RepositoryRemote,
    /// Branch de origem.
    pub branch: &'a str,
    /// Targets.
    pub targets: &'a [String],
    /// Título do PR.
    pub title: &'a str,
    /// Descrição (< 4000 chars).
    pub body: &'a str,
    /// Work Items vinculados.
    pub work_item_ids: &'a [String],
    /// Reviewer por target (vazio = sem reviewer).
    pub reviewer_for: &'a (dyn Fn(&str) -> String + Sync),
    /// Notifica cada PR criado, antes de continuar para o próximo target.
    ///
    /// Permite que uma UI mostre progresso real e preserve sucessos parciais
    /// quando um target posterior falhar.
    pub on_published: Option<&'a (dyn Fn(&PublishedPr) + Sync)>,
    /// Notifica a UI antes de qualquer chamada remota para um target.
    pub on_target_started: Option<&'a (dyn Fn(&str) + Sync)>,
}

/// Publica a descrição em todos os targets (espelha o publisher Dart).
///
/// Valida o limite de 4000 chars, resolve o ID do repositório, resolve cada
/// reviewer uma única vez (cache por email) e cria um PR por target.
///
/// # Errors
///
/// Retorna [`crate::error::AppError::DescriptionTooLong`] se exceder o
/// limite, `Git` sem remote, `Azure` sem ID de repositório/identidade ou em
/// falha HTTP.
pub async fn publish_pull_requests(
    client: &AzureClient,
    input: &PublishInput<'_>,
) -> Result<Vec<PublishedPr>> {
    use crate::error::AppError;
    use std::collections::HashMap;
    if !crate::ai::is_within_limit(input.body) {
        return Err(AppError::DescriptionTooLong {
            length: input.body.chars().count(),
        });
    }
    let remote = input.remote;
    let repository = get_repository(client, &remote.project, &remote.repository).await?;
    if repository.id.trim().is_empty() {
        return Err(AppError::Azure {
            status: 0,
            message: "Azure DevOps não retornou o ID do repositório.".to_owned(),
        });
    }
    let mut resolved: HashMap<String, String> = HashMap::new();
    let mut published = Vec::with_capacity(input.targets.len());
    for target in input.targets {
        if let Some(on_target_started) = input.on_target_started {
            on_target_started(target);
        }
        let reviewer = (input.reviewer_for)(target).trim().to_owned();
        let mut reviewer_ids = Vec::new();
        if !reviewer.is_empty() {
            let id = if let Some(id) = resolved.get(&reviewer) {
                id.clone()
            } else {
                let id = resolve_identity(client, &remote.organization, &reviewer).await?;
                resolved.insert(reviewer.clone(), id.clone());
                id
            };
            reviewer_ids.push(id);
        }
        let created = create_pull_request(
            client,
            &remote.project,
            repository.id.trim(),
            &CreatePrInput {
                title: input.title.to_owned(),
                description: input.body.to_owned(),
                source_ref: format!("refs/heads/{}", input.branch),
                target_ref: format!("refs/heads/{target}"),
                reviewer_ids,
                work_item_ids: input.work_item_ids.to_vec(),
            },
        )
        .await?;
        let item = PublishedPr {
            target: target.clone(),
            id: created.pull_request_id,
            url: created.web_link().to_owned(),
        };
        published.push(item.clone());
        if let Some(on_published) = input.on_published {
            on_published(&item);
        }
    }
    Ok(published)
}

/// Busca um PR (`{project}/_apis/git/repositories/{repo}/pullRequests/{id}`).
///
/// # Errors
///
/// Propaga [`crate::error::AppError::Azure`] em falha HTTP ou payload inválido.
pub async fn get_pull_request(
    client: &AzureClient,
    project: &str,
    repository: &str,
    id: i64,
) -> Result<PullRequest> {
    client
        .get(&format!(
            "{}/_apis/git/repositories/{}/pullRequests/{id}",
            encode_segment(project),
            encode_segment(repository),
        ))
        .await
}

/// Atualiza título e descrição do PR informado, sem alterar outros metadados.
///
/// # Errors
///
/// Propaga [`crate::error::AppError::Azure`] em falha HTTP ou payload
/// inválido e [`crate::error::AppError::Http`] em falha de transporte.
pub async fn update_pull_request(
    client: &AzureClient,
    project: &str,
    repository: &str,
    id: i64,
    input: &UpdatePullRequestInput,
) -> Result<PullRequest> {
    client
        .patch_json(
            &format!(
                "{}/_apis/git/repositories/{}/pullRequests/{id}",
                encode_segment(project),
                encode_segment(repository),
            ),
            &update_pr_body(input),
        )
        .await
}

/// IDs dos Work Items vinculados ao PR (`.../pullRequests/{id}/workitems`).
///
/// # Errors
///
/// Propaga [`crate::error::AppError::Azure`] em falha HTTP ou payload inválido.
pub async fn get_pull_request_work_item_ids(
    client: &AzureClient,
    project: &str,
    repository: &str,
    id: i64,
) -> Result<Vec<i64>> {
    let response: ValueList<LinkedRef> = client
        .get(&format!(
            "{}/_apis/git/repositories/{}/pullRequests/{id}/workitems",
            encode_segment(project),
            encode_segment(repository),
        ))
        .await?;
    Ok(response.value.into_iter().map(|item| item.id).collect())
}

/// Resumo textual das alterações da ÚLTIMA iteração, uma por linha no formato
/// `- [tipo] caminho` (até [`MAX_CHANGES`] entradas).
///
/// Espelha `getPullRequestChanges` do repositório Dart (iterações → última →
///
/// changes). Diferenças documentadas:
/// - O Dart pede `$top=200` na query; aqui o `$top` foi omitido porque
///   `AzureClient::url` sempre anexa `?api-version=7.1` ao final do path e um
///   `?$top=...` no meio corromperia a URL (só o dono de `mod.rs` pode
///   corrigir isso). Em vez disso, o corte é feito no cliente com
///   `take(MAX_CHANGES)`.
/// - Erros de rede/payload PROPAGAM (`Result`); quem chama decide. O `prepare`
///   do `test_card` trata como best-effort (string vazia), espelhando o
///   `either`+`fold` do service Dart.
/// - Sem iterações, retorna `Ok("")` (o Dart retorna lista vazia).
///
/// # Errors
///
/// Propaga [`crate::error::AppError::Azure`] em falha HTTP ou payload inválido.
pub async fn get_pull_request_changes(
    client: &AzureClient,
    project: &str,
    repository: &str,
    id: i64,
) -> Result<String> {
    let base = format!(
        "{}/_apis/git/repositories/{}",
        encode_segment(project),
        encode_segment(repository)
    );
    let iterations: ValueList<Iteration> = client
        .get(&format!("{base}/pullRequests/{id}/iterations"))
        .await?;
    let Some(last) = iterations.value.last() else {
        return Ok(String::new());
    };
    let changes: ChangesList = client
        .get(&format!(
            "{base}/pullRequests/{id}/iterations/{}/changes",
            last.id
        ))
        .await?;
    Ok(changes
        .entries
        .into_iter()
        .take(MAX_CHANGES)
        .map(|change| format!("- [{}] {}", change.change_type, change.item.path))
        .collect::<Vec<_>>()
        .join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pull_request_should_deserialize_azure_payload() {
        let pr: PullRequest = serde_json::from_value(serde_json::json!({
            "pullRequestId": 42,
            "title": "Corrige fluxo",
            "description": null,
            "sourceRefName": "refs/heads/feature/1",
            "targetRefName": "refs/heads/dev",
        }))
        .unwrap();
        assert_eq!(pr.pull_request_id, 42);
        assert_eq!(pr.title, "Corrige fluxo");
        assert_eq!(pr.description, "");
        assert_eq!(pr.source_ref_name, "refs/heads/feature/1");
    }

    #[test]
    fn pull_request_should_deserialize_update_snapshot_fields_and_get_should_build_exact_route() {
        let pr: PullRequest = serde_json::from_value(serde_json::json!({
            "pullRequestId": 42,
            "status": "active",
            "repository": {
                "id": "repo-id",
                "name": "repo",
                "project": {"name": "project"}
            },
            "sourceRefName": "refs/heads/feature/42",
            "targetRefName": "refs/heads/dev",
            "title": "Atual",
            "description": "Body"
        }))
        .unwrap();
        assert_eq!(pr.status, "active");
        assert_eq!(pr.repository.name, "repo");
        assert_eq!(pr.repository.project.name, "project");
        assert_eq!(pr.source_ref_name, "refs/heads/feature/42");
        assert_eq!(pr.target_ref_name, "refs/heads/dev");
        assert_eq!(pr.title, "Atual");
        assert_eq!(pr.description, "Body");

        let client = AzureClient::new("org", "pat");
        let get_request = client
            .build_get_request("project/_apis/git/repositories/repo/pullRequests/42")
            .unwrap();
        assert_eq!(get_request.method(), reqwest::Method::GET);
        assert_eq!(
            get_request.url().path(),
            "/org/project/_apis/git/repositories/repo/pullRequests/42"
        );
        assert_eq!(get_request.url().query(), Some("api-version=7.1"));

        let request = client
            .build_json_patch_request(
                "project/_apis/git/repositories/repo/pullRequests/42",
                &update_pr_body(&UpdatePullRequestInput {
                    title: "Novo".to_owned(),
                    description: "Descrição".to_owned(),
                }),
            )
            .unwrap();
        assert_eq!(
            request.url().path(),
            "/org/project/_apis/git/repositories/repo/pullRequests/42"
        );
        assert_eq!(request.url().query(), Some("api-version=7.1"));
        assert_eq!(request.method(), reqwest::Method::PATCH);
        assert_eq!(
            request
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok()),
            Some("application/json")
        );
        let body = request.body().and_then(reqwest::Body::as_bytes);
        let expected_body = r#"{"description":"Descrição","title":"Novo"}"#;
        assert_eq!(body, Some(expected_body.as_bytes()));
    }

    #[test]
    fn changes_should_read_change_entries() {
        let list: ChangesList = serde_json::from_value(serde_json::json!({
            "changeEntries": [
                {"changeType": "edit", "item": {"path": "/src/a.ts"}},
                {"changeType": "add"},
            ],
        }))
        .unwrap();
        assert_eq!(list.entries.len(), 2);
        assert_eq!(list.entries[0].item.path, "/src/a.ts");
        assert_eq!(list.entries[1].item.path, "");
    }

    #[test]
    fn encode_should_escape_reserved_chars() {
        assert_eq!(encode_segment("Meu Projeto"), "Meu%20Projeto");
        assert_eq!(encode_segment("$Test Case"), "%24Test%20Case");
        assert_eq!(encode_segment("repo_ok-1.2~x"), "repo_ok-1.2~x");
    }

    #[test]
    fn create_body_should_match_dart_shape() {
        let body = create_pr_body(&CreatePrInput {
            title: "T".to_owned(),
            description: "B".to_owned(),
            source_ref: "refs/heads/feat".to_owned(),
            target_ref: "refs/heads/dev".to_owned(),
            reviewer_ids: vec!["abc".to_owned()],
            work_item_ids: vec!["11763".to_owned()],
        });
        assert_eq!(body["title"], serde_json::json!("T"));
        assert_eq!(body["sourceRefName"], serde_json::json!("refs/heads/feat"));
        assert_eq!(body["reviewers"][0]["isRequired"], serde_json::json!(true));
        assert_eq!(body["workItemRefs"][0]["id"], serde_json::json!("11763"));
    }

    #[test]
    fn create_body_should_omit_empty_reviewers_and_work_items() {
        let body = create_pr_body(&CreatePrInput {
            title: "T".to_owned(),
            description: "B".to_owned(),
            source_ref: "refs/heads/feat".to_owned(),
            target_ref: "refs/heads/dev".to_owned(),
            reviewer_ids: Vec::new(),
            work_item_ids: Vec::new(),
        });
        assert!(body.get("reviewers").is_none());
        assert!(body.get("workItemRefs").is_none());
    }

    #[test]
    fn update_pr_body_should_allow_only_title_and_description() {
        let input = UpdatePullRequestInput {
            title: "  Título ✅  ".to_owned(),
            description: "Body\n- [ ] validar  ".to_owned(),
        };
        let body = update_pr_body(&input);
        assert_eq!(
            body,
            serde_json::json!({
                "title": "  Título ✅  ",
                "description": "Body\n- [ ] validar  "
            })
        );
        assert_eq!(body.as_object().unwrap().len(), 2);
    }

    #[test]
    fn created_pr_should_prefer_web_url() {
        let with_both = CreatedPullRequest {
            pull_request_id: 1,
            url: "https://dev.azure.com/x/_apis/git/repositories/y/pullRequests/1".to_owned(),
            web_url: "https://dev.azure.com/x/_git/y/pullrequest/1".to_owned(),
        };
        assert!(with_both.web_link().contains("_git"));
        let fallback = CreatedPullRequest {
            pull_request_id: 1,
            url: "https://api".to_owned(),
            web_url: String::new(),
        };
        assert_eq!(fallback.web_link(), "https://api");
    }

    #[test]
    fn recent_pull_request_list_should_deserialize_candidate_fields() {
        let list: PullRequestList = serde_json::from_value(serde_json::json!({
            "value": [{
                "pullRequestId": 42,
                "title": "Atualiza fluxo",
                "sourceRefName": "refs/heads/feature/1",
                "targetRefName": "refs/heads/dev",
                "url": "https://api/pr/42",
                "webUrl": "https://web/pr/42",
                "creationDate": "2026-09-11T20:00:00Z",
                "_links": {"web": {"href": "https://web/pr/42"}}
            }]
        }))
        .unwrap();
        assert_eq!(list.value.len(), 1);
        assert_eq!(list.value[0].pull_request_id, 42);
        assert_eq!(list.value[0].web_url, "https://web/pr/42");
        assert_eq!(list.value[0].links.web.href, "https://web/pr/42");
        assert_eq!(list.value[0].creation_date, "2026-09-11T20:00:00Z");
    }
}
