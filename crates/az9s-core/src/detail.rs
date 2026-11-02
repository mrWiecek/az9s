//! What the resource detail view shows, derived from the resource's full
//! document (a Resource Graph `resources` row: ARM's JSON plus
//! `subscriptionId` and `resourceGroup`).
//!
//! Everything here also works without the document (not loaded yet, or the
//! load failed), from the list row the user opened.

use crate::models::Resource;
use serde_json::Value;

/// Most property rows the Overview tab lists; the JSON tab has the rest.
pub const MAX_PROPERTY_ROWS: usize = 60;
/// How deep nested `properties` objects are flattened into dotted names.
const MAX_DEPTH: usize = 3;

/// A titled group of label/value rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    pub title: &'static str,
    pub rows: Vec<(String, String)>,
}

/// The Overview tab: identity of the resource, SKU, managed identity, and
/// the scalar values in `properties` (e.g. `hardwareProfile.vmSize`).
pub fn overview(doc: Option<&Value>, resource: &Resource) -> Vec<Section> {
    let text = |key: &str| doc.and_then(|d| d.get(key)).and_then(scalar);
    let mut main = vec![
        (
            "Name".to_string(),
            text("name").unwrap_or(resource.name.clone()),
        ),
        (
            "Type".to_string(),
            text("type").unwrap_or(resource.resource_type.clone()),
        ),
    ];
    if let Some(kind) = text("kind") {
        main.push(("Kind".into(), kind));
    }
    main.push((
        "Location".into(),
        text("location").unwrap_or(resource.location.clone()),
    ));
    if let Some(zones) = doc.and_then(|d| d.get("zones")).and_then(Value::as_array) {
        let zones: Vec<String> = zones.iter().filter_map(scalar).collect();
        if !zones.is_empty() {
            main.push(("Zones".into(), zones.join(", ")));
        }
    }
    main.push(("Resource group".into(), resource.resource_group.clone()));
    main.push(("Subscription".into(), resource.subscription_id.clone()));
    if let Some(state) = doc
        .and_then(|d| d.pointer("/properties/provisioningState"))
        .and_then(scalar)
    {
        main.push(("Provisioning state".into(), state));
    }
    if let Some(managed_by) = text("managedBy") {
        main.push(("Managed by".into(), managed_by));
    }
    main.push(("ID".into(), resource.id.clone()));

    let mut sections = vec![Section {
        title: "Resource",
        rows: main,
    }];
    for (title, key) in [("SKU", "sku"), ("Identity", "identity"), ("Plan", "plan")] {
        let mut rows = Vec::new();
        if let Some(value) = doc.and_then(|d| d.get(key)) {
            flatten("", value, 1, &mut rows);
        }
        if !rows.is_empty() {
            sections.push(Section { title, rows });
        }
    }

    let mut properties = Vec::new();
    if let Some(value) = doc.and_then(|d| d.get("properties")) {
        flatten("", value, 0, &mut properties);
    }
    properties.retain(|(name, _)| name != "provisioningState");
    if properties.len() > MAX_PROPERTY_ROWS {
        let more = properties.len() - MAX_PROPERTY_ROWS;
        properties.truncate(MAX_PROPERTY_ROWS);
        properties.push(("…".into(), format!("{more} more (see the JSON tab)")));
    }
    if !properties.is_empty() {
        sections.push(Section {
            title: "Properties",
            rows: properties,
        });
    }
    sections
}

/// Tags, sorted by key: from the document when loaded, else the list row.
pub fn tags(doc: Option<&Value>, resource: &Resource) -> Vec<(String, String)> {
    let mut tags: Vec<(String, String)> = match doc.and_then(|d| d.get("tags")) {
        Some(Value::Object(map)) => map
            .iter()
            .map(|(k, v)| (k.clone(), scalar(v).unwrap_or_default()))
            .collect(),
        _ => resource
            .tags
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect(),
    };
    tags.sort_by_cached_key(|(k, _)| k.to_lowercase());
    tags
}

/// The JSON tab's text: the document pretty-printed, or a placeholder.
pub fn json_text(doc: Option<&Value>) -> String {
    match doc {
        Some(doc) => serde_json::to_string_pretty(doc).unwrap_or_default(),
        None => "(not loaded yet)".into(),
    }
}

/// Azure Portal link for an ARM ID (resource, resource group, subscription
/// or management group).
pub fn portal_url(arm_id: &str, tenant_id: Option<&str>) -> String {
    let tenant = tenant_id
        .filter(|t| !t.is_empty())
        .map(|t| format!("@{t}"))
        .unwrap_or_default();
    format!("https://portal.azure.com/#{tenant}/resource{arm_id}")
}

