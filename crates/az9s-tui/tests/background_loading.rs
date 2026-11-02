//! The UI must never wait for Azure. These tests drive `TuiApp` with key
//! events against a deliberately slow fake fetcher.

use az9s_core::cache::Cache;
use az9s_core::loader::{ChunkSink, FetchOutcome, Fetcher, LoadRequest, LoadedData};
use az9s_core::models::{Resource, ResourceGroup, Subscription};
use az9s_core::state::ViewMode;
use az9s_core::App;
use az9s_tui::{Event, TuiApp};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use futures::future::BoxFuture;
use futures::FutureExt;
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const AZURE_LATENCY: Duration = Duration::from_millis(300);

/// Answers every request after `AZURE_LATENCY` and records what was asked.
#[derive(Default)]
struct SlowAzure {
    requests: Mutex<Vec<LoadRequest>>,
}

impl Fetcher for SlowAzure {
    fn fetch(&self, request: LoadRequest, _sink: ChunkSink) -> BoxFuture<'static, FetchOutcome> {
        self.requests.lock().unwrap().push(request.clone());
        async move {
            tokio::time::sleep(AZURE_LATENCY).await;
            let data = match request {
                LoadRequest::Subscriptions => LoadedData::Subscriptions(vec![sub("prod")]),
                LoadRequest::ResourceGroups { subscription_ids } => LoadedData::ResourceGroups(
                    subscription_ids
                        .iter()
                        .map(|s| ResourceGroup {
                            id: format!("{s}/rg-app"),
                            name: "rg-app".into(),
                            location: "westeurope".into(),
                            subscription_id: s.clone(),
                            tags: HashMap::new(),
                        })
                        .collect(),
                ),
                LoadRequest::Resources {
                    subscription_ids,
                    resource_group,
                } => LoadedData::Resources(
                    subscription_ids
                        .iter()
                        .map(|s| Resource {
                            id: format!("{s}/vm-1"),
                            name: "vm-1".into(),
                            resource_type: "Microsoft.Compute/virtualMachines".into(),
                            location: "westeurope".into(),
                            resource_group: resource_group.clone().unwrap_or_default(),
                            subscription_id: s.clone(),
                            tags: HashMap::new(),
                        })
                        .collect(),
                ),
            };
            FetchOutcome {
                data: Ok(data),
                failed_subscriptions: vec![],
                notice: None,
            }
        }
        .boxed()
    }
}

fn sub(name: &str) -> Subscription {
    Subscription {
        id: format!("id-{name}"),
        name: name.into(),
        state: "Enabled".into(),
        tenant_id: "t".into(),
    }
}

fn setup() -> (TuiApp, Arc<SlowAzure>) {
    let azure = Arc::new(SlowAzure::default());
    let cache = Cache::new(Path::new(":memory:")).unwrap();
    let mut core = App::with_fetcher(cache, Some(azure.clone()));
    core.state_mut().subscriptions = vec![sub("prod"), sub("dev")];
    (TuiApp::new(core), azure)
}

/// What the fake was last asked for, once spawned load tasks have started.
async fn last_request(azure: &SlowAzure) -> LoadRequest {
    tokio::time::sleep(Duration::from_millis(20)).await;
    azure.requests.lock().unwrap().last().cloned().unwrap()
}

fn key(code: KeyCode) -> Event {
    Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
}

async fn press(app: &mut TuiApp, code: KeyCode) -> Duration {
    let started = Instant::now();
    app.handle_event(key(code)).await.unwrap();
    started.elapsed()
}

async fn type_command(app: &mut TuiApp, command: &str) {
    press(app, KeyCode::Char(':')).await;
    for c in command.chars() {
        press(app, KeyCode::Char(c)).await;
    }
    press(app, KeyCode::Enter).await;
}

#[tokio::test]
async fn opening_a_subscription_switches_view_immediately_and_loads_in_background() {
    let (mut app, _azure) = setup();
    let mut loads = app.take_load_receiver().unwrap();

    let took = press(&mut app, KeyCode::Enter).await;
    assert!(took < AZURE_LATENCY / 3, "Enter blocked for {took:?}");
    assert_eq!(app.state().view, ViewMode::ResourceGroupList);
    assert!(app.state().loading);

    // The user can keep navigating while the load runs.
    let took = press(&mut app, KeyCode::Char('j')).await;
    assert!(took < AZURE_LATENCY / 3);

    app.apply_load(loads.recv().await.unwrap());
    assert!(!app.state().loading);
    assert_eq!(app.state().resource_groups.len(), 1);
}

#[tokio::test]
async fn opening_a_resource_group_requests_only_that_group() {
    let (mut app, azure) = setup();
    let mut loads = app.take_load_receiver().unwrap();

    press(&mut app, KeyCode::Enter).await; // subscription -> RG list
    app.apply_load(loads.recv().await.unwrap());
    press(&mut app, KeyCode::Enter).await; // RG -> resources
    assert_eq!(app.state().view, ViewMode::ResourceList);

    let last = last_request(&azure).await;
    assert_eq!(
        last,
        LoadRequest::Resources {
            subscription_ids: vec!["id-prod".into()],
            resource_group: Some("rg-app".into()),
        }
    );
    app.apply_load(loads.recv().await.unwrap());
    assert!(app
        .state()
        .resources
        .iter()
        .all(|r| r.resource_group == "rg-app"));
}

#[tokio::test]
async fn going_back_while_loading_then_opening_another_subscription_discards_stale_result() {
    let (mut app, _azure) = setup();
    let mut loads = app.take_load_receiver().unwrap();

    press(&mut app, KeyCode::Enter).await; // open "prod"
    press(&mut app, KeyCode::Esc).await; // back before it loads
    press(&mut app, KeyCode::Char('j')).await; // select "dev"
    press(&mut app, KeyCode::Enter).await; // open "dev"

    // Only the latest request's result is applied.
    app.apply_load(loads.recv().await.unwrap());
    let rgs = &app.state().resource_groups;
    assert_eq!(rgs.len(), 1);
    assert_eq!(rgs[0].subscription_id, "id-dev");
}

#[tokio::test]
async fn all_subscriptions_view_skips_disabled_and_survives_back_navigation() {
    let (mut app, azure) = setup();
    app.state_mut().subscriptions.push(Subscription {
        state: "Disabled".into(),
        ..sub("old")
    });
    let mut loads = app.take_load_receiver().unwrap();

    type_command(&mut app, "rg all").await;
    assert!(app.state().cross_subscription_mode);
    let last = last_request(&azure).await;
    assert_eq!(
        last,
        LoadRequest::ResourceGroups {
            subscription_ids: vec!["id-prod".into(), "id-dev".into()],
        }
    );
    app.apply_load(loads.recv().await.unwrap());

    press(&mut app, KeyCode::Enter).await; // into a group
    press(&mut app, KeyCode::Esc).await; // back to the all-subscriptions list
    assert_eq!(app.state().view, ViewMode::ResourceGroupList);
    assert!(app.state().cross_subscription_mode, "scope lost on back");
}
