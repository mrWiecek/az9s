//! Resource Graph backend: scope selection, streaming, retry and fallback,
//! against a local mock of the Resource Graph REST API.

use az9s_azure::testing::{error, ok, serve, StaticToken};
use az9s_azure::ResourceGraphClient;
use az9s_core::cache::Cache;
use az9s_core::loader::{ChunkSink, FetchOutcome, Fetcher, GraphFetcher, LoadRequest, LoadedData};
use az9s_core::models::Resource;
use az9s_core::App;
use futures::future::BoxFuture;
use futures::FutureExt;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

const TENANT: &str = "tenant-1";

/// Stands in for the ARM fallback; counts calls.
#[derive(Default)]
struct FakeArm {
    calls: AtomicUsize,
}

impl Fetcher for FakeArm {
    fn fetch(&self, _request: LoadRequest, _sink: ChunkSink) -> BoxFuture<'static, FetchOutcome> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        async {
            FetchOutcome {
                data: Ok(LoadedData::Resources(vec![Resource {
                    id: "/subscriptions/s1/resourceGroups/rg/providers/x/from-arm".into(),
                    name: "from-arm".into(),
                    resource_type: "x".into(),
                    location: "westeurope".into(),
                    resource_group: "rg".into(),
                    subscription_id: "s1".into(),
                    tags: HashMap::new(),
                }])),
                failed_subscriptions: vec![],
                notice: None,
            }
        }
        .boxed()
    }
}

fn rg_row(sub: &str, name: &str) -> Value {
    json!({
        "id": format!("/subscriptions/{sub}/resourceGroups/{name}"),
        "name": name, "location": "westeurope", "subscriptionId": sub, "tags": {}
    })
}

fn res_row(sub: &str, rg: &str, name: &str) -> Value {
    json!({
        "id": format!("/subscriptions/{sub}/resourceGroups/{rg}/providers/Microsoft.Compute/virtualMachines/{name}"),
        "name": name, "type": "microsoft.compute/virtualmachines", "location": "westeurope",
        "resourceGroup": rg.to_lowercase(), "subscriptionId": sub, "tags": {}
    })
}

fn app(endpoint: &str, arm: Arc<FakeArm>) -> App {
    let graph =
        ResourceGraphClient::with_endpoint(endpoint, Arc::new(StaticToken::for_tenant(TENANT)));
    let fetcher = GraphFetcher::new(Arc::new(graph), arm);
    App::with_fetcher(
        Cache::new(Path::new(":memory:")).unwrap(),
        Some(Arc::new(fetcher)),
    )
}

/// Request, then apply results until the load finishes. Returns how many
/// results (chunks + final) arrived.
async fn load(app: &mut App, request: LoadRequest) -> usize {
    let mut rx = app.take_load_receiver().unwrap();
    app.request(request);
    let mut messages = 0;
    while app.state().loading {
        let result = tokio::time::timeout(Duration::from_secs(5), rx.recv())
            .await
            .expect("load finished in time")
            .unwrap();
        messages += 1;
        app.apply(result);
    }
    messages
}

#[tokio::test]
async fn all_subscriptions_query_the_tenant_root_and_stream_pages() {
    let (endpoint, requests) = serve(|body| {
        if body["options"].get("$skipToken").is_none() {
            ok(json!({
                "data": [rg_row("sub-a", "rg-1"), rg_row("sub-elsewhere", "rg-x"), rg_row("sub-a", "rg-2")],
                "$skipToken": "page-2"
            }))
        } else {
            ok(json!({ "data": [rg_row("sub-b", "rg-3")] }))
        }
    })
    .await;
    let mut app = app(&endpoint, Arc::default());

    let messages = load(
        &mut app,
        LoadRequest::ResourceGroups {
            subscription_ids: vec!["sub-a".into(), "SUB-B".into()],
        },
    )
    .await;

    // Two streamed pages, then the final message.
    assert_eq!(messages, 3);
    let names: Vec<_> = app
        .state()
        .resource_groups
        .iter()
        .map(|r| r.name.as_str())
        .collect();
    assert_eq!(
        names,
        ["rg-1", "rg-2", "rg-3"],
        "rows outside the requested subscriptions are dropped"
    );
    assert!(app.state().error.is_none());

    let requests = requests.lock().unwrap();
    assert_eq!(requests[0]["managementGroups"], json!([TENANT]));
    assert!(requests[0].get("subscriptions").is_none());
    assert_eq!(requests[1]["options"]["$skipToken"], json!("page-2"));
}

