//! `:policy`: assignments that apply at a scope, with compliance.

use az9s_core::cache::Cache;
use az9s_core::loader::{ChunkSink, FetchOutcome, Fetcher, LoadRequest, LoadedData};
use az9s_core::models::Subscription;
use az9s_core::policy::{Compliance, PolicyAssignment, PolicyScope, PolicyState};
use az9s_core::scope_tree::{GroupNode, ScopeTree, SubscriptionNode};
use az9s_core::state::{DetailTab, ViewMode};
use az9s_core::App;
use az9s_tui::{ui, Event, TuiApp};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use futures::future::BoxFuture;
use futures::FutureExt;
use ratatui::{backend::TestBackend, Terminal};
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

fn mg(name: &str) -> String {
    format!("/providers/Microsoft.Management/managementGroups/{name}")
}

const KV: &str =
    "/subscriptions/s1/resourceGroups/rg-app/providers/Microsoft.KeyVault/vaults/kv-app";

fn state(resource_id: &str, assignment: &str, definition: &str, compliance: &str) -> PolicyState {
    PolicyState {
        resource_id: resource_id.into(),
        resource_type: "Microsoft.KeyVault/vaults".into(),
        resource_group: "rg-app".into(),
        subscription_id: "s1".into(),
        location: "westeurope".into(),
        assignment_id: assignment.into(),
        definition: definition.into(),
        state: compliance.into(),
        timestamp: String::new(),
    }
}

fn assignment(name: &str, scope: &str, initiative: bool) -> PolicyAssignment {
    PolicyAssignment {
        id: format!("{scope}/providers/Microsoft.Authorization/policyAssignments/{name}"),
        name: name.into(),
        display_name: format!("Require {name}"),
        scope: scope.into(),
        definition_id: if initiative {
            "/providers/Microsoft.Authorization/policySetDefinitions/x".into()
        } else {
            "/providers/Microsoft.Authorization/policyDefinitions/x".into()
        },
        enforcement: "Default".into(),
        description: String::new(),
    }
}

#[derive(Default)]
struct Azure(Mutex<Vec<LoadRequest>>);

impl Fetcher for Azure {
    fn fetch(&self, request: LoadRequest, _sink: ChunkSink) -> BoxFuture<'static, FetchOutcome> {
        self.0.lock().unwrap().push(request.clone());
        let data = match request {
            LoadRequest::PolicyAssignments => Ok(LoadedData::PolicyAssignments(vec![
                assignment("tags", &mg("root"), false),
                assignment("security-baseline", &mg("platform"), true),
                assignment("sub-only", "/subscriptions/s1", false),
            ])),
            LoadRequest::PolicyCompliance { scope } => {
                let id = assignment("security-baseline", &mg("platform"), true)
                    .id
                    .to_ascii_lowercase();
                Ok(LoadedData::PolicyCompliance(
                    scope,
                    HashMap::from([(
                        id,
                        Compliance {
                            non_compliant: 12,
                            total: 40,
                        },
                    )]),
                ))
            }
            LoadRequest::PolicyStates { assignment_id, .. } => Ok(LoadedData::PolicyStates(vec![
                state(KV, &assignment_id, "kv-purge-protection", "NonCompliant"),
                state(KV, &assignment_id, "kv-private-endpoint", "NonCompliant"),
                state(
                    "/subscriptions/s1/resourceGroups/rg-app/providers/Microsoft.Storage/storageAccounts/stapp",
                    &assignment_id,
                    "st-https-only",
                    "NonCompliant",
                ),
            ])),
            LoadRequest::ResourcePolicy { resource_id, .. } => {
                let baseline = assignment("security-baseline", &mg("platform"), true).id;
                let tags = assignment("tags", &mg("root"), false).id;
                Ok(LoadedData::ResourcePolicy(
                    resource_id.clone(),
                    vec![
                        state(&resource_id, &tags, "require-tag", "Compliant"),
                        state(&resource_id, &baseline, "kv-purge-protection", "NonCompliant"),
                    ],
                ))
            }
            _ => Err("not in this test".into()),
        };
        async move {
            FetchOutcome {
                data,
                failed_subscriptions: vec![],
                notice: None,
            }
        }
        .boxed()
    }
}

fn setup(with_tree: bool) -> (TuiApp, Arc<Azure>) {
    let azure = Arc::new(Azure::default());
    let mut core = App::with_fetcher(
        Cache::new(Path::new(":memory:")).unwrap(),
        Some(azure.clone()),
    );
    let state = core.state_mut();
    state.subscriptions = vec![Subscription {
        id: "s1".into(),
        name: "prod".into(),
        state: "Enabled".into(),
        tenant_id: "root".into(),
    }];
    if with_tree {
        state.scope_tree = Some(ScopeTree {
            root: "root".into(),
            root_display_name: "Tenant Root Group".into(),
            groups: vec![GroupNode {
                name: "platform".into(),
                display_name: "Platform".into(),
                parent: "root".into(),
            }],
            subscriptions: vec![SubscriptionNode {
                id: "s1".into(),
                display_name: "prod".into(),
                parent: "platform".into(),
            }],
        });
    }
    (TuiApp::new(core), azure)
}

async fn press(app: &mut TuiApp, code: KeyCode) {
    app.handle_event(Event::Key(KeyEvent::new(code, KeyModifiers::NONE)))
        .await
        .unwrap();
}

async fn command(app: &mut TuiApp, text: &str) {
    press(app, KeyCode::Char(':')).await;
    for c in text.chars() {
        press(app, KeyCode::Char(c)).await;
    }
    press(app, KeyCode::Enter).await;
}

