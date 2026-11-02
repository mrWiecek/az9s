//! Azure Resource Graph client.
//!
//! One KQL query lists resources across every subscription in scope, with
//! server-side filtering and paging. This replaces one ARM call per
//! subscription, which does not scale to hundreds of subscriptions.
//!
//! REST: `POST {endpoint}/providers/Microsoft.ResourceGraph/resources`.
//! Results are paged with `$skipToken`. Resource Graph throttles per user;
//! the remaining quota is reported in `x-ms-user-quota-remaining` and
//! `x-ms-user-quota-resets-after` headers, and exceeding it returns 429.

use crate::models::{AzureResource, AzureResourceGroup};
use crate::{AzureError, Result};
use azure_core::auth::TokenCredential;
use futures::future::BoxFuture;
use futures::FutureExt;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, warn};

pub const DEFAULT_ENDPOINT: &str = "https://management.azure.com";
const API_VERSION: &str = "2022-10-01";
const ARM_SCOPE: &str = "https://management.azure.com/.default";
/// Resource Graph's maximum page size.
pub const MAX_PAGE_SIZE: u32 = 1000;
/// Resource Graph accepts at most this many subscriptions per request.
const MAX_SUBSCRIPTIONS_PER_REQUEST: usize = 1000;
const MAX_RETRIES: u32 = 4;

/// Supplies bearer tokens for Azure Resource Manager.
pub trait TokenSource: Send + Sync {
    fn token(&self) -> BoxFuture<'_, Result<String>>;
}

/// Tokens from an Azure SDK credential (e.g. the Azure CLI credential).
pub struct CredentialTokenSource(pub Arc<dyn TokenCredential>);

impl TokenSource for CredentialTokenSource {
    fn token(&self) -> BoxFuture<'_, Result<String>> {
        async move {
            let token = self
                .0
                .get_token(&[ARM_SCOPE])
                .await
                .map_err(|e| AzureError::AuthenticationFailed(e.to_string()))?;
            Ok(token.token.secret().to_string())
        }
        .boxed()
    }
}

/// Where a query runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GraphScope {
    /// A management group and everything below it. The tenant root
    /// management group's ID is the tenant ID.
    ManagementGroup(String),
    /// An explicit list of subscriptions.
    Subscriptions(Vec<String>),
}

pub struct ResourceGraphClient {
    http: reqwest::Client,
    endpoint: String,
    tokens: Arc<dyn TokenSource>,
    page_size: u32,
}

impl ResourceGraphClient {
    pub fn new(tokens: Arc<dyn TokenSource>) -> Self {
        Self::with_endpoint(DEFAULT_ENDPOINT, tokens)
    }

    /// Use a different endpoint (sovereign clouds, tests).
    pub fn with_endpoint(endpoint: impl Into<String>, tokens: Arc<dyn TokenSource>) -> Self {
        Self {
            http: reqwest::Client::new(),
            endpoint: endpoint.into().trim_end_matches('/').to_string(),
            tokens,
            page_size: MAX_PAGE_SIZE,
        }
    }

    pub fn with_page_size(mut self, page_size: u32) -> Self {
        self.page_size = page_size.clamp(1, MAX_PAGE_SIZE);
        self
    }

    /// The tenant the current token belongs to (its tenant root management
    /// group has the same ID).
    pub async fn home_tenant_id(&self) -> Result<String> {
        let token = self.tokens.token().await?;
        tenant_id_from_jwt(&token).ok_or_else(|| {
            AzureError::AuthenticationFailed("token has no tenant (tid) claim".into())
        })
    }

    /// Run `query` over `scope`, calling `on_page` with each page of rows as
    /// it arrives. Returns the total number of rows.
    pub async fn query_pages<F>(
        &self,
        query: &str,
        scope: &GraphScope,
        mut on_page: F,
    ) -> Result<usize>
    where
        F: FnMut(Vec<Value>),
    {
        let batches: Vec<GraphScope> = match scope {
            GraphScope::Subscriptions(ids) if ids.len() > MAX_SUBSCRIPTIONS_PER_REQUEST => ids
                .chunks(MAX_SUBSCRIPTIONS_PER_REQUEST)
                .map(|c| GraphScope::Subscriptions(c.to_vec()))
                .collect(),
            other => vec![other.clone()],
        };

        let mut total = 0;
        for batch in &batches {
            let mut skip_token: Option<String> = None;
            loop {
                let page = self.query_page(query, batch, skip_token.as_deref()).await?;
                total += page.rows.len();
                on_page(page.rows);
                match page.skip_token {
                    Some(token) => skip_token = Some(token),
                    None => break,
                }
            }
        }
        Ok(total)
    }

