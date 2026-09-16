//! Work Items do Azure DevOps — espelha `infrastructure/azure/work_items.dart`.
//!
//! Leituras (`query_wiql`, exemplos de Test Case) e escritas
//! (`create_test_case` via `POST` json-patch, `update_parent_to_test_qa` via
//! `PATCH` json-patch) usam `AzureClient`.

use serde::Deserialize;
use serde_json::{Value, json};

use crate::azure::{AzureClient, WorkItem, encode_segment};
use crate::error::{AppError, Result};

/// Tipo de Work Item retornado pelo catálogo do projeto.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct WorkItemTypeMetadata {
    /// Nome exibido e usado no endpoint.
    pub name: String,
    /// Identificador interno, quando retornado pelo Azure.
    #[serde(default, rename = "referenceName")]
    pub reference_name: String,
}

#[derive(Debug, Deserialize)]
struct WorkItemTypeListResponse {
    #[serde(default, rename = "value")]
    items: Vec<WorkItemTypeMetadata>,
}

/// Metadados de um campo de Work Item Type.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct WorkItemFieldMetadata {
    /// Nome interno (`System.Title`, `Custom.Team`, etc.).
    #[serde(rename = "referenceName")]
    pub reference_name: String,
    /// Tipo Azure (`String`, `Integer`, `Identity`, ...).
    #[serde(rename = "type")]
    pub field_type: String,
    /// Se o campo precisa de valor no Work Item.
    #[serde(default, alias = "alwaysRequired")]
    pub required: bool,
    /// Valor padrão declarado pelo processo, quando houver.
    #[serde(default, rename = "defaultValue")]
    pub default_value: Option<Value>,
    /// Valores permitidos declarados pelo processo.
    #[serde(
        default,
        rename = "allowedValues",
        deserialize_with = "deserialize_nullable_values"
    )]
    pub allowed_values: Vec<Value>,
}

fn deserialize_nullable_values<'de, D>(deserializer: D) -> std::result::Result<Vec<Value>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Option::<Vec<Value>>::deserialize(deserializer).map(Option::unwrap_or_default)
}

#[derive(Debug, Deserialize)]
struct WorkItemFieldListResponse {
    #[serde(default, rename = "value")]
    items: Vec<WorkItemFieldMetadata>,
}

/// Estado permitido por um Work Item Type.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct WorkItemStateMetadata {
    /// Nome literal do estado (`Test QA`, por exemplo).
    pub name: String,
}

#[derive(Debug, Deserialize)]
struct WorkItemStateListResponse {
    #[serde(default, rename = "value")]
    items: Vec<WorkItemStateMetadata>,
}

/// Limite de cada campo rico projetado para o prompt de `prt desc`.
pub const FUNCTIONAL_RICH_FIELD_LIMIT: usize = 3000;
/// Limite combinado dos campos ricos projetados para o prompt de `prt desc`.
pub const FUNCTIONAL_RICH_FIELDS_LIMIT: usize = 6000;
const TRUNCATION_MARKER: &str = "\n[conteúdo truncado]";

/// Projeção segura dos campos funcionais de um Work Item.
///
/// O mapa bruto de `WorkItem::fields` permanece na fronteira Azure. Os dois
/// campos ricos são normalizados e limitados antes de deixarem esse módulo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionalWorkItemContext {
    /// ID do Work Item.
    pub id: i64,
    /// Título (`System.Title`).
    pub title: String,
    /// Tipo (`System.WorkItemType`).
    pub work_item_type: String,
    /// Área (`System.AreaPath`).
    pub area_path: String,
    /// Descrição HTML convertida para texto, quando disponível.
    pub description: Option<String>,
    /// Critérios HTML convertidos para texto, quando disponíveis.
    pub acceptance_criteria: Option<String>,
}

impl FunctionalWorkItemContext {
    /// Projeta os campos permitidos de um Work Item.
    #[must_use]
    pub fn from_work_item(item: &WorkItem) -> Self {
        let (description, acceptance_criteria) = limit_combined_rich_fields(
            rich_text_field(item, "System.Description"),
            rich_text_field(item, "Microsoft.VSTS.Common.AcceptanceCriteria"),
        );
        Self {
            id: item.id,
            title: item.field_text("System.Title").trim().to_owned(),
            work_item_type: item.field_text("System.WorkItemType").trim().to_owned(),
            area_path: item.field_text("System.AreaPath").trim().to_owned(),
            description,
            acceptance_criteria,
        }
    }
}

