//! Structured resource filters: `type=…`, `loc=…`, `tag=…`, as typed after
//! `:res`. They narrow the loaded list on the client, so they apply at once,
//! to cached rows too, and combine with the `/` text filter.
//!
//! - Different keys must all match (AND). Comma-separated values of one key
//!   are alternatives (OR): `type=virtualmachines,disks`.
//! - `type=` matches the full type (`Microsoft.Compute/virtualMachines`), its
//!   last segment (`virtualMachines`), a provider (`Microsoft.Compute`), or
//!   its short alias (`vm`, `vnet`, `nsg`; see `resource_type`).
//! - `loc=` matches the location exactly. `tag=env:prod` (or `tag=env=prod`)
//!   matches a tag key and value; `tag=env` matches any resource that has the key.
//! - Everything is case-insensitive.

use crate::models::{Resource, ResourceGroup};
use std::collections::HashMap;
use std::fmt;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResourceQuery {
    pub types: Vec<String>,
    pub locations: Vec<String>,
    /// (key, optional value)
    pub tags: Vec<(String, Option<String>)>,
}

impl ResourceQuery {
    pub fn is_empty(&self) -> bool {
        self.types.is_empty() && self.locations.is_empty() && self.tags.is_empty()
    }

    /// Whether `term` looks like a filter (`key=value`) rather than a scope.
    pub fn is_filter(term: &str) -> bool {
        term.contains('=')
    }

    /// Parse filter terms such as `["type=vm,disks", "loc=westeurope"]`.
    pub fn parse<'a>(terms: impl IntoIterator<Item = &'a str>) -> Result<Self, String> {
        let mut query = Self::default();
        for term in terms {
            let (key, values) = term
                .split_once('=')
                .ok_or_else(|| format!("'{term}' is not a filter (expected key=value)"))?;
            let values: Vec<&str> = values
                .split(',')
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .collect();
            if values.is_empty() {
                return Err(format!("'{term}' has no value"));
            }
            match key.to_ascii_lowercase().as_str() {
                "type" => query.types.extend(values.iter().map(|v| v.to_string())),
                "loc" | "location" => query.locations.extend(values.iter().map(|v| v.to_string())),
                "tag" => query
                    .tags
                    .extend(values.iter().map(|v| match v.split_once([':', '=']) {
                        // tag=env:prod and tag=env=prod both mean key env, value prod
                        // (split at the first separator: values may contain more).
                        Some((k, val)) => (k.to_string(), Some(val.to_string())),
                        None => (v.to_string(), None),
                    })),
                other => {
                    return Err(format!(
                        "unknown filter '{other}' (use type=, loc= or tag=)"
                    ))
                }
            }
        }
        Ok(query)
    }

    pub fn matches(&self, resource: &Resource) -> bool {
        (self.types.is_empty()
            || self
                .types
                .iter()
                .any(|t| type_matches(&resource.resource_type, t)))
            && self.matches_place(&resource.location, &resource.tags)
    }

    /// For resource groups: `loc=` and `tag=` (`type=` is rejected when
    /// parsing `:rg`).
    pub fn matches_group(&self, group: &ResourceGroup) -> bool {
        self.matches_place(&group.location, &group.tags)
    }

    fn matches_place(&self, location: &str, tags: &HashMap<String, String>) -> bool {
        (self.locations.is_empty()
            || self
                .locations
                .iter()
                .any(|l| l.eq_ignore_ascii_case(location)))
            && (self.tags.is_empty()
                || self.tags.iter().any(|(key, value)| {
                    tags.iter().any(|(k, v)| {
                        k.eq_ignore_ascii_case(key)
                            && value
                                .as_ref()
                                .is_none_or(|want| want.eq_ignore_ascii_case(v))
                    })
                }))
    }
}

fn type_matches(resource_type: &str, wanted: &str) -> bool {
    let full = resource_type.to_ascii_lowercase();
    let wanted = wanted.to_ascii_lowercase();
    // Dashes and spaces don't matter for aliases: key-vault, keyvault.
    let squash = |s: &str| s.replace(['-', ' '], "");
    full == wanted
        || full.rsplit('/').next() == Some(wanted.as_str())
        || full.starts_with(&format!("{wanted}/"))
        // A provider without "Microsoft.": type=keyvault, type=network
        || full.starts_with(&format!("microsoft.{wanted}/"))
        || squash(&crate::resource_type::short_label(resource_type)) == squash(&wanted)
}

