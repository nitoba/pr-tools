//! Cliente Azure DevOps REST — espelha `infrastructure/azure/*` do Dart.
//!
//! Auth: `Authorization: Basic base64(:PAT)`, `api-version=7.1`.
//! Usa `reqwest` + `tokio` (nunca segura lock através de `.await`).

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use reqwest::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE};
use serde::{Deserialize, Deserializer, Serialize};
use std::fmt::Write as _;
use std::time::Duration;

use crate::error::{AppError, Result};
use crate::git::RepositoryRemote;

pub mod pull_requests;
pub mod work_items;

/// Percent-encode de um segmento de path: tudo fora de `[A-Za-z0-9-_.~]` vira `%XX`.
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

/// Cliente HTTP autenticado.
#[derive(Debug, Clone)]
pub struct AzureClient {
    inner: reqwest::Client,
    base_url: String,
    pat: String,
    request_timeout: Option<Duration>,
}

/// Decodifica corpo JSON do Azure com erro acionável.
///
/// O Azure (ou um proxy no caminho) às vezes responde 2xx com corpo vazio
/// ou HTML (ex.: PAT inválido/expirado devolve página de login). Sem este
/// guarda, o usuário via só `expected value at line 1 column 1`.
fn decode_json<T: for<'de> Deserialize<'de>>(status: u16, body: &str) -> Result<T> {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return Err(AppError::Azure {
            status,
            message: "resposta vazia do Azure DevOps (PAT inválido/expirado? rode `prt doctor`)"
                .to_owned(),
        });
    }
    if !trimmed.starts_with('{') && !trimmed.starts_with('[') {
        let snippet: String = trimmed.split_whitespace().collect::<Vec<_>>().join(" ");
        let snippet: String = snippet.chars().take(160).collect();
        return Err(AppError::Azure {
            status,
            message: format!(
                "resposta não-JSON do Azure DevOps (PAT inválido/expirado? rode `prt doctor`): {snippet}"
            ),
        });
    }
    serde_json::from_str(trimmed).map_err(|e| AppError::Azure {
        status,
        message: e.to_string(),
    })
}

impl AzureClient {
    /// Cria cliente para a organização do remote.
    #[must_use]
    pub fn new(organization: &str, pat: &str) -> Self {
        Self::build(organization, pat, None)
    }

    /// Cria cliente com timeout por requisição.
    #[must_use]
    pub fn new_with_timeout(organization: &str, pat: &str, timeout: Duration) -> Self {
        Self::build(organization, pat, Some(timeout))
    }

    fn build(organization: &str, pat: &str, request_timeout: Option<Duration>) -> Self {
        Self {
            inner: reqwest::Client::new(),
            base_url: format!("https://dev.azure.com/{organization}/"),
            pat: pat.trim().to_owned(),
            request_timeout,
        }
    }

    #[cfg(test)]
    pub(crate) fn new_for_test(base_url: &str, pat: &str) -> Self {
        Self {
            inner: reqwest::Client::new(),
            base_url: format!("{}/", base_url.trim_end_matches('/')),
            pat: pat.trim().to_owned(),
            request_timeout: None,
        }
    }

    #[cfg(test)]
    pub(crate) fn new_for_test_with_timeout(base_url: &str, pat: &str, timeout: Duration) -> Self {
        Self {
            inner: reqwest::Client::new(),
            base_url: format!("{}/", base_url.trim_end_matches('/')),
            pat: pat.trim().to_owned(),
            request_timeout: Some(timeout),
        }
    }

    fn apply_timeout(&self, builder: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match self.request_timeout {
            Some(timeout) => builder.timeout(timeout),
            None => builder,
        }
    }

    fn auth_header(&self) -> String {
        format!("Basic {}", BASE64.encode(format!(":{}", self.pat)))
    }

    fn url(&self, path: &str) -> String {
        let separator = if path.contains('?') { '&' } else { '?' };
        format!(
            "{}{}{}api-version=7.1",
            self.base_url,
            path.trim_start_matches('/'),
            separator,
        )
    }

    /// GET genérico que decodifica JSON.
    ///
    /// # Errors
    ///
    /// Retorna [`AppError::Azure`] em status >= 300 ou falha de transporte.
    pub async fn get<T: for<'de> Deserialize<'de>>(&self, path: &str) -> Result<T> {
        let res = self
            .apply_timeout(self.get_builder(path))
            .header(ACCEPT, "application/json")
            .header(AUTHORIZATION, self.auth_header())
            .send()
            .await?;
        let status = res.status().as_u16();
        let body = res.text().await.unwrap_or_default();
        if status >= 300 {
            return Err(AppError::Azure {
                status,
                message: body.chars().take(500).collect(),
            });
        }
        decode_json(status, &body)
    }

    fn get_builder(&self, path: &str) -> reqwest::RequestBuilder {
        self.inner.get(self.url(path))
    }

