//! Background loading of Azure data.
//!
//! The UI never awaits the network. It sends a [`LoadRequest`] to the
//! [`Loader`], which runs it on a tokio task and delivers a [`LoadResult`] over
//! a channel that the event loop polls alongside terminal input.
//!
//! Each request gets a generation number. Starting a new request of the same
//! [`LoadKind`] aborts the previous task, and a result that is no longer the
//! latest for its kind is discarded by [`Loader::accept`], so a slow answer
//! for a view the user already left can never overwrite a newer one.

use crate::models::{Resource, ResourceGroup, Subscription};
use crate::timing;
use az9s_azure::resource_graph::{self, GraphScope, ResourceGraphClient};
use az9s_azure::{AzureClient, AzureError};
use futures::future::BoxFuture;
use futures::{stream, FutureExt, StreamExt};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tracing::warn;

/// Maximum concurrent ARM calls when loading many subscriptions at once.
pub const MAX_CONCURRENT_REQUESTS: usize = 16;

/// What to load.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadRequest {
    Subscriptions,
    ResourceGroups {
        subscription_ids: Vec<String>,
    },
    Resources {
        subscription_ids: Vec<String>,
        /// Only resources in this resource group (requires exactly one subscription).
        resource_group: Option<String>,
    },
}

/// Which list a request fills. One request per kind is in flight at a time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LoadKind {
    Subscriptions,
    ResourceGroups,
    Resources,
}

impl LoadKind {
    pub fn label(self) -> &'static str {
        match self {
            LoadKind::Subscriptions => "subscriptions",
            LoadKind::ResourceGroups => "resource groups",
            LoadKind::Resources => "resources",
        }
    }
}

impl LoadRequest {
    pub fn kind(&self) -> LoadKind {
        match self {
            LoadRequest::Subscriptions => LoadKind::Subscriptions,
            LoadRequest::ResourceGroups { .. } => LoadKind::ResourceGroups,
            LoadRequest::Resources { .. } => LoadKind::Resources,
        }
    }
}

#[derive(Debug)]
pub enum LoadedData {
    Subscriptions(Vec<Subscription>),
    ResourceGroups(Vec<ResourceGroup>),
    Resources(Vec<Resource>),
}

/// Outcome of a fetch: the data (or an error), plus the subscriptions that
/// failed when several were loaded at once.
#[derive(Debug)]
pub struct FetchOutcome {
    /// Shown to the user in the status bar (e.g. "fell back to ARM").
    pub notice: Option<String>,
    pub data: Result<LoadedData, String>,
    pub failed_subscriptions: Vec<String>,
}

#[derive(Debug)]
pub struct LoadResult {
    /// `false` for a partial chunk streamed while the load continues; `true`
    /// for the final message of a request.
    pub done: bool,
    pub kind: LoadKind,
    pub generation: u64,
    pub outcome: FetchOutcome,
}

/// Source of data. [`AzureFetcher`] talks to ARM; tests use fakes.
/// Lets a fetcher deliver rows as they arrive (e.g. one Resource Graph page at
/// a time), so the first page is visible before the whole load finishes.
#[derive(Clone)]
pub struct ChunkSink {
    tx: mpsc::UnboundedSender<LoadResult>,
    kind: LoadKind,
    generation: u64,
}

impl ChunkSink {
    /// Deliver a chunk of rows. The final [`FetchOutcome`] then carries only
    /// rows not already sent through the sink.
    pub fn send(&self, data: LoadedData) {
        let _ = self.tx.send(LoadResult {
            done: false,
            kind: self.kind,
            generation: self.generation,
            outcome: FetchOutcome {
                data: Ok(data),
                failed_subscriptions: Vec::new(),
                notice: None,
            },
        });
    }

    /// A sink whose chunks go nowhere (for fetchers called outside a Loader).
    pub fn discard() -> Self {
        let (tx, _) = mpsc::unbounded_channel();
        Self {
            tx,
            kind: LoadKind::Subscriptions,
            generation: 0,
        }
    }
}

/// How the loader treats an incoming [`LoadResult`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Accept {
    /// A newer request of the same kind exists; ignore it.
    Stale,
    /// A chunk of the current request; more will follow.
    Partial,
    /// The last message of the current request.
    Final,
}

pub trait Fetcher: Send + Sync + 'static {
    fn fetch(&self, request: LoadRequest, _sink: ChunkSink) -> BoxFuture<'static, FetchOutcome>;
}

pub type LoadReceiver = mpsc::UnboundedReceiver<LoadResult>;

