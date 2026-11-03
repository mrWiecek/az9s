//! Themes change every colour in the UI. Its own test binary (process):
//! the theme is global, so it must not change under other tests.

use az9s_core::cache::Cache;
use az9s_core::config::ThemeName;
use az9s_core::models::{Resource, Subscription};
use az9s_core::state::ViewMode;
use az9s_core::App;
use az9s_tui::{ui, TuiApp};
use ratatui::{backend::TestBackend, buffer::Buffer, style::Color, Terminal};
use std::collections::HashMap;
use std::path::Path;

fn app() -> TuiApp {
    let mut core = App::offline(Cache::new(Path::new(":memory:")).unwrap());
    let state = core.state_mut();
    state.subscriptions = vec![Subscription {
        id: "s1".into(),
        name: "prod".into(),
        state: "Enabled".into(),
        tenant_id: "t".into(),
    }];
    state.resources = (0..5)
        .map(|i| Resource {
            id: format!("/subscriptions/s1/resourceGroups/rg/providers/x/vm-{i}"),
            name: format!("vm-{i}"),
            resource_type: "Microsoft.Compute/virtualMachines".into(),
            location: "westeurope".into(),
            resource_group: "rg".into(),
            subscription_id: "s1".into(),
            tags: HashMap::new(),
        })
        .collect();
    state.view = ViewMode::ResourceList;
    TuiApp::new(core)
}

fn render(app: &mut TuiApp) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(100, 20)).unwrap();
    terminal.draw(|f| ui::draw(f, app).unwrap()).unwrap();
    terminal.backend().buffer().clone()
}

fn colours(buf: &Buffer) -> Vec<Color> {
    let mut all: Vec<Color> = buf
        .content
        .iter()
        .flat_map(|c| [c.fg, c.bg])
        .filter(|c| *c != Color::Reset)
        .collect();
    all.sort_by_key(|c| format!("{c:?}"));
    all.dedup();
    all
}

#[test]
fn each_theme_colours_the_screen_its_own_way() {
    let mut app = app();

    ui::theme::set_theme(ThemeName::Default);
    let dark = colours(&render(&mut app));
    assert!(
        dark.contains(&Color::Cyan) && dark.contains(&Color::White),
        "{dark:?}"
    );

    ui::theme::set_theme(ThemeName::Light);
    let light = colours(&render(&mut app));
    assert!(
        !light.contains(&Color::Yellow),
        "no yellow text on light: {light:?}"
    );
    assert!(light.contains(&Color::Blue), "{light:?}");

    ui::theme::set_theme(ThemeName::Mono);
    let buf = render(&mut app);
    assert!(
        colours(&buf).is_empty(),
        "mono uses no colours: {:?}",
        colours(&buf)
    );
    // The selected row is still visible: reversed text.
    let reversed = buf
        .content
        .iter()
        .any(|c| c.modifier.contains(ratatui::style::Modifier::REVERSED));
    assert!(reversed, "selection shown without colour");

    ui::theme::set_theme(ThemeName::Default);
}