#[tokio::test]
async fn forbidden_tenant_root_retries_scoped_to_subscriptions() {
    let (endpoint, requests) = serve(|body| {
        if body.get("managementGroups").is_some() {
            error(403, "no access to the root management group")
        } else {
            ok(json!({ "data": [rg_row("sub-a", "rg-1")] }))
        }
    })
    .await;
    let mut app = app(&endpoint, Arc::default());

    load(
        &mut app,
        LoadRequest::ResourceGroups {
            subscription_ids: vec!["sub-a".into(), "sub-b".into()],
        },
    )
    .await;

    assert_eq!(app.state().resource_groups.len(), 1);
    assert!(app.state().error.is_none());
    let requests = requests.lock().unwrap();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[1]["subscriptions"], json!(["sub-a", "sub-b"]));
}

#[tokio::test]
async fn one_resource_group_is_queried_in_its_subscription_with_a_filter() {
    let (endpoint, requests) =
        serve(|_| ok(json!({ "data": [res_row("s1", "RG-App", "vm-1")] }))).await;
    let mut app = app(&endpoint, Arc::default());

    load(
        &mut app,
        LoadRequest::Resources {
            subscription_ids: vec!["s1".into()],
            resource_group: Some("RG-App".into()),
        },
    )
    .await;

    let resources = &app.state().resources;
    assert_eq!(resources.len(), 1);
    assert_eq!(
        resources[0].resource_group, "RG-App",
        "original casing kept"
    );
    let requests = requests.lock().unwrap();
    assert_eq!(requests[0]["subscriptions"], json!(["s1"]));
    assert!(requests[0].get("managementGroups").is_none());
    let query = requests[0]["query"].as_str().unwrap();
    assert!(query.contains("resourceGroup =~ 'RG-App'"), "{query}");
}

#[tokio::test]
async fn resource_graph_failure_falls_back_to_arm_with_a_notice() {
    let (endpoint, _) = serve(|_| error(500, "internal error")).await;
    let arm = Arc::new(FakeArm::default());
    let mut app = app(&endpoint, arm.clone());

    load(
        &mut app,
        LoadRequest::Resources {
            subscription_ids: vec!["s1".into()],
            resource_group: None,
        },
    )
    .await;

    assert_eq!(arm.calls.load(Ordering::SeqCst), 1);
    assert_eq!(app.state().resources[0].name, "from-arm");
    let notice = app.state().notice.clone().unwrap();
    assert!(notice.contains("used ARM"), "{notice}");
    assert!(app.state().error.is_none());
}

#[tokio::test]
async fn failure_after_some_pages_keeps_rows_and_reports_without_fallback() {
    let (endpoint, _) = serve(|body| {
        if body["options"].get("$skipToken").is_none() {
            ok(json!({ "data": [res_row("s1", "rg", "vm-1")], "$skipToken": "p2" }))
        } else {
            error(500, "boom")
        }
    })
    .await;
    let arm = Arc::new(FakeArm::default());
    let mut app = app(&endpoint, arm.clone());

    load(
        &mut app,
        LoadRequest::Resources {
            subscription_ids: vec!["s1".into()],
            resource_group: None,
        },
    )
    .await;

    assert_eq!(
        arm.calls.load(Ordering::SeqCst),
        0,
        "no fallback: rows would duplicate"
    );
    assert_eq!(app.state().resources.len(), 1);
    let err = app.state().error.clone().unwrap();
    assert!(
        err.contains("after 1 rows") && err.contains("boom"),
        "{err}"
    );
}

#[tokio::test]
async fn refresh_replaces_rather_than_duplicates_streamed_rows() {
    let (endpoint, _) = serve(|_| {
        ok(json!({ "data": [res_row("s1", "rg", "vm-1"), res_row("s1", "rg", "vm-2")] }))
    })
    .await;
    let mut app = app(&endpoint, Arc::default());
    let request = LoadRequest::Resources {
        subscription_ids: vec!["s1".into()],
        resource_group: None,
    };

    let mut rx = app.take_load_receiver().unwrap();
    for _ in 0..2 {
        app.request(request.clone());
        while app.state().loading {
            app.apply(rx.recv().await.unwrap());
        }
    }
    assert_eq!(app.state().resources.len(), 2);
}

/// Counts credential calls; hands out a token for `TENANT`.
struct CountingToken {
    calls: Arc<AtomicUsize>,
    inner: StaticToken,
}

impl az9s_azure::resource_graph::TokenSource for CountingToken {
    fn token(&self) -> BoxFuture<'_, az9s_azure::Result<String>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.inner.token()
    }
}

