//! Browsing AWS: the profile's account, its resources and a resource's
//! document, through the AWS fetcher against a mock AWS API.
#![cfg(unix)]

use c9s_aws::{AwsClient, Session};
use c9s_azure::testing::{ok, serve_paths};
use c9s_core::aws::AwsFetcher;
use c9s_core::cache::Cache;
use c9s_core::loader::LoadRequest;
use c9s_core::provider::Provider;
use c9s_core::App;
use serde_json::json;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::sync::Arc;

const VM: &str = "arn:aws:ec2:eu-west-1:111122223333:instance/i-0abc";

#[tokio::test]
async fn account_resources_and_detail() {
    let (endpoint, _) = serve_paths(|path, _| {
        assert_eq!(path, "/Search");
        ok(json!({"Resources": [
            {"Arn": VM, "ResourceType": "ec2:instance", "Region": "eu-west-1",
             "OwningAccountId": "111122223333",
             "Properties": [{"Name": "tags", "Data": [{"Key": "env", "Value": "prod"}]}]},
            {"Arn": "arn:aws:s3:::billing-logs", "ResourceType": "s3:bucket", "Region": ""}
        ], "Count": {"Complete": true, "TotalResults": 2}}))
    })
    .await;
    let dir = tempfile::tempdir().unwrap();
    let aws = dir.path().join("aws");
    std::fs::write(
        &aws,
        r#"#!/bin/sh
case "$1 $2" in
  "configure export-credentials") echo '{"AccessKeyId":"A","SecretAccessKey":"S"}' ;;
  "sts get-caller-identity") echo '{"Account":"111122223333","Arn":"arn:aws:iam::111122223333:user/ada"}' ;;
esac
"#,
    )
    .unwrap();
    std::fs::set_permissions(&aws, std::fs::Permissions::from_mode(0o755)).unwrap();
    let session = Arc::new(Session::with_program(aws.to_string_lossy(), "prod"));
    let fetcher =
        AwsFetcher::with_client(AwsClient::with_endpoint(session, "eu-west-1", &endpoint));

    let mut app = App::with_fetcher(
        Cache::new(Path::new(":memory:")).unwrap(),
        Some(Arc::new(fetcher)),
    );
    app.state_mut().provider = Provider::Aws;
    let mut rx = app.take_load_receiver().unwrap();

    app.request(LoadRequest::Subscriptions);
    settle(&mut app, &mut rx).await;
    let accounts: Vec<(String, String)> = app
        .state()
        .subscriptions
        .iter()
        .map(|s| (s.id.clone(), s.name.clone()))
        .collect();
    assert_eq!(accounts, [("111122223333".to_string(), "prod".to_string())]);

    app.request(LoadRequest::Resources {
        subscription_ids: vec!["111122223333".into()],
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
            vm.location.as_str()
        ),
        ("i-0abc", "ec2:instance", "eu-west-1")
    );
    assert_eq!(vm.tags.get("env").map(String::as_str), Some("prod"));
    let bucket = resources.iter().find(|r| r.name == "billing-logs").unwrap();
    assert_eq!(bucket.location, "global");

    // The detail view's document is the search result.
    app.open_resource_detail(vm);
    settle(&mut app, &mut rx).await;
    let doc = app.state().resource_document.clone().expect("a document");
    assert_eq!(doc["OwningAccountId"], "111122223333");

    // The subscriptions cache is Azure's: the account is not written to it.
    let cached = app.cache().repository().cached_subscriptions().unwrap();
    assert!(cached.items.is_empty(), "{:?}", cached.items);
}

async fn settle(app: &mut App, rx: &mut c9s_core::loader::LoadReceiver) {
    while app.state().loading {
        app.apply(rx.recv().await.unwrap());
    }
}
