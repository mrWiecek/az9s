//! Azure Policy: which assignments apply at a scope, and how compliant the
//! resources there are.
//!
//! An assignment applies at its own scope and every scope below it, so a
//! subscription is governed by assignments on itself and on each management
//! group above it. The management group tree supplies those ancestors.

use crate::scope_tree::ScopeTree;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyAssignment {
    pub id: String,
    pub name: String,
    pub display_name: String,
    /// ARM ID of the scope it is assigned at.
    pub scope: String,
    /// The policy definition, or set definition (initiative), it assigns.
    pub definition_id: String,
    /// "Default" (enforced) or "DoNotEnforce".
    pub enforcement: String,
    pub description: String,
}

impl PolicyAssignment {
    /// From a row of `resource_graph::policy_assignments_query`.
    pub fn from_row(row: &Value) -> Option<Self> {
        let text = |key: &str| {
            row.get(key)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        let id = text("id");
        if id.is_empty() {
            return None;
        }
        let name = text("name");
        let display_name = Some(text("displayName"))
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| name.clone());
        Some(Self {
            id,
            name,
            display_name,
            scope: text("scope"),
            definition_id: text("definitionId"),
            enforcement: Some(text("enforcementMode"))
                .filter(|e| !e.is_empty())
                .unwrap_or_else(|| "Default".into()),
            description: text("description"),
        })
    }

    /// Whether it assigns an initiative (a policy set) rather than one policy.
    pub fn is_initiative(&self) -> bool {
        self.definition_id
            .to_ascii_lowercase()
            .contains("/policysetdefinitions/")
    }

    pub fn is_enforced(&self) -> bool {
        !self.enforcement.eq_ignore_ascii_case("DoNotEnforce")
    }
}

/// An exemption: resources at its scope (and below) are exempt from an
/// assignment, or from some of an initiative's policies.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyExemption {
    pub id: String,
    pub name: String,
    pub display_name: String,
    pub assignment_id: String,
    /// "Waiver" or "Mitigated"
    pub category: String,
    /// ISO 8601 date-time, empty when it never expires
    pub expires_on: String,
    pub description: String,
    /// The initiative's policies it covers (empty: the whole assignment)
    pub references: Vec<String>,
}

/// How soon an exemption expires.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Expiry {
    /// Expired this many days ago (0: today)
    Expired(i64),
    /// Expires in this many days
    In(i64),
    Never,
}

/// Exemptions expiring within this many days are flagged.
pub const EXPIRY_WARNING_DAYS: i64 = 30;

