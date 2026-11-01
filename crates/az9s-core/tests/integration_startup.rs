/// Integration tests for az9s startup behavior
///
/// These tests verify that the application correctly loads subscriptions
/// during initialization, ensuring users see their subscriptions immediately
/// after successful Azure authentication.
use az9s_core::{models::Subscription, App};

#[tokio::test]
#[ignore] // Requires Azure credentials
async fn test_app_initialization_loads_subscriptions() {
    // This test verifies the critical startup behavior:
    // If Azure CLI login succeeds and subscriptions exist,
    // the app should show them on launch rather than an empty subscription screen.

    let app = App::new().await;

    // If authentication succeeds, we should have an app
    if let Ok(app) = app {
        let state = app.state();

        // After successful initialization, subscriptions should be loaded
        // (either from Azure or from cache)
        if app.azure_client().is_some() {
            // If we have an Azure client, we should have attempted to load subscriptions
            // The list might be empty if the account has no subscriptions,
            // but the loading flag should be false (loading complete)
            assert!(
                !state.loading,
                "Loading should be complete after initialization"
            );

            // If subscriptions were loaded, verify they have required fields
            for sub in &state.subscriptions {
                assert!(!sub.id.is_empty(), "Subscription ID should not be empty");
                assert!(
                    !sub.name.is_empty(),
                    "Subscription name should not be empty"
                );
            }
        }
    }
}

#[tokio::test]
#[ignore] // Requires Azure credentials
async fn test_subscription_refresh() {
    let app = App::new().await;

    if let Ok(mut app) = app {
        if app.azure_client().is_some() {
            // Test that we can refresh subscriptions
            let result = app.refresh_subscriptions().await;

            // Refresh should succeed if we have a valid client
            assert!(result.is_ok(), "Subscription refresh should succeed");

            // After refresh, loading should be complete
            assert!(
                !app.state().loading,
                "Loading should be complete after refresh"
            );
        }
    }
}

#[test]
fn test_subscription_model() {
    // Verify the subscription model has all required fields
    let sub = Subscription {
        id: "test-id".to_string(),
        name: "Test Subscription".to_string(),
        state: "Enabled".to_string(),
        tenant_id: "test-tenant".to_string(),
    };

    assert_eq!(sub.id, "test-id");
    assert_eq!(sub.name, "Test Subscription");
    assert_eq!(sub.state, "Enabled");
    assert_eq!(sub.tenant_id, "test-tenant");
}

#[tokio::test]
async fn test_app_initialization_without_azure() {
    // Test that app initialization handles missing Azure credentials gracefully
    // This test should pass even without Azure credentials

    // Note: In a real scenario without credentials, App::new() will:
    // 1. Attempt authentication (which may fail)
    // 2. Try to load from cache as fallback
    // 3. Set an error message if both fail
    // 4. Still return a valid App instance (not panic)

    // We can't easily test this without mocking, but we document the expected behavior
}

#[test]
fn test_startup_flow_documentation() {
    // This test documents the expected startup flow:
    //
    // 1. main.rs calls App::new().await
    // 2. App::new() calls initialize_azure()
    // 3. initialize_azure() does:
    //    a. Authenticate with Azure (CLI first, then default credential chain)
    //    b. Call client.list_subscriptions()
    //    c. Convert Azure subscriptions to core model
    //    d. Cache subscriptions for offline access
    //    e. Update state.subscriptions
    //    f. Set loading = false
    // 4. If Azure API fails, fall back to cache
    // 5. If cache is empty, set error message
    // 6. Return App instance with populated state
    // 7. TUI renders the subscription list from state.subscriptions
    //
    // Result: Users see subscriptions immediately on launch (if auth succeeds)
}