fn screen(app: &mut TuiApp) -> String {
    let mut terminal = Terminal::new(TestBackend::new(150, 20)).unwrap();
    terminal.draw(|f| ui::draw(f, app).unwrap()).unwrap();
    let buf = terminal.backend().buffer();
    (0..buf.area.height)
        .map(|y| {
            (0..buf.area.width)
                .map(|x| buf.get(x, y).symbol().to_string())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[tokio::test]
async fn policy_for_the_highlighted_subscription_with_inheritance() {
    let (mut app, _) = setup(true);
    let mut loads = app.take_load_receiver().unwrap();

    command(&mut app, "policy").await; // subscription list, "prod" highlighted
    assert_eq!(app.state().view, ViewMode::PolicyAssignments);
    assert_eq!(
        app.state().policy_scope,
        Some(PolicyScope::Subscription("s1".into()))
    );
    app.apply_load(loads.recv().await.unwrap());
    app.apply_load(loads.recv().await.unwrap());

    let text = screen(&mut app);
    assert!(text.contains("Policy · prod (3 assignments)"), "{text}");
    assert!(text.contains("12 / 40 non-compliant"), "{text}");
    assert!(
        text.contains("from Platform") && text.contains("from Tenant Root Group"),
        "{text}"
    );
    assert!(text.contains("this scope"), "{text}");
    let first = &app.state().policy_rows()[0];
    assert_eq!(first.assignment.name, "security-baseline", "worst first");

    // d opens the assignment itself; Esc returns to the policy view.
    press(&mut app, KeyCode::Char('d')).await;
    assert_eq!(app.state().view, ViewMode::ResourceDetail);
    assert_eq!(
        app.state().selected_resource_detail.as_ref().unwrap().id,
        first.assignment.id
    );
    press(&mut app, KeyCode::Esc).await;
    assert_eq!(app.state().view, ViewMode::PolicyAssignments);
    assert_eq!(
        app.state().selected_policy_row().unwrap().assignment.id,
        first.assignment.id,
        "selection kept"
    );
}

#[tokio::test]
async fn drill_down_from_assignment_to_non_compliant_resource_and_back() {
    let (mut app, azure) = setup(true);
    let mut loads = app.take_load_receiver().unwrap();
    command(&mut app, "policy").await;
    app.apply_load(loads.recv().await.unwrap());
    app.apply_load(loads.recv().await.unwrap());
    let baseline = app.state().policy_rows()[0].assignment.clone();

    // Enter: the assignment's non-compliant resources in the policy scope.
    press(&mut app, KeyCode::Enter).await;
    assert_eq!(app.state().view, ViewMode::PolicyNonCompliant);
    app.apply_load(loads.recv().await.unwrap());
    assert!(azure
        .0
        .lock()
        .unwrap()
        .contains(&LoadRequest::PolicyStates {
            scope: PolicyScope::Subscription("s1".into()),
            assignment_id: baseline.id.clone(),
        }));
    let text = screen(&mut app);
    assert!(
        text.contains("Non-compliant · Require security-baseline · prod (2 resources)"),
        "{text}"
    );
    assert!(text.contains("kv-private-endpoint (+1 more)"), "{text}");
    assert!(text.contains("st-https-only"), "{text}");

    // `/` filters by failing policy too.
    app.state_mut().filter = "https".into();
    assert_eq!(app.state().non_compliant_rows().len(), 1);
    app.state_mut().filter.clear();

    // Enter: the resource's detail; its Policy tab loads its policy states.
    press(&mut app, KeyCode::Enter).await;
    assert_eq!(app.state().view, ViewMode::ResourceDetail);
    let resource = app.state().selected_resource_detail.clone().unwrap();
    assert_eq!(
        (resource.id.as_str(), resource.name.as_str()),
        (KV, "kv-app")
    );
    app.apply_load(loads.recv().await.unwrap()); // the document (not in this fake)
    press(&mut app, KeyCode::Char('3')).await;
    assert_eq!(app.state().detail_tab, DetailTab::Policy);
    app.apply_load(loads.recv().await.unwrap());
    let rows = app.state().detail_policy_states().unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(
        rows[0].0, "Require security-baseline",
        "non-compliant first"
    );
    let text = screen(&mut app);
    assert!(
        text.contains("NonCompliant  Require security-baseline"),
        "{text}"
    );
    assert!(text.contains("Compliant     Require tags"), "{text}");

    // Switching tabs back and forth does not load again.
    let before = azure.0.lock().unwrap().len();
    press(&mut app, KeyCode::Char('1')).await;
    press(&mut app, KeyCode::Char('3')).await;
    assert_eq!(azure.0.lock().unwrap().len(), before);

    // Esc, Esc: back to the non-compliant list, then the policy view.
    press(&mut app, KeyCode::Esc).await;
    assert_eq!(app.state().view, ViewMode::PolicyNonCompliant);
    assert_eq!(
        app.state()
            .selected_non_compliant()
            .unwrap()
            .state
            .resource_id,
        KV
    );
    press(&mut app, KeyCode::Esc).await;
    assert_eq!(app.state().view, ViewMode::PolicyAssignments);
}

#[tokio::test]
async fn policy_all_uses_the_tenant_root_and_needs_the_tree() {
    let (mut app, azure) = setup(true);
    command(&mut app, "policy all").await;
    assert_eq!(
        app.state().policy_scope,
        Some(PolicyScope::Group("root".into()))
    );
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    assert!(azure
        .0
        .lock()
        .unwrap()
        .contains(&LoadRequest::PolicyCompliance {
            scope: PolicyScope::Group("root".into())
        }));

    let (mut app, _) = setup(false);
    command(&mut app, "policy all").await;
    assert!(app
        .state()
        .error
        .as_deref()
        .unwrap()
        .contains("needs the management group tree"));
}
