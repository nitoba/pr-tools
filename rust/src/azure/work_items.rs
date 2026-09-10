//! Work Items do Azure DevOps — espelha `infrastructure/azure/work_items.dart`.
//!
//! Leituras (`query_wiql`, exemplos de Test Case) e escritas
//! (`create_test_case` via `POST` json-patch, `update_parent_to_test_qa` via
//! `PATCH` json-patch) usam `AzureClient`.

use serde::Deserialize;
use serde_json::{Value, json};
use std::fmt::Write as _;

use crate::azure::{AzureClient, WorkItem};
use crate::error::{AppError, Result};

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
    /// ID do Work Item pai (vira link `Hierarchy-Reverse`).
    pub parent_id: Option<i64>,
    /// `IterationPath` (`System.IterationPath`).
    pub iteration_path: Option<String>,
    /// Prioridade (`Microsoft.VSTS.Common.Priority`).
    pub priority: Option<f64>,
    /// Time (`Custom.Team`).
    pub team: Option<String>,
    /// Programa (`Custom.ProgramasAgrotrace`).
    pub program: Option<String>,
    /// Responsável (`System.AssignedTo`).
    pub assigned_to: Option<String>,
    /// Organização Azure (`dev.azure.com/{org}`).
    ///
    /// Necessária para montar a URL absoluta do link `Hierarchy-Reverse` do
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

/// Percent-encode de um segmento de path (espelha `pathSegment` do Dart).
///
/// Duplicado aqui e em `pull_requests.rs` porque `src/azure/mod.rs` — o único
/// lugar natural de compartilhamento — está sob responsabilidade de outro
/// agente e não pode ser editado.
fn encode_segment(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        if matches!(byte, b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~') {
            out.push(byte as char);
        } else {
            let _ = write!(out, "%{byte:02X}");
        }
    }
    out
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

/// URL absoluta de um Work Item (link `Hierarchy-Reverse` do pai).
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
/// `Custom.Team`, `Custom.ProgramasAgrotrace`, `System.AssignedTo`
/// (opcionais vazios são omitidos) + relação `Hierarchy-Reverse` com o pai
/// quando `parent_id > 0` e `organization` presente.
#[must_use]
pub fn build_create_patch(input: &TestCaseInput, parent_url: Option<&str>) -> Vec<Value> {
    let mut ops = vec![json!({"op": "add", "path": "/fields/System.Title", "value": input.title})];
    let optional: [(&str, Option<Value>); 7] = [
        (
            "/fields/System.Description",
            input.description_html.clone().map(Value::from),
        ),
        (
            "/fields/Microsoft.VSTS.TCM.Steps",
            input.steps_xml.clone().map(Value::from),
        ),
        (
            "/fields/System.AreaPath",
            input.area_path.clone().map(Value::from),
        ),
        (
            "/fields/System.IterationPath",
            input.iteration_path.clone().map(Value::from),
        ),
        (
            "/fields/Microsoft.VSTS.Common.Priority",
            input.priority.map(json_num),
        ),
        ("/fields/Custom.Team", input.team.clone().map(Value::from)),
        (
            "/fields/Custom.ProgramasAgrotrace",
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
                    "value": {"rel": "System.LinkTypes.Hierarchy-Reverse", "url": url},
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
    let mut ops = vec![json!({"op": "add", "path": "/fields/System.State", "value": "Test QA"})];
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
    let body = build_test_qa_patch(
        parse_optional_decimal(effort, "effort")?,
        parse_optional_decimal(real_effort, "real effort")?,
    );
    let _: Value = client
        .patch(&format!("_apis/wit/workitems/{parent_id}"), &body)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn parent_url_should_mirror_dart_format() {
        assert_eq!(
            parent_work_item_url("minhaorg", 11763),
            "https://dev.azure.com/minhaorg/_apis/wit/workitems/11763",
        );
    }

    #[test]
    fn create_patch_should_include_fields_and_parent_link() {
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
