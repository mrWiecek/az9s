//! Browsing AWS: the profile's account, its resources and a resource's
//! document, through the AWS fetcher against a mock AWS API.
#![cfg(unix)]

use c9s_aws::{AwsClient, Session};
use c9s_azure::testing::{error, ok, serve_paths};
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
        // Organizations: not this profile's.
        if path == "/" {
            let mut r = error(400, "x");
            r.body = json!({"__type": "AWSOrganizationsNotInUseException",
                            "Message": "not in an organization"})
            .to_string();
            return r;
        }
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

#[tokio::test]
async fn an_organization_lists_every_account_and_reaches_each() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let searches = Arc::new(AtomicUsize::new(0));
    let counter = searches.clone();
    let (endpoint, requests) = serve_paths(move |path, body| {
        if path == "/Search" {
            let n = counter.fetch_add(1, Ordering::SeqCst);
            return ok(json!({"Resources": [
                {"Arn": format!("arn:aws:sqs:eu-west-1:{n}:queue-{n}"), "ResourceType": "sqs:queue",
                 "Region": "eu-west-1"}
            ]}));
        }
        if body.is_null() {
            // STS AssumeRole (a form body).
            return c9s_azure::testing::Reply {
                status: 200,
                headers: vec![],
                body: "<Credentials><AccessKeyId>ASIAROLE</AccessKeyId>\
                       <SecretAccessKey>s</SecretAccessKey><SessionToken>t</SessionToken>\
                       </Credentials>"
                    .into(),
                delay: Default::default(),
            };
        }
        match body.get("ParentId").and_then(|p| p.as_str()) {
            None => ok(json!({"Roots": [{"Id": "r-1", "Name": "Root"}]})),
            Some("r-1") => ok(json!({
                "OrganizationalUnits": [{"Id": "ou-1", "Name": "Workloads"}],
                "Accounts": [{"Id": "111122223333", "Name": "management", "Status": "ACTIVE"}]
            })),
            Some("ou-1") => ok(json!({
                "OrganizationalUnits": [],
                "Accounts": [
                    {"Id": "222233334444", "Name": "payments", "Status": "ACTIVE"},
                    {"Id": "555566667777", "Name": "sandbox", "Status": "ACTIVE"},
                    {"Id": "888899990000", "Name": "closed", "Status": "SUSPENDED"}
                ]
            })),
            Some(_) => error(404, "unexpected"),
        }
    })
    .await;
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("calls.log");
    let aws = dir.path().join("aws");
    std::fs::write(
        &aws,
        format!(
            r#"#!/bin/sh
echo "$*" >> {}
case "$1 $2" in
  "configure export-credentials") echo '{{"AccessKeyId":"A","SecretAccessKey":"S"}}' ;;
  "sts get-caller-identity") echo '{{"Account":"111122223333","Arn":"arn:aws:iam::111122223333:user/ada"}}' ;;
esac
"#,
            log.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&aws, std::fs::Permissions::from_mode(0o755)).unwrap();
    let session = Arc::new(Session::with_program(aws.to_string_lossy(), "mgmt"));
    // payments has its own SSO profile; sandbox is reached through the role.
    let fetcher =
        AwsFetcher::with_client(AwsClient::with_endpoint(session, "eu-west-1", &endpoint))
            .with_profiles(vec![c9s_core::aws::Profile {
                name: "payments-sso".into(),
                region: "eu-west-1".into(),
                kind: "sso".into(),
                account_id: "222233334444".into(),
            }])
            .with_role("Auditor");
    let mut app = App::with_fetcher(
        Cache::new(Path::new(":memory:")).unwrap(),
        Some(Arc::new(fetcher)),
    );
    app.state_mut().provider = Provider::Aws;
    let mut rx = app.take_load_receiver().unwrap();

    app.request(LoadRequest::Subscriptions);
    settle(&mut app, &mut rx).await;
    let accounts: Vec<(String, String, String)> = app
        .state()
        .subscriptions
        .iter()
        .map(|s| (s.id.clone(), s.name.clone(), s.state.clone()))
        .collect();
    assert_eq!(accounts.len(), 4, "{accounts:?}");
    assert!(accounts.contains(&("888899990000".into(), "closed".into(), "Disabled".into())));

    app.request(LoadRequest::ScopeTree);
    settle(&mut app, &mut rx).await;
    let tree = app.state().scope_tree.clone().expect("the organization");
    assert_eq!(tree.root_display_name, "Root (organization)");
    let mut under = tree.subscriptions_under("ou-1", &[]);
    under.sort();
    assert_eq!(under, ["222233334444", "555566667777", "888899990000"]);

    app.request(LoadRequest::Resources {
        subscription_ids: vec![
            "111122223333".into(),
            "222233334444".into(),
            "555566667777".into(),
        ],
        resource_group: None,
    });
    settle(&mut app, &mut rx).await;
    assert_eq!(app.state().resources.len(), 3, "one search per account");
    let calls = std::fs::read_to_string(&log).unwrap();
    assert!(
        calls.contains("export-credentials --profile payments-sso"),
        "payments through its SSO profile:\n{calls}"
    );
    let assumed = requests
        .lock()
        .unwrap()
        .iter()
        .filter(|b| b.is_null())
        .count();
    assert_eq!(assumed, 1, "sandbox through the role, once");
}

async fn settle(app: &mut App, rx: &mut c9s_core::loader::LoadReceiver) {
    while app.state().loading {
        app.apply(rx.recv().await.unwrap());
    }
}
