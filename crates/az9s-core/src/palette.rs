//! The command palette (`Ctrl+P`): one fuzzy search over commands and what
//! az9s has loaded (subscriptions, management groups, and the current
//! resource group and resource lists). Choosing an item jumps there.

use crate::fuzzy::fuzzy_match;
use crate::resource_type::short_label;
use crate::state::AppState;

/// Most results the palette shows.
pub const MAX_RESULTS: usize = 200;

/// What choosing a palette item does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PaletteAction {
    /// Run a `:` command (without the colon).
    Command(String),
    Help,
    /// Open a subscription's resource groups.
    Subscription(String),
    /// Open the resource groups of every subscription below a management group.
    ManagementGroup(String),
    /// Open a resource group's resources.
    ResourceGroup {
        subscription_id: String,
        name: String,
    },
    /// Open a resource's detail view (by ID; it is in the loaded list).
    Resource(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteKind {
    Command,
    Subscription,
    ManagementGroup,
    ResourceGroup,
    Resource,
}

impl PaletteKind {
    /// Short tag shown before each result.
    pub fn tag(self) -> &'static str {
        match self {
            PaletteKind::Command => "cmd",
            PaletteKind::Subscription => "sub",
            PaletteKind::ManagementGroup => "mg",
            PaletteKind::ResourceGroup => "rg",
            PaletteKind::Resource => "res",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaletteItem {
    pub kind: PaletteKind,
    /// What is matched and shown.
    pub label: String,
    /// Shown dimmed after the label (not matched).
    pub detail: String,
    pub action: PaletteAction,
}

/// A search result: the item and the matched character positions in its label.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaletteResult {
    pub item: PaletteItem,
    pub positions: Vec<usize>,
}

const COMMANDS: &[(&str, &str)] = &[
    ("mg", "Management group tree"),
    ("sub", "Subscriptions"),
    ("rg", "Resource groups where you are"),
    ("rg all", "Resource groups in every subscription"),
    ("res", "Resources where you are"),
    ("res all", "Resources in every subscription"),
    ("type all", "Resource counts per type, every subscription"),
    (
        "tag all",
        "Tags and how many resources carry them, every subscription",
    ),
    ("policy", "Policy assignments and compliance where you are"),
    (
        "policy all",
        "Policy assignments and compliance for the whole tenant",
    ),
    ("ctx", "Switch tenant"),
];

/// Everything the palette can find, in the order shown for an empty query.
pub fn items(state: &AppState) -> Vec<PaletteItem> {
    let mut items: Vec<PaletteItem> = COMMANDS
        .iter()
        .map(|(cmd, what)| PaletteItem {
            kind: PaletteKind::Command,
            label: format!(":{cmd}"),
            detail: what.to_string(),
            action: PaletteAction::Command(cmd.to_string()),
        })
        .collect();
    items.push(PaletteItem {
        kind: PaletteKind::Command,
        label: "help".into(),
        detail: "Keys, commands, filters and examples (?)".into(),
        action: PaletteAction::Help,
    });

    if let Some(tree) = &state.scope_tree {
        items.push(PaletteItem {
            kind: PaletteKind::ManagementGroup,
            label: tree.root_display_name.clone(),
            detail: "tenant root group".into(),
            action: PaletteAction::ManagementGroup(tree.root.clone()),
        });
        items.extend(tree.groups.iter().map(|g| PaletteItem {
            kind: PaletteKind::ManagementGroup,
            label: g.display_name.clone(),
            detail: g.name.clone(),
            action: PaletteAction::ManagementGroup(g.name.clone()),
        }));
    }
    let in_tenant = state.subscriptions.iter().filter(|s| state.in_tenant(s));
    items.extend(in_tenant.map(|s| PaletteItem {
        kind: PaletteKind::Subscription,
        label: s.name.clone(),
        detail: format!("{} · {}", s.id, s.state),
        action: PaletteAction::Subscription(s.id.clone()),
    }));
    let sub_name = |id: &str| {
        state
            .subscriptions
            .iter()
            .find(|s| s.id.eq_ignore_ascii_case(id))
            .map(|s| s.name.clone())
            .unwrap_or_else(|| id.to_string())
    };
    items.extend(state.resource_groups.iter().map(|rg| PaletteItem {
        kind: PaletteKind::ResourceGroup,
        label: rg.name.clone(),
        detail: format!("{} · {}", rg.location, sub_name(&rg.subscription_id)),
        action: PaletteAction::ResourceGroup {
            subscription_id: rg.subscription_id.clone(),
            name: rg.name.clone(),
        },
    }));
    items.extend(state.resources.iter().map(|r| PaletteItem {
        kind: PaletteKind::Resource,
        label: r.name.clone(),
        detail: format!("{} · {}", short_label(&r.resource_type), r.resource_group),
        action: PaletteAction::Resource(r.id.clone()),
    }));
    items
}

/// The best matches for `query`, best first (ties keep the item order). An
/// empty query lists the first items unranked.
pub fn search(items: &[PaletteItem], query: &str) -> Vec<PaletteResult> {
    if query.trim().is_empty() {
        return items
            .iter()
            .take(MAX_RESULTS)
            .map(|item| PaletteResult {
                item: item.clone(),
                positions: vec![],
            })
            .collect();
    }
    let mut scored: Vec<(i64, usize, Vec<usize>)> = items
        .iter()
        .enumerate()
        .filter_map(|(i, item)| fuzzy_match(&item.label, query).map(|m| (m.score, i, m.positions)))
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    scored
        .into_iter()
        .take(MAX_RESULTS)
        .map(|(_, i, positions)| PaletteResult {
            item: items[i].clone(),
            positions,
        })
        .collect()
}

/// An open palette: the items as they were when it opened, the query,
/// the current results and the selected one.
#[derive(Debug, Clone, Default)]
pub struct Palette {
    items: Vec<PaletteItem>,
    pub query: String,
    pub results: Vec<PaletteResult>,
    pub selected: usize,
}

impl Palette {
    pub fn open(state: &AppState) -> Self {
        let items = items(state);
        let results = search(&items, "");
        Self {
            items,
            query: String::new(),
            results,
            selected: 0,
        }
    }

    /// Change the query and search again; the selection goes to the top.
    pub fn set_query(&mut self, query: String) {
        self.results = search(&self.items, &query);
        self.query = query;
        self.selected = 0;
    }

    /// Move the selection by `step`, wrapping around.
    pub fn select(&mut self, step: isize) {
        let n = self.results.len() as isize;
        if n > 0 {
            self.selected = (self.selected as isize + step).rem_euclid(n) as usize;
        }
    }

    pub fn chosen(&self) -> Option<&PaletteResult> {
        self.results.get(self.selected)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Resource, ResourceGroup, Subscription};
    use std::collections::HashMap;

    fn state() -> AppState {
        let mut state = AppState::new();
        state.subscriptions = vec![Subscription {
            id: "sub-1".into(),
            name: "prod-connectivity".into(),
            state: "Enabled".into(),
            tenant_id: "t".into(),
        }];
        state.resource_groups = vec![ResourceGroup {
            id: "/subscriptions/sub-1/resourceGroups/rg-hub".into(),
            name: "rg-hub".into(),
            location: "westeurope".into(),
            subscription_id: "sub-1".into(),
            tags: HashMap::new(),
        }];
        state.resources = vec![Resource {
            id: "/subscriptions/sub-1/resourceGroups/rg-hub/providers/Microsoft.KeyVault/vaults/kv-prod-weu".into(),
            name: "kv-prod-weu".into(),
            resource_type: "Microsoft.KeyVault/vaults".into(),
            location: "westeurope".into(),
            resource_group: "rg-hub".into(),
            subscription_id: "sub-1".into(),
            tags: HashMap::new(),
        }];
        state
    }

    #[test]
    fn items_cover_commands_and_everything_loaded() {
        let items = items(&state());
        let kinds = |k: PaletteKind| items.iter().filter(|i| i.kind == k).count();
        assert_eq!(kinds(PaletteKind::Command), COMMANDS.len() + 1);
        assert_eq!(kinds(PaletteKind::Subscription), 1);
        assert_eq!(kinds(PaletteKind::ResourceGroup), 1);
        assert_eq!(kinds(PaletteKind::Resource), 1);
        let rg = items
            .iter()
            .find(|i| i.kind == PaletteKind::ResourceGroup)
            .unwrap();
        assert_eq!(
            rg.detail, "westeurope · prod-connectivity",
            "subscription by name"
        );
        let kv = items
            .iter()
            .find(|i| i.kind == PaletteKind::Resource)
            .unwrap();
        assert_eq!(kv.detail, "key-vault · rg-hub");
    }

    #[test]
    fn search_ranks_fuzzy_matches() {
        let items = items(&state());
        let results = search(&items, "kvprod");
        assert_eq!(results[0].item.label, "kv-prod-weu");
        assert_eq!(results[0].positions, [0, 1, 3, 4, 5, 6]);

        let results = search(&items, "prod");
        let labels: Vec<&str> = results.iter().map(|r| r.item.label.as_str()).collect();
        assert!(labels.contains(&"prod-connectivity") && labels.contains(&"kv-prod-weu"));
        assert_eq!(
            labels[0], "prod-connectivity",
            "match at the start ranks first"
        );

        assert_eq!(
            search(&items, ":rg")[0].item.action,
            PaletteAction::Command("rg".into())
        );
        assert!(search(&items, "zzz").is_empty());
        assert_eq!(search(&items, "").len(), items.len().min(MAX_RESULTS));
    }

    #[test]
    fn an_open_palette_searches_and_wraps_its_selection() {
        let mut palette = Palette::open(&state());
        palette.set_query("rghub".into());
        assert_eq!(palette.chosen().unwrap().item.label, "rg-hub");
        palette.select(-1);
        assert_eq!(
            palette.selected,
            palette.results.len() - 1,
            "wraps to the bottom"
        );
        palette.set_query("zzz".into());
        assert!(palette.chosen().is_none());
        palette.select(1); // no results: no panic
    }
}
