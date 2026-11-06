//! `:group`: directory groups, their members (direct or nested), and the
//! groups a member belongs to.

use c9s_core::cache::Cache;
use c9s_core::loader::{ChunkSink, FetchOutcome, Fetcher, LoadReceiver, LoadRequest, LoadedData};
use c9s_core::rbac::{Group, Principal};
use c9s_core::state::ViewMode;
use c9s_core::App;
use c9s_tui::{ui, Event, TuiApp};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use futures::future::BoxFuture;
use futures::FutureExt;
use ratatui::{backend::TestBackend, Terminal};
use std::path::Path;
use std::sync::Arc;

fn principal(id: &str, name: &str, kind: &str, detail: &str) -> Principal {
    Principal {
        id: id.into(),
        display_name: name.into(),
        kind: kind.into(),
        detail: detail.into(),
    }
}

struct Graph;

impl Fetcher for Graph {
    fn fetch(&self, request: LoadRequest, _sink: ChunkSink) -> BoxFuture<'static, FetchOutcome> {
        let ada = principal("u1", "Ada Lovelace", "user", "ada@contoso.com");
        let bob = principal("u2", "Bob", "user", "bob@contoso.com");
        let ops = principal("g2", "Ops", "group", "");
        let data = match request {
            LoadRequest::Groups => Ok(LoadedData::Groups(vec![
                Group {
                    id: "g1".into(),
                    display_name: "Platform Admins".into(),
                    security_enabled: true,
                    role_assignable: true,
                    ..Default::default()
                },
                Group {
                    id: "g2".into(),
                    display_name: "Ops".into(),
                    security_enabled: true,
                    dynamic: true,
                    description: "everyone on call".into(),
                    ..Default::default()
                },
            ])),
            // Platform Admins: Ada and the Ops group, which has Bob.
            LoadRequest::GroupMembers {
                group_id,
                transitive,
            } => {
                let members = match (group_id.as_str(), transitive) {
                    ("g1", false) => vec![ada, ops],
                    ("g1", true) => vec![ada, ops, bob],
                    ("g2", _) => vec![bob],
                    _ => vec![],
                };
                Ok(LoadedData::GroupMembers(group_id, transitive, members))
            }
            LoadRequest::PrincipalGroups { principal_id } => Ok(LoadedData::PrincipalGroups(
                principal_id.clone(),
                if principal_id == "u2" {
                    vec![ops, principal("g1", "Platform Admins", "group", "")]
                } else {
                    vec![]
                },
            )),
            LoadRequest::RoleAssignments { .. } => Ok(LoadedData::RoleAssignments(vec![])),
            LoadRequest::RoleDefinitions { .. } => Ok(LoadedData::RoleDefinitions(vec![])),
            LoadRequest::Principals { .. } => Ok(LoadedData::Principals(vec![])),
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

fn setup() -> TuiApp {
    TuiApp::new(App::with_fetcher(
        Cache::new(Path::new(":memory:")).unwrap(),
        Some(Arc::new(Graph)),
    ))
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

async fn settle(app: &mut TuiApp, loads: &mut LoadReceiver) {
    while app.state().loading {
        app.apply_load(loads.recv().await.unwrap());
    }
}

fn screen(app: &mut TuiApp) -> String {
    let mut terminal = Terminal::new(TestBackend::new(160, 20)).unwrap();
    terminal.draw(|f| ui::draw(f, app).unwrap()).unwrap();
    let buf = terminal.backend().buffer();
    (0..buf.area.height)
        .map(|y| {
            (0..buf.area.width)
                .map(|x| buf[(x, y)].symbol().to_string())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn names(rows: Option<Vec<Principal>>) -> Vec<String> {
    rows.unwrap_or_default()
        .into_iter()
        .map(|p| p.display_name)
        .collect()
}

#[tokio::test]
async fn groups_members_nested_members_and_member_of() {
    let mut app = setup();
    let mut loads = app.take_load_receiver().unwrap();

    command(&mut app, "group").await;
    assert_eq!(app.state().view, ViewMode::GroupList);
    settle(&mut app, &mut loads).await;
    let text = screen(&mut app);
    for expected in [
        "Groups (2, 1 dynamic, 1 role-assignable)",
        "R Platform Admins",
        "security",
        "dynamic",
        "everyone on call",
    ] {
        assert!(text.contains(expected), "{expected}\n{text}");
    }
    app.state_mut().filter = "role-assignable".into();
    assert_eq!(app.state().group_rows().len(), 1);
    app.state_mut().filter.clear();

    // `:group ops` narrows by name.
    command(&mut app, "group ops").await;
    assert_eq!(app.state().group_rows().len(), 1);
    command(&mut app, "group").await;

    // Enter on Platform Admins: its direct members, groups first.
    press(&mut app, KeyCode::Down).await;
    press(&mut app, KeyCode::Enter).await;
    assert_eq!(app.state().view, ViewMode::GroupMembers);
    settle(&mut app, &mut loads).await;
    assert_eq!(names(app.state().member_rows()), ["Ops", "Ada Lovelace"]);
    assert!(screen(&mut app).contains("Members · Platform Admins · direct (1 group, 1 user)"));

    // t: nested members too.
    press(&mut app, KeyCode::Char('t')).await;
    settle(&mut app, &mut loads).await;
    assert_eq!(
        names(app.state().member_rows()),
        ["Ops", "Ada Lovelace", "Bob"]
    );
    assert!(
        screen(&mut app).contains("Members · Platform Admins · nested included (1 group, 2 users)")
    );

    // Enter on Bob: the groups he belongs to.
    press(&mut app, KeyCode::Char('G')).await;
    press(&mut app, KeyCode::Enter).await;
    assert_eq!(app.state().view, ViewMode::MemberOf);
    settle(&mut app, &mut loads).await;
    assert_eq!(
        names(app.state().member_of_rows()),
        ["Ops", "Platform Admins"]
    );
    assert!(
        screen(&mut app).contains("Member of · Bob (bob@contoso.com) · nested included (2 groups)")
    );

    // Enter on Ops: its members. Esc, Esc: back to Platform Admins' nested
    // members, on Bob.
    press(&mut app, KeyCode::Enter).await;
    settle(&mut app, &mut loads).await;
    assert_eq!(names(app.state().member_rows()), ["Bob"]);
    press(&mut app, KeyCode::Esc).await;
    assert_eq!(app.state().view, ViewMode::MemberOf);
    press(&mut app, KeyCode::Esc).await;
    assert_eq!(app.state().view, ViewMode::GroupMembers);
    assert_eq!(app.state().members_group.as_deref(), Some("g1"));
    assert!(app.state().members_transitive);
    assert_eq!(app.state().selected_member().unwrap().display_name, "Bob");

    // p: Bob's Azure access.
    press(&mut app, KeyCode::Char('p')).await;
    assert_eq!(app.state().view, ViewMode::RoleAssignments);
    assert_eq!(app.state().ra_principal.as_deref(), Some("u2"));
}