pub struct Loader {
    fetcher: Option<Arc<dyn Fetcher>>,
    tx: mpsc::UnboundedSender<LoadResult>,
    rx: Option<LoadReceiver>,
    next_generation: u64,
    in_flight: HashMap<LoadKind, (u64, JoinHandle<()>)>,
}

impl Loader {
    /// `None` means offline: every request fails immediately with an error.
    pub fn new(fetcher: Option<Arc<dyn Fetcher>>) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        Self {
            fetcher,
            tx,
            rx: Some(rx),
            next_generation: 0,
            in_flight: HashMap::new(),
        }
    }

    /// The receiving end of the result channel. Can be taken once.
    pub fn take_receiver(&mut self) -> Option<LoadReceiver> {
        self.rx.take()
    }

    /// Start `request` in the background, cancelling any in-flight request of
    /// the same kind. Returns the request's generation.
    pub fn request(&mut self, request: LoadRequest) -> Result<u64, String> {
        let fetcher = self
            .fetcher
            .clone()
            .ok_or_else(|| "Not connected to Azure (run `az login` and press r)".to_string())?;

        let kind = request.kind();
        self.next_generation += 1;
        let generation = self.next_generation;

        if let Some((_, previous)) = self.in_flight.remove(&kind) {
            previous.abort();
        }

        let tx = self.tx.clone();
        let sink = ChunkSink {
            tx: tx.clone(),
            kind,
            generation,
        };
        let handle = tokio::spawn(async move {
            let outcome = fetcher.fetch(request, sink).await;
            // The receiver may be gone during shutdown; nothing to do then.
            let _ = tx.send(LoadResult {
                done: true,
                kind,
                generation,
                outcome,
            });
        });
        self.in_flight.insert(kind, (generation, handle));
        Ok(generation)
    }

    /// Classify an incoming result. A final result of the current request
    /// also marks the request as finished.
    pub fn accept(&mut self, result: &LoadResult) -> Accept {
        match self.in_flight.get(&result.kind) {
            Some((generation, _)) if *generation == result.generation => {
                if result.done {
                    self.in_flight.remove(&result.kind);
                    Accept::Final
                } else {
                    Accept::Partial
                }
            }
            _ => Accept::Stale,
        }
    }

    /// Whether any request is still running.
    pub fn is_loading(&self) -> bool {
        !self.in_flight.is_empty()
    }

    pub fn is_loading_kind(&self, kind: LoadKind) -> bool {
        self.in_flight.contains_key(&kind)
    }
}

impl Drop for Loader {
    fn drop(&mut self) {
        for (_, (_, handle)) in self.in_flight.drain() {
            handle.abort();
        }
    }
}

/// Fetches from Azure Resource Manager.
pub struct AzureFetcher {
    client: Arc<AzureClient>,
}

impl AzureFetcher {
    pub fn new(client: Arc<AzureClient>) -> Self {
        Self { client }
    }
}

impl Fetcher for AzureFetcher {
    fn fetch(&self, request: LoadRequest, _sink: ChunkSink) -> BoxFuture<'static, FetchOutcome> {
        let client = self.client.clone();
        async move {
            match request {
                LoadRequest::Subscriptions => {
                    let _t = timing::timer("azure.list_subscriptions");
                    FetchOutcome {
                        data: client
                            .list_subscriptions()
                            .await
                            .map(|subs| {
                                LoadedData::Subscriptions(subs.iter().map(Into::into).collect())
                            })
                            .map_err(|e| e.to_string()),
                        failed_subscriptions: Vec::new(),
                        notice: None,
                    }
                }
                LoadRequest::ResourceGroups { subscription_ids } => {
                    let _t = timing::timer("azure.load_resource_groups");
                    let (items, failed) = fetch_per_subscription(subscription_ids, move |sub| {
                        let client = client.clone();
                        async move {
                            let _t = timing::timer("azure.list_resource_groups");
                            client
                                .list_resource_groups(&sub)
                                .await
                                .map(|rgs| rgs.iter().map(ResourceGroup::from).collect::<Vec<_>>())
                        }
                    })
                    .await;
                    FetchOutcome {
                        data: items.map(LoadedData::ResourceGroups),
                        failed_subscriptions: failed,
                        notice: None,
                    }
                }
                LoadRequest::Resources {
                    subscription_ids,
                    resource_group,
                } => {
                    let _t = timing::timer("azure.load_resources");
                    let (items, failed) = fetch_per_subscription(subscription_ids, move |sub| {
                        let client = client.clone();
                        let resource_group = resource_group.clone();
                        async move {
                            let fetched = match &resource_group {
                                Some(rg) => {
                                    let _t = timing::timer("azure.list_resources_in_group");
                                    client.list_resources_in_group(&sub, rg).await
                                }
                                None => {
                                    let _t = timing::timer("azure.list_resources");
                                    client.list_resources(&sub).await
                                }
                            };
                            fetched.map(|res| res.iter().map(Resource::from).collect::<Vec<_>>())
                        }
                    })
                    .await;
                    FetchOutcome {
                        data: items.map(LoadedData::Resources),
                        failed_subscriptions: failed,
                        notice: None,
                    }
                }
            }
        }
        .boxed()
    }
}

