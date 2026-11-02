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
use crate::{timing, AzureError, Result};
use azure_core::auth::TokenCredential;
use futures::future::BoxFuture;
use futures::FutureExt;
use futures::{stream, StreamExt};
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

/// Caches the bearer token until shortly before it expires, so a load of
/// many pages costs one credential call (an Azure CLI call is ~0.5-0.8 s).
/// Concurrent callers wait for the same refresh instead of each fetching.
pub struct CachingTokenSource<T> {
    inner: T,
    cached: tokio::sync::Mutex<Option<(String, i64)>>,
}

/// Refresh this long before the token's `exp`.
const TOKEN_REFRESH_MARGIN_SECS: i64 = 300;
/// Assumed lifetime when the token has no readable `exp` claim.
const TOKEN_DEFAULT_LIFETIME_SECS: i64 = 600;

impl<T: TokenSource> CachingTokenSource<T> {
    pub fn new(inner: T) -> Self {
        Self {
            inner,
            cached: tokio::sync::Mutex::new(None),
        }
    }
}

impl<T: TokenSource> TokenSource for CachingTokenSource<T> {
    fn token(&self) -> BoxFuture<'_, Result<String>> {
        async move {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or_default();
            let mut cached = self.cached.lock().await;
            if let Some((token, expires)) = cached.as_ref() {
                if expires - now > TOKEN_REFRESH_MARGIN_SECS {
                    return Ok(token.clone());
                }
            }
            let token = {
                let _t = timing::timer("azure.token");
                self.inner.token().await?
            };
            let expires = claims(&token)
                .and_then(|c| c.get("exp").and_then(Value::as_i64))
                .unwrap_or(now + TOKEN_DEFAULT_LIFETIME_SECS);
            *cached = Some((token.clone(), expires));
            Ok(token)
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
            total += self.query_batch(query, batch, &mut on_page).await?;
        }
        Ok(total)
    }

    /// Fetch every page of one scope, in order.
    ///
    /// The first page tells us `totalRecords`. The remaining pages are then
    /// requested by offset (`$skip`), [`PARALLEL_PAGES`] at a time, and
    /// delivered in order. Sequential `$skipToken` paging is about 0.6 s per
    /// page, which dominated large loads (3 extra pages ≈ 1.75 s at 400+
    /// subscriptions). If an offset request fails, paging continues with
    /// `$skipToken`, dropping rows already delivered so nothing repeats.
    async fn query_batch<F>(
        &self,
        query: &str,
        scope: &GraphScope,
        on_page: &mut F,
    ) -> Result<usize>
    where
        F: FnMut(Vec<Value>),
    {
        let first = self.query_page(query, scope, Paging::First).await?;
        let page_size = self.page_size as usize;
        let mut delivered = first.rows.len();
        let full_first_page = delivered == page_size;
        on_page(first.rows);
        let Some(next_token) = first.skip_token else {
            return Ok(delivered);
        };

        if let (Some(total_records), true) = (first.total_records, full_first_page) {
            let offsets: Vec<usize> = (1..)
                .map(|i| i * page_size)
                .take_while(|offset| *offset < total_records)
                .collect();
            let mut pages = stream::iter(offsets)
                .map(|offset| self.query_page(query, scope, Paging::Skip(offset)))
                .buffered(PARALLEL_PAGES);
            let mut failure = None;
            while let Some(page) = pages.next().await {
                match page {
                    Ok(page) if page.rows.is_empty() => break,
                    Ok(page) => {
                        delivered += page.rows.len();
                        on_page(page.rows);
                    }
                    Err(e) => {
                        failure = Some(e);
                        break;
                    }
                }
            }
            match failure {
                None => return Ok(delivered),
                Some(e) => warn!(
                    "Parallel Resource Graph paging failed after {} rows ({}); continuing with $skipToken",
                    delivered, e
                ),
            }
        }

        // Sequential paging, skipping rows the parallel pass already delivered.
        let mut already = delivered - page_size.min(delivered);
        let mut token = Some(next_token);
        while let Some(current) = token {
            let page = self
                .query_page(query, scope, Paging::Token(&current))
                .await?;
            let mut rows = page.rows;
            let drop = already.min(rows.len());
            rows.drain(..drop);
            already -= drop;
            if !rows.is_empty() {
                delivered += rows.len();
                on_page(rows);
            }
            token = page.skip_token;
        }
        Ok(delivered)
    }

    async fn query_page(
        &self,
        query: &str,
        scope: &GraphScope,
        paging: Paging<'_>,
    ) -> Result<Page> {
        let _t = timing::timer("azure.graph_page");
        let mut options = json!({ "$top": self.page_size, "resultFormat": "objectArray" });
        match paging {
            Paging::First => {}
            Paging::Token(token) => options["$skipToken"] = json!(token),
            Paging::Skip(offset) => options["$skip"] = json!(offset),
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
            let total_records = value
                .get("totalRecords")
                .and_then(Value::as_u64)
                .map(|n| n as usize);
            return Ok(Page {
                rows,
                skip_token,
                total_records,
            });
        }
    }
}

