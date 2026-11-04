//! The management group tree: tenant root → management groups → subscriptions.
//!
//! [`ScopeTree`] is the hierarchy as Azure returns it. [`ScopeTree::rows`]
//! flattens it into the rows the tree view shows, given which groups are
//! expanded and the current filter. Subscriptions you can see (from the Azure
//! CLI) but that are not in the tree (e.g. in another tenant) are listed under
//! a pseudo-group, so nothing silently disappears.

use crate::models::Subscription;
use crate::state::contains_ignore_ascii_case;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Key of the pseudo-group holding subscriptions that are not in the tree.
pub const UNPLACED: &str = "~unplaced";
const UNPLACED_LABEL: &str = "Not in the management group tree";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupNode {
    /// Management group name (the last segment of its ID).
    pub name: String,
    pub display_name: String,
    pub parent: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubscriptionNode {
    /// Subscription ID, lowercase.
    pub id: String,
    pub display_name: String,
    pub parent: String,
}

/// The management group hierarchy below the root group.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScopeTree {
    /// Root group name (for the tenant root group, the tenant ID).
    pub root: String,
    pub root_display_name: String,
    /// Every group below the root.
    pub groups: Vec<GroupNode>,
    pub subscriptions: Vec<SubscriptionNode>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TreeRowKind {
    ManagementGroup,
    Subscription,
    /// The pseudo-group of subscriptions missing from the tree.
    Unplaced,
}

/// One visible line of the tree view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeRow {
    pub kind: TreeRowKind,
    /// Group name, subscription ID, or [`UNPLACED`].
    pub key: String,
    pub label: String,
    pub depth: usize,
    pub expanded: bool,
    pub has_children: bool,
    /// Subscriptions anywhere below a group (0 for subscriptions).
    pub subscription_count: usize,
    /// Subscription state from the Azure CLI (e.g. "Disabled"), if known.
    pub state: Option<String>,
}

impl TreeRow {
    pub fn is_group(&self) -> bool {
        self.kind != TreeRowKind::Subscription
    }
}

/// Children of each group, sorted by label, built once per [`ScopeTree::rows`] call.
struct Index<'a> {
    groups: HashMap<&'a str, Vec<&'a GroupNode>>,
    subs: HashMap<&'a str, Vec<&'a SubscriptionNode>>,
    counts: HashMap<&'a str, usize>,
}

impl ScopeTree {
    /// Build a tree from the descendants of `root`.
    pub fn from_nodes(
        root: &str,
        root_display_name: &str,
        nodes: &[c9s_azure::management_groups::HierarchyNode],
    ) -> Self {
        use c9s_azure::management_groups::NodeKind;
        let mut tree = ScopeTree {
            root: root.to_string(),
            root_display_name: root_display_name.to_string(),
            ..Default::default()
        };
        for node in nodes {
            let parent = node.parent.clone().unwrap_or_else(|| root.to_string());
            match node.kind {
                NodeKind::ManagementGroup => tree.groups.push(GroupNode {
                    name: node.name.clone(),
                    display_name: node.display_name.clone(),
                    parent,
                }),
                NodeKind::Subscription => tree.subscriptions.push(SubscriptionNode {
                    id: node.name.to_ascii_lowercase(),
                    display_name: node.display_name.clone(),
                    parent,
                }),
            }
        }
        tree
    }