impl PolicyExemption {
    /// From a row of `resource_graph::policy_exemptions_query`.
    pub fn from_row(row: &Value) -> Option<Self> {
        let text = |key: &str| {
            row.get(key)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        let id = text("id");
        if id.is_empty() {
            return None;
        }
        let name = text("name");
        Some(Self {
            display_name: Some(text("displayName"))
                .filter(|n| !n.is_empty())
                .unwrap_or_else(|| name.clone()),
            name,
            assignment_id: text("assignmentId"),
            category: text("category"),
            expires_on: text("expiresOn"),
            description: text("description"),
            references: row
                .get("references")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default(),
            id,
        })
    }

    /// The scope it is made at (its ID up to the provider segment).
    pub fn scope(&self) -> &str {
        const MARKER: &str = "/providers/microsoft.authorization/policyexemptions/";
        match self.id.to_ascii_lowercase().find(MARKER) {
            Some(0) | None => "/",
            Some(i) => &self.id[..i],
        }
    }

    /// How soon it expires, as of `now` (unix seconds).
    pub fn expiry(&self, now: i64) -> Expiry {
        let Ok(at) = chrono::DateTime::parse_from_rfc3339(&self.expires_on) else {
            return Expiry::Never;
        };
        // Whole days, rounded down.
        let at = at.timestamp();
        if at <= now {
            Expiry::Expired((now - at) / 86_400)
        } else {
            Expiry::In((at - now) / 86_400)
        }
    }

    /// Whether it exempts `state`: same assignment, the resource at or below
    /// its scope, and (for a partial exemption) one of its policies.
    pub fn covers(&self, state: &PolicyState) -> bool {
        let scope = self.scope().to_ascii_lowercase();
        let resource = state.resource_id.to_ascii_lowercase();
        self.assignment_id
            .eq_ignore_ascii_case(&state.assignment_id)
            && (resource == scope || resource.starts_with(&format!("{scope}/")))
            && (self.references.is_empty()
                || self
                    .references
                    .iter()
                    .any(|r| r.eq_ignore_ascii_case(&state.definition)))
    }
}

/// "expired 3 days ago", "expires in 12 days", "never expires".
pub fn expiry_label(expiry: Expiry) -> String {
    let days = |n: i64| {
        if n == 1 {
            "1 day".to_string()
        } else {
            format!("{n} days")
        }
    };
    match expiry {
        Expiry::Expired(0) => "expired today".into(),
        Expiry::Expired(n) => format!("expired {} ago", days(n)),
        Expiry::In(0) => "expires today".into(),
        Expiry::In(n) => format!("expires in {}", days(n)),
        Expiry::Never => "never expires".into(),
    }
}

/// Whether an ARM ID is `target` or inside it: for a management group, any
/// group or subscription below it (and what is inside those).
pub fn within(arm_id: &str, target: &PolicyScope, tree: Option<&ScopeTree>) -> bool {
    let id = arm_id.to_ascii_lowercase();
    let inside = |root: &str| id == root || id.starts_with(&format!("{root}/"));
    let target_id = target.arm_id();
    if inside(&target_id) {
        return true;
    }
    match (target, tree) {
        (PolicyScope::Group(g), Some(tree)) => {
            descendant_groups(tree, g)
                .into_iter()
                .any(|d| inside(&group_id(&d)))
                || tree
                    .subscriptions_under(g, &[])
                    .into_iter()
                    .any(|s| inside(&PolicyScope::Subscription(s).arm_id()))
        }
        _ => false,
    }
}

/// Where policy is looked at: a management group or a subscription.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PolicyScope {
    /// Management group name (the tenant root group's name is the tenant ID)
    Group(String),
    /// Subscription ID
    Subscription(String),
}

impl PolicyScope {
    /// The scope's ARM ID, lowercase.
    pub fn arm_id(&self) -> String {
        match self {
            PolicyScope::Group(g) => group_id(g),
            PolicyScope::Subscription(s) => format!("/subscriptions/{}", s.to_ascii_lowercase()),
        }
    }
}

fn group_id(name: &str) -> String {
    format!(
        "/providers/microsoft.management/managementgroups/{}",
        name.to_ascii_lowercase()
    )
}

/// Resources evaluated, and those non-compliant, for one assignment.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Compliance {
    pub non_compliant: u64,
    pub total: u64,
}

/// Counts per assignment (lowercase ID) from rows of
/// `resource_graph::policy_compliance_query`.
pub fn compliance_from_rows(rows: &[Value]) -> HashMap<String, Compliance> {
    rows.iter()
        .filter_map(|row| {
            let id = row.get("assignment")?.as_str()?.to_ascii_lowercase();
            let num = |key: &str| row.get(key).and_then(Value::as_u64).unwrap_or(0);
            Some((
                id,
                Compliance {
                    non_compliant: num("nonCompliant"),
                    total: num("total"),
                },
            ))
        })
        .collect()
}

/// Resources evaluated, and those non-compliant with at least one policy,
/// per subscription (lowercase ID), from rows of
/// `resource_graph::compliance_summary_query`.
pub fn subscription_compliance_from_rows(rows: &[Value]) -> HashMap<String, Compliance> {
    rows.iter()
        .filter_map(|row| {
            let id = row.get("subscription")?.as_str()?.to_ascii_lowercase();
            let num = |key: &str| row.get(key).and_then(Value::as_u64).unwrap_or(0);
            Some((
                id,
                Compliance {
                    non_compliant: num("nonCompliant"),
                    total: num("resources"),
                },
            ))
        })
        .collect()
}

/// The compliance of a whole scope, summed from its subscriptions.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ComplianceSummary {
    /// Distinct resources evaluated / non-compliant with at least one policy
    pub resources: Compliance,
    /// Subscriptions with at least one non-compliant resource
    pub subscriptions_affected: usize,
    /// Subscriptions with evaluated resources
    pub subscriptions: usize,
}