/// Converte o rich text do Azure em texto legível sem transportar markup.
///
/// Tags de bloco, parágrafo, quebra e item de lista viram separadores de
/// linha. O parser é local e determinístico: campos malformados continuam
/// sendo texto, nunca uma falha de preparação.
#[must_use]
pub fn normalize_rich_text(value: &str) -> String {
    let mut plain = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(start) = rest.find('<') {
        plain.push_str(&rest[..start]);
        let after_start = &rest[start + 1..];
        if after_start.starts_with("!--") {
            if let Some(end) = after_start.find("-->") {
                rest = &after_start[end + 3..];
                plain.push('\n');
                continue;
            }
            break;
        }
        let Some(end) = after_start.find('>') else {
            rest = after_start;
            break;
        };
        let tag = &after_start[..end];
        if is_line_break_tag(tag) {
            plain.push('\n');
        }
        rest = &after_start[end + 1..];
    }
    plain.push_str(rest);

    let decoded = decode_html_entities(&plain);
    normalize_whitespace(&decoded)
}

fn is_line_break_tag(tag: &str) -> bool {
    let name = tag
        .trim()
        .trim_start_matches('/')
        .split(|c: char| c.is_ascii_whitespace() || c == '/')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    matches!(
        name.as_str(),
        "address"
            | "article"
            | "aside"
            | "blockquote"
            | "br"
            | "dd"
            | "div"
            | "dl"
            | "dt"
            | "footer"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "header"
            | "hr"
            | "li"
            | "ol"
            | "p"
            | "pre"
            | "section"
            | "table"
            | "tbody"
            | "td"
            | "tfoot"
            | "th"
            | "thead"
            | "tr"
            | "ul"
    )
}

fn decode_html_entities(value: &str) -> String {
    let mut decoded = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(start) = rest.find('&') {
        decoded.push_str(&rest[..start]);
        let entity_start = &rest[start + 1..];
        let Some(end) = entity_start.find(';') else {
            decoded.push_str(&rest[start..]);
            break;
        };
        let entity = &entity_start[..end];
        if let Some(replacement) = decode_html_entity(entity) {
            decoded.push_str(&replacement);
            rest = &entity_start[end + 1..];
        } else {
            decoded.push('&');
            rest = entity_start;
        }
    }
    decoded.push_str(rest);
    decoded
}

fn decode_html_entity(entity: &str) -> Option<String> {
    let named = match entity {
        "amp" => "&",
        "lt" => "<",
        "gt" => ">",
        "quot" => "\"",
        "apos" => "'",
        "nbsp" => " ",
        "ndash" => "–",
        "mdash" => "—",
        "hellip" => "…",
        "bull" => "•",
        "copy" => "©",
        "reg" => "®",
        _ => return decode_numeric_entity(entity),
    };
    Some(named.to_owned())
}

fn decode_numeric_entity(entity: &str) -> Option<String> {
    let number = entity
        .strip_prefix("#x")
        .or_else(|| entity.strip_prefix("#X"));
    let code_point = if let Some(hex) = number {
        u32::from_str_radix(hex, 16).ok()?
    } else {
        entity.strip_prefix('#')?.parse::<u32>().ok()?
    };
    char::from_u32(code_point).map(|character| character.to_string())
}

fn normalize_whitespace(value: &str) -> String {
    let lines: Vec<String> = value
        .lines()
        .map(|raw_line| raw_line.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|line| !line.is_empty())
        .collect();
    lines.join("\n")
}

fn truncate_rich_text(value: &str) -> String {
    truncate_rich_text_to(value, FUNCTIONAL_RICH_FIELD_LIMIT)
}

fn truncate_rich_text_to(value: &str, limit: usize) -> String {
    let length = value.chars().count();
    if length <= limit {
        return value.to_owned();
    }
    let marker_len = TRUNCATION_MARKER.chars().count();
    if limit <= marker_len {
        return TRUNCATION_MARKER.chars().take(limit).collect();
    }
    let prefix_len = limit - marker_len;
    let prefix: String = value.chars().take(prefix_len).collect();
    format!("{prefix}{TRUNCATION_MARKER}")
}