/// Fetches lists from Azure Resource Graph: one paged query instead of one
/// ARM call per subscription. Pages are streamed to the UI as they arrive.
///
/// Scope: a single subscription is queried directly. Several subscriptions
/// ("all") are queried through the tenant root management group (its ID is
/// the tenant ID), and the rows are limited to the requested subscriptions.
/// If the management group is not readable, the query is retried scoped to
/// the subscription list. If Resource Graph fails before any rows were
/// delivered, the request falls back to ARM and a notice says so.
pub struct GraphFetcher {
    graph: Arc<ResourceGraphClient>,
    /// Used for subscriptions, and as a fallback when Resource Graph fails.
    arm: Arc<dyn Fetcher>,
    home_tenant: tokio::sync::OnceCell<Option<String>>,
}

impl GraphFetcher {
    pub fn new(graph: Arc<ResourceGraphClient>, arm: Arc<dyn Fetcher>) -> Self {
        Self {
            graph,
            arm,
            home_tenant: tokio::sync::OnceCell::new(),
        }
    }
}

impl Fetcher for GraphFetcher {
    fn fetch(&self, request: LoadRequest, sink: ChunkSink) -> BoxFuture<'static, FetchOutcome> {
        // Subscriptions come from the Azure CLI (with tenant information).
        if request == LoadRequest::Subscriptions {
            return self.arm.fetch(request, sink);
        }
        let graph = self.graph.clone();
        let arm = self.arm.clone();
        let home_tenant = self.home_tenant.clone();
        async move {
            let (query, subscription_ids, kind) = match &request {
                LoadRequest::ResourceGroups { subscription_ids } => (
                    resource_graph::resource_groups_query(),
                    subscription_ids.clone(),
                    LoadKind::ResourceGroups,
                ),
                LoadRequest::Resources {
                    subscription_ids,
                    resource_group,
                } => (
                    resource_graph::resources_query(resource_group.as_deref()),
                    subscription_ids.clone(),
                    LoadKind::Resources,
                ),
                LoadRequest::Subscriptions => unreachable!("handled above"),
            };
            if subscription_ids.is_empty() {
                return FetchOutcome {
                    data: Ok(empty(kind)),
                    failed_subscriptions: vec![],
                    notice: None,
                };
            }

            let tenant = if subscription_ids.len() > 1 {
                home_tenant
                    .get_or_init(|| async { graph.home_tenant_id().await.ok() })
                    .await
                    .clone()
            } else {
                None
            };

            let _t = timing::timer("azure.graph_query");
            let mut delivered = 0usize;
            let mut result = Err(AzureError::InvalidConfig("no scope".into()));
            if let Some(tenant) = tenant {
                result = query_into_sink(
                    &graph,
                    &query,
                    &GraphScope::ManagementGroup(tenant),
                    &subscription_ids,
                    kind,
                    &sink,
                    &mut delivered,
                )
                .await;
                if let Err(e) = &result {
                    warn!("Resource Graph at tenant root failed: {}", e);
                }
            }
            if result.is_err() && delivered == 0 {
                result = query_into_sink(
                    &graph,
                    &query,
                    &GraphScope::Subscriptions(subscription_ids.clone()),
                    &subscription_ids,
                    kind,
                    &sink,
                    &mut delivered,
                )
                .await;
            }

            match result {
                Ok(_) => FetchOutcome {
                    // Everything was streamed through the sink already.
                    data: Ok(empty(kind)),
                    failed_subscriptions: vec![],
                    notice: None,
                },
                Err(e) if delivered == 0 => {
                    warn!("Resource Graph failed, falling back to ARM: {}", e);
                    let mut outcome = arm.fetch(request, sink).await;
                    outcome.notice = Some(format!("Resource Graph unavailable ({e}); used ARM"));
                    outcome
                }
                Err(e) => FetchOutcome {
                    data: Err(format!("Resource Graph failed after {delivered} rows: {e}")),
                    failed_subscriptions: vec![],
                    notice: None,
                },
            }
        }
        .boxed()
    }
}