impl ComplianceSummary {
    pub fn from_subscriptions(by_subscription: &HashMap<String, Compliance>) -> Self {
        let mut summary = Self::default();
        for c in by_subscription.values().filter(|c| c.total > 0) {
            summary.resources.total += c.total;
            summary.resources.non_compliant += c.non_compliant;
            summary.subscriptions += 1;
            summary.subscriptions_affected += usize::from(c.non_compliant > 0);
        }
        summary
    }

    /// Share of evaluated resources that are compliant, in percent (`None`
    /// when nothing was evaluated).
    pub fn percent_compliant(&self) -> Option<f64> {
        let r = self.resources;
        (r.total > 0)
            .then(|| 100.0 * (r.total - r.non_compliant.min(r.total)) as f64 / r.total as f64)
    }
}

/// One policy evaluation: a resource against one policy of an assignment.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyState {
    pub resource_id: String,
    pub resource_type: String,
    pub resource_group: String,
    pub subscription_id: String,
    pub location: String,
    pub assignment_id: String,
    /// The policy within the assignment (its reference ID in an initiative,
    /// or the definition's name).
    pub definition: String,
    /// The policy definition's ARM ID
    #[serde(default)]
    pub definition_id: String,
    /// The initiative's ARM ID, when the policy is part of one
    #[serde(default)]
    pub set_definition_id: String,
    /// The policy definition's display name, when looked up
    #[serde(default)]
    pub definition_name: String,
    /// The initiative's display name, when looked up
    #[serde(default)]
    pub initiative_name: String,
    /// "Compliant", "NonCompliant", "Exempt", "Unknown", …
    pub state: String,
    pub timestamp: String,
}

impl PolicyState {
    pub fn from_row(row: &Value) -> Option<Self> {
        let text = |key: &str| {
            row.get(key)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        let resource_id = text("resourceId");
        if resource_id.is_empty() {
            return None;
        }
        Some(Self {
            resource_id,
            resource_type: text("resourceType"),
            resource_group: text("resourceGroup"),
            subscription_id: text("subscriptionId"),
            location: text("location"),
            assignment_id: text("assignmentId"),
            definition: text("definition"),
            definition_id: text("definitionId"),
            set_definition_id: text("setDefinitionId"),
            definition_name: String::new(),
            initiative_name: String::new(),
            state: text("state"),
            timestamp: text("timestamp"),
        })
    }

    /// The policy came with an initiative rather than assigned on its own.
    pub fn via_initiative(&self) -> bool {
        !self.set_definition_id.is_empty()
    }

    /// The policy's name to show: its display name when known, else its
    /// reference ID or definition name.
    pub fn policy_label(&self) -> &str {
        [&self.definition_name, &self.definition]
            .into_iter()
            .find(|s| !s.is_empty())
            .map(String::as_str)
            .unwrap_or_else(|| last_segment(&self.definition_id))
    }

    /// "initiative <name>" or "direct".
    pub fn source_label(&self) -> String {
        if !self.via_initiative() {
            return "direct".into();
        }
        let name = if self.initiative_name.is_empty() {
            last_segment(&self.set_definition_id)
        } else {
            &self.initiative_name
        };
        format!("initiative {name}")
    }

    pub fn is_non_compliant(&self) -> bool {
        self.state.eq_ignore_ascii_case("NonCompliant")
    }

    /// The resource's name: the last segment of its ID.
    pub fn resource_name(&self) -> &str {
        self.resource_id
            .rsplit('/')
            .next()
            .unwrap_or(&self.resource_id)
    }
}

fn last_segment(id: &str) -> &str {
    id.rsplit('/').next().unwrap_or(id)
}

/// The scope (ARM ID) an assignment is made at: its ID up to
/// `/providers/Microsoft.Authorization/policyAssignments/`.
pub fn assignment_scope(assignment_id: &str) -> &str {
    const MARKER: &str = "/providers/microsoft.authorization/policyassignments/";
    match assignment_id.to_ascii_lowercase().find(MARKER) {
        Some(0) => "/",
        Some(i) => &assignment_id[..i],
        None => assignment_id,
    }
}

/// A non-compliant resource and the policies it fails (one assignment).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NonCompliantResource {
    pub state: PolicyState,
    /// Failing policies of the assignment, sorted, without duplicates.
    pub policies: Vec<String>,
}

