//! Background loading of Azure data.
//!
//! The UI never awaits the network. It sends a [`LoadRequest`] to the
//! [`Loader`], which runs it on a tokio task and delivers a [`LoadResult`] over
//! a channel that the event loop polls alongside terminal input.
//!
//! Each request gets a generation number. Starting a new request of the same
//! [`LoadKind`] aborts the previous task, and a result that is no longer the
//! latest for its kind is discarded by [`Loader::complete`], so a slow answer
//! for a view the user already left can never overwrite a newer one.

use crate::models::{Resource, ResourceGroup, Subscription};
use crate::timing;
use az9s_azure::AzureClient;
use futures::future::BoxFuture;
use futures::{stream, FutureExt, StreamExt};
use std::collections::HashMap;
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
    pub data: Result<LoadedData, String>,
    pub failed_subscriptions: Vec<String>,
}

#[derive(Debug)]
pub struct LoadResult {
    pub kind: LoadKind,
    pub generation: u64,
    pub outcome: FetchOutcome,
}

/// Source of data. [`AzureFetcher`] talks to ARM; tests use fakes.
pub trait Fetcher: Send + Sync + 'static {
    fn fetch(&self, request: LoadRequest) -> BoxFuture<'static, FetchOutcome>;
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
        let handle = tokio::spawn(async move {
            let outcome = fetcher.fetch(request).await;
            // The receiver may be gone during shutdown; nothing to do then.
            let _ = tx.send(LoadResult {
                kind,
                generation,
                outcome,
            });
        });
        self.in_flight.insert(kind, (generation, handle));
        Ok(generation)
    }

    /// Mark `result` as received. Returns `true` if it is the latest request
    /// of its kind and should be applied; stale results return `false`.
    pub fn complete(&mut self, result: &LoadResult) -> bool {
        match self.in_flight.get(&result.kind) {
            Some((generation, _)) if *generation == result.generation => {
                self.in_flight.remove(&result.kind);
                true
            }
            _ => false,
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
    fn fetch(&self, request: LoadRequest) -> BoxFuture<'static, FetchOutcome> {
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
                    }
                }
            }
        }
        .boxed()
    }
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
        fn fetch(&self, request: LoadRequest) -> BoxFuture<'static, FetchOutcome> {
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
        assert!(loader.complete(&result));
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
        assert!(loader.complete(&result));

        // A late result with an old generation is rejected.
        let stale = LoadResult {
            kind: LoadKind::ResourceGroups,
            generation: first,
            outcome: FetchOutcome {
                data: Ok(LoadedData::ResourceGroups(vec![])),
                failed_subscriptions: vec![],
            },
        };
        assert!(!loader.complete(&stale));
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
