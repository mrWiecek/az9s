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
    assert!(app.state().loading, "subscriptions should be loading");

    let mut rx = app.take_load_receiver().expect("receiver");
    let result = tokio::time::timeout(Duration::from_secs(60), rx.recv())
        .await
        .expect("subscriptions within 60s")
        .expect("channel open");
    app.apply(result);

    assert!(!app.state().loading);
    assert!(app.state().error.is_none(), "{:?}", app.state().error);
    for sub in &app.state().subscriptions {
        assert!(!sub.id.is_empty());
        assert!(!sub.name.is_empty());
    }
}