fn counting_app(endpoint: &str) -> (App, Arc<AtomicUsize>, Arc<GraphFetcher>) {
    let calls = Arc::new(AtomicUsize::new(0));
    let tokens = CountingToken {
        calls: calls.clone(),
        inner: StaticToken::for_tenant(TENANT),
    };
    let graph = ResourceGraphClient::with_endpoint(endpoint, Arc::new(tokens));
    let fetcher = Arc::new(GraphFetcher::new(
        Arc::new(graph),
        Arc::new(FakeArm::default()),
    ));
    let app = App::with_fetcher(
        Cache::new(Path::new(":memory:")).unwrap(),
        Some(fetcher.clone()),
    );
    (app, calls, fetcher)
}

/// Regression: the tenant cache was a cloned `OnceCell`, so every
/// "all subscriptions" request resolved the tenant (an extra token fetch,
/// ~0.8 s with the Azure CLI) again.
#[tokio::test]
async fn tenant_is_resolved_once_across_requests() {
    let (endpoint, _) = serve(|_| ok(json!({ "data": [rg_row("sub-a", "rg-1")] }))).await;
    let (mut app, calls, _) = counting_app(&endpoint);
    let request = LoadRequest::ResourceGroups {
        subscription_ids: vec!["sub-a".into(), "sub-b".into()],
    };

    let mut rx = app.take_load_receiver().unwrap();
    for _ in 0..3 {
        app.request(request.clone());
        while app.state().loading {
            app.apply(rx.recv().await.unwrap());
        }
    }
    // 1 token to resolve the tenant + 1 per request for its single page.
    assert_eq!(calls.load(Ordering::SeqCst), 1 + 3);
}

#[tokio::test]
async fn warm_up_resolves_the_tenant_before_the_first_request() {
    let (endpoint, requests) = serve(|_| ok(json!({ "data": [rg_row("sub-a", "rg-1")] }))).await;
    let (mut app, calls, fetcher) = counting_app(&endpoint);

    fetcher.warm_up();
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "tenant resolved in the background"
    );
    assert!(
        requests.lock().unwrap().is_empty(),
        "warm-up sends no queries"
    );

    load(
        &mut app,
        LoadRequest::ResourceGroups {
            subscription_ids: vec!["sub-a".into(), "sub-b".into()],
        },
    )
    .await;
    assert_eq!(
        calls.load(Ordering::SeqCst),
        2,
        "only the page's token, no tenant lookup"
    );
}

#[tokio::test]
async fn resource_detail_queries_one_id_in_its_subscription() {
    let id = "/subscriptions/s1/resourceGroups/rg/providers/Microsoft.Compute/virtualMachines/vm";
    let (url, requests) = serve(move |_| {
        ok(json!({"totalRecords": 1, "count": 1, "data": [
            {"id": id, "name": "vm", "properties": {"hardwareProfile": {"vmSize": "B2s"}}}
        ]}))
    })
    .await;
    let graph = ResourceGraphClient::with_endpoint(&url, Arc::new(StaticToken::for_tenant(TENANT)));
    let fetcher = GraphFetcher::new(Arc::new(graph), Arc::new(FakeArm::default()));

    let outcome = fetcher
        .fetch(
            LoadRequest::ResourceDetail {
                resource_id: id.into(),
                subscription_id: "s1".into(),
            },
            ChunkSink::discard(),
        )
        .await;
    match outcome.data {
        Ok(LoadedData::ResourceDetail(doc)) => {
            assert_eq!(doc["properties"]["hardwareProfile"]["vmSize"], "B2s")
        }
        other => panic!("unexpected {other:?}"),
    }
    let body = requests.lock().unwrap()[0].clone();
    assert_eq!(body["subscriptions"], json!(["s1"]));
    assert!(body["query"]
        .as_str()
        .unwrap()
        .contains(&format!("id =~ '{id}'")));
}

#[tokio::test]
async fn a_resource_missing_from_resource_graph_is_an_error_not_arm() {
    let (url, _) = serve(|_| ok(json!({"totalRecords": 0, "count": 0, "data": []}))).await;
    let graph = ResourceGraphClient::with_endpoint(&url, Arc::new(StaticToken::for_tenant(TENANT)));
    let arm = Arc::new(FakeArm::default());
    let fetcher = GraphFetcher::new(Arc::new(graph), arm.clone());
    let outcome = fetcher
        .fetch(
            LoadRequest::ResourceDetail {
                resource_id: "/subscriptions/s1/x".into(),
                subscription_id: "s1".into(),
            },
            ChunkSink::discard(),
        )
        .await;
    assert!(matches!(outcome.data, Err(e) if e.contains("not found")));
    assert_eq!(arm.calls.load(Ordering::SeqCst), 0);
}