    async fn query_page(
        &self,
        query: &str,
        scope: &GraphScope,
        skip_token: Option<&str>,
    ) -> Result<Page> {
        let mut options = json!({ "$top": self.page_size, "resultFormat": "objectArray" });
        if let Some(token) = skip_token {
            options["$skipToken"] = json!(token);
        }
        let mut body = json!({ "query": query, "options": options });
        match scope {
            GraphScope::ManagementGroup(id) => body["managementGroups"] = json!([id]),
            GraphScope::Subscriptions(ids) => body["subscriptions"] = json!(ids),
        }
        let url = format!(
            "{}/providers/Microsoft.ResourceGraph/resources?api-version={}",
            self.endpoint, API_VERSION
        );

        let mut attempt = 0;
        loop {
            let token = self.tokens.token().await?;
            let response = self
                .http
                .post(&url)
                .bearer_auth(token)
                .header(reqwest::header::CONTENT_TYPE, "application/json")
                .body(body.to_string())
                .send()
                .await
                .map_err(|e| AzureError::ApiError(format!("Resource Graph request failed: {e}")))?;

            let status = response.status();
            let throttle = Throttle::from_headers(response.headers());
            let text = response
                .text()
                .await
                .map_err(|e| AzureError::ApiError(format!("Resource Graph response: {e}")))?;

            if status.as_u16() == 429 && attempt < MAX_RETRIES {
                attempt += 1;
                let wait = throttle
                    .retry_after
                    .or(throttle.resets_after)
                    .unwrap_or(Duration::from_secs(1 << attempt));
                warn!("Resource Graph throttled; retrying in {:?}", wait);
                tokio::time::sleep(wait).await;
                continue;
            }
            if !status.is_success() {
                return Err(AzureError::Http {
                    status: status.as_u16(),
                    message: error_message(&text),
                });
            }

            // Stay under the per-user quota rather than hitting 429s.
            if throttle.remaining == Some(0) {
                if let Some(wait) = throttle.resets_after {
                    debug!("Resource Graph quota exhausted; waiting {:?}", wait);
                    tokio::time::sleep(wait).await;
                }
            }

            let value: Value = serde_json::from_str(&text).map_err(|e| {
                AzureError::ApiError(format!("Invalid Resource Graph response: {e}"))
            })?;
            let rows = match value.get("data") {
                Some(Value::Array(rows)) => rows.clone(),
                _ => Vec::new(),
            };
            let skip_token = value
                .get("$skipToken")
                .and_then(Value::as_str)
                .filter(|t| !t.is_empty())
                .map(str::to_string);
            debug!(
                "Resource Graph page: {} rows, more: {}",
                rows.len(),
                skip_token.is_some()
            );
            return Ok(Page { rows, skip_token });
        }
    }
}

struct Page {
    rows: Vec<Value>,
    skip_token: Option<String>,
}

#[derive(Debug, Default)]
struct Throttle {
    remaining: Option<u64>,
    resets_after: Option<Duration>,
    retry_after: Option<Duration>,
}

impl Throttle {
    fn from_headers(headers: &reqwest::header::HeaderMap) -> Self {
        let get = |name: &str| headers.get(name).and_then(|v| v.to_str().ok());
        Self {
            remaining: get("x-ms-user-quota-remaining").and_then(|v| v.parse().ok()),
            resets_after: get("x-ms-user-quota-resets-after").and_then(parse_hms),
            retry_after: get("retry-after")
                .and_then(|v| v.parse::<u64>().ok())
                .map(Duration::from_secs),
        }
    }
}