fn rich_text_field(item: &WorkItem, field: &str) -> Option<String> {
    let normalized = normalize_rich_text(item.field_text(field));
    (!normalized.is_empty()).then(|| truncate_rich_text(&normalized))
}

fn limit_combined_rich_fields(
    description: Option<String>,
    acceptance_criteria: Option<String>,
) -> (Option<String>, Option<String>) {
    let total = description
        .as_ref()
        .map_or(0, |value| value.chars().count())
        + acceptance_criteria
            .as_ref()
            .map_or(0, |value| value.chars().count());
    if total <= FUNCTIONAL_RICH_FIELDS_LIMIT {
        return (description, acceptance_criteria);
    }
    let description_len = description
        .as_ref()
        .map_or(0, |value| value.chars().count());
    let description = description
        .as_deref()
        .map(|value| truncate_rich_text_to(value, FUNCTIONAL_RICH_FIELDS_LIMIT));
    let remaining = FUNCTIONAL_RICH_FIELDS_LIMIT.saturating_sub(description_len);
    let acceptance_criteria = acceptance_criteria
        .as_deref()
        .map(|value| truncate_rich_text_to(value, remaining));
    (description, acceptance_criteria)
}

/// Entrada para criação de Test Case (espelha `CreateTestCaseInput` do Dart).
#[derive(Debug, Clone, Default)]
pub struct TestCaseInput {
    /// Título (`System.Title`, obrigatório).
    pub title: String,
    /// Descrição em HTML (`System.Description`).
    pub description_html: Option<String>,
    /// Steps em XML (`Microsoft.VSTS.TCM.Steps`).
    pub steps_xml: Option<String>,
    /// `AreaPath` (`System.AreaPath`).
    pub area_path: Option<String>,
    /// ID do Work Item pai (vira link `Related`).
    pub parent_id: Option<i64>,
    /// `IterationPath` (`System.IterationPath`).
    pub iteration_path: Option<String>,
    /// Prioridade (`Microsoft.VSTS.Common.Priority`).
    pub priority: Option<f64>,
    /// Time (`Custom.Team`).
    pub team: Option<String>,
    /// Programa no campo fixo do perfil selecionado.
    pub program: Option<String>,
    /// Reference name do campo fixo de programa do perfil selecionado.
    pub program_field: Option<String>,
    /// Responsável (`System.AssignedTo`).
    pub assigned_to: Option<String>,
    /// Organização Azure (`dev.azure.com/{org}`).
    ///
    /// Necessária para montar a URL absoluta do link `Related` do
    /// pai. O `AzureClient` não expõe a organização (campo privado), por isso
    /// ela viaja aqui, preenchida por quem conhece o remote (o `test_card`).
    pub organization: Option<String>,
}

/// Referência `{"id": ...}` retornada pela WIQL.
#[derive(Debug, Deserialize)]
struct WiqlRef {
    /// ID.
    id: i64,
}

/// Resposta `{"workItems": [...]}` da WIQL.
#[derive(Debug, Deserialize)]
struct WiqlResponse {
    /// Itens.
    #[serde(default, rename = "workItems")]
    items: Vec<WiqlRef>,
}

/// Monta a WIQL de Test Cases mais recentes (espelha `queryTestCaseIds`).
///
/// Aspas simples do projeto são escapadas como `''`, como no Dart.
#[must_use]
pub fn test_case_query(project: &str) -> String {
    format!(
        "SELECT [System.Id],[System.Title] FROM WorkItems WHERE \
         [System.WorkItemType]='Test Case' AND [System.TeamProject]='{}' \
         ORDER BY [System.ChangedDate] DESC",
        project.replace('\'', "''"),
    )
}

/// Monta a WIQL de candidatos recentes com o mesmo título.
#[must_use]
pub fn test_case_candidates_query(project: &str, title: &str) -> String {
    format!(
        "SELECT TOP 20 [System.Id],[System.Title] FROM WorkItems WHERE \
         [System.WorkItemType]='Test Case' AND [System.TeamProject]='{}' AND \
         [System.Title]='{}' AND [System.CreatedDate] >= @Today - 1 \
         ORDER BY [System.CreatedDate] DESC",
        project.replace('\'', "''"),
        title.replace('\'', "''"),
    )
}

