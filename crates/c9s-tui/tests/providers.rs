//! The context's cloud: its words on screen, and Azure-only features
//! refused elsewhere with the reason.

use c9s_core::cache::Cache;
use c9s_core::models::Subscription;
use c9s_core::provider::Provider;
use c9s_core::state::ViewMode;
use c9s_core::App;
use c9s_tui::{ui, Event, TuiApp};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{backend::TestBackend, Terminal};
use std::path::Path;

fn app(provider: Provider) -> TuiApp {
    let mut core = App::offline(Cache::new(Path::new(":memory:")).unwrap());
    let state = core.state_mut();
    state.provider = provider;
    state.subscriptions = vec![Subscription {
        id: "p1".into(),
        name: "billing-prod".into(),
        state: "Enabled".into(),
        tenant_id: "t".into(),
    }];
    state.view = ViewMode::SubscriptionList;
    TuiApp::new(core)
}

async fn command(app: &mut TuiApp, text: &str) {
    let key = |c| Event::Key(KeyEvent::new(c, KeyModifiers::NONE));
    app.handle_event(key(KeyCode::Char(':'))).await.unwrap();
    for c in text.chars() {
        app.handle_event(key(KeyCode::Char(c))).await.unwrap();
    }
    app.handle_event(key(KeyCode::Enter)).await.unwrap();
}

fn screen(app: &mut TuiApp) -> String {
    let mut terminal = Terminal::new(TestBackend::new(120, 12)).unwrap();
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

#[tokio::test]
async fn azure_is_the_context_and_speaks_azure() {
    let mut app = app(Provider::Azure);
    let text = screen(&mut app);
    assert!(
        text.contains("azure |"),
        "the status bar starts with the cloud\n{text}"
    );
    assert!(text.contains("Subscriptions (1)"), "{text}");
}

#[tokio::test]
async fn another_cloud_uses_its_words_and_refuses_azure_only_features() {
    let mut app = app(Provider::Gcp);
    let text = screen(&mut app);
    assert!(
        text.contains("gcp |") && text.contains("Projects (1)"),
        "{text}"
    );
    assert!(!text.contains("Subscriptions"), "{text}");

    for cmd in [
        "policy",
        "compliance",
        "exemption all",
        "ra",
        "role",
        "access ada",
        "group",
        "app",
    ] {
        app.state_mut().error = None;
        command(&mut app, cmd).await;
        let error = app.state().error.clone().unwrap_or_default();
        assert!(
            error.contains("Azure only") && error.contains("gcp"),
            ":{cmd} → {error:?}"
        );
        assert_eq!(
            app.state().view,
            ViewMode::SubscriptionList,
            ":{cmd} stays put"
        );
    }

    // Browsing commands still work.
    app.state_mut().error = None;
    command(&mut app, "sub").await;
    assert!(app.state().error.is_none(), "{:?}", app.state().error);
}

#[tokio::test]
async fn gcp_folders_are_the_tree_and_open_onto_resources() {
    use c9s_core::scope_tree::{GroupNode, ScopeTree, SubscriptionNode};
    let mut app = app(Provider::Gcp);
    command(&mut app, "mg").await;
    let error = app.state().error.clone().unwrap_or_default();
    assert!(!error.contains("Azure only"), ":mg is allowed: {error}");

    let state = app.state_mut();
    state.set_scope_tree(ScopeTree {
        root: "organizations/1".into(),
        root_display_name: "acme.com".into(),
        groups: vec![GroupNode {
            name: "folders/2".into(),
            display_name: "Prod".into(),
            parent: "organizations/1".into(),
        }],
        subscriptions: vec![SubscriptionNode {
            id: "p1".into(),
            display_name: "billing-prod".into(),
            parent: "folders/2".into(),
        }],
    });
    state.error = None;
    state.view = ViewMode::ManagementGroupTree;
    state.list_state.reset();
    let text = screen(&mut app);
    assert!(
        text.contains("Folders (2) - Enter opens resources") && text.contains("1 projects"),
        "{text}"
    );

    // The folder row: Enter lists the resources of every project below it.
    let rows = app.state().tree_rows();
    let folder = rows.iter().position(|r| r.key == "folders/2").unwrap();
    app.state_mut().list_state.selected = folder;
    assert_eq!(app.state().selected_arm_id().as_deref(), Some("folders/2"));
    app.handle_event(Event::Key(KeyEvent::new(
        KeyCode::Enter,
        KeyModifiers::NONE,
    )))
    .await
    .unwrap();
    assert_eq!(app.state().view, ViewMode::ResourceList);
    assert_eq!(app.state().scope_group.as_deref(), Some("folders/2"));
}

#[tokio::test]
async fn aws_speaks_accounts_and_names_its_profile() {
    let mut app = app(Provider::Aws);
    app.state_mut().aws_profile = Some("prod-admin".into());
    let text = screen(&mut app);
    assert!(
        text.contains("aws · prod-admin") && text.contains("Accounts (1)"),
        "{text}"
    );
    command(&mut app, "mg").await;
    let error = app.state().error.clone().unwrap_or_default();
    assert!(
        !error.contains("available"),
        ":mg is allowed (organizational units): {error}"
    );
}
