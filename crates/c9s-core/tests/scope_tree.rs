//! The management group tree end to end: GraphFetcher + ManagementGroupClient
//! against a local mock of the management groups API, through App.

use c9s_azure::management_groups::ManagementGroupClient;
use c9s_azure::testing::{error, ok, serve_paths, StaticToken};
use c9s_azure::ResourceGraphClient;
use c9s_core::cache::Cache;
use c9s_core::loader::{ChunkSink, FetchOutcome, Fetcher, GraphFetcher, LoadRequest};
use c9s_core::scope_tree::ScopeTree;
use c9s_core::state::ViewMode;
use c9s_core::App;
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
                    "parent": {"id": root.as_str()}}},
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
        notice.starts_with("No read access to management groups: showing subscriptions"),
        "a plain explanation, not Azure's long 403 text: {notice}"
    );
}

#[tokio::test]
async fn other_tree_failures_still_say_why() {
    let (url, _) = serve_paths(|_, _| error(500, "Internal error")).await;
    let mut app = app(&url, Cache::new(Path::new(":memory:")).unwrap());
    app.open_scope_tree();
    settle(&mut app).await;
    assert_eq!(app.state().view, ViewMode::SubscriptionList);
    let notice = app.state().notice.clone().unwrap();
    assert!(
        notice.contains("Management group tree unavailable") && notice.contains("500"),
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

#[tokio::test]
async fn config_can_start_on_subscriptions_and_reports_a_broken_file() {
    use c9s_core::config::{Config, Loaded, StartView};
    let (url, _) = serve_paths(|_, _| error(500, "unused")).await;
    let mut app = app(&url, Cache::new(Path::new(":memory:")).unwrap());
    app.apply_config(Loaded {
        config: Config {
            start: StartView::Sub,
            ..Config::default()
        },
        path: None,
        problem: Some("/x/config.toml: line 1: unknown field `colour`; using defaults".into()),
    });
    assert_eq!(app.state().view, ViewMode::SubscriptionList);
    let notice = app.state().notice.clone().unwrap();
    assert!(
        notice.starts_with("Config /x/config.toml: line 1"),
        "{notice}"
    );
}

#[tokio::test]
async fn switching_tenant_rebuilds_the_fetcher_and_keeps_a_tree_per_tenant() {
    use c9s_core::app::FetcherFactory;
    use c9s_core::models::Subscription;
    use std::sync::Mutex;

    let (url, _) = serve_paths(|_, _| error(500, "unused")).await;
    let cache = Cache::new(Path::new(":memory:")).unwrap();
    // A cached tree for tenant t-2 only.
    let t2_tree = ScopeTree {
        root: "t-2".into(),
        root_display_name: "Fabrikam Root".into(),
        ..Default::default()
    };
    cache
        .repository()
        .put_document("scope_tree:t-2", &serde_json::to_string(&t2_tree).unwrap())
        .unwrap();
    let mut app = app(&url, cache);
    let asked: Arc<Mutex<Vec<Option<String>>>> = Arc::default();
    let asked_in_factory = asked.clone();
    let factory: FetcherFactory = Arc::new(move |_provider, tenant: Option<&str>| {
        asked_in_factory
            .lock()
            .unwrap()
            .push(tenant.map(str::to_string));
        Some(Arc::new(NoArm) as Arc<dyn Fetcher>)
    });
    app.set_fetcher_factory(factory);
    let sub = |id: &str, tenant: &str| Subscription {
        id: id.into(),
        name: id.into(),
        state: "Enabled".into(),
        tenant_id: tenant.into(),
    };
    app.state_mut().subscriptions = vec![sub("a", "t-1"), sub("b", "t-2"), sub("c", "T-2")];
    app.state_mut().resources = vec![]; // stand-in for lists of the old tenant

    app.switch_tenant(Some("t-2".into()));
    assert_eq!(asked.lock().unwrap().as_slice(), [Some("t-2".to_string())]);
    assert_eq!(app.state().view, ViewMode::ManagementGroupTree);
    assert_eq!(
        app.state()
            .scope_tree
            .as_ref()
            .map(|t| t.root_display_name.as_str()),
        Some("Fabrikam Root"),
        "that tenant's cached tree"
    );
    app.state_mut().view = ViewMode::SubscriptionList;
    let visible: Vec<String> = app
        .state()
        .filtered_subscriptions()
        .iter()
        .map(|s| s.id.clone())
        .collect();
    assert_eq!(visible, ["b", "c"], "only that tenant's subscriptions");

    app.switch_tenant(None);
    assert_eq!(asked.lock().unwrap().last().unwrap(), &None);
    assert!(
        app.state().scope_tree.is_none(),
        "no tree cached for the default"
    );
    app.state_mut().view = ViewMode::SubscriptionList;
    assert_eq!(app.state().filtered_subscriptions().len(), 3);
}

#[test]
fn tenant_rows_merge_names_with_subscription_tenants() {
    use c9s_core::models::{Subscription, Tenant};
    use c9s_core::state::AppState;
    let mut state = AppState::new();
    state.tenants = vec![Tenant {
        id: "t-1".into(),
        name: "Contoso".into(),
        domain: Some("contoso.onmicrosoft.com".into()),
    }];
    state.subscriptions = ["t-1", "t-1", "t-9"]
        .iter()
        .enumerate()
        .map(|(i, t)| Subscription {
            id: format!("s{i}"),
            name: format!("s{i}"),
            state: "Enabled".into(),
            tenant_id: t.to_string(),
        })
        .collect();
    state.tenant = Some("T-1".into());
    let rows = state.tenant_rows();
    let summary: Vec<(Option<&str>, &str, usize, bool)> = rows
        .iter()
        .map(|r| (r.id.as_deref(), r.name.as_str(), r.subscriptions, r.current))
        .collect();
    assert_eq!(
        summary,
        [
            (None, "All tenants", 3, false),
            (Some("t-1"), "Contoso", 2, true),
            (Some("t-9"), "t-9", 1, false),
        ]
    );
    assert_eq!(state.tenant_label().as_deref(), Some("Contoso"));
}

#[tokio::test]
async fn config_tenant_starts_in_that_tenant() {
    use c9s_core::app::FetcherFactory;
    use c9s_core::config::{Config, Loaded};
    use std::sync::Mutex;
    let (url, _) = serve_paths(|_, _| error(500, "unused")).await;
    let mut app = app(&url, Cache::new(Path::new(":memory:")).unwrap());
    let asked: Arc<Mutex<Vec<Option<String>>>> = Arc::default();
    let asked_in_factory = asked.clone();
    let factory: FetcherFactory = Arc::new(move |_provider, tenant: Option<&str>| {
        asked_in_factory
            .lock()
            .unwrap()
            .push(tenant.map(str::to_string));
        Some(Arc::new(NoArm) as Arc<dyn Fetcher>)
    });
    app.set_fetcher_factory(factory);
    app.apply_config(Loaded {
        config: Config {
            tenant: Some("t-2".into()),
            ..Config::default()
        },
        path: None,
        problem: None,
    });
    assert_eq!(app.state().tenant.as_deref(), Some("t-2"));
    assert_eq!(asked.lock().unwrap().as_slice(), [Some("t-2".to_string())]);
}

#[tokio::test]
async fn policy_view_shows_inherited_assignments_with_compliance() {
    use c9s_core::policy::PolicyScope;
    use c9s_core::scope_tree::{GroupNode, SubscriptionNode};
    use serde_json::json;
    let mg = |n: &str| format!("/providers/Microsoft.Management/managementGroups/{n}");
    let assignment = |name: &str, scope: &str| {
        json!({
            "id": format!("{scope}/providers/Microsoft.Authorization/policyAssignments/{name}"),
            "name": name, "displayName": format!("Policy {name}"), "scope": scope,
            "definitionId": "/providers/Microsoft.Authorization/policySetDefinitions/x",
            "enforcementMode": "Default", "description": ""
        })
    };
    let rows = vec![
        assignment("root-a", &mg(TENANT)),
        assignment("plat-a", &mg("platform")),
        assignment("sub-a", "/subscriptions/sub-1"),
        assignment("other", &mg("elsewhere")),
    ];
    let sub_a = rows[2]["id"].as_str().unwrap().to_lowercase();
    let root_a = rows[0]["id"].as_str().unwrap().to_lowercase();
    let (url, requests) = serve_paths(move |_, body| {
        let query = body["query"].as_str().unwrap_or_default();
        if query.contains("policyassignments") {
            ok(json!({"totalRecords": 4, "count": 4, "data": rows.clone()}))
        } else if query.contains("by subscription") {
            ok(json!({"totalRecords": 1, "count": 1, "data": [
                {"subscription": "SUB-1", "resources": 12, "nonCompliant": 8}
            ]}))
        } else if query.contains("policystates") {
            ok(json!({"totalRecords": 2, "count": 2, "data": [
                {"assignment": sub_a, "total": 10, "nonCompliant": 1},
                {"assignment": root_a, "total": 10, "nonCompliant": 7}
            ]}))
        } else {
            error(400, "unexpected query")
        }
    })
    .await;
    let mut app = app(&url, Cache::new(Path::new(":memory:")).unwrap());
    app.state_mut().scope_tree = Some(ScopeTree {
        root: TENANT.into(),
        root_display_name: "Tenant Root Group".into(),
        groups: vec![GroupNode {
            name: "platform".into(),
            display_name: "Platform".into(),
            parent: TENANT.into(),
        }],
        subscriptions: vec![SubscriptionNode {
            id: "sub-1".into(),
            display_name: "sub-1".into(),
            parent: "platform".into(),
        }],
    });

    app.open_policy(PolicyScope::Subscription("sub-1".into()));
    assert_eq!(app.state().view, ViewMode::PolicyAssignments);
    settle(&mut app).await;

    let rows = app.state().policy_rows();
    let summary: Vec<(&str, &str, Option<u64>)> = rows
        .iter()
        .map(|r| {
            (
                r.assignment.name.as_str(),
                r.origin_label.as_str(),
                r.compliance.map(|c| c.non_compliant),
            )
        })
        .collect();
    assert_eq!(
        summary,
        [
            ("root-a", "from Tenant Root Group", Some(7)),
            ("sub-a", "this scope", Some(1)),
            ("plat-a", "from Platform", None),
        ],
        "most non-compliant first; other management groups' assignments left out"
    );

    // Compliance was asked for this subscription only; assignments tenant-wide.
    let bodies = requests.lock().unwrap().clone();
    let compliance = bodies
        .iter()
        .find(|b| {
            let q = b["query"].as_str().unwrap();
            q.contains("policystates") && !q.contains("by subscription")
        })
        .unwrap();
    assert_eq!(compliance["subscriptions"], json!(["sub-1"]));
    let totals = bodies
        .iter()
        .find(|b| b["query"].as_str().unwrap().contains("by subscription"))
        .unwrap();
    assert_eq!(totals["subscriptions"], json!(["sub-1"]));

    // The scope's summary: distinct resources, summed per subscription.
    let summary = app.state().policy_summary().unwrap();
    assert_eq!(
        (summary.resources.total, summary.resources.non_compliant),
        (12, 8)
    );
    assert_eq!(
        (summary.subscriptions, summary.subscriptions_affected),
        (1, 1)
    );
    let assignments = bodies
        .iter()
        .find(|b| b["query"].as_str().unwrap().contains("policyassignments"))
        .unwrap();
    assert_eq!(assignments["managementGroups"], json!([TENANT]));

    // Both are cached for next time.
    let repo = app.cache().repository();
    assert!(repo.document("policy_assignments").unwrap().is_some());
    assert!(repo
        .document("policy_compliance:/subscriptions/sub-1")
        .unwrap()
        .is_some());
    assert!(repo
        .document("compliance_summary:/subscriptions/sub-1")
        .unwrap()
        .is_some());
}

#[tokio::test]
async fn resource_policy_states_get_definition_names_from_graph_then_arm() {
    use serde_json::json;
    const DEF_A: &str = "/providers/Microsoft.Authorization/policyDefinitions/a";
    const DEF_B: &str = "/providers/Microsoft.Authorization/policyDefinitions/b";
    const SET: &str = "/providers/Microsoft.Authorization/policySetDefinitions/set";
    let resource = "/subscriptions/sub-1/resourceGroups/rg/providers/x/y/kv";
    let (url, _) = serve_paths(move |path, body| {
        let query = body["query"].as_str().unwrap_or_default();
        if path.starts_with(DEF_B) {
            // ARM: the definition Resource Graph didn't list (a built-in).
            ok(json!({"id": DEF_B, "properties": {"displayName": "Built-in B"}}))
        } else if query.contains("displayName = tostring(properties.displayName)") {
            ok(json!({"totalRecords": 2, "count": 2, "data": [
                {"id": DEF_A.to_lowercase(), "displayName": "Custom A"},
                {"id": SET.to_lowercase(), "displayName": "My initiative"}
            ]}))
        } else if query.contains("policystates") {
            ok(json!({"totalRecords": 2, "count": 2, "data": [
                {"resourceId": resource, "assignmentId": "/a1", "definition": "ref-a",
                 "definitionId": DEF_A, "setDefinitionId": SET, "state": "NonCompliant"},
                {"resourceId": resource, "assignmentId": "/a2", "definition": "b",
                 "definitionId": DEF_B, "setDefinitionId": "", "state": "Compliant"}
            ]}))
        } else {
            error(400, "unexpected request")
        }
    })
    .await;
    let mut app = app(&url, Cache::new(Path::new(":memory:")).unwrap());
    app.state_mut().selected_resource_detail = Some(c9s_core::models::Resource {
        id: resource.into(),
        name: "kv".into(),
        resource_type: "x/y".into(),
        location: String::new(),
        resource_group: "rg".into(),
        subscription_id: "sub-1".into(),
        tags: Default::default(),
    });
    app.load_resource_policy(false);
    settle(&mut app).await;

    let rows = app.state().detail_policy_states().unwrap();
    let a = &rows[0].1;
    assert_eq!(
        (a.policy_label(), a.source_label().as_str()),
        ("Custom A", "initiative My initiative")
    );
    let b = &rows[1].1;
    assert_eq!(
        (b.policy_label(), b.source_label().as_str()),
        ("Built-in B", "direct")
    );
}

#[tokio::test]
async fn role_assignments_fall_back_to_the_subscriptions_without_tenant_root_access() {
    use serde_json::json;
    let (url, requests) = serve_paths(|_, body| {
        if body.get("managementGroups").is_some() {
            error(403, "AuthorizationFailed")
        } else {
            ok(json!({"totalRecords": 1, "count": 1, "data": [{
                "id": "/subscriptions/sub-1/providers/Microsoft.Authorization/roleAssignments/a",
                "scope": "/subscriptions/sub-1",
                "roleDefinitionId": "/subscriptions/sub-1/providers/Microsoft.Authorization/roleDefinitions/r",
                "principalId": "p", "principalType": "User"
            }]}))
        }
    })
    .await;
    let mut app = app(&url, Cache::new(Path::new(":memory:")).unwrap());
    app.state_mut().subscriptions = vec![c9s_core::models::Subscription {
        id: "sub-1".into(),
        name: "sub-1".into(),
        state: "Enabled".into(),
        tenant_id: TENANT.into(),
    }];
    app.open_role_assignments(None, None);
    settle(&mut app).await;

    assert_eq!(app.state().role_assignments.len(), 1);
    assert!(
        app.state()
            .notice
            .as_deref()
            .unwrap_or_default()
            .contains("No access at the tenant root"),
        "{:?}",
        app.state().notice
    );
    let bodies = requests.lock().unwrap().clone();
    assert!(bodies
        .iter()
        .any(|b| b["subscriptions"] == json!(["sub-1"])));
}

#[tokio::test]
async fn built_in_role_names_come_from_arm_when_resource_graph_lists_none() {
    use serde_json::json;
    let reader = "/providers/Microsoft.Authorization/roleDefinitions/acdd72a7";
    let (url, _) = serve_paths(move |path, body| {
        let query = body["query"].as_str().unwrap_or_default();
        if path.contains("/providers/Microsoft.Authorization/roleDefinitions?") {
            // ARM: every role usable at the tenant root, built-in ones too.
            ok(json!({"value": [{"id": reader, "name": "acdd72a7",
                "properties": {"roleName": "Reader", "type": "BuiltInRole"}}]}))
        } else if path.contains("/roleDefinitions/acdd72a7?") {
            ok(json!({"id": reader, "name": "acdd72a7", "type": "Microsoft.Authorization/roleDefinitions",
                "properties": {"roleName": "Reader", "type": "BuiltInRole"}}))
        } else if query.contains("roledefinitions") {
            // Resource Graph: only the custom role.
            ok(json!({"totalRecords": 1, "count": 1, "data": [{
                "id": "/subscriptions/sub-1/providers/Microsoft.Authorization/roleDefinitions/c1",
                "roleName": "App Deployer", "roleType": "CustomRole"}]}))
        } else if query.contains("roleassignments") {
            ok(json!({"totalRecords": 0, "count": 0, "data": []}))
        } else if query.contains("authorizationresources | where id") {
            // The definition by ID: not in Resource Graph.
            ok(json!({"totalRecords": 0, "count": 0, "data": []}))
        } else {
            error(400, "unexpected")
        }
    })
    .await;
    let mut app = app(&url, Cache::new(Path::new(":memory:")).unwrap());
    let mut rx = app.take_load_receiver().unwrap();
    app.open_role_definitions();
    while app.state().loading {
        app.apply(rx.recv().await.unwrap());
    }
    let mut names: Vec<String> = app
        .state()
        .role_definitions
        .iter()
        .map(|d| format!("{} ({})", d.role_name, d.kind_label()))
        .collect();
    names.sort();
    assert_eq!(names, ["App Deployer (custom)", "Reader (built-in)"]);

    // Opening a role named at a subscription: found through ARM.
    app.open_resource_detail(c9s_core::models::Resource {
        id: "/subscriptions/sub-1/providers/Microsoft.Authorization/roleDefinitions/acdd72a7"
            .into(),
        name: "acdd72a7".into(),
        resource_type: "Microsoft.Authorization/roleDefinitions".into(),
        location: String::new(),
        resource_group: String::new(),
        subscription_id: String::new(),
        tags: Default::default(),
    });
    while app.state().loading {
        app.apply(rx.recv().await.unwrap());
    }
    let doc = app
        .state()
        .resource_document
        .clone()
        .expect("the definition from ARM");
    assert_eq!(doc["properties"]["roleName"], "Reader");
}
