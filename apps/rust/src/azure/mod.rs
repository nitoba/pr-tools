//! Cliente Azure DevOps REST — espelha `infrastructure/azure/*` do Dart.
//!
//! Auth: `Authorization: Basic base64(:PAT)`, `api-version=7.1`.
//! Usa `reqwest` + `tokio` (nunca segura lock através de `.await`).

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use reqwest::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE};
use serde::{Deserialize, Serialize};

use crate::error::{AppError, Result};
use crate::git::RepositoryRemote;

pub mod pull_requests;
pub mod work_items;

/// Cliente HTTP autenticado.
#[derive(Debug, Clone)]
pub struct AzureClient {
    inner: reqwest::Client,
    organization: String,
    pat: String,
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
        Self {
            inner: reqwest::Client::new(),
            organization: organization.to_owned(),
            pat: pat.trim().to_owned(),
        }
    }

    fn auth_header(&self) -> String {
        format!("Basic {}", BASE64.encode(format!(":{}", self.pat)))
    }

    fn url(&self, path: &str) -> String {
        format!(
            "https://dev.azure.com/{}/{}?api-version=7.1",
            self.organization,
            path.trim_start_matches('/')
        )
    }

    /// GET genérico que decodifica JSON.
    ///
    /// # Errors
    ///
    /// Retorna [`AppError::Azure`] em status >= 300 ou falha de transporte.
    pub async fn get<T: for<'de> Deserialize<'de>>(&self, path: &str) -> Result<T> {
        let res = self
            .inner
            .get(self.url(path))
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

    /// GET em URL absoluta (ex.: `vssps` de identidades).
    ///
    /// # Errors
    ///
    /// Retorna [`AppError::Azure`] em status >= 300 ou falha de transporte.
    pub async fn get_abs<T: for<'de> Deserialize<'de>>(&self, url: &str) -> Result<T> {
        let res = self
            .inner
            .get(url)
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

    /// Envia corpo JSON com auth + accept padrão e decodifica a resposta.
    async fn request<B: Serialize, T: for<'de> Deserialize<'de>>(
        &self,
        builder: reqwest::RequestBuilder,
        body: &B,
    ) -> Result<T> {
        let res = builder
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

/// Work item mínimo.
#[derive(Debug, Clone, Deserialize)]
pub struct WorkItem {
    /// ID.
    pub id: i64,
    /// Campos (título, tipo, etc).
    #[serde(default)]
    pub fields: std::collections::HashMap<String, serde_json::Value>,
}

impl WorkItem {
    /// Título (`System.Title`).
    #[must_use]
    pub fn title(&self) -> &str {
        self.fields
            .get("System.Title")
            .and_then(|v| v.as_str())
            .unwrap_or("")
    }

    /// Tipo (`System.WorkItemType`).
    #[must_use]
    pub fn work_item_type(&self) -> &str {
        self.fields
            .get("System.WorkItemType")
            .and_then(|v| v.as_str())
            .unwrap_or("")
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
    fn work_item_should_read_title() {
        let wi: WorkItem = serde_json::from_value(serde_json::json!({
            "id": 11763,
            "fields": {"System.Title": "Minha task", "System.WorkItemType": "Task"}
        }))
        .unwrap();
        assert_eq!(wi.title(), "Minha task");
        assert_eq!(wi.work_item_type(), "Task");
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
