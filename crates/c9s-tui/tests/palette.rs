//! The command palette (Ctrl+P) and `:` autocomplete, driven by keys.

use c9s_core::cache::Cache;
use c9s_core::loader::{ChunkSink, FetchOutcome, Fetcher, LoadRequest};
use c9s_core::models::{Resource, Subscription};
use c9s_core::state::ViewMode;
use c9s_core::App;
use c9s_tui::{ui, Event, TuiApp};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use futures::future::BoxFuture;
use futures::FutureExt;
use ratatui::{backend::TestBackend, Terminal};
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

/// Records requests; answers resources with nothing (lists are preloaded).
#[derive(Default)]
struct Azure(Mutex<Vec<LoadRequest>>);

impl Fetcher for Azure {
    fn fetch(&self, request: LoadRequest, _sink: ChunkSink) -> BoxFuture<'static, FetchOutcome> {
        self.0.lock().unwrap().push(request);
        async {
            FetchOutcome {
                data: Err("offline in tests".into()),
                failed_subscriptions: vec![],
                notice: None,
            }
        }
        .boxed()
    }
}

fn res(name: &str, t: &str) -> Resource {
    Resource {
        id: format!("/subscriptions/s1/resourceGroups/rg-hub/providers/{t}/{name}"),
        name: name.into(),
        resource_type: t.into(),
        location: "westeurope".into(),
        resource_group: "rg-hub".into(),
        subscription_id: "s1".into(),
        tags: HashMap::from([("env".to_string(), "prod".to_string())]),
    }
}

fn setup() -> (TuiApp, Arc<Azure>) {
    let azure = Arc::new(Azure::default());
    let mut core = App::with_fetcher(
        Cache::new(Path::new(":memory:")).unwrap(),
        Some(azure.clone()),
    );
    let state = core.state_mut();
    state.subscriptions = vec![Subscription {
        id: "s1".into(),
        name: "prod-connectivity".into(),
        state: "Enabled".into(),
        tenant_id: "t".into(),
    }];
    state.resources = vec![
        res("kv-prod-weu", "Microsoft.KeyVault/vaults"),
        res("vm-web-1", "Microsoft.Compute/virtualMachines"),
    ];
    state.view = ViewMode::ResourceList;
    (TuiApp::new(core), azure)
}

async fn key(app: &mut TuiApp, code: KeyCode, modifiers: KeyModifiers) {
    app.handle_event(Event::Key(KeyEvent::new(code, modifiers)))
        .await
        .unwrap();
}

async fn press(app: &mut TuiApp, code: KeyCode) {
    key(app, code, KeyModifiers::NONE).await;
}

async fn type_text(app: &mut TuiApp, text: &str) {
    for c in text.chars() {
        press(app, KeyCode::Char(c)).await;
    }
}

fn screen(app: &mut TuiApp) -> String {
    let mut terminal = Terminal::new(TestBackend::new(110, 30)).unwrap();
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
async fn ctrl_p_finds_a_resource_and_enter_opens_it() {
    let (mut app, _) = setup();
    key(&mut app, KeyCode::Char('p'), KeyModifiers::CONTROL).await;
    assert!(app.state().palette.is_some());
    type_text(&mut app, "kvprod").await;

    let text = screen(&mut app);
    assert!(text.contains("> kvprod_"), "{text}");
    assert!(
        text.contains("kv-prod-weu") && text.contains("key-vault · rg-hub"),
        "{text}"
    );
    assert!(text.contains("1 match"), "{text}");

    press(&mut app, KeyCode::Enter).await;
    assert!(app.state().palette.is_none());
    assert_eq!(app.state().view, ViewMode::ResourceDetail);
    assert_eq!(
        app.state().selected_resource_detail.as_ref().unwrap().name,
        "kv-prod-weu"
    );

    // Esc returns to where the palette was opened.
    press(&mut app, KeyCode::Esc).await;
    assert_eq!(app.state().view, ViewMode::ResourceList);
}

#[tokio::test]
async fn palette_runs_commands_and_opens_subscriptions() {
    let (mut app, azure) = setup();
    key(&mut app, KeyCode::Char('p'), KeyModifiers::CONTROL).await;
    type_text(&mut app, ":rg all").await;
    press(&mut app, KeyCode::Enter).await;
    assert_eq!(app.state().view, ViewMode::ResourceGroupList);
    assert!(app.state().cross_subscription_mode);

    key(&mut app, KeyCode::Char('p'), KeyModifiers::CONTROL).await;
    type_text(&mut app, "prodconn").await;
    press(&mut app, KeyCode::Enter).await;
    assert_eq!(app.state().view, ViewMode::ResourceGroupList);
    assert!(!app.state().cross_subscription_mode);
    // The request is recorded by the spawned load task.
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    assert_eq!(
        azure.0.lock().unwrap().last().unwrap(),
        &LoadRequest::ResourceGroups {
            subscription_ids: vec!["s1".into()]
        }
    );
}

#[tokio::test]
async fn esc_closes_the_palette_without_doing_anything() {
    let (mut app, azure) = setup();
    key(&mut app, KeyCode::Char('p'), KeyModifiers::CONTROL).await;
    type_text(&mut app, "zzz").await;
    assert!(screen(&mut app).contains("Nothing matches"));
    press(&mut app, KeyCode::Esc).await;
    assert!(app.state().palette.is_none());
    assert_eq!(app.state().view, ViewMode::ResourceList);
    assert!(azure.0.lock().unwrap().is_empty());
}

#[tokio::test]
async fn tab_completes_commands_scopes_and_filter_values() {
    let (mut app, _) = setup();
    press(&mut app, KeyCode::Char(':')).await;
    type_text(&mut app, "re").await;
    let text = screen(&mut app);
    assert!(text.contains("Tab completes"), "{text}");
    press(&mut app, KeyCode::Tab).await;
    assert_eq!(app.state().command_input, "res ");

    type_text(&mut app, "ty").await;
    press(&mut app, KeyCode::Tab).await;
    assert_eq!(app.state().command_input, "res type=");

    // Values from what's loaded (one of each: alphabetical by type, so vm
    // first). Down twice wraps back to the first.
    let text = screen(&mut app);
    assert!(
        text.contains("type=key-vault") && text.contains("type=vm"),
        "{text}"
    );
    press(&mut app, KeyCode::Down).await;
    assert_eq!(app.state().command_selection, 1);
    press(&mut app, KeyCode::Down).await;
    assert_eq!(app.state().command_selection, 0, "wraps around");
    press(&mut app, KeyCode::Tab).await;
    assert_eq!(app.state().command_input, "res type=vm ");

    type_text(&mut app, "tag=").await;
    press(&mut app, KeyCode::Tab).await;
    assert_eq!(app.state().command_input, "res type=vm tag=env=");
    press(&mut app, KeyCode::Tab).await;
    assert_eq!(app.state().command_input, "res type=vm tag=env=prod ");

    press(&mut app, KeyCode::Enter).await;
    assert_eq!(app.state().view, ViewMode::ResourceList);
    assert_eq!(
        app.state().resource_query.to_string(),
        "type=vm tag=env:prod"
    );
}