/// URL absoluta de um Work Item (link `Related` do pai).
///
/// Espelha `azureUrl(config, '/_apis/wit/workitems/$parentId')` do Dart
/// (sem `api-version`, como lá).
#[must_use]
pub fn parent_work_item_url(organization: &str, parent_id: i64) -> String {
    format!("https://dev.azure.com/{organization}/_apis/wit/workitems/{parent_id}")
}

/// Executa uma WIQL e retorna os IDs (`POST {project}/_apis/wit/wiql`).
///
/// Usa `Content-Type: application/json` (correto para WIQL — só os endpoints
/// de escrita de Work Item exigem `json-patch`).
///
/// # Errors
///
/// Propaga [`AppError::Azure`] em falha HTTP ou payload inválido.
pub async fn query_wiql(client: &AzureClient, project: &str, wiql: &str) -> Result<Vec<i64>> {
    let response: WiqlResponse = client
        .post(
            &format!("{}/_apis/wit/wiql", encode_segment(project)),
            &json!({"query": wiql}),
        )
        .await?;
    Ok(response.items.into_iter().map(|item| item.id).collect())
}

/// Lista os Work Item Types disponíveis no projeto.
///
/// # Errors
///
/// Propaga [`AppError::Azure`] em falha HTTP ou payload inválido.
pub async fn list_work_item_types(
    client: &AzureClient,
    project: &str,
) -> Result<Vec<WorkItemTypeMetadata>> {
    let response: WorkItemTypeListResponse = client
        .get(&format!(
            "{}/_apis/wit/workitemtypes",
            encode_segment(project)
        ))
        .await?;
    Ok(response.items)
}

/// Lista todos os fields de um Work Item Type, incluindo defaults e allowed values.
///
/// # Errors
///
/// Propaga [`AppError::Azure`] em falha HTTP ou payload inválido.
pub async fn list_work_item_type_fields(
    client: &AzureClient,
    project: &str,
    work_item_type: &str,
) -> Result<Vec<WorkItemFieldMetadata>> {
    let response: WorkItemFieldListResponse = client
        .get(&format!(
            "{}/_apis/wit/workitemtypes/{}/fields?$expand=all",
            encode_segment(project),
            encode_segment(work_item_type),
        ))
        .await?;
    Ok(response.items)
}

/// Lista os estados permitidos por um Work Item Type.
///
/// # Errors
///
/// Propaga [`AppError::Azure`] em falha HTTP ou payload inválido.
pub async fn list_work_item_type_states(
    client: &AzureClient,
    project: &str,
    work_item_type: &str,
) -> Result<Vec<WorkItemStateMetadata>> {
    let response: WorkItemStateListResponse = client
        .get(&format!(
            "{}/_apis/wit/workitemtypes/{}/states",
            encode_segment(project),
            encode_segment(work_item_type),
        ))
        .await?;
    Ok(response.items)
}

/// Busca até `count` Test Cases recentes (WIQL + um GET por item).
///
/// Espelha o trecho de exemplos do `prepare` do service Dart: a WIQL propaga
/// erro, mas a busca individual de cada exemplo é best-effort (falhas por
/// item são ignoradas, como o `fold` que descarta erros no Dart).
/// Reusa `super::get_work_item`.
///
/// # Errors
///
/// Propaga [`AppError::Azure`] se a WIQL falhar.
pub async fn get_test_case_examples(
    client: &AzureClient,
    project: &str,
    count: usize,
) -> Result<Vec<WorkItem>> {
    let ids = query_wiql(client, project, &test_case_query(project)).await?;
    let mut examples = Vec::new();
    for id in ids.into_iter().take(count) {
        if let Ok(item) = super::get_work_item(client, &id.to_string()).await {
            examples.push(item);
        }
    }
    Ok(examples)
}

/// Busca Test Cases com título exato, incluindo relações para validação do pai.
///
/// Falhas ao carregar um candidato individual são ignoradas: a busca continua
/// com os demais IDs retornados pela WIQL, como na consulta de exemplos.
///
/// # Errors
///
/// Propaga falhas da consulta WIQL.
pub async fn find_test_case_candidates(
    client: &AzureClient,
    project: &str,
    title: &str,
) -> Result<Vec<WorkItem>> {
    let ids = query_wiql(client, project, &test_case_candidates_query(project, title)).await?;
    let mut candidates = Vec::new();
    for id in ids {
        if let Ok(item) = super::get_work_item_with_relations(client, &id.to_string()).await {
            if item.title() == title {
                candidates.push(item);
            }
        }
    }
    Ok(candidates)
}