    fn index(&self) -> Index<'_> {
        let mut groups: HashMap<&str, Vec<&GroupNode>> = HashMap::new();
        for g in &self.groups {
            groups.entry(g.parent.as_str()).or_default().push(g);
        }
        for list in groups.values_mut() {
            list.sort_by_cached_key(|g| g.display_name.to_lowercase());
        }
        let mut subs: HashMap<&str, Vec<&SubscriptionNode>> = HashMap::new();
        for s in &self.subscriptions {
            subs.entry(s.parent.as_str()).or_default().push(s);
        }
        for list in subs.values_mut() {
            list.sort_by_cached_key(|s| s.display_name.to_lowercase());
        }
        let mut index = Index {
            groups,
            subs,
            counts: HashMap::new(),
        };
        let mut visiting = HashSet::new();
        count(&mut index, &self.root, &mut visiting);
        index
    }

    /// Subscription IDs (lowercase) anywhere below `group`, including
    /// subscriptions directly in it. For [`UNPLACED`], the subscriptions in
    /// `known` that are not in the tree.
    pub fn subscriptions_under(&self, group: &str, known: &[Subscription]) -> Vec<String> {
        if group == UNPLACED {
            return self
                .unplaced(known)
                .into_iter()
                .map(|s| s.id.to_ascii_lowercase())
                .collect();
        }
        let index = self.index();
        let mut out = Vec::new();
        let mut stack = vec![group];
        let mut seen = HashSet::new();
        while let Some(g) = stack.pop() {
            if !seen.insert(g) {
                continue;
            }
            out.extend(
                index
                    .subs
                    .get(g)
                    .into_iter()
                    .flatten()
                    .map(|s| s.id.clone()),
            );
            stack.extend(
                index
                    .groups
                    .get(g)
                    .into_iter()
                    .flatten()
                    .map(|c| c.name.as_str()),
            );
        }
        out
    }

    /// Display name of a group or subscription in the tree.
    pub fn label_of(&self, key: &str) -> Option<&str> {
        if key == self.root {
            return Some(&self.root_display_name);
        }
        if key == UNPLACED {
            return Some(UNPLACED_LABEL);
        }
        self.groups
            .iter()
            .find(|g| g.name == key)
            .map(|g| g.display_name.as_str())
            .or_else(|| {
                self.subscriptions
                    .iter()
                    .find(|s| s.id.eq_ignore_ascii_case(key))
                    .map(|s| s.display_name.as_str())
            })
    }

    /// Known subscriptions that are not in the tree.
    fn unplaced<'a>(&self, known: &'a [Subscription]) -> Vec<&'a Subscription> {
        let placed: HashSet<&str> = self.subscriptions.iter().map(|s| s.id.as_str()).collect();
        let mut out: Vec<&Subscription> = known
            .iter()
            .filter(|s| !placed.contains(s.id.to_ascii_lowercase().as_str()))
            .collect();
        out.sort_by_cached_key(|s| s.name.to_lowercase());
        out
    }

    /// The rows to show. Groups in `expanded` show their children. With a
    /// filter, only matching rows and their ancestors are shown, expanded.
    pub fn rows(
        &self,
        expanded: &HashSet<String>,
        known: &[Subscription],
        filter: &str,
    ) -> Vec<TreeRow> {
        let index = self.index();
        let states: HashMap<String, &str> = known
            .iter()
            .map(|s| (s.id.to_ascii_lowercase(), s.state.as_str()))
            .collect();
        let needle = filter.to_ascii_lowercase();
        let walker = Walker {
            index: &index,
            expanded,
            states: &states,
            needle: &needle,
        };
        let mut rows = Vec::new();
        let mut visiting = HashSet::new();
        walker.group(
            &self.root,
            &self.root_display_name,
            0,
            &mut rows,
            &mut visiting,
        );

        let unplaced = self.unplaced(known);
        if !unplaced.is_empty() {
            let children: Vec<TreeRow> = unplaced
                .iter()
                .filter(|s| {
                    needle.is_empty()
                        || contains_ignore_ascii_case(&s.name, &needle)
                        || contains_ignore_ascii_case(&s.id, &needle)
                })
                .map(|s| TreeRow {
                    kind: TreeRowKind::Subscription,
                    key: s.id.to_ascii_lowercase(),
                    label: s.name.clone(),
                    depth: 1,
                    expanded: false,
                    has_children: false,
                    subscription_count: 0,
                    state: Some(s.state.clone()),
                })
                .collect();
            let self_match =
                needle.is_empty() || contains_ignore_ascii_case(UNPLACED_LABEL, &needle);
            if self_match || !children.is_empty() {
                let open = if needle.is_empty() {
                    expanded.contains(UNPLACED)
                } else {
                    !children.is_empty()
                };
                rows.push(TreeRow {
                    kind: TreeRowKind::Unplaced,
                    key: UNPLACED.to_string(),
                    label: UNPLACED_LABEL.to_string(),
                    depth: 0,
                    expanded: open,
                    has_children: true,
                    subscription_count: unplaced.len(),
                    state: None,
                });
                if open {
                    rows.extend(children);
                }
            }
        }
        rows
    }
}

