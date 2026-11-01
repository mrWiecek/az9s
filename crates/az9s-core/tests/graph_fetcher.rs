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