/// One row per resource from an assignment's non-compliant states (several
/// rows per resource when an initiative has several failing policies).
pub fn group_by_resource(states: &[PolicyState]) -> Vec<NonCompliantResource> {
    let mut by_id: HashMap<String, NonCompliantResource> = HashMap::new();
    for s in states.iter().filter(|s| s.is_non_compliant()) {
        let entry = by_id
            .entry(s.resource_id.to_ascii_lowercase())
            .or_insert_with(|| NonCompliantResource {
                state: s.clone(),
                policies: vec![],
            });
        if !s.definition.is_empty() && !entry.policies.contains(&s.definition) {
            entry.policies.push(s.definition.clone());
        }
    }
    let mut rows: Vec<NonCompliantResource> = by_id.into_values().collect();
    for r in &mut rows {
        r.policies.sort();
    }
    rows.sort_by_cached_key(|r| r.state.resource_name().to_lowercase());
    rows
}

/// How an assignment comes to apply at a scope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Origin {
    /// Assigned at this very scope
    Here,
    /// Assigned at an ancestor (its scope's ARM ID)
    Inherited(String),
}

/// The scope's ancestors' ARM IDs (lowercase), the scope itself first, then
/// up to the tenant root. Without a tree, only the scope itself.
pub fn ancestors(target: &PolicyScope, tree: Option<&ScopeTree>) -> Vec<String> {
    let mut chain = vec![target.arm_id()];
    let Some(tree) = tree else {
        return chain;
    };
    let parent_of_group: HashMap<String, &str> = tree
        .groups
        .iter()
        .map(|g| (g.name.to_ascii_lowercase(), g.parent.as_str()))
        .collect();
    let mut next = match target {
        PolicyScope::Group(g) => parent_of_group.get(&g.to_ascii_lowercase()).copied(),
        PolicyScope::Subscription(s) => tree
            .subscriptions
            .iter()
            .find(|n| n.id.eq_ignore_ascii_case(s))
            .map(|n| n.parent.as_str()),
    };
    let mut seen = HashSet::new();
    while let Some(group) = next {
        if !seen.insert(group.to_ascii_lowercase()) {
            break; // a cycle in bad data
        }
        chain.push(group_id(group));
        next = parent_of_group.get(&group.to_ascii_lowercase()).copied();
    }
    chain
}

/// Assignments that apply at `target`, nearest scope first, each with its
/// origin.
pub fn applicable<'a>(
    assignments: &'a [PolicyAssignment],
    target: &PolicyScope,
    tree: Option<&ScopeTree>,
) -> Vec<(&'a PolicyAssignment, Origin)> {
    let chain = ancestors(target, tree);
    let mut found: Vec<(usize, &PolicyAssignment, Origin)> = assignments
        .iter()
        .filter_map(|a| {
            let scope = a.scope.to_ascii_lowercase();
            let depth = chain.iter().position(|c| *c == scope)?;
            let origin = if depth == 0 {
                Origin::Here
            } else {
                Origin::Inherited(scope)
            };
            Some((depth, a, origin))
        })
        .collect();
    found.sort_by_key(|(depth, _, _)| *depth);
    found.into_iter().map(|(_, a, o)| (a, o)).collect()
}