fn empty(kind: LoadKind) -> LoadedData {
    match kind {
        LoadKind::Subscriptions => LoadedData::Subscriptions(vec![]),
        LoadKind::ResourceGroups => LoadedData::ResourceGroups(vec![]),
        LoadKind::Resources => LoadedData::Resources(vec![]),
    }
}

/// Run a Resource Graph query and stream each page into `sink`, keeping only
/// rows from the requested subscriptions. `delivered` counts rows sent.
async fn query_into_sink(
    graph: &ResourceGraphClient,
    query: &str,
    scope: &GraphScope,
    subscription_ids: &[String],
    kind: LoadKind,
    sink: &ChunkSink,
    delivered: &mut usize,
) -> az9s_azure::Result<usize> {
    let wanted: HashSet<String> = subscription_ids
        .iter()
        .map(|s| s.to_ascii_lowercase())
        .collect();
    let wanted_sub = |sub: &str| wanted.contains(&sub.to_ascii_lowercase());
    let started = std::time::Instant::now();
    let mut first_page = true;

    graph
        .query_pages(query, scope, |rows| {
            if first_page {
                first_page = false;
                timing::record("azure.graph_first_page", started.elapsed());
            }
            let chunk = match kind {
                LoadKind::ResourceGroups => {
                    let items: Vec<ResourceGroup> = rows
                        .iter()
                        .filter_map(resource_graph::resource_group_from_row)
                        .filter(|rg| wanted_sub(&rg.subscription_id))
                        .map(|rg| ResourceGroup::from(&rg))
                        .collect();
                    *delivered += items.len();
                    LoadedData::ResourceGroups(items)
                }
                _ => {
                    let items: Vec<Resource> = rows
                        .iter()
                        .filter_map(resource_graph::resource_from_row)
                        .filter(|r| wanted_sub(&r.subscription_id))
                        .map(|r| Resource::from(&r))
                        .collect();
                    *delivered += items.len();
                    LoadedData::Resources(items)
                }
            };
            sink.send(chunk);
        })
        .await
}

