//! Rendering behaviour with large lists, at the scale of the target user
//! (400+ subscriptions, thousands of resources; we test 100k rows for margin).

use az9s_core::cache::Cache;
use az9s_core::models::Resource;
use az9s_core::state::ViewMode;
use az9s_core::App;
use az9s_tui::{ui, TuiApp};
use ratatui::{backend::TestBackend, Terminal};
use std::collections::HashMap;
use std::path::Path;
use std::time::{Duration, Instant};

fn app_with_resources(n: usize) -> TuiApp {
    let cache = Cache::new(Path::new(":memory:")).expect("in-memory cache");
    let mut app = TuiApp::new(App::offline(cache));
    let state = app.state_mut();
    state.resources = (0..n)
        .map(|i| Resource {
            id: format!("/subscriptions/s{}/resourceGroups/rg-{}/providers/Microsoft.Compute/virtualMachines/vm-{i:06}", i % 400, i % 1000),
            name: format!("vm-{i:06}"),
            resource_type: "Microsoft.Compute/virtualMachines".into(),
            location: if i % 2 == 0 { "westeurope" } else { "northeurope" }.into(),
            resource_group: format!("rg-{}", i % 1000),
            subscription_id: format!("s{}", i % 400),
            tags: HashMap::new(),
        })
        .collect();
    state.view = ViewMode::ResourceList;
    app
}

fn draw(terminal: &mut Terminal<TestBackend>, app: &mut TuiApp) -> Duration {
    let start = Instant::now();
    terminal
        .draw(|f| ui::draw(f, app).expect("draw"))
        .expect("terminal draw");
    start.elapsed()
}

fn screen_text(terminal: &Terminal<TestBackend>) -> String {
    let buf = terminal.backend().buffer();
    buf.content.iter().map(|c| c.symbol()).collect()
}

#[test]
fn scrolling_to_the_end_of_100k_rows_shows_the_last_row() {
    let mut app = app_with_resources(100_000);
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();

    draw(&mut terminal, &mut app);
    assert!(screen_text(&terminal).contains("vm-000000"));

    app.state_mut().list_state.selected = 99_999;
    draw(&mut terminal, &mut app);
    let text = screen_text(&terminal);
    assert!(text.contains("vm-099999"));
    assert!(!text.contains("vm-000000"));
}

#[test]
fn scroll_offset_persists_when_moving_up() {
    let mut app = app_with_resources(1_000);
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();

    app.state_mut().list_state.selected = 999;
    draw(&mut terminal, &mut app);
    let offset_at_bottom = app.state().list_state.offset;
    assert!(offset_at_bottom > 0);

    // One step up stays inside the viewport: the view must not scroll.
    app.state_mut().list_state.selected = 998;
    draw(&mut terminal, &mut app);
    assert_eq!(app.state().list_state.offset, offset_at_bottom);
}

#[test]
fn filtering_shrinks_the_list_and_keeps_selection_valid() {
    let mut app = app_with_resources(10_000);
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();

    app.state_mut().list_state.selected = 9_000;
    app.state_mut().filter = "VM-00012".into(); // case-insensitive; matches 10 rows
    draw(&mut terminal, &mut app);

    assert_eq!(app.state().filtered_resources().len(), 10);
    assert_eq!(app.state().list_state.selected, 9);
    assert!(screen_text(&terminal).contains("vm-000129"));
}

/// Performance check. Ignored by default because wall-clock limits are flaky
/// on shared CI runners; run with `just perf`.
#[test]
#[ignore]
fn perf_frame_time_with_100k_rows() {
    let mut app = app_with_resources(100_000);
    let mut terminal = Terminal::new(TestBackend::new(200, 60)).unwrap();
    draw(&mut terminal, &mut app); // warm up

    let frames = 50;
    let mut total = Duration::ZERO;
    for i in 0..frames {
        app.state_mut().list_state.selected = i * 1_000;
        total += draw(&mut terminal, &mut app);
    }
    let unfiltered = total / frames as u32;

    app.state_mut().filter = "europe".into();
    let mut total = Duration::ZERO;
    for _ in 0..frames {
        total += draw(&mut terminal, &mut app);
    }
    let filtered = total / frames as u32;

    // Structured filter (`:res loc=westeurope`) on top of the text filter.
    app.state_mut().resource_query =
        az9s_core::query::ResourceQuery::parse(["loc=westeurope"]).unwrap();
    let mut total = Duration::ZERO;
    for _ in 0..frames {
        total += draw(&mut terminal, &mut app);
    }
    let structured = total / frames as u32;

    // Type summary (`:type`): counted once per list/filter change, then memoized.
    app.state_mut().filter.clear();
    app.state_mut().view = ViewMode::ResourceTypeSummary;
    let first_summary = draw(&mut terminal, &mut app);
    let mut total = Duration::ZERO;
    for _ in 0..frames {
        total += draw(&mut terminal, &mut app);
    }
    let summary = total / frames as u32;

    println!(
        "100k rows: avg frame {unfiltered:?} unfiltered, {filtered:?} with filter, \
         {structured:?} with loc= filter, type summary {summary:?} (first {first_summary:?})"
    );
    for (name, frame) in [
        ("unfiltered", unfiltered),
        ("filtered", filtered),
        ("structured filter", structured),
        ("type summary", summary),
    ] {
        assert!(frame < Duration::from_millis(16), "{name} frame {frame:?}");
    }
}