    /// GET em URL absoluta (ex.: `vssps` de identidades).
    ///
    /// # Errors
    ///
    /// Retorna [`AppError::Azure`] em status >= 300 ou falha de transporte.
    pub async fn get_abs<T: for<'de> Deserialize<'de>>(&self, url: &str) -> Result<T> {
        let res = self
            .apply_timeout(self.inner.get(url))
            .header(ACCEPT, "application/json")
            .header(AUTHORIZATION, self.auth_header())
            .send()
            .await?;
        let status = res.status().as_u16();
        let body = res.text().await.unwrap_or_default();
        if status >= 300 {
            return Err(AppError::Azure {
                status,
                message: body.chars().take(500).collect(),
            });
        }
        decode_json(status, &body)
    }

    /// POST genérico com corpo JSON.
    ///
    /// # Errors
    ///
    /// Retorna [`AppError::Azure`] em status >= 300.
    pub async fn post<B: Serialize, T: for<'de> Deserialize<'de>>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T> {
        self.request(self.inner.post(self.url(path)), body).await
    }

    /// POST com `Content-Type: application/json-patch+json` (Work Items).
    ///
    /// A API de Work Items do Azure DevOps rejeita `application/json`
    /// na criação/atualização — exige o content-type de json-patch.
    ///
    /// # Errors
    ///
    /// Retorna [`AppError::Azure`] em status >= 300.
    pub async fn post_patch<B: Serialize, T: for<'de> Deserialize<'de>>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T> {
        let builder = self
            .inner
            .post(self.url(path))
            .header(CONTENT_TYPE, "application/json-patch+json");
        self.request(builder, body).await
    }

    /// PATCH com `Content-Type: application/json-patch+json` (Work Items).
    ///
    /// # Errors
    ///
    /// Retorna [`AppError::Azure`] em status >= 300.
    pub async fn patch<B: Serialize, T: for<'de> Deserialize<'de>>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T> {
        let builder = self
            .inner
            .patch(self.url(path))
            .header(CONTENT_TYPE, "application/json-patch+json");
        self.request(builder, body).await
    }

    /// PATCH com `Content-Type: application/json` (Git Pull Requests).
    ///
    /// Este método é separado de [`Self::patch`] porque Work Items exigem
    /// `application/json-patch+json`, enquanto Git Pull Requests aceitam o
    /// objeto JSON mínimo de atualização.
    ///
    /// # Errors
    ///
    /// Retorna [`AppError::Azure`] em status >= 300 ou payload inválido e
    /// [`AppError::Http`] em falha de transporte.
    pub async fn patch_json<B: Serialize, T: for<'de> Deserialize<'de>>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T> {
        let builder = self.json_patch_builder(path);
        self.request(builder, body).await
    }

    fn json_patch_builder(&self, path: &str) -> reqwest::RequestBuilder {
        self.inner
            .patch(self.url(path))
            .header(CONTENT_TYPE, "application/json")
    }

    /// Executa um `DELETE` que não precisa de corpo de resposta.
    ///
    /// # Errors
    ///
    /// Retorna [`AppError::Azure`] quando o Azure devolve status HTTP >= 300
    /// ou [`AppError::Http`] em falha de transporte.
    pub async fn delete(&self, path: &str) -> Result<()> {
        let res = self
            .apply_timeout(self.inner.delete(self.url(path)))
            .header(ACCEPT, "application/json")
            .header(AUTHORIZATION, self.auth_header())
            .send()
            .await?;
        let status = res.status().as_u16();
        if status >= 300 {
            let body = res.text().await.unwrap_or_default();
            return Err(AppError::Azure {
                status,
                message: body.chars().take(500).collect(),
            });
        }
        Ok(())
    }

    /// Envia corpo JSON com auth + accept padrão e decodifica a resposta.
    async fn request<B: Serialize, T: for<'de> Deserialize<'de>>(
        &self,
        builder: reqwest::RequestBuilder,
        body: &B,
    ) -> Result<T> {
        let res = self
            .apply_timeout(builder)
            .header(ACCEPT, "application/json")
            .header(AUTHORIZATION, self.auth_header())
            .json(body)
            .send()
            .await?;
        let status = res.status().as_u16();
        let text = res.text().await.unwrap_or_default();
        if status >= 300 {
            return Err(AppError::Azure {
                status,
                message: text.chars().take(500).collect(),
            });
        }
        decode_json(status, &text)
    }
}

/// Constrói cliente a partir do remote + PAT.
///
/// # Errors
///
/// Retorna erro se PAT vazio ou sem remote.
pub fn client_for(remote: Option<&RepositoryRemote>, pat: &str) -> Result<AzureClient> {
    if pat.trim().is_empty() {
        return Err(AppError::Config {
            message: "azure pat não configurado (execute `prt init`)".to_owned(),
        });
    }
    let Some(r) = remote else {
        return Err(AppError::Git {
            message: "remote azure devops não encontrado".to_owned(),
        });
    };
    Ok(AzureClient::new(&r.organization, pat))
}