/// How many assignments sit below `target` (on child management groups,
/// subscriptions, resource groups or resources): they apply only to part of
/// it, so they are shown at those scopes instead.
pub fn assigned_below(
    assignments: &[PolicyAssignment],
    target: &PolicyScope,
    tree: Option<&ScopeTree>,
) -> usize {
    // Scopes strictly below the target: an assignment is below if its scope
    // is one of them or inside one of them.
    let roots: Vec<String> = match (target, tree) {
        (PolicyScope::Subscription(s), _) => {
            // Only what is inside the subscription (resource groups, resources).
            let sub = PolicyScope::Subscription(s.clone()).arm_id();
            return assignments
                .iter()
                .filter(|a| a.scope.to_ascii_lowercase().starts_with(&format!("{sub}/")))
                .count();
        }
        (PolicyScope::Group(g), Some(tree)) => {
            let mut roots: Vec<String> = descendant_groups(tree, g)
                .into_iter()
                .map(|d| group_id(&d))
                .collect();
            roots.extend(
                tree.subscriptions_under(g, &[])
                    .into_iter()
                    .map(|s| PolicyScope::Subscription(s).arm_id()),
            );
            roots
        }
        (PolicyScope::Group(_), None) => return 0,
    };
    assignments
        .iter()
        .filter(|a| {
            let scope = a.scope.to_ascii_lowercase();
            roots
                .iter()
                .any(|r| scope == *r || scope.starts_with(&format!("{r}/")))
        })
        .count()
}