/// The filter as typed: `type=a,b loc=c tag=k:v`. Parses back to the same query.
impl fmt::Display for ResourceQuery {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut parts = Vec::new();
        if !self.types.is_empty() {
            parts.push(format!("type={}", self.types.join(",")));
        }
        if !self.locations.is_empty() {
            parts.push(format!("loc={}", self.locations.join(",")));
        }
        if !self.tags.is_empty() {
            let tags: Vec<String> = self
                .tags
                .iter()
                .map(|(k, v)| match v {
                    Some(v) => format!("{k}:{v}"),
                    None => k.clone(),
                })
                .collect();
            parts.push(format!("tag={}", tags.join(",")));
        }
        f.write_str(&parts.join(" "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn res(resource_type: &str, location: &str, tags: &[(&str, &str)]) -> Resource {
        Resource {
            id: "id".into(),
            name: "n".into(),
            resource_type: resource_type.into(),
            location: location.into(),
            resource_group: "rg".into(),
            subscription_id: "s".into(),
            tags: tags
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect::<HashMap<_, _>>(),
        }
    }

    fn q(terms: &[&str]) -> ResourceQuery {
        ResourceQuery::parse(terms.iter().copied()).unwrap()
    }

    #[test]
    fn type_matches_full_type_last_segment_or_provider() {
        let vm = res("microsoft.compute/virtualmachines", "westeurope", &[]);
        assert!(q(&["type=Microsoft.Compute/virtualMachines"]).matches(&vm));
        assert!(q(&["type=virtualMachines"]).matches(&vm));
        assert!(q(&["type=microsoft.compute"]).matches(&vm));
        assert!(!q(&["type=disks"]).matches(&vm));
        assert!(!q(&["type=virtual"]).matches(&vm), "no substring matches");
        assert!(q(&["type=vm"]).matches(&vm), "short alias");
        let kv = res("microsoft.keyvault/vaults", "westeurope", &[]);
        for alias in ["key-vault", "keyvault", "KeyVault", "key vault", "vaults"] {
            assert!(q(&[&format!("type={alias}")]).matches(&kv), "{alias}");
        }
        assert!(
            q(&["type=compute"]).matches(&vm),
            "provider without Microsoft."
        );
        assert!(!q(&["type=comp"]).matches(&vm));
        assert!(
            q(&["type=disks,virtualmachines"]).matches(&vm),
            "OR within a key"
        );
    }

    #[test]
    fn location_and_tags() {
        let r = res("x/y", "WestEurope", &[("Env", "Prod"), ("owner", "a")]);
        assert!(q(&["loc=westeurope"]).matches(&r));
        assert!(!q(&["loc=northeurope"]).matches(&r));
        assert!(q(&["tag=env:prod"]).matches(&r));
        assert!(
            q(&["tag=env=prod"]).matches(&r),
            "= works as the separator too"
        );
        assert!(!q(&["tag=env=dev"]).matches(&r));
        assert!(!q(&["tag=env:dev"]).matches(&r));
        assert!(q(&["tag=OWNER"]).matches(&r), "key only: has the tag");
        assert!(!q(&["tag=cost"]).matches(&r));
    }

    #[test]
    fn different_keys_must_all_match() {
        let r = res("x/vm", "westeurope", &[("env", "prod")]);
        assert!(q(&["type=vm", "loc=westeurope", "tag=env:prod"]).matches(&r));
        assert!(!q(&["type=vm", "loc=northeurope"]).matches(&r));
        assert!(ResourceQuery::default().matches(&r));
    }

    #[test]
    fn parse_errors_and_round_trip() {
        assert!(ResourceQuery::parse(["size=big"])
            .unwrap_err()
            .contains("unknown filter"));
        assert!(ResourceQuery::parse(["type="])
            .unwrap_err()
            .contains("no value"));
        let query = q(&["type=vm,disks", "location=westeurope", "tag=env:prod,owner"]);
        assert_eq!(
            query.to_string(),
            "type=vm,disks loc=westeurope tag=env:prod,owner"
        );
        let reparsed = ResourceQuery::parse(query.to_string().split_whitespace()).unwrap();
        assert_eq!(reparsed, query);
    }
}
