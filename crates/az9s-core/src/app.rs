use crate::{
    cache::Cache,
    loader::{
        AzureFetcher, Fetcher, LoadKind, LoadReceiver, LoadRequest, LoadResult, LoadedData, Loader,
    },
    state::{AppState, DataSource},
    timing,
};
use anyhow::Context;
use az9s_azure::{AzureAuth, AzureClient};
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{info, warn};

/// Main application coordinator.
///
/// Owns the UI state, the local cache and the background [`Loader`]. Nothing
/// here awaits the network on the UI path: [`App::request`] starts a load and
/// returns immediately, and the event loop hands results back via
/// [`App::apply`].
pub struct App {
    state: AppState,
    cache: Cache,
    loader: Loader,
}

impl App {
    /// Create the application: open the cache, show cached subscriptions right
    /// away, and start fetching fresh subscriptions in the background.
    pub async fn new() -> anyhow::Result<Self> {
        let _t = timing::timer("startup.app_init");
        info!("Initializing az9s application");

        let cache_dir = Self::cache_dir()?;
        std::fs::create_dir_all(&cache_dir).context("Failed to create cache directory")?;
        let cache = Cache::new(&cache_dir.join("az9s.db"))?;

        let fetcher: Option<Arc<dyn Fetcher>> = match AzureAuth::new_auto().await {
            Ok(auth) => Some(Arc::new(AzureFetcher::new(Arc::new(AzureClient::new(
                Arc::new(auth),
            ))))),
            Err(e) => {
                warn!("Azure authentication failed: {}", e);
                None
            }
        };

        let mut app = Self::with_fetcher(cache, fetcher);
        app.load_cached_subscriptions();
        app.request(LoadRequest::Subscriptions);
        Ok(app)
    }

    /// Create an application with no Azure connection, backed by `cache`.
    /// Every load request fails with a "not connected" error.
    pub fn offline(cache: Cache) -> Self {
        Self::with_fetcher(cache, None)
    }

    /// Create an application that loads data through `fetcher`.
    pub fn with_fetcher(cache: Cache, fetcher: Option<Arc<dyn Fetcher>>) -> Self {
        Self {
            state: AppState::new(),
            cache,
            loader: Loader::new(fetcher),
        }
    }

    /// Show cached subscriptions immediately (before Azure answers).
    fn load_cached_subscriptions(&mut self) {
        let _t = timing::timer("cache.get_subscriptions");
        match self.cache.repository().get_subscriptions() {
            Ok(subs) if !subs.is_empty() => {
                info!("Showing {} cached subscriptions", subs.len());
                self.state.subscriptions = subs;
                self.state.set_data_source(DataSource::Cache);
            }
            Ok(_) => {}
            Err(e) => warn!("Failed to read cached subscriptions: {}", e),
        }
    }

    /// Start loading `request` in the background. Never blocks.
    ///
    /// Errors (e.g. not connected) are shown in the UI; there is nothing for
    /// the caller to handle.
    pub fn request(&mut self, request: LoadRequest) {
        self.state.notice = None;
        if let Err(e) = self.loader.request(request) {
            self.state.set_error(Some(e));
        }
        self.state.set_loading(self.loader.is_loading());
    }

    /// The channel on which load results arrive. The event loop takes it once
    /// and passes each result to [`App::apply`].
    pub fn take_load_receiver(&mut self) -> Option<LoadReceiver> {
        self.loader.take_receiver()
    }

    /// Whether a load of `kind` is in flight.
    pub fn is_loading(&self, kind: LoadKind) -> bool {
        self.loader.is_loading_kind(kind)
    }

    /// Apply a finished load: update state and the cache. Stale results (a
    /// newer request of the same kind was made) are ignored.
    pub fn apply(&mut self, result: LoadResult) {
        if !self.loader.complete(&result) {
            return;
        }
        self.state.set_loading(self.loader.is_loading());

        let failed = result.outcome.failed_subscriptions.len();
        match result.outcome.data {
            Ok(data) => {
                self.store(data);
                self.state.set_data_source(DataSource::Azure);
                if failed > 0 {
                    self.state.notice = Some(format!(
                        "{} subscription(s) failed to load {}",
                        failed,
                        result.kind.label()
                    ));
                }
            }
            Err(e) => {
                warn!("Loading {} failed: {}", result.kind.label(), e);
                let message = if result.kind == LoadKind::Subscriptions
                    && !self.state.subscriptions.is_empty()
                {
                    format!("Using cached subscriptions (Azure unavailable: {})", e)
                } else {
                    format!("Failed to load {}: {}", result.kind.label(), e)
                };
                self.state.set_error(Some(message));
            }
        }
    }

