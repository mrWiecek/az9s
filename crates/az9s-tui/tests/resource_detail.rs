//! The resource detail view: opens at once from the list row, loads the full
//! document in the background, tabs and scrolling, copy and Portal links.

use az9s_core::cache::Cache;
use az9s_core::loader::{ChunkSink, FetchOutcome, Fetcher, LoadRequest, LoadedData};
use az9s_core::models::{Resource, Subscription};
use az9s_core::state::{DetailTab, ViewMode};
use az9s_core::App;
use az9s_tui::{ui, Desktop, Event, TuiApp};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use futures::future::BoxFuture;
use futures::FutureExt;
use ratatui::{backend::TestBackend, Terminal};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

const VM_ID: &str =
    "/subscriptions/sub-1/resourceGroups/rg-app/providers/Microsoft.Compute/virtualMachines/vm-1";

fn vm() -> Resource {
    Resource {
        id: VM_ID.into(),
        name: "vm-1".into(),
        resource_type: "microsoft.compute/virtualmachines".into(),
        location: "westeurope".into(),
        resource_group: "rg-app".into(),
        subscription_id: "sub-1".into(),
        tags: HashMap::from([("env".to_string(), "dev".to_string())]),
    }
}

fn vm_document() -> Value {
    let disks: Vec<Value> = (0..40)
        .map(|i| json!({"lun": i, "name": format!("data-{i}")}))
        .collect();
    json!({
        "id": VM_ID, "name": "vm-1", "type": "microsoft.compute/virtualmachines",
        "location": "westeurope", "zones": ["2"],
        "tags": {"env": "prod", "owner": "platform"},
        "properties": {
            "provisioningState": "Succeeded",
            "hardwareProfile": {"vmSize": "Standard_D4s_v5"},
            "storageProfile": {"dataDisks": disks}
        }
    })
}

/// Answers detail requests with `document` (or an error); records requests.
struct FakeAzure {
    document: Result<Value, String>,
    requests: Mutex<Vec<LoadRequest>>,
}

