//! `:policy`: assignments that apply at a scope, with compliance.

use c9s_core::cache::Cache;
use c9s_core::loader::{ChunkSink, FetchOutcome, Fetcher, LoadRequest, LoadedData};
use c9s_core::models::Subscription;
use c9s_core::policy::{Compliance, PolicyAssignment, PolicyExemption, PolicyScope, PolicyState};
use c9s_core::scope_tree::{GroupNode, ScopeTree, SubscriptionNode};
use c9s_core::state::{DetailTab, ViewMode};
use c9s_core::App;
use c9s_tui::{ui, Event, TuiApp};
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
        ..Default::default()
    }
}

const PURGE_DEF: &str = "/providers/Microsoft.Authorization/policyDefinitions/purge";

/// Three exemptions: expired (on the key vault), expiring in 10 days (on the
/// subscription), never (on the Platform group).
fn exemptions() -> Vec<PolicyExemption> {
    // An hour past the whole days, so the day count can't flip mid-test.
    let at = |days: i64| {
        let hour = chrono::Duration::hours(days.signum());
        (chrono::Utc::now() + chrono::Duration::days(days) + hour).to_rfc3339()
    };
    let exemption =
        |scope: &str, name: &str, assignment_name: &str, expires: String| PolicyExemption {
            id: format!("{scope}/providers/Microsoft.Authorization/policyExemptions/{name}"),
            name: name.into(),
            display_name: format!("Exempt {name}"),
            assignment_id: assignment(assignment_name, &mg("root"), false).id,
            category: "Waiver".into(),
            expires_on: expires,
            description: format!("because {name}"),
            references: vec![],
        };
    vec![
        exemption(&mg("platform"), "forever", "tags", String::new()),
        exemption("/subscriptions/s1", "soon", "tags", at(10)),
        exemption(KV, "old", "tags", at(-3)),
    ]
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
                        PolicyState {
                            definition_id:
                                "/providers/Microsoft.Authorization/policyDefinitions/tag".into(),
                            definition_name: "Require a tag on resources".into(),
                            ..state(&resource_id, &tags, "require-tag", "Compliant")
                        },
                        PolicyState {
                            definition_id: PURGE_DEF.into(),
                            definition_name: "Key vaults should have purge protection".into(),
                            set_definition_id:
                                "/providers/Microsoft.Authorization/policySetDefinitions/mcsb"
                                    .into(),
                            initiative_name: "Security Benchmark".into(),
                            ..state(&resource_id, &baseline, "kv-purge-protection", "NonCompliant")
                        },
                    ],
                ))
            }
            LoadRequest::PolicyExemptions => Ok(LoadedData::PolicyExemptions(exemptions())),
            LoadRequest::ComplianceRollup => Ok(LoadedData::ComplianceRollup(HashMap::from([
                (
                    "s1".to_string(),
                    Compliance {
                        non_compliant: 30,
                        total: 200,
                    },
                ),
                (
                    "s2".to_string(),
                    Compliance {
                        non_compliant: 0,
                        total: 50,
                    },
                ),
            ]))),
            LoadRequest::ComplianceSummary { scope } => Ok(LoadedData::ComplianceSummary(
                scope,
                HashMap::from([
                    (
                        "s1".to_string(),
                        Compliance {
                            non_compliant: 12,
                            total: 1500,
                        },
                    ),
                    (
                        "s2".to_string(),
                        Compliance {
                            non_compliant: 0,
                            total: 100,
                        },
                    ),
                ]),
            )),
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
    for _ in 0..3 {
        app.apply_load(loads.recv().await.unwrap()); // assignments, compliance, summary
    }

    let text = screen(&mut app);
    assert!(text.contains("Policy · prod (3 assignments)"), "{text}");
    assert!(text.contains("12 / 40 non-compliant"), "{text}");
    assert!(
        text.contains(
            "Compliance 99.2%  ·  12 of 1,600 resources non-compliant  ·  1 of 2 subscriptions \
             affected  ·  1 of 3 assignments with non-compliant resources"
        ),
        "{text}"
    );
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
    for _ in 0..3 {
        app.apply_load(loads.recv().await.unwrap()); // assignments, compliance, summary
    }
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
    app.apply_load(loads.recv().await.unwrap()); // states
    app.apply_load(loads.recv().await.unwrap()); // exemptions, for Exempt rows
    let rows = app.state().detail_policy_states().unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(
        rows[0].0, "Require security-baseline",
        "non-compliant first"
    );
    let text = screen(&mut app);
    for expected in [
        "State         Policy",
        "NonCompliant  Key vaults should have purge protection  initiative Security Benchmark  \
         Require security-baseline  Platform",
        "Compliant     Require a tag on resources               direct                         \
         Require tags               Tenant Root Group",
    ] {
        assert!(text.contains(expected), "{expected}\n{text}");
    }

    // Enter on a policy: its definition (from the tenant), Esc back to the
    // resource's Policy tab on the same row.
    press(&mut app, KeyCode::Enter).await;
    let definition = app.state().selected_resource_detail.clone().unwrap();
    assert_eq!(definition.id, PURGE_DEF);
    assert_eq!(definition.name, "Key vaults should have purge protection");
    assert_eq!(
        definition.subscription_id, "",
        "looked up across the tenant"
    );
    app.apply_load(loads.recv().await.unwrap()); // its document (not in this fake)
    press(&mut app, KeyCode::Esc).await;
    assert_eq!(
        app.state().selected_resource_detail.as_ref().unwrap().id,
        KV
    );
    assert_eq!(app.state().detail_tab, DetailTab::Policy);
    assert_eq!(
        app.state().selected_detail_policy().unwrap().definition_id,
        PURGE_DEF
    );
    app.apply_load(loads.recv().await.unwrap()); // the resource's document again
    press(&mut app, KeyCode::Char('j')).await;
    assert_eq!(app.state().detail_cursor, 1, "j moves the row cursor");
    press(&mut app, KeyCode::Char('j')).await;
    assert_eq!(app.state().detail_cursor, 1, "stops at the last row");

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