/// Run `fetch_one` for every subscription, up to [`MAX_CONCURRENT_REQUESTS`]
/// at a time, and concatenate the results.
///
/// Returns an error only if every subscription failed; otherwise returns the
/// combined items plus the IDs of the subscriptions that failed.
pub async fn fetch_per_subscription<T, F, Fut, E>(
    subscription_ids: Vec<String>,
    fetch_one: F,
) -> (Result<Vec<T>, String>, Vec<String>)
where
    F: Fn(String) -> Fut,
    Fut: std::future::Future<Output = Result<Vec<T>, E>>,
    E: std::fmt::Display,
{
    let total = subscription_ids.len();
    let results: Vec<(String, Result<Vec<T>, E>)> = stream::iter(subscription_ids)
        .map(|sub| {
            let fut = fetch_one(sub.clone());
            async move { (sub, fut.await) }
        })
        .buffer_unordered(MAX_CONCURRENT_REQUESTS)
        .collect()
        .await;

    let mut items = Vec::new();
    let mut failed = Vec::new();
    let mut first_error = None;
    for (sub, result) in results {
        match result {
            Ok(mut chunk) => items.append(&mut chunk),
            Err(e) => {
                warn!("Loading subscription {} failed: {}", sub, e);
                first_error.get_or_insert_with(|| e.to_string());
                failed.push(sub);
            }
        }
    }

    if total > 0 && failed.len() == total {
        (
            Err(first_error.unwrap_or_else(|| "unknown error".into())),
            failed,
        )
    } else {
        (Ok(items), failed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    /// Returns `n` resource groups after `delay`, where `n` is the number of
    /// subscriptions requested (so tests can tell requests apart).
    struct FakeFetcher {
        delay: Duration,
    }

    impl Fetcher for FakeFetcher {
        fn fetch(
            &self,
            request: LoadRequest,
            _sink: ChunkSink,
        ) -> BoxFuture<'static, FetchOutcome> {
            let delay = self.delay;
            async move {
                tokio::time::sleep(delay).await;
                let n = match &request {
                    LoadRequest::ResourceGroups { subscription_ids } => subscription_ids.len(),
                    _ => 0,
                };
                FetchOutcome {
                    data: Ok(LoadedData::ResourceGroups(
                        (0..n)
                            .map(|i| ResourceGroup {
                                id: format!("rg-{i}"),
                                name: format!("rg-{i}"),
                                location: "westeurope".into(),
                                subscription_id: "s".into(),
                                tags: HashMap::new(),
                            })
                            .collect(),
                    )),
                    failed_subscriptions: vec![],
                    notice: None,
                }
            }
            .boxed()
        }
    }

    fn rg_request(n: usize) -> LoadRequest {
        LoadRequest::ResourceGroups {
            subscription_ids: (0..n).map(|i| format!("sub-{i}")).collect(),
        }
    }

    #[tokio::test]
    async fn request_returns_immediately_and_result_arrives_later() {
        let mut loader = Loader::new(Some(Arc::new(FakeFetcher {
            delay: Duration::from_millis(50),
        })));
        let mut rx = loader.take_receiver().unwrap();

        let started = std::time::Instant::now();
        loader.request(rg_request(3)).unwrap();
        assert!(
            started.elapsed() < Duration::from_millis(20),
            "request must not block"
        );
        assert!(loader.is_loading_kind(LoadKind::ResourceGroups));

        let result = rx.recv().await.unwrap();
        assert_eq!(loader.accept(&result), Accept::Final);
        assert!(!loader.is_loading());
        match result.outcome.data {
            Ok(LoadedData::ResourceGroups(rgs)) => assert_eq!(rgs.len(), 3),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[tokio::test]
    async fn newer_request_supersedes_older_one() {
        let mut loader = Loader::new(Some(Arc::new(FakeFetcher {
            delay: Duration::from_millis(30),
        })));
        let mut rx = loader.take_receiver().unwrap();

        let first = loader.request(rg_request(1)).unwrap();
        let second = loader.request(rg_request(2)).unwrap();
        assert!(second > first);

        // The first task was aborted, so only the second result arrives.
        let result = rx.recv().await.unwrap();
        assert_eq!(result.generation, second);
        assert_eq!(loader.accept(&result), Accept::Final);

        // A late result with an old generation is rejected.
        let stale = LoadResult {
            done: true,
            kind: LoadKind::ResourceGroups,
            generation: first,
            outcome: FetchOutcome {
                data: Ok(LoadedData::ResourceGroups(vec![])),
                failed_subscriptions: vec![],
                notice: None,
            },
        };
        assert_eq!(loader.accept(&stale), Accept::Stale);
    }

    #[tokio::test]
    async fn different_kinds_load_concurrently() {
        let mut loader = Loader::new(Some(Arc::new(FakeFetcher {
            delay: Duration::from_millis(10),
        })));
        loader.request(LoadRequest::Subscriptions).unwrap();
        loader.request(rg_request(1)).unwrap();
        assert!(loader.is_loading_kind(LoadKind::Subscriptions));
        assert!(loader.is_loading_kind(LoadKind::ResourceGroups));
    }

    #[test]
    fn offline_loader_rejects_requests() {
        let mut loader = Loader::new(None);
        assert!(loader.request(LoadRequest::Subscriptions).is_err());
        assert!(!loader.is_loading());
    }

    #[tokio::test]
    async fn fetch_per_subscription_runs_concurrently_and_reports_partial_failures() {
        let subs: Vec<String> = (0..64).map(|i| format!("sub-{i}")).collect();
        let started = std::time::Instant::now();
        let (items, failed) = fetch_per_subscription(subs, |sub| async move {
            tokio::time::sleep(Duration::from_millis(20)).await;
            if sub == "sub-7" {
                Err("forbidden")
            } else {
                Ok(vec![sub])
            }
        })
        .await;
        // 64 calls x 20ms sequentially would be 1.28s; 16 at a time is ~80ms.
        assert!(started.elapsed() < Duration::from_millis(600));
        assert_eq!(items.unwrap().len(), 63);
        assert_eq!(failed, vec!["sub-7".to_string()]);
    }

    #[tokio::test]
    async fn fetch_per_subscription_errors_only_when_everything_failed() {
        let (items, failed) = fetch_per_subscription(vec!["a".into(), "b".into()], |_| async {
            Err::<Vec<()>, _>("boom")
        })
        .await;
        assert_eq!(items.unwrap_err(), "boom");
        assert_eq!(failed.len(), 2);

        let (items, _) = fetch_per_subscription(Vec::<String>::new(), |_| async {
            Ok::<Vec<()>, &str>(vec![])
        })
        .await;
        assert!(items.unwrap().is_empty());
    }
}