impl Fetcher for FakeAzure {
    fn fetch(&self, request: LoadRequest, _sink: ChunkSink) -> BoxFuture<'static, FetchOutcome> {
        self.requests.lock().unwrap().push(request.clone());
        let data = match request {
            LoadRequest::ResourceDetail { .. } => {
                self.document.clone().map(LoadedData::ResourceDetail)
            }
            _ => Err("unexpected request".into()),
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

/// Records what would have been copied or opened.
#[derive(Clone, Default)]
struct Recorder(Arc<Mutex<Vec<String>>>);

impl Desktop for Recorder {
    fn copy(&mut self, text: &str) -> Result<(), String> {
        self.0.lock().unwrap().push(format!("copy {text}"));
        Ok(())
    }
    fn open(&mut self, url: &str) -> Result<(), String> {
        self.0.lock().unwrap().push(format!("open {url}"));
        Err("no display".into())
    }
}

fn setup(document: Result<Value, String>) -> (TuiApp, Arc<FakeAzure>, Recorder) {
    let azure = Arc::new(FakeAzure {
        document,
        requests: Mutex::new(vec![]),
    });
    let mut core = App::with_fetcher(
        Cache::new(Path::new(":memory:")).unwrap(),
        Some(azure.clone()),
    );
    let state = core.state_mut();
    state.subscriptions = vec![Subscription {
        id: "sub-1".into(),
        name: "prod".into(),
        state: "Enabled".into(),
        tenant_id: "tenant-1".into(),
    }];
    state.selected_subscription = state.subscriptions.first().cloned();
    state.resources = vec![vm()];
    state.view = ViewMode::ResourceList;
    let recorder = Recorder::default();
    (
        TuiApp::with_desktop(core, Box::new(recorder.clone())),
        azure,
        recorder,
    )
}

async fn press(app: &mut TuiApp, code: KeyCode) {
    app.handle_event(Event::Key(KeyEvent::new(code, KeyModifiers::NONE)))
        .await
        .unwrap();
}

fn screen(terminal: &mut Terminal<TestBackend>, app: &mut TuiApp) -> String {
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
async fn detail_opens_from_the_list_row_then_shows_the_full_document() {
    let (mut app, azure, _) = setup(Ok(vm_document()));
    let mut loads = app.take_load_receiver().unwrap();
    let mut terminal = Terminal::new(TestBackend::new(110, 40)).unwrap();

    press(&mut app, KeyCode::Enter).await;
    assert_eq!(app.state().view, ViewMode::ResourceDetail);
    // Before Azure answers: what the list knew.
    let text = screen(&mut terminal, &mut app);
    assert!(text.contains("vm-1") && text.contains("rg-app"), "{text}");
    assert!(text.contains("loading details"), "{text}");

    app.apply_load(loads.recv().await.unwrap());
    assert_eq!(
        azure.requests.lock().unwrap().last().unwrap(),
        &LoadRequest::ResourceDetail {
            resource_id: VM_ID.into(),
            subscription_id: "sub-1".into(),
        }
    );
    let text = screen(&mut terminal, &mut app);
    if std::env::var("AZ9S_SHOW_SCREEN").is_ok() {
        println!("{text}");
    }
    assert!(text.contains("Standard_D4s_v5"), "{text}");
    assert!(text.contains("Succeeded"), "{text}");
    assert!(!text.contains("loading details"));

    // Tags tab, from the document.
    press(&mut app, KeyCode::Char('2')).await;
    let text = screen(&mut terminal, &mut app);
    assert!(
        text.contains("owner") && text.contains("platform"),
        "{text}"
    );

    // JSON tab scrolls; G reaches the end, g the top.
    press(&mut app, KeyCode::Tab).await;
    assert_eq!(app.state().detail_tab, DetailTab::Json);
    let top = screen(&mut terminal, &mut app);
    assert!(top.contains("\"hardwareProfile\""), "{top}");
    press(&mut app, KeyCode::Char('G')).await;
    let bottom = screen(&mut terminal, &mut app);
    assert!(bottom.contains("data-39"), "{bottom}");
    assert!(!bottom.contains("\"hardwareProfile\""));
    press(&mut app, KeyCode::Char('g')).await;
    assert_eq!(app.state().detail_scroll, 0);

    // Esc returns to the list with the resource still selected.
    press(&mut app, KeyCode::Esc).await;
    assert_eq!(app.state().view, ViewMode::ResourceList);
    assert_eq!(app.state().selected_id().as_deref(), Some(VM_ID));
}

#[tokio::test]
async fn reopening_shows_the_cached_document_before_azure_answers() {
    let (mut app, _, _) = setup(Ok(vm_document()));
    // The resource must be cached for its details to be.
    let repo = app.core().cache().repository();
    repo.replace_subscriptions(&app.state().subscriptions.clone())
        .unwrap();
    repo.replace_resources(&["sub-1".into()], None, &[vm()])
        .unwrap();
    let mut loads = app.take_load_receiver().unwrap();

    press(&mut app, KeyCode::Enter).await;
    app.apply_load(loads.recv().await.unwrap());
    press(&mut app, KeyCode::Esc).await;

    press(&mut app, KeyCode::Enter).await;
    assert!(app.state().loading);
    assert_eq!(
        app.state().resource_document.as_ref().unwrap()["zones"][0],
        "2"
    );
    let fresh = app.state().current_freshness().unwrap();
    assert_eq!(fresh.source, az9s_core::state::DataSource::Cache);
}

#[tokio::test]
async fn a_failed_load_keeps_the_basic_view_with_a_notice() {
    let (mut app, _, _) = setup(Err("AuthorizationFailed".into()));
    let mut loads = app.take_load_receiver().unwrap();
    press(&mut app, KeyCode::Enter).await;
    app.apply_load(loads.recv().await.unwrap());

    assert_eq!(app.state().view, ViewMode::ResourceDetail);
    assert!(app.state().error.is_none(), "no popup");
    let notice = app.state().notice.clone().unwrap();
    assert!(notice.contains("Details unavailable") && notice.contains("AuthorizationFailed"));
}

#[tokio::test]
async fn y_copies_and_o_opens_the_portal_with_the_tenant() {
    let (mut app, _, recorder) = setup(Ok(vm_document()));
    press(&mut app, KeyCode::Char('y')).await; // from the list
    press(&mut app, KeyCode::Enter).await;
    press(&mut app, KeyCode::Char('o')).await; // from the detail view

    let calls = recorder.0.lock().unwrap().clone();
    assert_eq!(
        calls,
        [
            format!("copy {VM_ID}"),
            format!("open https://portal.azure.com/#@tenant-1/resource{VM_ID}"),
            // No browser (e.g. WSL without wslview): the link is copied instead,
            // because it is too long for the status bar.
            format!("copy https://portal.azure.com/#@tenant-1/resource{VM_ID}"),
        ]
    );
    let notice = app.state().notice.clone().unwrap();
    assert!(
        notice.contains("No browser could be started") && notice.contains("Portal link copied"),
        "{notice}"
    );
}

#[tokio::test]
async fn c_copies_the_current_view() {
    let (mut app, _, recorder) = setup(Ok(vm_document()));
    let mut loads = app.take_load_receiver().unwrap();

    // In the resource list: every row, tab-separated with a header.
    press(&mut app, KeyCode::Char('c')).await;
    // In the detail view, JSON tab: the whole document.
    press(&mut app, KeyCode::Enter).await;
    app.apply_load(loads.recv().await.unwrap());
    press(&mut app, KeyCode::Char('3')).await;
    press(&mut app, KeyCode::Char('c')).await;

    let calls = recorder.0.lock().unwrap().clone();
    assert!(
        calls[0].starts_with("copy name\ttype\tlocation\tresource group\tsubscription\tid\nvm-1\t"),
        "{}",
        calls[0]
    );
    let json = calls[1].strip_prefix("copy ").unwrap();
    let parsed: Value = serde_json::from_str(json).expect("valid JSON is copied");
    assert_eq!(parsed, vm_document());
    assert!(app
        .state()
        .notice
        .as_deref()
        .unwrap()
        .starts_with("Copied JSON of vm-1"));
}
