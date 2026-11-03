//! `:res` filters and the `:type` summary, driven by keys.

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
use std::sync::Arc;

fn res(name: &str, resource_type: &str, location: &str, env: &str) -> Resource {
    Resource {
        id: format!("/subscriptions/s1/resourceGroups/rg/providers/{resource_type}/{name}"),
        name: name.into(),
        resource_type: resource_type.into(),
        location: location.into(),
        resource_group: "rg".into(),
        subscription_id: "s1".into(),
        tags: HashMap::from([("env".to_string(), env.to_string())]),
    }
}

/// Every request returns the same rows: five resources, three groups.
struct Azure;

impl Fetcher for Azure {
    fn fetch(&self, request: LoadRequest, _sink: ChunkSink) -> BoxFuture<'static, FetchOutcome> {
        let data = match request {
            LoadRequest::Resources { .. } => Ok(LoadedData::Resources(vec![
                res(
                    "vm-1",
                    "Microsoft.Compute/virtualMachines",
                    "westeurope",
                    "prod",
                ),
                res(
                    "vm-2",
                    "microsoft.compute/virtualmachines",
                    "northeurope",
                    "dev",
                ),
                res(
                    "vm-3",
                    "Microsoft.Compute/virtualMachines",
                    "westeurope",
                    "dev",
                ),
                res("disk-1", "Microsoft.Compute/disks", "westeurope", "prod"),
                res("kv-1", "Microsoft.KeyVault/vaults", "westeurope", "prod"),
            ])),
            LoadRequest::ResourceGroups { .. } => Ok(LoadedData::ResourceGroups(
                [
                    ("rg-weu", "westeurope", "prod"),
                    ("rg-neu", "northeurope", "prod"),
                    ("rg-dev", "westeurope", "dev"),
                ]
                .iter()
                .map(|(name, loc, env)| ResourceGroup {
                    id: format!("/subscriptions/s1/resourceGroups/{name}"),
                    name: name.to_string(),
                    location: loc.to_string(),
                    subscription_id: "s1".into(),
                    tags: HashMap::from([("env".to_string(), env.to_string())]),
                })
                .collect(),
            )),
            _ => Err("unexpected".into()),
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
    let mut core = App::with_fetcher(
        Cache::new(Path::new(":memory:")).unwrap(),
        Some(Arc::new(Azure)),
    );
    core.state_mut().subscriptions = vec![Subscription {
        id: "s1".into(),
        name: "prod".into(),
        state: "Enabled".into(),
        tenant_id: "t".into(),
    }];
    TuiApp::new(core)
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

fn visible_names(app: &TuiApp) -> Vec<String> {
    app.state()
        .filtered_resources()
        .iter()
        .map(|r| r.name.clone())
        .collect()
}

#[tokio::test]
async fn res_filters_narrow_the_list_and_survive_a_refresh() {
    let mut app = setup();
    let mut loads = app.take_load_receiver().unwrap();

    command(&mut app, "res all type=virtualmachines loc=westeurope").await;
    app.apply_load(loads.recv().await.unwrap());
    assert_eq!(visible_names(&app), ["vm-1", "vm-3"]);

    press(&mut app, KeyCode::Char('r')).await;
    app.apply_load(loads.recv().await.unwrap());
    assert_eq!(
        visible_names(&app),
        ["vm-1", "vm-3"],
        "refresh keeps filters"
    );

    command(&mut app, "res tag=env:prod").await;
    app.apply_load(loads.recv().await.unwrap());
    assert_eq!(visible_names(&app), ["vm-1", "disk-1", "kv-1"]);

    command(&mut app, "res").await;
    app.apply_load(loads.recv().await.unwrap());
    assert_eq!(visible_names(&app).len(), 5, "a new :res clears filters");
}

#[tokio::test]
async fn a_bad_filter_is_an_error_and_changes_nothing() {
    let mut app = setup();
    command(&mut app, "res all size=big").await;
    assert!(app
        .state()
        .error
        .as_deref()
        .unwrap()
        .contains("unknown filter"));
    assert_eq!(app.state().view, ViewMode::SubscriptionList);
}

#[tokio::test]
async fn type_summary_counts_types_and_opens_a_filtered_list() {
    let mut app = setup();
    let mut loads = app.take_load_receiver().unwrap();

    command(&mut app, "type all loc=westeurope").await;
    assert_eq!(app.state().view, ViewMode::ResourceTypeSummary);
    app.apply_load(loads.recv().await.unwrap());
    assert_eq!(
        app.state().type_summary(),
        [
            ("microsoft.compute/virtualmachines".to_string(), 2),
            ("microsoft.compute/disks".to_string(), 1),
            ("microsoft.keyvault/vaults".to_string(), 1),
        ]
    );

    // Enter on virtual machines: the list, both filters applied, no reload.
    press(&mut app, KeyCode::Enter).await;
    assert_eq!(app.state().view, ViewMode::ResourceList);
    assert!(!app.state().loading, "rows were already loaded");
    assert_eq!(visible_names(&app), ["vm-1", "vm-3"]);

    // Esc: back to the summary with its own filter (loc only).
    press(&mut app, KeyCode::Esc).await;
    assert_eq!(app.state().view, ViewMode::ResourceTypeSummary);
    assert_eq!(app.state().resource_query.to_string(), "loc=westeurope");
    assert_eq!(app.state().type_summary().len(), 3);

    // Into a resource and back keeps the type filter.
    press(&mut app, KeyCode::Char('j')).await; // disks
    press(&mut app, KeyCode::Enter).await;
    press(&mut app, KeyCode::Enter).await; // detail
    assert_eq!(app.state().view, ViewMode::ResourceDetail);
    press(&mut app, KeyCode::Esc).await;
    assert_eq!(visible_names(&app), ["disk-1"]);
}

#[tokio::test]
async fn rg_takes_location_and_tag_filters() {
    let mut app = setup();
    let mut loads = app.take_load_receiver().unwrap();
    command(&mut app, "rg all loc=westeurope tag=env:prod").await;
    app.apply_load(loads.recv().await.unwrap());
    let names: Vec<String> = app
        .state()
        .filtered_resource_groups()
        .iter()
        .map(|r| r.name.clone())
        .collect();
    assert_eq!(names, ["rg-weu"]);

    // Into the group and back: the group filter is restored.
    press(&mut app, KeyCode::Enter).await;
    assert!(
        app.state().resource_query.is_empty(),
        "resources start unfiltered"
    );
    press(&mut app, KeyCode::Esc).await;
    assert_eq!(app.state().filtered_resource_groups().len(), 1);

    command(&mut app, "rg all type=vm").await;
    assert!(app
        .state()
        .error
        .as_deref()
        .unwrap()
        .contains("not resource groups"));
}

#[tokio::test]
async fn aliases_work_in_type_filters_and_the_type_summary() {
    let mut app = setup();
    let mut loads = app.take_load_receiver().unwrap();
    command(&mut app, "res all type=vm").await;
    app.apply_load(loads.recv().await.unwrap());
    assert_eq!(visible_names(&app), ["vm-1", "vm-2", "vm-3"]);

    command(&mut app, "type all").await;
    app.apply_load(loads.recv().await.unwrap());
    press(&mut app, KeyCode::Char('/')).await;
    for c in "key-vault".chars() {
        press(&mut app, KeyCode::Char(c)).await;
    }
    assert_eq!(
        app.state().type_summary(),
        [("microsoft.keyvault/vaults".to_string(), 1)]
    );
}

#[tokio::test]
async fn tag_key_equals_value_and_keyvault_alias_from_the_command_line() {
    let mut app = setup();
    let mut loads = app.take_load_receiver().unwrap();
    command(&mut app, "res all tag=env=prod").await;
    app.apply_load(loads.recv().await.unwrap());
    assert_eq!(visible_names(&app), ["vm-1", "disk-1", "kv-1"]);

    command(&mut app, "res all type=keyvault tag=env=prod").await;
    app.apply_load(loads.recv().await.unwrap());
    assert_eq!(visible_names(&app), ["kv-1"]);
}

#[tokio::test]
async fn tag_view_counts_tags_and_opens_the_tagged_resources() {
    let mut app = setup();
    let mut loads = app.take_load_receiver().unwrap();

    command(&mut app, "tag all").await;
    assert_eq!(app.state().view, ViewMode::TagSummary);
    app.apply_load(loads.recv().await.unwrap());
    let rows: Vec<(String, String, usize)> = app
        .state()
        .tag_summary()
        .into_iter()
        .map(|t| (t.key, t.value, t.count))
        .collect();
    assert_eq!(
        rows,
        [
            ("env".to_string(), "prod".to_string(), 3),
            ("env".to_string(), "dev".to_string(), 2),
        ]
    );

    // It renders: counts, keys and values.
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(100, 20)).unwrap();
    terminal
        .draw(|f| az9s_tui::ui::draw(f, &mut app).unwrap())
        .unwrap();
    let screen: String = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|c| c.symbol())
        .collect();
    assert!(screen.contains("Tags (2 values of 1 key)"), "{screen}");
    assert!(screen.contains("env") && screen.contains("prod"));

    // Enter on env=prod: those resources, no reload; Esc comes back.
    press(&mut app, KeyCode::Enter).await;
    assert_eq!(app.state().view, ViewMode::ResourceList);
    assert!(!app.state().loading);
    assert_eq!(visible_names(&app), ["vm-1", "disk-1", "kv-1"]);
    press(&mut app, KeyCode::Esc).await;
    assert_eq!(app.state().view, ViewMode::TagSummary);
    assert!(app.state().resource_query.is_empty());
}