/// Count subscriptions below each group (guarding against cycles).
fn count<'a>(index: &mut Index<'a>, group: &'a str, visiting: &mut HashSet<&'a str>) -> usize {
    if let Some(&n) = index.counts.get(group) {
        return n;
    }
    if !visiting.insert(group) {
        return 0;
    }
    let mut n = index.subs.get(group).map_or(0, Vec::len);
    let children: Vec<&'a str> = index
        .groups
        .get(group)
        .into_iter()
        .flatten()
        .map(|g| g.name.as_str())
        .collect();
    for child in children {
        n += count(index, child, visiting);
    }
    index.counts.insert(group, n);
    n
}

struct Walker<'a> {
    index: &'a Index<'a>,
    expanded: &'a HashSet<String>,
    states: &'a HashMap<String, &'a str>,
    needle: &'a str,
}

impl<'a> Walker<'a> {
    fn matches(&self, label: &str, key: &str) -> bool {
        self.needle.is_empty()
            || contains_ignore_ascii_case(label, self.needle)
            || contains_ignore_ascii_case(key, self.needle)
    }

    /// Append `group`'s row and, if it is open, its children's rows. Returns
    /// whether anything was appended.
    fn group(
        &self,
        name: &'a str,
        label: &str,
        depth: usize,
        out: &mut Vec<TreeRow>,
        visiting: &mut HashSet<&'a str>,
    ) -> bool {
        if !visiting.insert(name) {
            return false;
        }
        let child_groups = self
            .index
            .groups
            .get(name)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let child_subs = self.index.subs.get(name).map(Vec::as_slice).unwrap_or(&[]);
        let has_children = !child_groups.is_empty() || !child_subs.is_empty();
        let filtering = !self.needle.is_empty();

        // Without a filter, children are only needed when the group is open.
        let open = !filtering && self.expanded.contains(name);
        let mut children = Vec::new();
        if open || filtering {
            for g in child_groups {
                self.group(&g.name, &g.display_name, depth + 1, &mut children, visiting);
            }
            for s in child_subs {
                if self.matches(&s.display_name, &s.id) {
                    children.push(TreeRow {
                        kind: TreeRowKind::Subscription,
                        key: s.id.clone(),
                        label: s.display_name.clone(),
                        depth: depth + 1,
                        expanded: false,
                        has_children: false,
                        subscription_count: 0,
                        state: self.states.get(&s.id).map(|s| s.to_string()),
                    });
                }
            }
        }
        visiting.remove(name);

        let show = !filtering || self.matches(label, name) || !children.is_empty();
        if !show {
            return false;
        }
        let expanded = if filtering {
            !children.is_empty()
        } else {
            open
        };
        out.push(TreeRow {
            kind: TreeRowKind::ManagementGroup,
            key: name.to_string(),
            label: label.to_string(),
            depth,
            expanded,
            has_children,
            subscription_count: self.index.counts.get(name).copied().unwrap_or(0),
            state: None,
        });
        if expanded {
            out.extend(children);
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use c9s_azure::management_groups::{HierarchyNode, NodeKind};

    fn mg(name: &str, parent: &str) -> HierarchyNode {
        HierarchyNode {
            kind: NodeKind::ManagementGroup,
            name: name.into(),
            display_name: format!("MG {name}"),
            parent: Some(parent.into()),
        }
    }

    fn sub_node(id: &str, parent: &str) -> HierarchyNode {
        HierarchyNode {
            kind: NodeKind::Subscription,
            name: id.into(),
            display_name: format!("Sub {id}"),
            parent: Some(parent.into()),
        }
    }

    fn known(id: &str, state: &str) -> Subscription {
        Subscription {
            id: id.into(),
            name: format!("Sub {id}"),
            state: state.into(),
            tenant_id: "t".into(),
        }
    }

    /// root
    /// ├── platform
    /// │   ├── connectivity: s1
    /// │   └── s2
    /// └── workloads: s3, s4
    fn tree() -> ScopeTree {
        ScopeTree::from_nodes(
            "root",
            "Tenant Root Group",
            &[
                mg("platform", "root"),
                mg("workloads", "root"),
                mg("connectivity", "platform"),
                sub_node("S1", "connectivity"),
                sub_node("s2", "platform"),
                sub_node("s3", "workloads"),
                sub_node("s4", "workloads"),
            ],
        )
    }

    fn keys(rows: &[TreeRow]) -> Vec<(usize, &str)> {
        rows.iter().map(|r| (r.depth, r.key.as_str())).collect()
    }

    fn set(items: &[&str]) -> HashSet<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn collapsed_tree_shows_root_only_with_counts() {
        let rows = tree().rows(&set(&[]), &[], "");
        assert_eq!(keys(&rows), [(0, "root")]);
        assert_eq!(rows[0].subscription_count, 4);
        assert!(rows[0].has_children && !rows[0].expanded);
    }

    #[test]
    fn expanded_groups_show_groups_before_subscriptions_sorted() {
        let rows = tree().rows(&set(&["root", "platform"]), &[], "");
        assert_eq!(
            keys(&rows),
            [
                (0, "root"),
                (1, "platform"),
                (2, "connectivity"),
                (2, "s2"),
                (1, "workloads"),
            ]
        );
        assert_eq!(rows[1].subscription_count, 2, "s1 via connectivity, s2");
        assert_eq!(rows[1].label, "MG platform");
        assert_eq!(rows[3].kind, TreeRowKind::Subscription);
    }

    #[test]
    fn subscription_ids_are_lowercased_and_carry_cli_state() {
        let rows = tree().rows(
            &set(&["root", "platform", "connectivity"]),
            &[known("s1", "Disabled")],
            "",
        );
        let s1 = rows.iter().find(|r| r.key == "s1").unwrap();
        assert_eq!(s1.state.as_deref(), Some("Disabled"));
    }

    #[test]
    fn filter_shows_matches_with_their_ancestors_expanded() {
        let rows = tree().rows(&set(&[]), &[], "sub s1");
        assert_eq!(
            keys(&rows),
            [(0, "root"), (1, "platform"), (2, "connectivity"), (3, "s1")]
        );
        assert!(rows[..3].iter().all(|r| r.expanded));

        let rows = tree().rows(&set(&[]), &[], "workloads");
        assert_eq!(keys(&rows), [(0, "root"), (1, "workloads")]);
        assert!(
            !rows[1].expanded,
            "a matching group with no matching children stays closed"
        );

        assert!(tree().rows(&set(&[]), &[], "nothing-matches").is_empty());
    }

    #[test]
    fn subscriptions_outside_the_tree_get_a_pseudo_group() {
        let known = [known("s1", "Enabled"), known("other", "Enabled")];
        let rows = tree().rows(&set(&[UNPLACED]), &known, "");
        assert_eq!(keys(&rows), [(0, "root"), (0, UNPLACED), (1, "other")]);
        assert_eq!(rows[1].subscription_count, 1);
        assert_eq!(tree().subscriptions_under(UNPLACED, &known), ["other"]);
    }

    #[test]
    fn subscriptions_under_walks_the_whole_subtree() {
        let t = tree();
        let mut all = t.subscriptions_under("root", &[]);
        all.sort();
        assert_eq!(all, ["s1", "s2", "s3", "s4"]);
        let mut platform = t.subscriptions_under("platform", &[]);
        platform.sort();
        assert_eq!(platform, ["s1", "s2"]);
        assert!(t.subscriptions_under("missing", &[]).is_empty());
    }

    #[test]
    fn a_parent_cycle_does_not_hang() {
        let t = ScopeTree::from_nodes("root", "Root", &[mg("a", "b"), mg("b", "a")]);
        assert_eq!(
            keys(&t.rows(&set(&["root", "a", "b"]), &[], "")),
            [(0, "root")]
        );
        assert!(t.subscriptions_under("a", &[]).is_empty());
    }

    #[test]
    fn serializes_round_trip() {
        let t = tree();
        let json = serde_json::to_string(&t).unwrap();
        assert_eq!(serde_json::from_str::<ScopeTree>(&json).unwrap(), t);
    }
}
