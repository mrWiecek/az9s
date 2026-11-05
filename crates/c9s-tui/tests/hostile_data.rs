//! Data from the clouds is untrusted: any tenant member can name a
//! resource, tag or group. Control characters in it (terminal escape
//! sequences that could set the title, write the clipboard or move the
//! cursor) must never reach the terminal.

use c9s_core::cache::Cache;
use c9s_core::models::{Resource, Subscription};
use c9s_core::state::ViewMode;
use c9s_core::App;
use c9s_tui::{ui, TuiApp};
use ratatui::{backend::TestBackend, Terminal};
use std::path::Path;

/// OSC 52 (clipboard write), OSC 0 (title), CSI (colours), BEL, CR, a C1
/// control, a bidi override and a zero-width joiner.
const HOSTILE: &str =
    "evil\x1b]52;c;cm0gLXJmIH4=\x07\x1b]0;pwned\x07\x1b[2J\x1b[31mred\r\u{9b}31m\u{202e}gnp.exe\u{200d}";

fn hostile_app(view: ViewMode) -> TuiApp {
    let mut core = App::offline(Cache::new(Path::new(":memory:")).unwrap());
    let state = core.state_mut();
    state.subscriptions = vec![Subscription {
        id: "s1".into(),
        name: format!("sub-{HOSTILE}"),
        state: "Enabled".into(),
        tenant_id: "t".into(),
    }];
    state.resources = vec![Resource {
        id: format!("/subscriptions/s1/resourceGroups/rg/providers/X/y/{HOSTILE}"),
        name: format!("vm-{HOSTILE}"),
        resource_type: format!("Microsoft.Compute/{HOSTILE}"),
        resource_group: format!("rg-{HOSTILE}"),
        location: HOSTILE.into(),
        subscription_id: "s1".into(),
        tags: [(HOSTILE.to_string(), HOSTILE.to_string())]
            .into_iter()
            .collect(),
    }];
    state.notice = Some(format!("notice {HOSTILE}"));
    state.view = view;
    TuiApp::new(core)
}

fn rendered(app: &mut TuiApp) -> Vec<String> {
    let mut terminal = Terminal::new(TestBackend::new(200, 30)).unwrap();
    terminal.draw(|f| ui::draw(f, app).unwrap()).unwrap();
    let buf = terminal.backend().buffer().clone();
    buf.content()
        .iter()
        .map(|c| c.symbol().to_string())
        .collect()
}

fn assert_clean(cells: &[String]) {
    for (i, symbol) in cells.iter().enumerate() {
        assert!(
            !symbol.chars().any(|c| c.is_control()
                || matches!(c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')),
            "cell {i} carries a control character: {symbol:?}"
        );
    }
}

#[test]
fn control_characters_never_reach_the_terminal() {
    for view in [ViewMode::SubscriptionList, ViewMode::ResourceList] {
        let mut app = hostile_app(view);
        assert_clean(&rendered(&mut app));
    }
    // An error popup with hostile text (API error messages are untrusted).
    let mut app = hostile_app(ViewMode::ResourceList);
    app.state_mut()
        .set_error(Some(format!("Failed: {HOSTILE}")));
    assert_clean(&rendered(&mut app));
}