    /// Put loaded data into state and write it to the cache.
    fn store(&mut self, data: LoadedData) {
        let repo = self.cache.repository();
        let cached = match &data {
            LoadedData::Subscriptions(items) => {
                let _t = timing::timer("cache.cache_subscriptions");
                repo.cache_subscriptions(items)
            }
            LoadedData::ResourceGroups(items) => {
                let _t = timing::timer("cache.cache_resource_groups");
                repo.cache_resource_groups(items)
            }
            LoadedData::Resources(items) => {
                let _t = timing::timer("cache.cache_resources");
                repo.cache_resources(items)
            }
        };
        if let Err(e) = cached {
            warn!("Failed to write cache: {}", e);
        }

        match data {
            LoadedData::Subscriptions(items) => self.state.subscriptions = items,
            LoadedData::ResourceGroups(items) => self.state.resource_groups = items,
            LoadedData::Resources(items) => self.state.resources = items,
        }
    }

    /// Get the application state
    pub fn state(&self) -> &AppState {
        &self.state
    }

    /// Get mutable application state
    pub fn state_mut(&mut self) -> &mut AppState {
        &mut self.state
    }

    /// Get the cache
    pub fn cache(&self) -> &Cache {
        &self.cache
    }

    /// Determine the cache directory (platform-specific local data dir)
    fn cache_dir() -> anyhow::Result<PathBuf> {
        let dirs = directories::ProjectDirs::from("", "", "az9s")
            .context("Could not determine data directory")?;
        Ok(dirs.data_local_dir().to_path_buf())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::loader::FetchOutcome;
    use crate::models::Subscription;
    use futures::future::BoxFuture;
    use futures::FutureExt;
    use std::path::Path;

    struct Fake(Result<Vec<Subscription>, String>, Vec<String>);

    impl Fetcher for Fake {
        fn fetch(&self, _request: LoadRequest) -> BoxFuture<'static, FetchOutcome> {
            let data = self.0.clone().map(LoadedData::Subscriptions);
            let failed = self.1.clone();
            async move {
                FetchOutcome {
                    data,
                    failed_subscriptions: failed,
                }
            }
            .boxed()
        }
    }

    fn sub(name: &str) -> Subscription {
        Subscription {
            id: format!("id-{name}"),
            name: name.into(),
            state: "Enabled".into(),
            tenant_id: "t".into(),
        }
    }

    fn app(fake: Fake) -> App {
        App::with_fetcher(
            Cache::new(Path::new(":memory:")).unwrap(),
            Some(Arc::new(fake)),
        )
    }

    async fn run_one(app: &mut App, rx: &mut LoadReceiver, request: LoadRequest) {
        app.request(request);
        assert!(app.state().loading);
        let result = rx.recv().await.unwrap();
        app.apply(result);
    }

    #[tokio::test]
    async fn successful_load_updates_state_and_cache() {
        let mut app = app(Fake(Ok(vec![sub("a"), sub("b")]), vec![]));
        let mut rx = app.take_load_receiver().unwrap();
        run_one(&mut app, &mut rx, LoadRequest::Subscriptions).await;

        assert!(!app.state().loading);
        assert_eq!(app.state().subscriptions.len(), 2);
        assert_eq!(app.state().data_source, DataSource::Azure);
        assert_eq!(
            app.cache().repository().get_subscriptions().unwrap().len(),
            2
        );
    }

    #[tokio::test]
    async fn failure_keeps_cached_data_and_explains() {
        let mut app = app(Fake(Err("token expired".into()), vec![]));
        app.state_mut().subscriptions = vec![sub("cached")];
        let mut rx = app.take_load_receiver().unwrap();
        run_one(&mut app, &mut rx, LoadRequest::Subscriptions).await;

        assert_eq!(app.state().subscriptions.len(), 1);
        let err = app.state().error.clone().unwrap();
        assert!(
            err.contains("cached") && err.contains("token expired"),
            "{err}"
        );
    }

    #[tokio::test]
    async fn partial_failures_become_a_notice() {
        let mut app = app(Fake(Ok(vec![sub("a")]), vec!["sub-x".into()]));
        let mut rx = app.take_load_receiver().unwrap();
        run_one(&mut app, &mut rx, LoadRequest::Subscriptions).await;

        assert!(app.state().error.is_none());
        assert!(app
            .state()
            .notice
            .as_deref()
            .unwrap()
            .contains("1 subscription"));
    }

    #[test]
    fn offline_request_shows_error_and_does_not_load() {
        let mut app = App::offline(Cache::new(Path::new(":memory:")).unwrap());
        app.request(LoadRequest::Subscriptions);
        assert!(!app.state().loading);
        assert!(app.state().error.as_deref().unwrap().contains("az login"));
    }
}