/// Constrói cliente autenticado com timeout por requisição.
///
/// # Errors
///
/// Retorna os mesmos erros de [`client_for`].
pub fn client_for_with_timeout(
    remote: Option<&RepositoryRemote>,
    pat: &str,
    timeout: Duration,
) -> Result<AzureClient> {
    if pat.trim().is_empty() {
        return Err(AppError::Config {
            message: "azure pat não configurado (execute `prt init`)".to_owned(),
        });
    }
    let Some(r) = remote else {
        return Err(AppError::Git {
            message: "remote azure devops não encontrado".to_owned(),
        });
    };
    Ok(AzureClient::new_with_timeout(&r.organization, pat, timeout))
}

/// Work item mínimo.
#[derive(Debug, Clone, Deserialize)]
pub struct WorkItem {
    /// ID.
    pub id: i64,
    /// Campos (título, tipo, etc).
    #[serde(default)]
    pub fields: std::collections::HashMap<String, serde_json::Value>,
    /// Relações com outros work items, quando solicitadas à API.
    #[serde(default, deserialize_with = "deserialize_relations")]
    pub relations: Vec<WorkItemRelation>,
}

/// Relação retornada pela API de Work Items.
#[derive(Debug, Clone, Deserialize)]
pub struct WorkItemRelation {
    /// Tipo da relação (`System.LinkTypes.Related`, por exemplo).
    pub rel: String,
    /// URL do recurso relacionado.
    pub url: String,
}

fn deserialize_relations<'de, D>(
    deserializer: D,
) -> std::result::Result<Vec<WorkItemRelation>, D::Error>
where
    D: Deserializer<'de>,
{
    Option::<Vec<WorkItemRelation>>::deserialize(deserializer).map(Option::unwrap_or_default)
}

impl WorkItem {
    /// Lê um campo textual (vazio quando ausente ou não textual).
    #[must_use]
    pub fn field_text(&self, field: &str) -> &str {
        self.fields
            .get(field)
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
    }

    /// Título (`System.Title`).
    #[must_use]
    pub fn title(&self) -> &str {
        self.field_text("System.Title")
    }

    /// Tipo (`System.WorkItemType`).
    #[must_use]
    pub fn work_item_type(&self) -> &str {
        self.field_text("System.WorkItemType")
    }
}

/// Busca work item por id.
///
/// # Errors
///
/// Propaga [`AppError::Azure`].
pub async fn get_work_item(client: &AzureClient, id: &str) -> Result<WorkItem> {
    client.get(&format!("_apis/wit/workitems/{id}")).await
}

/// Busca um work item incluindo suas relações.
///
/// # Errors
///
/// Propaga [`AppError::Azure`] e [`AppError::Http`] do cliente.
pub async fn get_work_item_with_relations(client: &AzureClient, id: &str) -> Result<WorkItem> {
    client
        .get(&format!("_apis/wit/workitems/{id}?$expand=relations"))
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_should_require_pat() {
        assert!(client_for(None, "").is_err());
    }

    #[test]
    fn client_should_trim_pat_before_authentication() {
        let client = AzureClient::new("org", "  token  ");
        assert_eq!(client.pat, "token");
    }

    #[test]
    fn request_timeout_should_be_opt_in_for_tui_clients() {
        assert_eq!(AzureClient::new("org", "token").request_timeout, None);
        assert_eq!(
            AzureClient::new_with_timeout("org", "token", Duration::from_secs(30)).request_timeout,
            Some(Duration::from_secs(30))
        );
    }

    #[test]
    fn work_item_should_read_title() {
        let wi: WorkItem = serde_json::from_value(serde_json::json!({
            "id": 11763,
            "fields": {"System.Title": "Minha task", "System.WorkItemType": "Task"}
        }))
        .unwrap();
        assert_eq!(wi.title(), "Minha task");
        assert_eq!(wi.work_item_type(), "Task");
        assert!(wi.relations.is_empty());

        let wi: WorkItem = serde_json::from_value(serde_json::json!({
            "id": 11764,
            "relations": null,
        }))
        .unwrap();
        assert!(wi.relations.is_empty());
    }

    #[test]
    fn decode_should_reject_empty_body_with_actionable_message() {
        let err = decode_json::<serde_json::Value>(203, "").unwrap_err();
        assert_eq!(
            err.to_string(),
            "azure devops (http 203): resposta vazia do Azure DevOps (PAT inválido/expirado? rode `prt doctor`)"
        );
    }

    #[test]
    fn decode_should_reject_html_body_with_snippet() {
        let err =
            decode_json::<serde_json::Value>(203, "<html>\n<head><title>Login</title></head>")
                .unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("não-JSON"), "{msg}");
        assert!(msg.contains("Login"), "{msg}");
    }

    #[test]
    fn decode_should_accept_json_with_leading_whitespace() {
        let v: serde_json::Value = decode_json(200, "  \n{\"a\": 1}  ").expect("json válido");
        assert_eq!(v["a"], 1);
    }
}
