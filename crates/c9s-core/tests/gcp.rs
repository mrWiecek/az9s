//! Browsing Google Cloud: projects, their resources and a resource's
//! document, through the GCP fetcher against a mock Google API.
#![cfg(unix)]

use c9s_azure::testing::{error, ok, serve_paths};
use c9s_core::cache::Cache;
use c9s_core::gcp::GcpFetcher;
use c9s_core::loader::LoadRequest;
use c9s_core::provider::Provider;
use c9s_core::App;
use c9s_gcp::{GcpClient, Tokens};
use serde_json::json;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::sync::Arc;

const VM: &str =
    "//compute.googleapis.com/projects/billing-prod/zones/europe-west1-b/instances/vm-1";

#[tokio::test]
async fn projects_resources_and_detail() {
    let (endpoint, _) = serve_paths(|path, _| {
        if path.starts_with("/v3/projects:search") {
            return ok(json!({"projects": [
                {"projectId": "billing-prod", "displayName": "Billing", "state": "ACTIVE",
                 "parent": "folders/2"}
            ]}));
        }
        // The login may not see the organization; folders still come.
        if path.starts_with("/v3/organizations:search") {
            return error(403, "denied");
        }
        if path.starts_with("/v3/folders:search") {
            return ok(json!({"folders": [
                {"name": "folders/2", "displayName": "Finance", "parent": "organizations/1",
                 "state": "ACTIVE"}
            ]}));
        }
        if path.starts_with("/v1/projects/billing-prod:searchAllResources") {
            return ok(json!({"results": [
                {"name": VM, "displayName": "vm-1", "assetType": "compute.googleapis.com/Instance",
                 "location": "europe-west1-b", "labels": {"env": "prod"}, "state": "RUNNING"},
                {"name": "//storage.googleapis.com/billing-logs", "assetType": "storage.googleapis.com/Bucket",
                 "location": "eu"}
            ]}));
        }
        error(404, "unexpected")
    })
    .await;
    let dir = tempfile::tempdir().unwrap();
    let gcloud = dir.path().join("gcloud");
    std::fs::write(&gcloud, "#!/bin/sh\necho token\n").unwrap();
    std::fs::set_permissions(&gcloud, std::fs::Permissions::from_mode(0o755)).unwrap();
    let tokens = Arc::new(Tokens::with_program(gcloud.to_string_lossy(), None));
    let fetcher = GcpFetcher::with_client(GcpClient::with_endpoints(tokens, &endpoint, &endpoint));

    let mut app = App::with_fetcher(
        Cache::new(Path::new(":memory:")).unwrap(),
        Some(Arc::new(fetcher)),
    );
    app.state_mut().provider = Provider::Gcp;
    let mut rx = app.take_load_receiver().unwrap();

    app.request(LoadRequest::Subscriptions);
    settle(&mut app, &mut rx).await;
    let projects: Vec<(String, String)> = app
        .state()
        .subscriptions
        .iter()
        .map(|s| (s.id.clone(), s.name.clone()))
        .collect();
    assert_eq!(
        projects,
        [("billing-prod".to_string(), "Billing".to_string())]
    );

    app.request(LoadRequest::Resources {
        subscription_ids: vec!["billing-prod".into()],
        resource_group: None,
    });
    settle(&mut app, &mut rx).await;
    let resources = app.state().resources.clone();
    assert_eq!(resources.len(), 2);
    let vm = resources.iter().find(|r| r.id == VM).unwrap().clone();
    assert_eq!(
        (
            vm.name.as_str(),
            vm.resource_type.as_str(),
            vm.resource_group.as_str()
        ),
        ("vm-1", "compute.googleapis.com/Instance", "europe-west1-b")
    );
    assert_eq!(vm.tags.get("env").map(String::as_str), Some("prod"));

    // A location as the container narrows the list.
    app.state_mut().resources.clear();
    app.request(LoadRequest::Resources {
        subscription_ids: vec!["billing-prod".into()],
        resource_group: Some("eu".into()),
    });
    settle(&mut app, &mut rx).await;
    assert_eq!(app.state().resources.len(), 1);

    // The detail view's document is the asset as Cloud Asset returned it.
    app.open_resource_detail(vm);
    settle(&mut app, &mut rx).await;
    let doc = app.state().resource_document.clone().expect("a document");
    assert_eq!(doc["state"], "RUNNING");

    // Folders are the tree; without the organization, a root holds them.
    app.request(LoadRequest::ScopeTree);
    settle(&mut app, &mut rx).await;
    let tree = app.state().scope_tree.clone().expect("a tree");
    assert_eq!(tree.root_display_name, "No organization");
    assert_eq!(tree.subscriptions_under("folders/2", &[]), ["billing-prod"]);

    // The subscriptions cache is Azure's: projects are not written to it.
    let cached = app.cache().repository().cached_subscriptions().unwrap();
    assert!(cached.items.is_empty(), "{:?}", cached.items);
}

async fn settle(app: &mut App, rx: &mut c9s_core::loader::LoadReceiver) {
    while app.state().loading {
        app.apply(rx.recv().await.unwrap());
    }
}