/// Serializa número preservando inteiro quando possível (espelha o `num` do
/// Dart: `2` vira `2`, não `2.0` — o campo Priority do Azure é inteiro).
/// Não-finito vira `null` (sem pânico, ao contrário de `json!(f64)`).
fn json_num(value: f64) -> Value {
    if value.fract() == 0.0 && (-9.0e15..=9.0e15).contains(&value) {
        #[allow(clippy::cast_possible_truncation)]
        Value::from(value as i64)
    } else {
        serde_json::Number::from_f64(value).map_or(Value::Null, Value::Number)
    }
}

/// Monta o documento `json-patch` de criação do Test Case (puro).
///
/// Espelha `createTestCase` do Dart na ordem e nos campos: `System.Title`,
/// `System.Description` (html), `Microsoft.VSTS.TCM.Steps` (xml),
/// `System.AreaPath`, `System.IterationPath`, `Microsoft.VSTS.Common.Priority`,
/// `Custom.Team`, o campo de programa do perfil e `System.AssignedTo`
/// (opcionais vazios são omitidos) + relação `Related` com o pai
/// quando `parent_id > 0` e `organization` presente.
#[must_use]
pub fn build_create_patch(input: &TestCaseInput, parent_url: Option<&str>) -> Vec<Value> {
    let mut ops = vec![json!({"op": "add", "path": "/fields/System.Title", "value": input.title})];
    let optional: Vec<(String, Option<Value>)> = vec![
        (
            "/fields/System.Description".to_owned(),
            input.description_html.clone().map(Value::from),
        ),
        (
            "/fields/Microsoft.VSTS.TCM.Steps".to_owned(),
            input.steps_xml.clone().map(Value::from),
        ),
        (
            "/fields/System.AreaPath".to_owned(),
            input.area_path.clone().map(Value::from),
        ),
        (
            "/fields/System.IterationPath".to_owned(),
            input.iteration_path.clone().map(Value::from),
        ),
        (
            "/fields/Microsoft.VSTS.Common.Priority".to_owned(),
            input.priority.map(json_num),
        ),
        (
            "/fields/Custom.Team".to_owned(),
            input.team.clone().map(Value::from),
        ),
        (
            format!(
                "/fields/{}",
                input
                    .program_field
                    .as_deref()
                    .unwrap_or(crate::config::AGROTRACE_PROGRAM_FIELD)
            ),
            input.program.clone().map(Value::from),
        ),
    ];
    for (path, value) in optional {
        let present = value
            .as_ref()
            .is_some_and(|v| !v.is_null() && v.as_str() != Some(""));
        if present {
            ops.push(json!({"op": "add", "path": path, "value": value}));
        }
    }
    if let Some(assigned) = input.assigned_to.as_deref().map(str::trim) {
        if !assigned.is_empty() {
            ops.push(json!({"op": "add", "path": "/fields/System.AssignedTo", "value": assigned}));
        }
    }
    if let Some(parent_id) = input.parent_id {
        if parent_id > 0 {
            if let Some(url) = parent_url {
                ops.push(json!({
                    "op": "add",
                    "path": "/relations/-",
                    "value": {"rel": "System.LinkTypes.Related", "url": url},
                }));
            }
        }
    }
    ops
}

/// Monta o documento `json-patch` de transição do pai para `Test QA` (puro).
///
/// Espelha `updateToTestQa` do Dart: estado sempre + esforços só quando
/// presentes.
#[must_use]
pub fn build_test_qa_patch(effort: Option<f64>, real_effort: Option<f64>) -> Vec<Value> {
    build_parent_patch(Some("Test QA"), effort, real_effort)
}

