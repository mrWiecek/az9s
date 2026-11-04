//! Themes change every colour in the UI. Its own test binary (process):
//! the theme is global, so it must not change under other tests.

use c9s_core::cache::Cache;
use c9s_core::config::ThemeName;
use c9s_core::models::{Resource, Subscription};
use c9s_core::state::ViewMode;
use c9s_core::App;
use c9s_tui::{ui, TuiApp};
use ratatui::{backend::TestBackend, buffer::Buffer, style::Color, Terminal};
use std::collections::HashMap;
use std::path::Path;

/// The theme is global: tests that change it take turns.
static THEME_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

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
    let _turn = THEME_LOCK.lock().unwrap_or_else(|e| e.into_inner());
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

    // Schemes with their own background paint every cell with it, the
    // selected row aside; all of them look different.
    let mut seen = Vec::new();
    for name in ThemeName::ALL {
        ui::theme::set_theme(name);
        let buf = render(&mut app);
        if let Some(bg) = ui::theme::theme().background {
            let painted = buf.content.iter().filter(|c| c.bg == bg).count();
            assert!(
                painted * 10 > buf.content.len() * 9,
                "{}: {painted} of {} cells on its background",
                name.as_str(),
                buf.content.len()
            );
        }
        let palette = colours(&buf);
        assert!(
            !seen.contains(&palette),
            "{} looks like another theme",
            name.as_str()
        );
        seen.push(palette);
    }

    ui::theme::set_theme(ThemeName::Default);
}

#[test]
fn theme_switches_while_running() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    let _turn = THEME_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    runtime.block_on(async {
        let mut app = app();
        let press = |code| c9s_tui::Event::Key(KeyEvent::new(code, KeyModifiers::NONE));
        for c in ":theme nord".chars() {
            app.handle_event(press(KeyCode::Char(c))).await.unwrap();
        }
        app.handle_event(press(KeyCode::Enter)).await.unwrap();
        assert_eq!(ui::theme::theme().name, ThemeName::Nord);
        assert_eq!(app.state().config.theme, ThemeName::Nord);
        assert!(app
            .state()
            .notice
            .as_deref()
            .unwrap()
            .contains("theme = \"nord\""));

        for c in ":theme neon".chars() {
            app.handle_event(press(KeyCode::Char(c))).await.unwrap();
        }
        app.handle_event(press(KeyCode::Enter)).await.unwrap();
        assert!(app
            .state()
            .error
            .as_deref()
            .unwrap()
            .contains("tokyo-night"));
        assert_eq!(ui::theme::theme().name, ThemeName::Nord, "unchanged");
        ui::theme::set_theme(ThemeName::Default);
    });
}
