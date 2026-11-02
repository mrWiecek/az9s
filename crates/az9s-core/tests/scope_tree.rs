//! The management group tree end to end: GraphFetcher + ManagementGroupClient
//! against a local mock of the management groups API, through App.

use az9s_azure::management_groups::ManagementGroupClient;
use az9s_azure::testing::{error, ok, serve_paths, StaticToken};
use az9s_azure::ResourceGraphClient;
use az9s_core::cache::Cache;
use az9s_core::loader::{ChunkSink, FetchOutcome, Fetcher, GraphFetcher, LoadRequest};
use az9s_core::scope_tree::ScopeTree;
use az9s_core::state::ViewMode;
use az9s_core::App;
use futures::future::BoxFuture;
use futures::FutureExt;
use serde_json::json;
use std::path::Path;
use std::sync::Arc;

const TENANT: &str = "tenant-1";

/// The ARM side is never used for the tree.
struct NoArm;

impl Fetcher for NoArm {
    fn fetch(&self, _request: LoadRequest, _sink: ChunkSink) -> BoxFuture<'static, FetchOutcome> {
        async {
            FetchOutcome {
                data: Err("unexpected ARM call".into()),
                failed_subscriptions: vec![],
                notice: None,
            }
        }
        .boxed()
    }
}

fn app(endpoint: &str, cache: Cache) -> App {
    let tokens = Arc::new(StaticToken::for_tenant(TENANT));
    let graph = ResourceGraphClient::with_endpoint(endpoint, tokens.clone());
    let groups = ManagementGroupClient::with_endpoint(endpoint, tokens);
    let fetcher = GraphFetcher::new(Arc::new(graph), Arc::new(NoArm))
        .with_management_groups(Arc::new(groups));
    App::with_fetcher(cache, Some(Arc::new(fetcher)))
}

async fn settle(app: &mut App) {
    let mut rx = app.take_load_receiver().unwrap();
    while app.state().loading {
        app.apply(rx.recv().await.unwrap());
    }
}

#[tokio::test]
async fn tree_loads_from_the_tenant_root_and_is_cached() {
    let (url, _) = serve_paths(|path, _| {
        let root = format!("/providers/Microsoft.Management/managementGroups/{TENANT}");
        if path.starts_with(&format!("{root}/descendants")) {
            ok(json!({"value": [
                {"name": "platform", "type": "Microsoft.Management/managementGroups",
                 "properties": {"displayName": "Platform",
                    "parent": {"id": format!("{root}")}}},
                {"name": "SUB-1", "type": "Microsoft.Management/managementGroups/subscriptions",
                 "properties": {"displayName": "Connectivity",
                    "parent": {"id": "/providers/Microsoft.Management/managementGroups/platform"}}}
            ]}))
        } else if path.starts_with(&root) {
            ok(json!({"properties": {"displayName": "Tenant Root Group"}}))
        } else {
            error(404, "unexpected path")
        }
    })
    .await;
    let mut app = app(&url, Cache::new(Path::new(":memory:")).unwrap());

    app.open_scope_tree();
    assert_eq!(app.state().view, ViewMode::ManagementGroupTree);
    settle(&mut app).await;

    let tree = app.state().scope_tree.clone().expect("tree loaded");
    assert_eq!(tree.root, TENANT);
    assert_eq!(tree.root_display_name, "Tenant Root Group");
    assert_eq!(tree.subscriptions_under("platform", &[]), ["sub-1"]);
    // The root opens by default.
    let rows = app.state().tree_rows();
    let labels: Vec<_> = rows.iter().map(|r| r.label.as_str()).collect();
    assert_eq!(labels, ["Tenant Root Group", "Platform"]);
    assert_eq!(rows[0].subscription_count, 1);

    let (json, _) = app
        .cache()
        .repository()
        .document("scope_tree")
        .unwrap()
        .expect("tree cached");
    assert_eq!(serde_json::from_str::<ScopeTree>(&json).unwrap(), tree);
}

#[tokio::test]
async fn a_cached_tree_shows_at_once_and_survives_a_failed_refresh() {
    let (url, _) = serve_paths(|_, _| error(403, "AuthorizationFailed")).await;
    let cache = Cache::new(Path::new(":memory:")).unwrap();
    let cached = ScopeTree {
        root: TENANT.into(),
        root_display_name: "Cached Root".into(),
        ..Default::default()
    };
    cache
        .repository()
        .put_document("scope_tree", &serde_json::to_string(&cached).unwrap())
        .unwrap();
    let mut app = app(&url, cache);

    app.open_scope_tree();
    assert_eq!(
        app.state().scope_tree.as_ref(),
        Some(&cached),
        "before Azure answers"
    );
    settle(&mut app).await;

    assert_eq!(app.state().view, ViewMode::ManagementGroupTree);
    assert!(app.state().error.is_none());
    let notice = app.state().notice.clone().unwrap();
    assert!(notice.contains("cached"), "{notice}");
}

#[tokio::test]
async fn without_access_or_cache_it_falls_back_to_subscriptions() {
    let (url, _) = serve_paths(|_, _| error(403, "AuthorizationFailed")).await;
    let mut app = app(&url, Cache::new(Path::new(":memory:")).unwrap());

    app.open_scope_tree();
    settle(&mut app).await;

    assert_eq!(app.state().view, ViewMode::SubscriptionList);
    assert!(app.state().error.is_none(), "no error popup on startup");
    let notice = app.state().notice.clone().unwrap();
    assert!(
        notice.contains("Management group tree unavailable") && notice.contains("403"),
        "{notice}"
    );
}

#[test]
fn offline_without_cache_falls_back_to_subscriptions() {
    let mut app = App::offline(Cache::new(Path::new(":memory:")).unwrap());
    app.open_scope_tree();
    assert_eq!(app.state().view, ViewMode::SubscriptionList);
    assert!(app.state().error.is_none());
}

#[tokio::test]
async fn a_fetcher_without_management_groups_reports_it() {
    let (url, _) = serve_paths(|_, _| error(500, "unused")).await;
    let graph = ResourceGraphClient::with_endpoint(&url, Arc::new(StaticToken::for_tenant(TENANT)));
    let fetcher = GraphFetcher::new(Arc::new(graph), Arc::new(NoArm));
    let outcome = fetcher
        .fetch(LoadRequest::ScopeTree, ChunkSink::discard())
        .await;
    assert!(matches!(outcome.data, Err(e) if e.contains("not configured")));
}