#[tokio::test]
async fn tree_shows_compliance_and_compliance_ranks_subscriptions() {
    let (mut app, _) = setup(true);
    let mut loads = app.take_load_receiver().unwrap();
    {
        // A second subscription, directly below the root; a third with no
        // evaluated resources.
        let state = app.state_mut();
        for (id, name) in [("s2", "dev"), ("s3", "sandbox")] {
            state.subscriptions.push(Subscription {
                id: id.into(),
                name: name.into(),
                state: "Enabled".into(),
                tenant_id: "root".into(),
            });
            state
                .scope_tree
                .as_mut()
                .unwrap()
                .subscriptions
                .push(SubscriptionNode {
                    id: id.into(),
                    display_name: name.into(),
                    parent: "root".into(),
                });
        }
    }

    // `:compliance all`: worst first, unevaluated last.
    command(&mut app, "compliance all").await;
    assert_eq!(app.state().view, ViewMode::ComplianceRanking);
    app.apply_load(loads.recv().await.unwrap());
    let rows = app.state().compliance_rows();
    let names: Vec<&str> = rows.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names, ["prod", "dev", "sandbox"]);
    assert_eq!(rows[0].group, "Platform");
    let text = screen(&mut app);
    for expected in [
        "Compliance · whole tenant (1 of 3 subscriptions with non-compliant resources)",
        "85.0% · 30 non-compliant",
        "100.0% · all 50 compliant",
        "not evaluated",
    ] {
        assert!(text.contains(expected), "{expected}\n{text}");
    }

    // A management group by name: only its subscriptions.
    command(&mut app, "compliance platform").await;
    let names: Vec<String> = app
        .state()
        .compliance_rows()
        .into_iter()
        .map(|r| r.name)
        .collect();
    assert_eq!(names, ["prod"]);

    // Enter: the subscription's policy view.
    press(&mut app, KeyCode::Enter).await;
    assert_eq!(app.state().view, ViewMode::PolicyAssignments);
    assert_eq!(
        app.state().policy_scope,
        Some(PolicyScope::Subscription("s1".into()))
    );
    assert_eq!(
        app.state()
            .policy_summary()
            .unwrap()
            .resources
            .non_compliant,
        30,
        "the summary shows at once, from the roll-up"
    );

    // The tree: groups add up their subscriptions.
    command(&mut app, "mg").await;
    let text = screen(&mut app);
    assert!(
        text.contains("88.0% · 30 non-compliant"),
        "root: 30 of 250\n{text}"
    );
}

#[tokio::test]
async fn exemptions_list_soonest_to_expire_first_and_exempt_rows_open_them() {
    let (mut app, _) = setup(true);
    let mut loads = app.take_load_receiver().unwrap();
    command(&mut app, "exemption all").await;
    assert_eq!(app.state().view, ViewMode::ExemptionList);
    app.apply_load(loads.recv().await.unwrap());
    app.apply_load(loads.recv().await.unwrap()); // assignments, for names
    let names: Vec<String> = app
        .state()
        .exemption_rows()
        .into_iter()
        .map(|r| r.exemption.name)
        .collect();
    assert_eq!(names, ["old", "soon", "forever"]);
    let text = screen(&mut app);
    for expected in [
        "Exemptions · whole tenant (3 exemptions, 1 expired, 1 expiring within 30 days)",
        "expired 3 days ago",
        "expires in 10 days",
        "never expires",
        "Require tags",
        "because old",
    ] {
        assert!(text.contains(expected), "{expected}\n{text}");
    }

    // At or below a scope: the subscription has its own and the vault's.
    command(&mut app, "exemption prod").await;
    let names: Vec<String> = app
        .state()
        .exemption_rows()
        .into_iter()
        .map(|r| r.exemption.name)
        .collect();
    assert_eq!(names, ["old", "soon"]);

    // Enter: the exemption's details.
    press(&mut app, KeyCode::Enter).await;
    assert_eq!(app.state().view, ViewMode::ResourceDetail);
    let detail = app.state().selected_resource_detail.clone().unwrap();
    assert!(detail.id.ends_with("/policyExemptions/old"));
    assert_eq!(detail.subscription_id, "s1");

    // A resource's Policy tab: the Exempt row names the exemption that
    // applies, and
    // Enter opens it.
    {
        let state = app.state_mut();
        state.selected_resource_detail = Some(c9s_core::models::Resource {
            id: KV.into(),
            name: "kv-app".into(),
            resource_type: "Microsoft.KeyVault/vaults".into(),
            location: String::new(),
            resource_group: "rg-app".into(),
            subscription_id: "s1".into(),
            tags: Default::default(),
        });
        let tags = assignment("tags", &mg("root"), false).id;
        state.resource_policy = Some((KV.into(), vec![state_of(&tags)]));
        state.detail_tab = DetailTab::Policy;
        state.detail_cursor = 0;
    }
    // The vault's own exemption expired, so it no longer exempts: the
    // subscription's still-active one does.
    let text = screen(&mut app);
    assert!(
        text.contains("exemption Exempt soon, expires in 10 days"),
        "{text}"
    );
    press(&mut app, KeyCode::Enter).await;
    assert!(app
        .state()
        .selected_resource_detail
        .as_ref()
        .unwrap()
        .id
        .ends_with("/policyExemptions/soon"));
}

fn state_of(assignment: &str) -> PolicyState {
    state(KV, assignment, "require-tag", "Exempt")
}