/// Management groups strictly below `group` (names).
fn descendant_groups(tree: &ScopeTree, group: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![group.to_ascii_lowercase()];
    let mut seen = HashSet::new();
    while let Some(g) = stack.pop() {
        if !seen.insert(g.clone()) {
            continue;
        }
        for child in tree
            .groups
            .iter()
            .filter(|c| c.parent.eq_ignore_ascii_case(&g))
        {
            out.push(child.name.clone());
            stack.push(child.name.to_ascii_lowercase());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scope_tree::{GroupNode, SubscriptionNode};
    use serde_json::json;

    /// root → platform → connectivity (sub-c); root → workloads (sub-w)
    fn tree() -> ScopeTree {
        let g = |name: &str, parent: &str| GroupNode {
            name: name.into(),
            display_name: name.to_uppercase(),
            parent: parent.into(),
        };
        let s = |id: &str, parent: &str| SubscriptionNode {
            id: id.into(),
            display_name: id.into(),
            parent: parent.into(),
        };
        ScopeTree {
            root: "root".into(),
            root_display_name: "Tenant Root Group".into(),
            groups: vec![
                g("platform", "root"),
                g("connectivity", "platform"),
                g("workloads", "root"),
            ],
            subscriptions: vec![s("sub-c", "connectivity"), s("sub-w", "workloads")],
        }
    }

    fn at(name: &str, scope: &str) -> PolicyAssignment {
        PolicyAssignment {
            id: format!("{scope}/providers/Microsoft.Authorization/policyAssignments/{name}"),
            name: name.into(),
            display_name: name.into(),
            scope: scope.into(),
            definition_id: "/providers/Microsoft.Authorization/policyDefinitions/x".into(),
            enforcement: "Default".into(),
            description: String::new(),
        }
    }

    fn mg(name: &str) -> String {
        format!("/providers/Microsoft.Management/managementGroups/{name}")
    }

    fn assignments() -> Vec<PolicyAssignment> {
        vec![
            at("tenant-wide", &mg("root")),
            at("platform-only", &mg("Platform")),
            at("conn", &mg("connectivity")),
            at("workloads", &mg("workloads")),
            at("on-sub-c", "/subscriptions/SUB-C"),
            at("on-rg", "/subscriptions/sub-c/resourceGroups/rg-hub"),
        ]
    }

    fn names(found: &[(&PolicyAssignment, Origin)]) -> Vec<String> {
        found.iter().map(|(a, _)| a.name.clone()).collect()
    }

    #[test]
    fn a_subscription_gets_its_own_and_every_ancestors_assignments() {
        let all = assignments();
        let found = applicable(
            &all,
            &PolicyScope::Subscription("sub-c".into()),
            Some(&tree()),
        );
        assert_eq!(
            names(&found),
            ["on-sub-c", "conn", "platform-only", "tenant-wide"]
        );
        assert_eq!(found[0].1, Origin::Here);
        assert_eq!(
            found[1].1,
            Origin::Inherited(mg("connectivity").to_ascii_lowercase())
        );
    }

    #[test]
    fn a_group_gets_its_own_and_its_ancestors_not_its_childrens() {
        let all = assignments();
        let found = applicable(&all, &PolicyScope::Group("platform".into()), Some(&tree()));
        assert_eq!(names(&found), ["platform-only", "tenant-wide"]);
        assert_eq!(
            assigned_below(&all, &PolicyScope::Group("platform".into()), Some(&tree())),
            3,
            "conn, on-sub-c, on-rg"
        );
        assert_eq!(
            assigned_below(
                &all,
                &PolicyScope::Subscription("sub-c".into()),
                Some(&tree())
            ),
            1,
            "on-rg"
        );
    }

    #[test]
    fn without_a_tree_only_direct_assignments_are_known() {
        let all = assignments();
        let found = applicable(&all, &PolicyScope::Subscription("sub-c".into()), None);
        assert_eq!(names(&found), ["on-sub-c"]);
    }

    #[test]
    fn exemptions_expire_cover_states_and_sit_within_scopes() {
        let day = 86_400;
        let now = 1_700_000_000;
        let iso = |t: i64| chrono::DateTime::from_timestamp(t, 0).unwrap().to_rfc3339();
        let mut e = PolicyExemption {
            id: "/subscriptions/S1/resourceGroups/rg/providers/Microsoft.Authorization/policyExemptions/x".into(),
            assignment_id: "/a/policyAssignments/A1".into(),
            ..Default::default()
        };
        assert_eq!(e.scope(), "/subscriptions/S1/resourceGroups/rg");
        assert_eq!(e.expiry(now), Expiry::Never);
        e.expires_on = iso(now - 3 * day - 60);
        assert_eq!(e.expiry(now), Expiry::Expired(3));
        e.expires_on = iso(now + 10 * day + 60);
        assert_eq!(e.expiry(now), Expiry::In(10));
        assert_eq!(expiry_label(Expiry::In(1)), "expires in 1 day");
        assert_eq!(expiry_label(Expiry::Expired(0)), "expired today");
        assert!(Expiry::Expired(9) < Expiry::In(0) && Expiry::In(400) < Expiry::Never);

        let state = |resource: &str, definition: &str| PolicyState {
            resource_id: resource.into(),
            assignment_id: "/A/policyassignments/a1".into(),
            definition: definition.into(),
            ..Default::default()
        };
        let inside = "/subscriptions/s1/resourcegroups/RG/providers/x/y/kv";
        assert!(e.covers(&state(inside, "p1")), "whole assignment, any case");
        assert!(!e.covers(&state("/subscriptions/s1/resourceGroups/rg2/x", "p1")));
        assert!(
            !e.covers(&state("/subscriptions/s1/resourceGroups/rgx", "p1")),
            "not a prefix match"
        );
        e.references = vec!["P2".into()];
        assert!(!e.covers(&state(inside, "p1")) && e.covers(&state(inside, "p2")));

        let tree = crate::scope_tree::ScopeTree {
            root: "root".into(),
            root_display_name: "Root".into(),
            groups: vec![crate::scope_tree::GroupNode {
                name: "corp".into(),
                display_name: "Corp".into(),
                parent: "root".into(),
            }],
            subscriptions: vec![crate::scope_tree::SubscriptionNode {
                id: "s1".into(),
                display_name: "s1".into(),
                parent: "corp".into(),
            }],
        };
        let root = PolicyScope::Group("root".into());
        assert!(
            within(e.scope(), &root, Some(&tree)),
            "a subscription's group below root"
        );
        assert!(within(&group_id("corp"), &root, Some(&tree)));
        assert!(!within(
            e.scope(),
            &PolicyScope::Subscription("s2".into()),
            Some(&tree)
        ));
        assert!(within(
            e.scope(),
            &PolicyScope::Subscription("S1".into()),
            None
        ));
    }

    #[test]
    fn assignment_scope_and_labels() {
        assert_eq!(
            assignment_scope(
                "/subscriptions/S/providers/Microsoft.Authorization/policyAssignments/x"
            ),
            "/subscriptions/S"
        );
        assert_eq!(
            assignment_scope("/providers/Microsoft.Authorization/policyAssignments/x"),
            "/"
        );
        let mut s = PolicyState {
            definition: "ref-1".into(),
            definition_id: "/p/policyDefinitions/guid".into(),
            ..Default::default()
        };
        assert_eq!(
            (s.policy_label(), s.source_label().as_str()),
            ("ref-1", "direct")
        );
        s.definition.clear();
        assert_eq!(
            s.policy_label(),
            "guid",
            "the ID's last part as a last resort"
        );
        s.set_definition_id = "/p/policySetDefinitions/mcsb".into();
        assert_eq!(s.source_label(), "initiative mcsb");
        s.initiative_name = "Security Benchmark".into();
        assert_eq!(s.source_label(), "initiative Security Benchmark");
    }

    #[test]
    fn compliance_summary_sums_subscriptions() {
        let rows = vec![
            serde_json::json!({"subscription": "S1", "resources": 100, "nonCompliant": 10}),
            serde_json::json!({"subscription": "s2", "resources": 50, "nonCompliant": 0}),
            serde_json::json!({"subscription": "s3", "resources": 0, "nonCompliant": 0}),
            serde_json::json!({"resources": 5}),
        ];
        let by_sub = subscription_compliance_from_rows(&rows);
        assert_eq!(by_sub.len(), 3, "rows without a subscription are skipped");
        assert_eq!(by_sub["s1"].total, 100, "keys are lowercase");
        let summary = ComplianceSummary::from_subscriptions(&by_sub);
        assert_eq!(
            summary.resources,
            Compliance {
                non_compliant: 10,
                total: 150
            }
        );
        assert_eq!(
            (summary.subscriptions, summary.subscriptions_affected),
            (2, 1),
            "subscriptions without evaluated resources don't count"
        );
        assert!((summary.percent_compliant().unwrap() - 93.33).abs() < 0.01);
        assert_eq!(ComplianceSummary::default().percent_compliant(), None);
    }

    #[test]
    fn non_compliant_states_group_per_resource() {
        let state = |id: &str, def: &str, st: &str| PolicyState {
            resource_id: id.into(),
            resource_type: "microsoft.storage/storageaccounts".into(),
            resource_group: "rg".into(),
            subscription_id: "s".into(),
            location: "westeurope".into(),
            assignment_id: "/a".into(),
            definition: def.into(),
            state: st.into(),
            ..Default::default()
        };
        let rows = group_by_resource(&[
            state("/x/storageAccounts/zeta", "https-only", "NonCompliant"),
            state("/x/storageAccounts/alpha", "tls12", "NonCompliant"),
            state("/X/storageAccounts/ZETA", "no-public-blob", "NonCompliant"),
            state("/x/storageAccounts/zeta", "https-only", "NonCompliant"),
            state("/x/storageAccounts/fine", "tls12", "Compliant"),
        ]);
        let summary: Vec<(&str, Vec<String>)> = rows
            .iter()
            .map(|r| (r.state.resource_name(), r.policies.clone()))
            .collect();
        assert_eq!(
            summary,
            [
                ("alpha", vec!["tls12".to_string()]),
                (
                    "zeta",
                    vec!["https-only".to_string(), "no-public-blob".to_string()]
                ),
            ]
        );
        let parsed = PolicyState::from_row(&json!({
            "resourceId": "/x/vaults/kv", "state": "NonCompliant", "definition": "d"
        }))
        .unwrap();
        assert!(parsed.is_non_compliant());
        assert_eq!(parsed.resource_name(), "kv");
        assert!(PolicyState::from_row(&json!({"state": "x"})).is_none());
    }

    #[test]
    fn rows_parse() {
        let a = PolicyAssignment::from_row(&json!({
            "id": "/x/policyAssignments/a1", "name": "a1", "displayName": "",
            "scope": "/subscriptions/s",
            "definitionId": "/providers/Microsoft.Authorization/policySetDefinitions/asb",
            "enforcementMode": "DoNotEnforce", "description": "d"
        }))
        .unwrap();
        assert_eq!(a.display_name, "a1", "falls back to the name");
        assert!(a.is_initiative() && !a.is_enforced());
        assert!(PolicyAssignment::from_row(&json!({"name": "no id"})).is_none());

        let counts = compliance_from_rows(&[
            json!({"assignment": "/X/A1", "total": 10, "nonCompliant": 3}),
            json!({"total": 1}),
        ]);
        assert_eq!(
            counts["/x/a1"],
            Compliance {
                non_compliant: 3,
                total: 10
            }
        );
        assert_eq!(counts.len(), 1);
    }
}