/// Monta o patch do pai preservando esforços e incluindo estado somente
/// quando o perfil declarou uma transição.
#[must_use]
pub fn build_parent_patch(
    transition: Option<&str>,
    effort: Option<f64>,
    real_effort: Option<f64>,
) -> Vec<Value> {
    let mut ops = transition
        .filter(|state| !state.trim().is_empty())
        .map_or_else(Vec::new, |state| {
            vec![json!({"op": "add", "path": "/fields/System.State", "value": state})]
        });
    if let Some(value) = effort {
        ops.push(json!({
            "op": "add",
            "path": "/fields/Microsoft.VSTS.Scheduling.Effort",
            "value": json_num(value),
        }));
    }
    if let Some(value) = real_effort {
        ops.push(json!({
            "op": "add",
            "path": "/fields/Custom.RealEffort",
            "value": json_num(value),
        }));
    }
    ops
}

/// Interpreta decimal opcional não-negativo (vírgula vira ponto, como `_decimal`).
///
/// `None`/vazio vira `None` (espelha `validateNonNegativeDecimal`: vazio é válido).
///
/// # Errors
///
/// Retorna [`AppError::Cli`] se o texto não for um número finito ≥ 0.
fn parse_optional_decimal(raw: Option<&str>, label: &str) -> Result<Option<f64>> {
    let Some(text) = raw.map(str::trim).filter(|t| !t.is_empty()) else {
        return Ok(None);
    };
    let number: f64 = text.replace(',', ".").parse().unwrap_or(f64::NAN);
    if number.is_finite() && number >= 0.0 {
        Ok(Some(number))
    } else {
        Err(AppError::cli(format!(
            "{label} inválido: informe um número válido."
        )))
    }
}

/// Cria o Test Case (`POST {project}/_apis/wit/workitems/$Test Case` json-patch).
///
/// # Errors
///
/// Retorna [`AppError::Cli`] se o título estiver vazio; propaga
/// [`AppError::Azure`] em falha HTTP.
pub async fn create_test_case(
    client: &AzureClient,
    project: &str,
    input: &TestCaseInput,
) -> Result<WorkItem> {
    if input.title.trim().is_empty() {
        return Err(AppError::cli(
            "título é obrigatório para criar o test case.",
        ));
    }
    let parent_url = match (input.parent_id, input.organization.as_deref()) {
        (Some(id), Some(org)) if id > 0 && !org.is_empty() => Some(parent_work_item_url(org, id)),
        _ => None,
    };
    let body = build_create_patch(input, parent_url.as_deref());
    client
        .post_patch(
            &format!(
                "{}/_apis/wit/workitems/{}",
                encode_segment(project),
                encode_segment("$Test Case"),
            ),
            &body,
        )
        .await
}

/// Atualiza o pai para `Test QA` (+ esforços opcionais) via `PATCH` json-patch.
///
/// # Errors
///
/// Retorna [`AppError::Cli`] se algum esforço for inválido; propaga
/// [`AppError::Azure`] em falha HTTP.
pub async fn update_parent_to_test_qa(
    client: &AzureClient,
    parent_id: i64,
    effort: Option<&str>,
    real_effort: Option<&str>,
) -> Result<()> {
    update_parent_with_transition(client, parent_id, Some("Test QA"), effort, real_effort).await
}

/// Atualiza o pai usando a transição declarada pelo perfil, quando houver.
///
/// # Errors
///
/// Retorna [`AppError::Cli`] se algum esforço for inválido; propaga
/// [`AppError::Azure`] em falha HTTP.
pub async fn update_parent_with_transition(
    client: &AzureClient,
    parent_id: i64,
    transition: Option<&str>,
    effort: Option<&str>,
    real_effort: Option<&str>,
) -> Result<()> {
    let body = build_parent_patch(
        transition,
        parse_optional_decimal(effort, "effort")?,
        parse_optional_decimal(real_effort, "real effort")?,
    );
    if body.is_empty() {
        return Ok(());
    }
    let _: Value = client
        .patch(&format!("_apis/wit/workitems/{parent_id}"), &body)
        .await?;
    Ok(())
}