struct Page {
    rows: Vec<Value>,
    skip_token: Option<String>,
    total_records: Option<usize>,
}

/// Which page to ask for.
#[derive(Clone, Copy)]
enum Paging<'a> {
    First,
    Token(&'a str),
    Skip(usize),
}

/// Pages requested at once after the first. Resource Graph allows about 15
/// requests per 5 seconds per user; 4 in flight stays well inside that for
/// typical loads, and 429s are retried with the server's back-off anyway.
pub const PARALLEL_PAGES: usize = 4;

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
    claims(token)?.get("tid")?.as_str().map(str::to_string)
}

/// Decode a JWT's payload (claims) without verifying it.
fn claims(token: &str) -> Option<Value> {
    let payload = token.split('.').nth(1)?;
    let bytes = base64::decode_config(payload, base64::URL_SAFE_NO_PAD).ok()?;
    serde_json::from_slice(&bytes).ok()
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
    use crate::testing::{error, ok, serve, serve_sequence, Reply, StaticToken};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Instant;

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
                delay: Default::default(),
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
            delay: Default::default(),
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

    /// Mock of a result set of `total` rows (`{"id": "<n>"}`) paged by
    /// `page` rows: the first page reports totalRecords and a skipToken;
    /// `$skip` requests return that offset, slower for earlier offsets so the
    /// test can tell whether delivery stays in order.
    fn paged(
        total: usize,
        page: usize,
        skip_fails_from: Option<usize>,
    ) -> impl Fn(&Value) -> Reply {
        move |body: &Value| {
            let rows = |from: usize| -> Vec<Value> {
                (from..(from + page).min(total))
                    .map(|i| json!({"id": i.to_string()}))
                    .collect()
            };
            let opts = &body["options"];
            if let Some(skip) = opts.get("$skip").and_then(Value::as_u64) {
                let skip = skip as usize;
                if skip_fails_from.is_some_and(|f| skip >= f) {
                    return error(400, "$skip not supported here");
                }
                let delay = Duration::from_millis(((total - skip) * 10) as u64);
                return ok(json!({"data": rows(skip), "totalRecords": total})).after(delay);
            }
            let from = match opts.get("$skipToken").and_then(Value::as_str) {
                None => 0,
                Some(t) => t.trim_start_matches('t').parse::<usize>().unwrap(),
            };
            let mut reply = json!({"data": rows(from), "totalRecords": total});
            if from + page < total {
                reply["$skipToken"] = json!(format!("t{}", from + page));
            }
            ok(reply)
        }
    }

    fn ids(pages: &[Vec<Value>]) -> Vec<usize> {
        pages
            .iter()
            .flatten()
            .map(|r| r["id"].as_str().unwrap().parse().unwrap())
            .collect()
    }

    #[tokio::test]
    async fn later_pages_are_fetched_in_parallel_by_offset_and_delivered_in_order() {
        let (endpoint, requests) = serve(paged(7, 2, None)).await;
        let mut pages = Vec::new();
        let total = client(&endpoint)
            .with_page_size(2)
            .query_pages(
                "resources",
                &GraphScope::ManagementGroup("t".into()),
                |rows| pages.push(rows),
            )
            .await
            .unwrap();

        assert_eq!(total, 7);
        assert_eq!(
            ids(&pages),
            (0..7).collect::<Vec<_>>(),
            "in order, no gaps, no repeats"
        );
        let requests = requests.lock().unwrap();
        let mut skips: Vec<u64> = requests
            .iter()
            .filter_map(|b| b["options"].get("$skip").and_then(Value::as_u64))
            .collect();
        skips.sort();
        assert_eq!(skips, [2, 4, 6]);
        assert!(requests
            .iter()
            .all(|b| b["options"].get("$skipToken").is_none()));
    }

    #[tokio::test]
    async fn parallel_paging_is_faster_than_sequential() {
        let page_delay = Duration::from_millis(200);
        let (endpoint, _) = serve(move |body: &Value| {
            let skip = body["options"]
                .get("$skip")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            let rows: Vec<Value> = (skip..skip + 2)
                .map(|i| json!({"id": i.to_string()}))
                .collect();
            let mut reply = json!({"data": rows, "totalRecords": 12});
            if skip == 0 {
                reply["$skipToken"] = json!("t2");
            }
            ok(reply).after(page_delay)
        })
        .await;

        let started = Instant::now();
        let total = client(&endpoint)
            .with_page_size(2)
            .query_pages(
                "resources",
                &GraphScope::Subscriptions(vec!["s".into()]),
                |_| {},
            )
            .await
            .unwrap();
        let elapsed = started.elapsed();
        assert_eq!(total, 12);
        // 6 pages one after another: 6 x 200 ms. First page, then 5 more
        // with 4 in flight: about 3 x 200 ms.
        assert!(elapsed < Duration::from_millis(1000), "took {elapsed:?}");
    }

    #[tokio::test]
    async fn failed_offset_paging_continues_by_token_without_repeating_rows() {
        // Offsets 2 succeeds, offset 4 fails: rows 0-3 are delivered, then the
        // $skipToken pass must skip its first page (rows 2-3) and continue.
        let (endpoint, requests) = serve(paged(7, 2, Some(4))).await;
        let mut pages = Vec::new();
        let total = client(&endpoint)
            .with_page_size(2)
            .query_pages(
                "resources",
                &GraphScope::ManagementGroup("t".into()),
                |rows| pages.push(rows),
            )
            .await
            .unwrap();

        assert_eq!(total, 7);
        assert_eq!(ids(&pages), (0..7).collect::<Vec<_>>());
        assert!(requests
            .lock()
            .unwrap()
            .iter()
            .any(|b| b["options"].get("$skipToken").is_some()));
    }

    #[tokio::test]
    async fn a_short_first_page_needs_no_further_requests() {
        let (endpoint, requests) = serve(paged(1, 2, None)).await;
        let total = client(&endpoint)
            .with_page_size(2)
            .query_pages(
                "resources",
                &GraphScope::ManagementGroup("t".into()),
                |_| {},
            )
            .await
            .unwrap();
        assert_eq!(total, 1);
        assert_eq!(requests.lock().unwrap().len(), 1);
    }

    /// Token source that counts calls and hands out JWTs expiring at `exp`.
    struct Counting {
        calls: AtomicUsize,
        exp: i64,
    }

    impl TokenSource for Counting {
        fn token(&self) -> BoxFuture<'_, Result<String>> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let exp = self.exp;
            async move {
                tokio::time::sleep(Duration::from_millis(20)).await;
                let payload = base64::encode_config(
                    format!(r#"{{"tid":"t","exp":{exp}}}"#),
                    base64::URL_SAFE_NO_PAD,
                );
                Ok(format!("h.{payload}.s"))
            }
            .boxed()
        }
    }

    fn unix_now() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64
    }

    #[tokio::test]
    async fn cached_token_is_reused_until_close_to_expiry() {
        let source = CachingTokenSource::new(Counting {
            calls: AtomicUsize::new(0),
            exp: unix_now() + 3600,
        });
        for _ in 0..3 {
            source.token().await.unwrap();
        }
        assert_eq!(source.inner.calls.load(Ordering::SeqCst), 1);

        let expiring = CachingTokenSource::new(Counting {
            calls: AtomicUsize::new(0),
            exp: unix_now() + 60, // inside the refresh margin
        });
        expiring.token().await.unwrap();
        expiring.token().await.unwrap();
        assert_eq!(expiring.inner.calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn concurrent_callers_share_one_token_fetch() {
        let source = CachingTokenSource::new(Counting {
            calls: AtomicUsize::new(0),
            exp: unix_now() + 3600,
        });
        let results = futures::future::join_all((0..5).map(|_| source.token())).await;
        assert!(results.iter().all(|r| r.is_ok()));
        assert_eq!(source.inner.calls.load(Ordering::SeqCst), 1);
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

/// Real TLS handshake using the same HTTP stack as the Resource Graph client
/// (rustls with bundled and OS root certificates, no OpenSSL). Any HTTP
/// status counts as success; only a connection or TLS failure fails.
/// Needs network; set `AZ9S_TLS_TEST_URL` to test another host.
#[cfg(test)]
mod tls_tests {
    #[tokio::test]
    #[ignore] // Requires network access
    async fn tls_handshake_works_without_openssl() {
        let url = std::env::var("AZ9S_TLS_TEST_URL")
            .unwrap_or_else(|_| "https://management.azure.com/".to_string());
        let response = reqwest::Client::new().get(&url).send().await;
        match response {
            Ok(r) => println!("{url}: HTTP {}", r.status()),
            Err(e) => panic!("TLS/connection to {url} failed: {e:?}"),
        }
    }
}
