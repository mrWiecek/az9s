use crate::{
    cache::{Cache, WriteJob},
    loader::{
        Accept, AzureFetcher, Fetcher, GraphFetcher, LoadKind, LoadReceiver, LoadRequest,
        LoadResult, LoadedData, Loader,
    },
    state::{AppState, DataSource, Freshness, ViewMode},
    timing,
};
use anyhow::Context;
use az9s_azure::resource_graph::{CachingTokenSource, CredentialTokenSource};
use az9s_azure::{AuthProvider, AzureAuth, AzureClient, ResourceGraphClient};
use std::collections::HashMap;
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
    /// The in-flight request of each kind
    pending: HashMap<LoadKind, Pending>,
}

/// A request in flight, as the App tracks it.
struct Pending {
    generation: u64,
    request: LoadRequest,
    /// Rows were on screen when the request started (cached or older data).
    /// Streamed chunks then collect in `staging` and replace the list in one
    /// swap at the end, instead of the list shrinking to the first page.
    staged: bool,
    staging: Option<LoadedData>,
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
            Ok(auth) => Some(Self::azure_fetcher(Arc::new(auth))),
            Err(e) => {
                warn!("Azure authentication failed: {}", e);
                None
            }
        };

        if let Some(fetcher) = &fetcher {
            fetcher.warm_up();
        }
        let mut app = Self::with_fetcher(cache, fetcher);
        app.load_cached_subscriptions();
        app.request(LoadRequest::Subscriptions);
        Ok(app)
    }

    /// Resource Graph for lists (with ARM fallback), unless `AZ9S_BACKEND=arm`.
    fn azure_fetcher(auth: Arc<AzureAuth>) -> Arc<dyn Fetcher> {
        let arm: Arc<dyn Fetcher> =
            Arc::new(AzureFetcher::new(Arc::new(AzureClient::new(auth.clone()))));
        if std::env::var("AZ9S_BACKEND").is_ok_and(|v| v.eq_ignore_ascii_case("arm")) {
            info!("Using ARM backend (AZ9S_BACKEND=arm)");
            return arm;
        }
        let tokens = Arc::new(CachingTokenSource::new(CredentialTokenSource(
            auth.credential(),
        )));
        Arc::new(GraphFetcher::new(
            Arc::new(ResourceGraphClient::new(tokens)),
            arm,
        ))
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
            pending: HashMap::new(),
        }
    }

    /// Show cached subscriptions immediately (before Azure answers).
    fn load_cached_subscriptions(&mut self) {
        let _t = timing::timer("cache.read_subscriptions");
        match self.cache.repository().cached_subscriptions() {
            Ok(cached) if !cached.items.is_empty() => {
                info!("Showing {} cached subscriptions", cached.items.len());
                self.state.subscriptions = cached.items;
                self.mark(LoadKind::Subscriptions, DataSource::Cache, cached.oldest);
            }
            Ok(_) => {}
            Err(e) => warn!("Failed to read cached subscriptions: {}", e),
        }
    }

    /// Fill an empty list from the cache for this request's scope, so the
    /// view shows data instantly while Azure is asked in the background.
    fn show_cached(&mut self, request: &LoadRequest) {
        let repo = self.cache.repository();
        let result = match request {
            LoadRequest::Subscriptions => return,
            LoadRequest::ResourceGroups { subscription_ids } => {
                let _t = timing::timer("cache.read_resource_groups");
                repo.cached_resource_groups(subscription_ids).map(|c| {
                    let oldest = c.oldest;
                    (LoadedData::ResourceGroups(c.items), oldest)
                })
            }
            LoadRequest::Resources {
                subscription_ids,
                resource_group,
            } => {
                let _t = timing::timer("cache.read_resources");
                repo.cached_resources(subscription_ids, resource_group.as_deref())
                    .map(|c| {
                        let oldest = c.oldest;
                        (LoadedData::Resources(c.items), oldest)
                    })
            }
        };
        match result {
            Ok((data, Some(oldest))) => {
                self.put(data, false);
                self.mark(request.kind(), DataSource::Cache, Some(oldest));
            }
            Ok((_, None)) => {}
            Err(e) => warn!("Failed to read cache: {}", e),
        }
    }

    /// Record where a list's data came from and when.
    fn mark(&mut self, kind: LoadKind, source: DataSource, as_of: Option<i64>) {
        let as_of = as_of.unwrap_or_else(|| chrono::Utc::now().timestamp());
        self.state
            .freshness
            .insert(view_of(kind), Freshness { source, as_of });
        self.state.set_data_source(source);
    }

    fn list_len(&self, kind: LoadKind) -> usize {
        match kind {
            LoadKind::Subscriptions => self.state.subscriptions.len(),
            LoadKind::ResourceGroups => self.state.resource_groups.len(),
            LoadKind::Resources => self.state.resources.len(),
        }
    }

    /// Start loading `request` in the background. Never blocks.
    ///
    /// If the target list is empty, it is first filled from the cache, so the
    /// view shows (possibly stale) data immediately. Errors (e.g. not
    /// connected) are shown in the UI; there is nothing for the caller to
    /// handle.
    pub fn request(&mut self, request: LoadRequest) {
        self.state.notice = None;
        let kind = request.kind();
        if self.list_len(kind) == 0 {
            self.show_cached(&request);
        }
        let staged = self.list_len(kind) > 0;
        match self.loader.request(request.clone()) {
            Ok(generation) => {
                self.pending.insert(
                    kind,
                    Pending {
                        generation,
                        request,
                        staged,
                        staging: None,
                    },
                );
            }
            Err(e) if staged => self.state.notice = Some(format!("{e}; showing cached data")),
            Err(e) => self.state.set_error(Some(e)),
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

    /// Apply a load result: a streamed chunk or the final message of a
    /// request. Stale results (a newer request of the same kind exists) are
    /// ignored. A successful final result replaces the cached scope.
    pub fn apply(&mut self, result: LoadResult) {
        let kind = result.kind;
        let accept = self.loader.accept(&result);
        if accept == Accept::Stale {
            return;
        }
        let Some(pending) = self
            .pending
            .get_mut(&kind)
            .filter(|p| p.generation == result.generation)
        else {
            return;
        };
        let outcome = result.outcome;

        if accept == Accept::Partial {
            if let Ok(data) = outcome.data {
                if pending.staged {
                    merge(&mut pending.staging, data);
                } else {
                    self.put(data, true);
                }
            }
            return;
        }

        let pending = self.pending.remove(&kind).expect("checked above");
        self.state.set_loading(self.loader.is_loading());
        if let Some(notice) = outcome.notice {
            self.state.notice = Some(notice);
        }

        match outcome.data {
            Ok(data) => {
                if pending.staged {
                    let mut all = pending.staging;
                    merge(&mut all, data);
                    self.swap_in(all.expect("merge always sets a value"));
                } else {
                    self.put(data, true);
                }
                self.write_scope(&pending.request, &outcome.failed_subscriptions);
                self.mark(kind, DataSource::Azure, None);
                let failed = outcome.failed_subscriptions.len();
                if failed > 0 {
                    self.state.notice = Some(format!(
                        "{} subscription(s) failed to load {}",
                        failed,
                        kind.label()
                    ));
                }
            }
            Err(e) => {
                warn!("Loading {} failed: {}", kind.label(), e);
                // Cached rows are still on screen: a non-blocking notice is
                // enough. Otherwise the list is empty or incomplete: error.
                if pending.staged {
                    let age = self
                        .state
                        .freshness
                        .get(&view_of(kind))
                        .map(|f| format!(" from {}", crate::state::format_age(f.as_of)))
                        .unwrap_or_default();
                    self.state.notice = Some(format!(
                        "Azure refresh failed ({e}); showing cached {}{age}",
                        kind.label()
                    ));
                } else {
                    self.state
                        .set_error(Some(format!("Failed to load {}: {}", kind.label(), e)));
                }
            }
        }
    }

    /// Put rows into their list, appending or replacing.
    fn put(&mut self, data: LoadedData, append: bool) {
        fn put<T>(list: &mut Vec<T>, items: Vec<T>, append: bool) {
            if append {
                list.extend(items);
            } else {
                *list = items;
            }
        }
        match data {
            LoadedData::Subscriptions(items) => put(&mut self.state.subscriptions, items, append),
            LoadedData::ResourceGroups(items) => {
                put(&mut self.state.resource_groups, items, append)
            }
            LoadedData::Resources(items) => put(&mut self.state.resources, items, append),
        }
    }

    /// Replace a list with fresh rows, keeping the selection on the same item
    /// when the list is on screen.
    fn swap_in(&mut self, data: LoadedData) {
        let selected = self.state.selected_id();
        self.put(data, false);
        if let Some(id) = selected {
            self.state.select_id(&id);
        }
    }

    /// Replace the cache for the request's scope with the list now in state.
    /// Subscriptions that failed to load keep their cached rows. The write
    /// runs on the cache's background thread.
    fn write_scope(&self, request: &LoadRequest, failed: &[String]) {
        let scope = |ids: &[String]| -> Vec<String> {
            ids.iter()
                .filter(|id| !failed.iter().any(|f| f.eq_ignore_ascii_case(id)))
                .cloned()
                .collect()
        };
        let _t = timing::timer("cache.queue_write");
        let job = match request {
            LoadRequest::Subscriptions => WriteJob::Subscriptions(self.state.subscriptions.clone()),
            LoadRequest::ResourceGroups { subscription_ids } => WriteJob::ResourceGroups {
                subscription_ids: scope(subscription_ids),
                items: self.state.resource_groups.clone(),
            },
            LoadRequest::Resources {
                subscription_ids,
                resource_group,
            } => WriteJob::Resources {
                subscription_ids: scope(subscription_ids),
                resource_group: resource_group.clone(),
                items: self.state.resources.clone(),
            },
        };
        self.cache.write(job);
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

/// The list view that shows a kind of data.
fn view_of(kind: LoadKind) -> ViewMode {
    match kind {
        LoadKind::Subscriptions => ViewMode::SubscriptionList,
        LoadKind::ResourceGroups => ViewMode::ResourceGroupList,
        LoadKind::Resources => ViewMode::ResourceList,
    }
}

/// Append `data` to the accumulated chunks.
fn merge(acc: &mut Option<LoadedData>, data: LoadedData) {
    match (acc.as_mut(), data) {
        (None, data) => *acc = Some(data),
        (Some(LoadedData::Subscriptions(a)), LoadedData::Subscriptions(b)) => a.extend(b),
        (Some(LoadedData::ResourceGroups(a)), LoadedData::ResourceGroups(b)) => a.extend(b),
        (Some(LoadedData::Resources(a)), LoadedData::Resources(b)) => a.extend(b),
        (Some(_), other) => *acc = Some(other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::loader::{ChunkSink, FetchOutcome};
    use crate::models::{ResourceGroup, Subscription};
    use futures::future::BoxFuture;
    use futures::FutureExt;
    use std::path::Path;
    use std::sync::Mutex;

    /// Answers every request with a scripted outcome: `chunks` are streamed
    /// through the sink, then `last` is the final result.
    struct Script {
        chunks: Vec<LoadedData>,
        last: Result<LoadedData, String>,
        failed: Vec<String>,
    }

    struct Fake(Mutex<Vec<Script>>);

    impl Fake {
        fn new(scripts: Vec<Script>) -> Self {
            Self(Mutex::new(scripts))
        }
    }

    impl Fetcher for Fake {
        fn fetch(
            &self,
            _request: LoadRequest,
            sink: ChunkSink,
        ) -> BoxFuture<'static, FetchOutcome> {
            let script = self.0.lock().unwrap().remove(0);
            async move {
                for chunk in script.chunks {
                    sink.send(chunk);
                }
                FetchOutcome {
                    data: script.last,
                    failed_subscriptions: script.failed,
                    notice: None,
                }
            }
            .boxed()
        }
    }

    fn done(last: Result<LoadedData, String>) -> Script {
        Script {
            chunks: vec![],
            last,
            failed: vec![],
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

    fn rg(sub: &str, name: &str) -> ResourceGroup {
        ResourceGroup {
            id: format!("/subscriptions/{sub}/resourceGroups/{name}"),
            name: name.into(),
            location: "westeurope".into(),
            subscription_id: sub.into(),
            tags: Default::default(),
        }
    }

    fn rgs(items: Vec<ResourceGroup>) -> LoadedData {
        LoadedData::ResourceGroups(items)
    }

    fn names(app: &App) -> Vec<String> {
        app.state()
            .resource_groups
            .iter()
            .map(|r| r.name.clone())
            .collect()
    }

    /// App over a cache that already holds subscriptions `id-a`, `id-b` and
    /// these resource groups.
    fn app_with_cache(scripts: Vec<Script>, cached_rgs: &[ResourceGroup]) -> App {
        let cache = Cache::new(Path::new(":memory:")).unwrap();
        let repo = cache.repository();
        repo.replace_subscriptions(&[sub("a"), sub("b")]).unwrap();
        repo.replace_resource_groups(&["id-a".into(), "id-b".into()], cached_rgs)
            .unwrap();
        App::with_fetcher(cache, Some(Arc::new(Fake::new(scripts))))
    }

    fn rg_request(subs: &[&str]) -> LoadRequest {
        LoadRequest::ResourceGroups {
            subscription_ids: subs.iter().map(|s| s.to_string()).collect(),
        }
    }

    /// Request, then apply results until the load finishes.
    async fn load(app: &mut App, rx: &mut LoadReceiver, request: LoadRequest) {
        app.request(request);
        while app.state().loading {
            let result = rx.recv().await.unwrap();
            app.apply(result);
        }
    }

    #[tokio::test]
    async fn successful_load_updates_state_and_cache() {
        let mut app = app_with_cache(
            vec![done(Ok(LoadedData::Subscriptions(vec![
                sub("a"),
                sub("c"),
            ])))],
            &[],
        );
        let mut rx = app.take_load_receiver().unwrap();
        load(&mut app, &mut rx, LoadRequest::Subscriptions).await;

        assert_eq!(app.state().subscriptions.len(), 2);
        assert_eq!(app.state().data_source, DataSource::Azure);
        let cached = app.cache().repository().get_subscriptions().unwrap();
        let ids: Vec<_> = cached.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(
            ids,
            ["id-a", "id-c"],
            "removed subscription dropped from cache"
        );
    }

    #[tokio::test]
    async fn cached_rows_show_immediately_then_azure_replaces_them() {
        let mut app = app_with_cache(
            vec![done(Ok(rgs(vec![rg("id-a", "rg-new")])))],
            &[rg("id-a", "rg-old"), rg("id-b", "rg-b")],
        );
        let mut rx = app.take_load_receiver().unwrap();

        app.request(rg_request(&["id-a"]));
        // Before Azure answers: cached rows for exactly this scope.
        assert_eq!(names(&app), ["rg-old"]);
        let fresh = app.state().freshness[&ViewMode::ResourceGroupList];
        assert_eq!(fresh.source, DataSource::Cache);

        app.apply(rx.recv().await.unwrap());
        assert_eq!(names(&app), ["rg-new"]);
        assert_eq!(
            app.state().freshness[&ViewMode::ResourceGroupList].source,
            DataSource::Azure
        );
        // Cache scope replaced: "rg-old" (deleted in Azure) is gone, other
        // subscriptions untouched.
        let repo = app.cache().repository();
        let a = repo.cached_resource_groups(&["id-a".into()]).unwrap().items;
        assert_eq!(
            a.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(),
            ["rg-new"]
        );
        assert_eq!(
            repo.cached_resource_groups(&["id-b".into()])
                .unwrap()
                .items
                .len(),
            1
        );
    }

    #[tokio::test]
    async fn with_cached_rows_on_screen_pages_are_staged_and_swapped_once() {
        let mut app = app_with_cache(
            vec![Script {
                chunks: vec![rgs(vec![rg("id-a", "rg-1")]), rgs(vec![rg("id-a", "rg-2")])],
                last: Ok(rgs(vec![rg("id-a", "rg-3")])),
                failed: vec![],
            }],
            &[rg("id-a", "rg-1"), rg("id-a", "rg-2"), rg("id-a", "rg-3")],
        );
        app.state_mut().view = ViewMode::ResourceGroupList;
        let mut rx = app.take_load_receiver().unwrap();

        app.request(rg_request(&["id-a"]));
        app.state_mut().list_state.selected = 2; // user is on rg-3

        app.apply(rx.recv().await.unwrap()); // first page
        assert_eq!(
            names(&app),
            ["rg-1", "rg-2", "rg-3"],
            "no shrink to one page"
        );
        app.apply(rx.recv().await.unwrap()); // second page
        app.apply(rx.recv().await.unwrap()); // final
        assert_eq!(names(&app), ["rg-1", "rg-2", "rg-3"]);
        assert_eq!(
            app.state().list_state.selected,
            2,
            "selection stays on rg-3"
        );
    }

    #[tokio::test]
    async fn swap_keeps_selection_on_the_same_item_when_rows_move() {
        let mut app = app_with_cache(
            vec![done(Ok(rgs(vec![
                rg("id-a", "rg-0"),
                rg("id-a", "rg-1"),
                rg("id-a", "rg-2"),
            ])))],
            &[rg("id-a", "rg-1"), rg("id-a", "rg-2")],
        );
        app.state_mut().view = ViewMode::ResourceGroupList;
        let mut rx = app.take_load_receiver().unwrap();
        app.request(rg_request(&["id-a"]));
        app.state_mut().list_state.selected = 1; // rg-2
        app.apply(rx.recv().await.unwrap());
        assert_eq!(app.state().selected_id().unwrap(), rg("id-a", "rg-2").id);
    }

    #[tokio::test]
    async fn without_cache_pages_stream_into_view() {
        let mut app = app_with_cache(
            vec![Script {
                chunks: vec![rgs(vec![rg("id-a", "rg-1")])],
                last: Ok(rgs(vec![rg("id-a", "rg-2")])),
                failed: vec![],
            }],
            &[],
        );
        let mut rx = app.take_load_receiver().unwrap();
        app.request(rg_request(&["id-a"]));
        assert!(names(&app).is_empty());
        app.apply(rx.recv().await.unwrap());
        assert_eq!(names(&app), ["rg-1"], "first page visible right away");
        app.apply(rx.recv().await.unwrap());
        assert_eq!(names(&app), ["rg-1", "rg-2"]);
    }

    #[tokio::test]
    async fn failed_subscriptions_keep_their_cached_rows() {
        let mut app = app_with_cache(
            vec![Script {
                chunks: vec![],
                last: Ok(rgs(vec![rg("id-a", "rg-a2")])),
                failed: vec!["ID-B".into()],
            }],
            &[rg("id-a", "rg-a1"), rg("id-b", "rg-b1")],
        );
        let mut rx = app.take_load_receiver().unwrap();
        load(&mut app, &mut rx, rg_request(&["id-a", "id-b"])).await;

        let repo = app.cache().repository();
        let b = repo.cached_resource_groups(&["id-b".into()]).unwrap().items;
        assert_eq!(b.len(), 1, "a failed subscription's cache is not wiped");
        assert!(app
            .state()
            .notice
            .as_deref()
            .unwrap()
            .contains("1 subscription"));
    }

    #[tokio::test]
    async fn failure_with_cached_rows_is_a_notice_not_an_error() {
        let mut app = app_with_cache(
            vec![done(Err("token expired".into()))],
            &[rg("id-a", "rg-1")],
        );
        let mut rx = app.take_load_receiver().unwrap();
        load(&mut app, &mut rx, rg_request(&["id-a"])).await;

        assert_eq!(names(&app), ["rg-1"]);
        assert!(app.state().error.is_none());
        let notice = app.state().notice.clone().unwrap();
        assert!(
            notice.contains("token expired") && notice.contains("cached"),
            "{notice}"
        );
    }

    #[tokio::test]
    async fn failure_without_cached_rows_is_an_error() {
        let mut app = app_with_cache(vec![done(Err("boom".into()))], &[]);
        let mut rx = app.take_load_receiver().unwrap();
        load(&mut app, &mut rx, rg_request(&["id-a"])).await;
        assert!(app.state().error.as_deref().unwrap().contains("boom"));
    }

    #[test]
    fn offline_shows_cached_rows_with_a_notice() {
        let cache = Cache::new(Path::new(":memory:")).unwrap();
        cache
            .repository()
            .replace_subscriptions(&[sub("a")])
            .unwrap();
        cache
            .repository()
            .replace_resource_groups(&["id-a".into()], &[rg("id-a", "rg-1")])
            .unwrap();
        let mut app = App::offline(cache);
        app.request(rg_request(&["id-a"]));
        assert_eq!(names(&app), ["rg-1"]);
        assert!(app.state().error.is_none());
        assert!(app.state().notice.as_deref().unwrap().contains("cached"));
    }

    #[test]
    fn offline_without_cache_shows_error() {
        let mut app = App::offline(Cache::new(Path::new(":memory:")).unwrap());
        app.request(LoadRequest::Subscriptions);
        assert!(!app.state().loading);
        assert!(app.state().error.as_deref().unwrap().contains("az login"));
    }
}