/// A tenant at the target scale: 100 management groups (10 under the root,
/// 9 below each) and 2,000 subscriptions, everything expanded.
fn app_with_large_tree() -> TuiApp {
    use az9s_core::scope_tree::{GroupNode, ScopeTree, SubscriptionNode};
    let cache = Cache::new(Path::new(":memory:")).expect("in-memory cache");
    let mut app = TuiApp::new(App::offline(cache));
    let mut groups = Vec::new();
    for top in 0..10 {
        groups.push(GroupNode {
            name: format!("mg-{top}"),
            display_name: format!("Landing zone {top}"),
            parent: "root".into(),
        });
        for child in 0..9 {
            groups.push(GroupNode {
                name: format!("mg-{top}-{child}"),
                display_name: format!("Zone {top} / team {child}"),
                parent: format!("mg-{top}"),
            });
        }
    }
    let subscriptions = (0..2000)
        .map(|i| SubscriptionNode {
            id: format!("00000000-0000-0000-0000-{i:012}"),
            display_name: format!("sub-{i:04}"),
            parent: format!("mg-{}-{}", i % 10, i % 9),
        })
        .collect();
    let state = app.state_mut();
    state.set_scope_tree(ScopeTree {
        root: "root".into(),
        root_display_name: "Tenant Root Group".into(),
        groups: groups.clone(),
        subscriptions,
    });
    for g in &groups {
        state.set_tree_expanded(&g.name, true);
    }
    state.view = ViewMode::ManagementGroupTree;
    app
}

#[test]
fn large_management_group_tree_renders_within_a_frame_budget() {
    let mut app = app_with_large_tree();
    assert_eq!(app.state().current_list_len(), 1 + 100 + 2000);
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
    draw(&mut terminal, &mut app); // warm up

    let mut worst = Duration::ZERO;
    for i in 0..20 {
        app.state_mut().list_state.selected = i * 100;
        worst = worst.max(draw(&mut terminal, &mut app));
    }
    let text = screen_text(&terminal);
    assert!(text.contains("sub-"), "subscriptions on screen");
    // Debug builds are much slower than release; the budget is for release.
    let budget = if cfg!(debug_assertions) {
        Duration::from_millis(250)
    } else {
        Duration::from_millis(16)
    };
    assert!(worst < budget, "worst frame {worst:?}");
    println!("worst tree frame: {worst:?}");
}

#[test]
fn tree_view_shows_indentation_markers_and_counts() {
    let mut app = app_with_large_tree();
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
    draw(&mut terminal, &mut app);
    let buf = terminal.backend().buffer().clone();
    let lines: Vec<String> = (0..buf.area.height)
        .map(|y| {
            (0..buf.area.width)
                .map(|x| buf.get(x, y).symbol().to_string())
                .collect::<String>()
        })
        .collect();
    let screen = lines.join("\n");
    println!("{screen}");
    assert!(screen.contains("▾ ◆ Tenant Root Group"));
    assert!(screen.contains("2000 subscriptions"));
    assert!(screen.contains("    ▾ ◆ Zone 0 / team 0"));
}

#[test]
fn help_scrolls_to_its_end_on_a_short_terminal() {
    let mut app = app_with_resources(1);
    let mut terminal = Terminal::new(TestBackend::new(100, 24)).unwrap();
    app.state_mut().help_open = true;
    draw(&mut terminal, &mut app);
    assert!(screen_text(&terminal).contains("Commands"));
    app.state_mut().help_scroll = usize::MAX / 2;
    draw(&mut terminal, &mut app);
    assert!(
        screen_text(&terminal).contains("Cached"),
        "last lines reachable"
    );
    assert!(app.state().help_scroll < 200, "clamped");
}

#[test]
fn long_names_are_cut_before_the_next_column() {
    let mut app = app_with_resources(1);
    app.state_mut().resources[0].name =
        "an-extremely-long-resource-name-that-would-overlap-the-type-column-0001".into();
    let mut terminal = Terminal::new(TestBackend::new(80, 20)).unwrap();
    draw(&mut terminal, &mut app);
    let text = screen_text(&terminal);
    // Cut with "…", then at least one space before the type column.
    assert!(
        text.contains("an-extremely-long-resource-name-th… vm"),
        "{text}"
    );
    assert!(!text.contains("0001"));
}

/// The palette over 100k resources: opening it, and each keystroke's search.
/// Ignored by default (wall clock); run with `just perf`.
#[test]
#[ignore]
fn perf_palette_with_100k_resources() {
    use az9s_core::palette::Palette;
    let app = app_with_resources(100_000);
    let start = Instant::now();
    let mut palette = Palette::open(app.state());
    let open = start.elapsed();

    let mut worst = Duration::ZERO;
    let mut query = String::new();
    for c in "vm0999".chars() {
        query.push(c);
        let start = Instant::now();
        palette.set_query(query.clone());
        worst = worst.max(start.elapsed());
    }
    assert_eq!(palette.chosen().unwrap().item.label, "vm-099900");
    println!("palette, 100k resources: open {open:?}, slowest keystroke {worst:?}");
    assert!(open < Duration::from_millis(250), "open {open:?}");
    assert!(worst < Duration::from_millis(100), "keystroke {worst:?}");
}