/// Move um Work Item para a lixeira do Azure DevOps.
///
/// O endpoint padrão é reversível pela lixeira; não usa `destroy=true`.
///
/// # Errors
///
/// Propaga falhas HTTP do cliente.
pub async fn delete_work_item(client: &AzureClient, project: &str, id: i64) -> Result<()> {
    client
        .delete(&format!(
            "{}/_apis/wit/workitems/{id}",
            encode_segment(project)
        ))
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn field_metadata_should_accept_null_allowed_values() {
        let response: WorkItemFieldListResponse = serde_json::from_value(serde_json::json!({
            "value": [{
                "referenceName": "Custom.Team",
                "type": "String",
                "required": true,
                "defaultValue": null,
                "allowedValues": null,
                "pickList": null
            }]
        }))
        .expect("metadata de fields válida");

        assert_eq!(response.items.len(), 1);
        assert!(response.items[0].allowed_values.is_empty());
    }

    #[test]
    fn functional_context_should_omit_absent_or_non_text_optional_fields() {
        let item: WorkItem = serde_json::from_value(serde_json::json!({
            "id": 11763,
            "fields": {
                "System.Title": "Implementar contexto",
                "System.WorkItemType": "User Story",
                "System.AreaPath": "Produto\\CLI",
                "System.Description": "",
                "Microsoft.VSTS.Common.AcceptanceCriteria": 42
            }
        }))
        .expect("work item válido");

        let context = FunctionalWorkItemContext::from_work_item(&item);
        assert_eq!(context.id, 11763);
        assert_eq!(context.title, "Implementar contexto");
        assert_eq!(context.work_item_type, "User Story");
        assert_eq!(context.area_path, "Produto\\CLI");
        assert_eq!(context.description, None);
        assert_eq!(context.acceptance_criteria, None);
    }

    #[test]
    fn functional_context_should_normalize_and_bound_rich_text() {
        let item: WorkItem = serde_json::from_value(serde_json::json!({
            "id": 11763,
            "fields": {
                "System.Title": "Título",
                "System.WorkItemType": "Task",
                "System.Description": "<p>  primeiro &amp; segundo </p><ul><li>item &#x31;</li><li>item &lt;dois&gt;</li></ul><script>remover tag</script>",
                "Microsoft.VSTS.Common.AcceptanceCriteria": format!("<div>{}</div>", "a".repeat(4000))
            }
        }))
        .expect("work item válido");

        let context = FunctionalWorkItemContext::from_work_item(&item);
        let description = context.description.expect("descrição");
        let acceptance = context.acceptance_criteria.expect("critérios");
        assert_eq!(
            description,
            "primeiro & segundo\nitem 1\nitem <dois>\nremover tag"
        );
        assert!(!description.contains("<ul>"));
        assert_eq!(acceptance.chars().count(), FUNCTIONAL_RICH_FIELD_LIMIT);
        assert!(acceptance.contains("[conteúdo truncado]"));
        assert!(
            description.chars().count() + acceptance.chars().count()
                <= FUNCTIONAL_RICH_FIELDS_LIMIT
        );
    }

    #[test]
    fn wiql_should_filter_test_cases_by_project() {
        let query = test_case_query("MeuProj");
        assert!(query.contains("[System.WorkItemType]='Test Case'"));
        assert!(query.contains("[System.TeamProject]='MeuProj'"));
        assert!(query.contains("ORDER BY [System.ChangedDate] DESC"));
    }

    #[test]
    fn wiql_should_escape_single_quotes() {
        assert!(test_case_query("Proj'A").contains("[System.TeamProject]='Proj''A'"));
    }

    #[test]
    fn candidate_query_should_match_title_and_limit_recent_items() {
        let query = test_case_candidates_query("Proj'A", "Card 'de teste'");
        assert!(query.contains("SELECT TOP 20"));
        assert!(query.contains("[System.TeamProject]='Proj''A'"));
        assert!(query.contains("[System.Title]='Card ''de teste'"));
        assert!(query.contains("[System.CreatedDate] >= @Today - 1"));
        assert!(query.contains("ORDER BY [System.CreatedDate] DESC"));
    }

    #[test]
    fn parent_url_should_mirror_dart_format() {
        assert_eq!(
            parent_work_item_url("minhaorg", 11763),
            "https://dev.azure.com/minhaorg/_apis/wit/workitems/11763",
        );
    }

    #[test]
    fn create_patch_should_include_fields_and_related_parent_link() {
        let input = TestCaseInput {
            title: "Card".to_owned(),
            description_html: Some("<p>x</p>".to_owned()),
            area_path: Some("Proj\\Time".to_owned()),
            parent_id: Some(7),
            priority: Some(2.0),
            team: Some("Time".to_owned()),
            program: Some(String::new()),
            assigned_to: Some("  ana@x.com  ".to_owned()),
            ..TestCaseInput::default()
        };
        let ops = build_create_patch(
            &input,
            Some("https://dev.azure.com/o/_apis/wit/workitems/7"),
        );
        let paths: Vec<&str> = ops
            .iter()
            .filter_map(|op| op.get("path").and_then(Value::as_str))
            .collect();
        assert!(paths.contains(&"/fields/System.Title"));
        assert!(paths.contains(&"/fields/System.Description"));
        assert!(paths.contains(&"/fields/System.AreaPath"));
        assert!(paths.contains(&"/fields/Custom.Team"));
        assert!(!paths.contains(&"/fields/Custom.ProgramasAgrotrace"));
        assert!(paths.contains(&"/relations/-"));
        let relation = ops
            .iter()
            .find(|op| op.get("path").and_then(Value::as_str) == Some("/relations/-"))
            .and_then(|op| op.get("value"))
            .and_then(|value| value.get("rel"))
            .and_then(Value::as_str);
        assert_eq!(relation, Some("System.LinkTypes.Related"));
        let priority = ops
            .iter()
            .find(|op| {
                op.get("path").and_then(Value::as_str)
                    == Some("/fields/Microsoft.VSTS.Common.Priority")
            })
            .and_then(|op| op.get("value"));
        assert_eq!(priority, Some(&Value::from(2)));
        let assigned = ops
            .iter()
            .find(|op| op.get("path").and_then(Value::as_str) == Some("/fields/System.AssignedTo"))
            .and_then(|op| op.get("value").and_then(Value::as_str));
        assert_eq!(assigned, Some("ana@x.com"));
    }

    #[test]
    fn qa_patch_should_set_state_and_efforts() {
        let ops = build_test_qa_patch(Some(3.0), Some(2.5));
        assert_eq!(ops.len(), 3);
        assert_eq!(ops[0].get("value").and_then(Value::as_str), Some("Test QA"),);
        let effort_path = "/fields/Microsoft.VSTS.Scheduling.Effort";
        let effort = ops
            .iter()
            .find(|op| op.get("path").and_then(Value::as_str) == Some(effort_path))
            .and_then(|op| op.get("value"));
        assert_eq!(effort, Some(&Value::from(3)));

        let only_state = build_test_qa_patch(None, None);
        assert_eq!(only_state.len(), 1);
    }

    #[test]
    fn checkmilk_patch_should_use_only_checkmilk_program_field() {
        let input = TestCaseInput {
            title: "Card CheckMilk".to_owned(),
            team: Some("DevOps".to_owned()),
            program: Some("Checkmilk".to_owned()),
            program_field: Some(crate::config::CHECKMILK_PROGRAM_FIELD.to_owned()),
            ..TestCaseInput::default()
        };
        let patch = build_create_patch(&input, None);
        let paths: Vec<&str> = patch
            .iter()
            .filter_map(|operation| operation.get("path").and_then(Value::as_str))
            .collect();
        assert!(paths.contains(&"/fields/Custom.ProgramasCheckmilk"));
        assert!(!paths.contains(&"/fields/Custom.ProgramasAgrotrace"));
    }

    #[test]
    fn optional_parent_transition_should_preserve_efforts() {
        let without_state = build_parent_patch(None, Some(1.0), Some(2.0));
        assert_eq!(without_state.len(), 2);
        assert!(
            without_state
                .iter()
                .all(|operation| operation.get("path").and_then(Value::as_str)
                    != Some("/fields/System.State"))
        );
        let with_state = build_parent_patch(Some("Test QA"), Some(1.0), Some(2.0));
        assert_eq!(with_state[0]["value"], "Test QA");
        assert!(with_state.iter().any(|operation| {
            operation.get("path").and_then(Value::as_str)
                == Some("/fields/Microsoft.VSTS.Scheduling.Effort")
        }));
    }

    #[test]
    fn decimal_should_accept_comma_and_reject_negative() {
        assert_eq!(parse_optional_decimal(None, "effort").unwrap(), None);
        assert_eq!(parse_optional_decimal(Some(""), "effort").unwrap(), None);
        assert_eq!(
            parse_optional_decimal(Some("2,5"), "effort").unwrap(),
            Some(2.5)
        );
        assert!(parse_optional_decimal(Some("-1"), "effort").is_err());
        assert!(parse_optional_decimal(Some("abc"), "effort").is_err());
    }
}
