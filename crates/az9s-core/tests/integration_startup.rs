//! Startup behaviour against real Azure. Ignored by default because it needs
//! `az login`; run with `cargo test -p az9s-core -- --ignored`.

use az9s_core::App;
use std::time::{Duration, Instant};

#[tokio::test]
#[ignore] // Requires Azure CLI login
async fn startup_does_not_block_and_subscriptions_arrive_in_background() {
    let started = Instant::now();
    let mut app = App::new().await.expect("app init");
    // App::new only opens the cache and starts the fetch; it must not wait
    // for Azure (a full `az account list` takes seconds).
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "App::new took {:?}",
        started.elapsed()
    );
    assert!(
        app.state().loading,
        "subscriptions and the tree should be loading"
    );

    // Two loads run: subscriptions and the management group tree.
    let mut rx = app.take_load_receiver().expect("receiver");
    while app.state().loading {
        let result = tokio::time::timeout(Duration::from_secs(60), rx.recv())
            .await
            .expect("results within 60s")
            .expect("channel open");
        app.apply(result);
    }

    assert!(!app.state().loading);
    assert!(app.state().error.is_none(), "{:?}", app.state().error);
    for sub in &app.state().subscriptions {
        assert!(!sub.id.is_empty());
        assert!(!sub.name.is_empty());
    }
}

/// End-to-end timing of the "all subscriptions" resource group view against
/// a real tenant. Run with:
/// `cargo test --release -p az9s-core --test integration_startup -- --ignored --nocapture`
#[tokio::test(flavor = "multi_thread")]
#[ignore] // Requires Azure CLI login
async fn all_resource_groups_via_resource_graph_live() {
    use az9s_core::loader::LoadRequest;

    let mut app = App::new().await.expect("app init");
    let mut rx = app.take_load_receiver().expect("receiver");
    while app.state().loading {
        app.apply(rx.recv().await.unwrap());
    }
    let subscription_ids: Vec<String> = app
        .state()
        .subscriptions
        .iter()
        .filter(|s| !s.state.eq_ignore_ascii_case("Disabled"))
        .map(|s| s.id.clone())
        .collect();
    println!("{} enabled subscriptions", subscription_ids.len());

    let started = Instant::now();
    app.request(LoadRequest::ResourceGroups { subscription_ids });
    let mut first_rows = None;
    while app.state().loading {
        app.apply(rx.recv().await.unwrap());
        if first_rows.is_none() && !app.state().resource_groups.is_empty() {
            first_rows = Some(started.elapsed());
        }
    }
    println!(
        "resource groups: {} total in {:?}; first rows after {:?}; notice: {:?}; error: {:?}",
        app.state().resource_groups.len(),
        started.elapsed(),
        first_rows,
        app.state().notice,
        app.state().error
    );
    println!("{}", az9s_core::timing::summary());
    assert!(app.state().error.is_none());
}