/// Parse `hh:mm:ss` (as in `x-ms-user-quota-resets-after`).
fn parse_hms(value: &str) -> Option<Duration> {
    let mut parts = value.split(':');
    let h: u64 = parts.next()?.trim().parse().ok()?;
    let m: u64 = parts.next()?.trim().parse().ok()?;
    let s: f64 = parts.next()?.trim().parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some(Duration::from_secs(h * 3600 + m * 60) + Duration::from_secs_f64(s))
}

/// Extract `error.message` from an ARM error body, or return the raw text.
fn error_message(body: &str) -> String {
    serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|v| {
            v.pointer("/error/message")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_else(|| body.chars().take(300).collect())
}

/// Read the `tid` (tenant ID) claim from a JWT without verifying it. The token
/// comes from our own credential; we only need to know which tenant it is for.
pub fn tenant_id_from_jwt(token: &str) -> Option<String> {
    let payload = token.split('.').nth(1)?;
    let bytes = base64::decode_config(payload, base64::URL_SAFE_NO_PAD).ok()?;
    let claims: Value = serde_json::from_slice(&bytes).ok()?;
    claims.get("tid")?.as_str().map(str::to_string)
}

/// Quote a value as a KQL string literal.
pub fn kql_string(value: &str) -> String {
    format!("'{}'", value.replace('\\', "\\\\").replace('\'', "\\'"))
}

/// KQL listing resource groups.
pub fn resource_groups_query() -> String {
    "resourcecontainers \
     | where type =~ 'microsoft.resources/subscriptions/resourcegroups' \
     | project id, name, location, subscriptionId, tags \
     | order by name asc"
        .to_string()
}

/// KQL listing resources, optionally only those in `resource_group`.
pub fn resources_query(resource_group: Option<&str>) -> String {
    let filter = resource_group
        .map(|rg| format!("| where resourceGroup =~ {} ", kql_string(rg)))
        .unwrap_or_default();
    format!(
        "resources {filter}\
         | project id, name, type, location, resourceGroup, subscriptionId, tags \
         | order by name asc"
    )
}

fn str_field(row: &Value, key: &str) -> String {
    row.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn tags(row: &Value) -> HashMap<String, String> {
    row.get("tags")
        .and_then(Value::as_object)
        .map(|obj| {
            obj.iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                .collect()
        })
        .unwrap_or_default()
}

/// Resource group name in its original casing, taken from the resource ID.
/// (Resource Graph's `resourceGroup` column is lowercased.)
fn resource_group_from_id(id: &str) -> Option<String> {
    let mut parts = id.split('/');
    while let Some(part) = parts.next() {
        if part.eq_ignore_ascii_case("resourceGroups") {
            return parts.next().map(str::to_string);
        }
    }
    None
}

pub fn resource_group_from_row(row: &Value) -> Option<AzureResourceGroup> {
    let id = str_field(row, "id");
    if id.is_empty() {
        return None;
    }
    Some(AzureResourceGroup {
        name: str_field(row, "name"),
        location: str_field(row, "location"),
        subscription_id: str_field(row, "subscriptionId"),
        tags: tags(row),
        id,
    })
}

pub fn resource_from_row(row: &Value) -> Option<AzureResource> {
    let id = str_field(row, "id");
    if id.is_empty() {
        return None;
    }
    let resource_group =
        resource_group_from_id(&id).unwrap_or_else(|| str_field(row, "resourceGroup"));
    Some(AzureResource {
        name: str_field(row, "name"),
        resource_type: str_field(row, "type"),
        location: str_field(row, "location"),
        resource_group,
        subscription_id: str_field(row, "subscriptionId"),
        tags: tags(row),
        id,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{ok, serve_sequence, Reply, StaticToken};

    fn client(endpoint: &str) -> ResourceGraphClient {
        ResourceGraphClient::with_endpoint(endpoint, Arc::new(StaticToken("t".into())))
    }

    #[tokio::test]
    async fn pages_through_skip_tokens_and_streams_each_page() {
        let (endpoint, bodies) = serve_sequence(vec![
            ok(json!({"data": [{"id": "1"}, {"id": "2"}], "$skipToken": "next"})),
            ok(json!({"data": [{"id": "3"}]})),
        ])
        .await;

        let mut pages = Vec::new();
        let total = client(&endpoint)
            .query_pages(
                "resources",
                &GraphScope::ManagementGroup("tenant-1".into()),
                |rows| pages.push(rows.len()),
            )
            .await
            .unwrap();

        assert_eq!(total, 3);
        assert_eq!(pages, vec![2, 1]);
        let bodies = bodies.lock().unwrap();
        assert_eq!(bodies[0]["managementGroups"], json!(["tenant-1"]));
        assert_eq!(bodies[0]["options"]["$top"], json!(1000));
        assert!(bodies[0]["options"].get("$skipToken").is_none());
        assert_eq!(bodies[1]["options"]["$skipToken"], json!("next"));
    }

    #[tokio::test]
    async fn retries_after_throttling() {
        let (endpoint, bodies) = serve_sequence(vec![
            Reply {
                status: 429,
                headers: vec![("x-ms-user-quota-resets-after", "00:00:00.05")],
                body: "{}".into(),
            },
            ok(json!({"data": [{"id": "1"}]})),
        ])
        .await;

        let total = client(&endpoint)
            .query_pages(
                "resources",
                &GraphScope::Subscriptions(vec!["s".into()]),
                |_| {},
            )
            .await
            .unwrap();
        assert_eq!(total, 1);
        assert_eq!(bodies.lock().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn http_errors_carry_status_and_arm_message() {
        let (endpoint, _) = serve_sequence(vec![Reply {
            status: 403,
            headers: vec![],
            body: json!({"error": {"code": "AuthorizationFailed", "message": "no access to root"}})
                .to_string(),
        }])
        .await;

        let err = client(&endpoint)
            .query_pages(
                "resources",
                &GraphScope::ManagementGroup("t".into()),
                |_| {},
            )
            .await
            .unwrap_err();
        match err {
            AzureError::Http { status, message } => {
                assert_eq!(status, 403);
                assert_eq!(message, "no access to root");
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn parses_quota_reset_header() {
        assert_eq!(parse_hms("00:00:05"), Some(Duration::from_secs(5)));
        assert_eq!(parse_hms("00:01:00.5"), Some(Duration::from_millis(60_500)));
        assert_eq!(parse_hms("garbage"), None);
    }

    #[test]
    fn reads_tenant_from_jwt() {
        let payload =
            base64::encode_config(r#"{"tid":"tenant-123","oid":"x"}"#, base64::URL_SAFE_NO_PAD);
        let token = format!("header.{payload}.signature");
        assert_eq!(tenant_id_from_jwt(&token).as_deref(), Some("tenant-123"));
        assert_eq!(tenant_id_from_jwt("not-a-jwt"), None);
    }

    #[test]
    fn builds_queries_with_escaped_filters() {
        assert!(resources_query(None).starts_with("resources | project"));
        let q = resources_query(Some("rg-o'brien"));
        assert!(q.contains(r"where resourceGroup =~ 'rg-o\'brien'"), "{q}");
        assert!(resource_groups_query().contains("resourcecontainers"));
    }

    #[test]
    fn maps_rows_and_keeps_original_resource_group_casing() {
        let row = json!({
            "id": "/subscriptions/s1/resourceGroups/RG-Prod/providers/Microsoft.Compute/virtualMachines/vm1",
            "name": "vm1",
            "type": "microsoft.compute/virtualmachines",
            "location": "westeurope",
            "resourceGroup": "rg-prod",
            "subscriptionId": "s1",
            "tags": {"env": "prod", "n": 1}
        });
        let r = resource_from_row(&row).unwrap();
        assert_eq!(r.resource_group, "RG-Prod");
        assert_eq!(r.resource_type, "microsoft.compute/virtualmachines");
        assert_eq!(r.tags.get("env").map(String::as_str), Some("prod"));
        assert_eq!(r.tags.len(), 1, "non-string tag values are dropped");
        assert!(resource_from_row(&json!({"name": "no id"})).is_none());

        let rg = resource_group_from_row(&json!({
            "id": "/subscriptions/s1/resourceGroups/RG-Prod",
            "name": "RG-Prod", "location": "westeurope", "subscriptionId": "s1"
        }))
        .unwrap();
        assert_eq!(rg.name, "RG-Prod");
        assert!(rg.tags.is_empty());
    }
}