/// Text for a JSON scalar; `None` for objects, arrays and null.
fn scalar(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

/// Collect scalar leaves of `value` as `a.b.c` rows. Arrays become a count
/// (or the values, when they are a few scalars).
fn flatten(prefix: &str, value: &Value, depth: usize, out: &mut Vec<(String, String)>) {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                let name = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{prefix}.{key}")
                };
                match child {
                    Value::Object(_) if depth < MAX_DEPTH => flatten(&name, child, depth + 1, out),
                    Value::Object(_) => out.push((name, "{…}".into())),
                    _ => flatten(&name, child, depth + 1, out),
                }
            }
        }
        Value::Array(items) => {
            let scalars: Vec<String> = items.iter().filter_map(scalar).collect();
            let text = if !items.is_empty() && scalars.len() == items.len() && items.len() <= 5 {
                scalars.join(", ")
            } else {
                format!("[{} items]", items.len())
            };
            out.push((prefix.to_string(), text));
        }
        Value::Null => {}
        other => {
            if let Some(text) = scalar(other) {
                out.push((prefix.to_string(), text));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::HashMap;

    fn resource() -> Resource {
        Resource {
            id: "/subscriptions/s1/resourceGroups/rg/providers/Microsoft.Compute/virtualMachines/vm1"
                .into(),
            name: "vm1".into(),
            resource_type: "microsoft.compute/virtualmachines".into(),
            location: "westeurope".into(),
            resource_group: "rg".into(),
            subscription_id: "s1".into(),
            tags: HashMap::from([("env".to_string(), "dev".to_string())]),
        }
    }

    fn vm() -> Value {
        json!({
            "name": "vm1",
            "type": "microsoft.compute/virtualmachines",
            "location": "westeurope",
            "zones": ["1", "2"],
            "sku": null,
            "identity": {"type": "SystemAssigned", "principalId": "p-1"},
            "tags": {"Owner": "team-a", "env": "prod"},
            "properties": {
                "provisioningState": "Succeeded",
                "hardwareProfile": {"vmSize": "Standard_D2s_v5"},
                "storageProfile": {"osDisk": {"osType": "Linux", "managedDisk": {"id": "/x"}}},
                "networkProfile": {"networkInterfaces": [{"id": "/nic"}]},
                "priority": "Regular",
                "a": {"b": {"c": {"d": {"e": 1}}}},
                "licenseType": null
            }
        })
    }

    fn rows<'a>(sections: &'a [Section], title: &str) -> &'a [(String, String)] {
        &sections.iter().find(|s| s.title == title).unwrap().rows
    }

    fn get<'a>(rows: &'a [(String, String)], label: &str) -> Option<&'a str> {
        rows.iter()
            .find(|(l, _)| l == label)
            .map(|(_, v)| v.as_str())
    }

    #[test]
    fn overview_without_a_document_uses_the_list_row() {
        let sections = overview(None, &resource());
        assert_eq!(sections.len(), 1);
        let main = rows(&sections, "Resource");
        assert_eq!(get(main, "Name"), Some("vm1"));
        assert_eq!(get(main, "Resource group"), Some("rg"));
        assert!(get(main, "Provisioning state").is_none());
    }

    #[test]
    fn overview_flattens_properties_identity_and_zones() {
        let doc = vm();
        let sections = overview(Some(&doc), &resource());
        let main = rows(&sections, "Resource");
        assert_eq!(get(main, "Zones"), Some("1, 2"));
        assert_eq!(get(main, "Provisioning state"), Some("Succeeded"));

        let identity = rows(&sections, "Identity");
        assert_eq!(get(identity, "type"), Some("SystemAssigned"));
        assert!(
            sections.iter().all(|s| s.title != "SKU"),
            "null sku is skipped"
        );

        let props = rows(&sections, "Properties");
        assert_eq!(
            get(props, "hardwareProfile.vmSize"),
            Some("Standard_D2s_v5")
        );
        assert_eq!(get(props, "storageProfile.osDisk.osType"), Some("Linux"));
        assert_eq!(
            get(props, "storageProfile.osDisk.managedDisk.id"),
            Some("/x")
        );
        assert_eq!(
            get(props, "a.b.c.d"),
            Some("{…}"),
            "deeper objects are summarized"
        );
        assert_eq!(
            get(props, "networkProfile.networkInterfaces"),
            Some("[1 items]")
        );
        assert!(get(props, "provisioningState").is_none(), "shown above");
        assert!(get(props, "licenseType").is_none(), "nulls are skipped");
    }

    #[test]
    fn many_properties_are_capped_with_a_pointer_to_json() {
        let props: serde_json::Map<String, Value> =
            (0..100).map(|i| (format!("p{i:03}"), json!(i))).collect();
        let doc = json!({ "properties": props });
        let sections = overview(Some(&doc), &resource());
        let props = rows(&sections, "Properties");
        assert_eq!(props.len(), MAX_PROPERTY_ROWS + 1);
        assert_eq!(props.last().unwrap().1, "40 more (see the JSON tab)");
    }

    #[test]
    fn tags_prefer_the_document_and_sort_case_insensitively() {
        let doc = vm();
        assert_eq!(
            tags(Some(&doc), &resource()),
            [
                ("env".to_string(), "prod".to_string()),
                ("Owner".to_string(), "team-a".to_string())
            ]
        );
        assert_eq!(
            tags(None, &resource()),
            [("env".to_string(), "dev".to_string())]
        );
    }

    #[test]
    fn portal_urls() {
        assert_eq!(
            portal_url("/subscriptions/s1", Some("t1")),
            "https://portal.azure.com/#@t1/resource/subscriptions/s1"
        );
        assert_eq!(
            portal_url("/subscriptions/s1", None),
            "https://portal.azure.com/#/resource/subscriptions/s1"
        );
    }
}
