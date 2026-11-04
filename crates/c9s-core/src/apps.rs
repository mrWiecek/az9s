//! Applications as one item: an app registration and its enterprise
//! application (service principal), joined by app ID. Either may be
//! missing: Microsoft's and other tenants' apps and managed identities have
//! no registration here; a new registration may have no enterprise app yet.

use crate::policy::Expiry;
pub use c9s_azure::apps::{
    AppDetail, Application, Credential, ResourceApi, ServicePrincipal, MICROSOFT_TENANT,
};
use std::collections::HashMap;

/// What kind of application a row is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AppKind {
    /// Registered in this tenant
    Registered,
    /// Only an enterprise application here (another tenant's app)
    External,
    /// Microsoft's own application
    Microsoft,
    ManagedIdentity,
}

impl AppKind {
    pub fn label(self) -> &'static str {
        match self {
            AppKind::Registered => "registered",
            AppKind::External => "external",
            AppKind::Microsoft => "Microsoft",
            AppKind::ManagedIdentity => "managed identity",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppRow {
    pub app: Option<Application>,
    pub sp: Option<ServicePrincipal>,
    pub name: String,
    pub app_id: String,
    pub kind: AppKind,
    /// The soonest credential to expire (registration's and enterprise
    /// app's), if any
    pub soonest: Option<(Expiry, Credential)>,
}

impl AppRow {
    /// The registration's and the enterprise application's credentials.
    pub fn credentials(&self) -> Vec<(&'static str, &Credential)> {
        let app = self
            .app
            .iter()
            .flat_map(|a| a.credentials.iter().map(|c| ("registration", c)));
        let sp = self
            .sp
            .iter()
            .flat_map(|s| s.credentials.iter().map(|c| ("enterprise app", c)));
        app.chain(sp).collect()
    }
}

/// How soon a credential expires (`Expiry::Never` without an end date).
pub fn credential_expiry(c: &Credential, now: i64) -> Expiry {
    crate::policy::PolicyExemption {
        expires_on: c.end.clone(),
        ..Default::default()
    }
    .expiry(now)
}

/// One row per application, joined by app ID: registered first, then
/// external apps, then managed identities (Microsoft's only with
/// `microsoft`), by name; `filter` narrows by name, app ID or kind.
pub fn app_rows(
    apps: &[Application],
    sps: &[ServicePrincipal],
    microsoft: bool,
    filter: &str,
    now: i64,
) -> Vec<AppRow> {
    let mut by_app_id: HashMap<&str, &ServicePrincipal> = HashMap::new();
    for sp in sps {
        by_app_id.insert(sp.app_id.as_str(), sp);
    }
    let mut rows: Vec<AppRow> = apps
        .iter()
        .map(|a| (Some(a), by_app_id.remove(a.app_id.as_str())))
        .collect::<Vec<_>>()
        .into_iter()
        .chain(by_app_id.into_values().map(|sp| (None, Some(sp))))
        .map(|(app, sp)| {
            let kind = match (app, sp) {
                (Some(_), _) => AppKind::Registered,
                (None, Some(sp)) if sp.is_managed_identity() => AppKind::ManagedIdentity,
                (None, Some(sp)) if sp.is_microsoft() => AppKind::Microsoft,
                _ => AppKind::External,
            };
            let mut row = AppRow {
                name: app
                    .map(|a| a.display_name.clone())
                    .or_else(|| sp.map(|s| s.display_name.clone()))
                    .unwrap_or_default(),
                app_id: app
                    .map(|a| a.app_id.clone())
                    .or_else(|| sp.map(|s| s.app_id.clone()))
                    .unwrap_or_default(),
                app: app.cloned(),
                sp: sp.cloned(),
                kind,
                soonest: None,
            };
            row.soonest = row
                .credentials()
                .into_iter()
                .map(|(_, c)| (credential_expiry(c, now), c.clone()))
                .min_by(|a, b| a.0.cmp(&b.0));
            row
        })
        .filter(|r| microsoft || r.kind != AppKind::Microsoft)
        .collect();
    // Fully ordered: apps of the same name (e.g. managed identities) keep
    // their places instead of trading them on every redraw.
    rows.sort_by(|a, b| {
        a.kind
            .cmp(&b.kind)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            .then_with(|| a.app_id.cmp(&b.app_id))
            .then_with(|| {
                let id = |r: &AppRow| r.sp.as_ref().map(|s| s.id.clone()).unwrap_or_default();
                id(a).cmp(&id(b))
            })
    });
    if filter.is_empty() {
        return rows;
    }
    let needle = filter.to_ascii_lowercase();
    rows.into_iter()
        .filter(|r| {
            [r.name.as_str(), &r.app_id, r.kind.label()]
                .iter()
                .any(|f| f.to_ascii_lowercase().contains(&needle))
        })
        .collect()
}

/// Whether and how a permission is granted.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Consent {
    /// For everyone (admin consent; always for application permissions)
    Admin,
    /// By this many users for themselves
    Users(usize),
    NotGranted,
}

/// One API permission of an application: requested by its registration,
/// granted to its enterprise application, or both.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionRow {
    pub api: String,
    /// e.g. "User.Read.All" (its ID if the API couldn't be read)
    pub permission: String,
    pub description: String,
    /// "delegated" or "application"
    pub kind: &'static str,
    pub requested: bool,
    pub consent: Consent,
}

impl PermissionRow {
    /// "granted (admin consent)", "not granted", "granted, not requested", …
    pub fn status(&self) -> String {
        let granted = match &self.consent {
            Consent::Admin => "granted (admin consent)".to_string(),
            Consent::Users(1) => "granted by 1 user".to_string(),
            Consent::Users(n) => format!("granted by {n} users"),
            Consent::NotGranted => "not granted".to_string(),
        };
        match (self.requested, &self.consent) {
            (false, Consent::NotGranted) => granted,
            (false, _) => format!("{granted}, not requested"),
            (true, _) => granted,
        }
    }
}

/// Requested and granted permissions, merged per API and permission: not
/// granted first (what still needs consent), then the rest, by API.
pub fn permission_rows(app: Option<&Application>, detail: &AppDetail) -> Vec<PermissionRow> {
    let by_app_id = |id: &str| detail.resources.iter().find(|r| r.app_id == id);
    let by_id = |id: &str| detail.resources.iter().find(|r| r.id == id);
    // (API name, kind, permission) → row
    let mut rows: Vec<PermissionRow> = Vec::new();
    let mut add = |api: String,
                   kind: &'static str,
                   permission: String,
                   description: String,
                   requested: bool,
                   consent: Option<Consent>| {
        let row = match rows.iter_mut().find(|r| {
            r.api == api && r.kind == kind && r.permission.eq_ignore_ascii_case(&permission)
        }) {
            Some(row) => row,
            None => {
                rows.push(PermissionRow {
                    api,
                    permission,
                    description,
                    kind,
                    requested: false,
                    consent: Consent::NotGranted,
                });
                rows.last_mut().expect("just pushed")
            }
        };
        row.requested |= requested;
        if let Some(c) = consent {
            row.consent = match (&row.consent, c) {
                (Consent::Admin, _) | (_, Consent::Admin) => Consent::Admin,
                (Consent::Users(a), Consent::Users(b)) => Consent::Users(a + b),
                (_, c) => c,
            };
        }
    };
    for required in app.map(|a| a.required.as_slice()).unwrap_or_default() {
        let api = by_app_id(&required.resource_app_id);
        let api_name = api.map_or(required.resource_app_id.clone(), |r| r.display_name.clone());
        for (ids, kind) in [
            (&required.scopes, "delegated"),
            (&required.roles, "application"),
        ] {
            for id in ids {
                let defined = api.and_then(|r| {
                    let list = if kind == "delegated" {
                        &r.scopes
                    } else {
                        &r.roles
                    };
                    list.iter().find(|p| p.id == *id)
                });
                add(
                    api_name.clone(),
                    kind,
                    defined.map_or(id.clone(), |p| p.value.clone()),
                    defined.map(|p| p.display.clone()).unwrap_or_default(),
                    true,
                    None,
                );
            }
        }
    }
    for grant in &detail.delegated {
        let api = by_id(&grant.resource_id);
        let api_name = api.map_or(grant.resource_id.clone(), |r| r.display_name.clone());
        let consent = if grant.consent_type.eq_ignore_ascii_case("AllPrincipals") {
            Consent::Admin
        } else {
            Consent::Users(1)
        };
        for scope in &grant.scopes {
            let description = api
                .and_then(|r| {
                    r.scopes
                        .iter()
                        .find(|p| p.value.eq_ignore_ascii_case(scope))
                })
                .map(|p| p.display.clone())
                .unwrap_or_default();
            add(
                api_name.clone(),
                "delegated",
                scope.clone(),
                description,
                false,
                Some(consent.clone()),
            );
        }
    }
    for grant in &detail.app_roles {
        let api = by_id(&grant.resource_id);
        let api_name = api.map_or(grant.resource_name.clone(), |r| r.display_name.clone());
        let defined = api.and_then(|r| r.roles.iter().find(|p| p.id == grant.app_role_id));
        add(
            api_name,
            "application",
            defined.map_or(grant.app_role_id.clone(), |p| p.value.clone()),
            defined.map(|p| p.display.clone()).unwrap_or_default(),
            false,
            Some(Consent::Admin),
        );
    }
    rows.sort_by(|a, b| {
        (a.consent != Consent::NotGranted)
            .cmp(&(b.consent != Consent::NotGranted))
            .then_with(|| a.api.cmp(&b.api))
            .then_with(|| a.kind.cmp(b.kind))
            .then_with(|| {
                a.permission
                    .to_lowercase()
                    .cmp(&b.permission.to_lowercase())
            })
    });
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use c9s_azure::apps::{ApiPermission, DelegatedGrant, RequiredResource, RoleGrant};

    fn perm(id: &str, value: &str) -> ApiPermission {
        ApiPermission {
            id: id.into(),
            value: value.into(),
            display: format!("{value} description"),
            ..Default::default()
        }
    }

    #[test]
    fn rows_join_registrations_and_enterprise_apps() {
        let now = 1_700_000_000;
        let iso = |t: i64| chrono::DateTime::from_timestamp(t, 0).unwrap().to_rfc3339();
        let apps = vec![Application {
            id: "a1".into(),
            app_id: "x".into(),
            display_name: "billing".into(),
            credentials: vec![
                Credential {
                    name: "old".into(),
                    end: iso(now - 86_400 * 2),
                    ..Default::default()
                },
                Credential {
                    name: "new".into(),
                    end: iso(now + 86_400 * 90),
                    ..Default::default()
                },
            ],
            ..Default::default()
        }];
        let sp =
            |id: &str, app_id: &str, name: &str, sp_type: &str, tenant: &str| ServicePrincipal {
                id: id.into(),
                app_id: app_id.into(),
                display_name: name.into(),
                sp_type: sp_type.into(),
                owner_tenant: tenant.into(),
                ..Default::default()
            };
        let sps = vec![
            sp("s1", "x", "billing", "Application", "t"),
            sp("s2", "y", "vm-identity", "ManagedIdentity", ""),
            sp("s3", "z", "Office 365", "Application", MICROSOFT_TENANT),
            sp("s4", "w", "Partner App", "Application", "other"),
        ];
        let rows = app_rows(&apps, &sps, false, "", now);
        let summary: Vec<(&str, AppKind, bool, bool)> = rows
            .iter()
            .map(|r| (r.name.as_str(), r.kind, r.app.is_some(), r.sp.is_some()))
            .collect();
        assert_eq!(
            summary,
            [
                ("billing", AppKind::Registered, true, true),
                ("Partner App", AppKind::External, false, true),
                ("vm-identity", AppKind::ManagedIdentity, false, true),
            ],
            "Microsoft's apps hidden"
        );
        let (expiry, cred) = rows[0].soonest.clone().unwrap();
        assert_eq!((expiry, cred.name.as_str()), (Expiry::Expired(2), "old"));
        assert_eq!(app_rows(&apps, &sps, true, "", now).len(), 4);
        assert_eq!(app_rows(&apps, &sps, false, "managed", now).len(), 1);
    }

    #[test]
    fn apps_of_the_same_name_keep_their_order() {
        let mi = |id: &str, app_id: &str| ServicePrincipal {
            id: id.into(),
            app_id: app_id.into(),
            display_name: "vm-identity".into(),
            sp_type: "ManagedIdentity".into(),
            ..Default::default()
        };
        let a = vec![mi("s1", "b"), mi("s2", "a"), mi("s3", "c")];
        let mut b = a.clone();
        b.reverse();
        let ids = |sps: &[ServicePrincipal]| -> Vec<String> {
            app_rows(&[], sps, false, "", 0)
                .into_iter()
                .map(|r| r.app_id)
                .collect()
        };
        assert_eq!(ids(&a), ["a", "b", "c"], "then by app ID");
        assert_eq!(ids(&a), ids(&b), "whatever order Graph returned them in");
    }

    #[test]
    fn permissions_merge_requested_and_granted() {
        let graph = ResourceApi {
            id: "sp-graph".into(),
            app_id: "graph".into(),
            display_name: "Microsoft Graph".into(),
            scopes: vec![perm("s1", "User.Read"), perm("s2", "Mail.Send")],
            roles: vec![perm("r1", "User.Read.All")],
        };
        let app = Application {
            required: vec![RequiredResource {
                resource_app_id: "graph".into(),
                scopes: vec!["s1".into(), "s2".into()],
                roles: vec!["r1".into()],
            }],
            ..Default::default()
        };
        let detail = AppDetail {
            resources: vec![graph],
            delegated: vec![
                DelegatedGrant {
                    resource_id: "sp-graph".into(),
                    scopes: vec!["User.Read".into(), "offline_access".into()],
                    consent_type: "AllPrincipals".into(),
                },
                DelegatedGrant {
                    resource_id: "sp-graph".into(),
                    scopes: vec!["Mail.Send".into()],
                    consent_type: "Principal".into(),
                },
                DelegatedGrant {
                    resource_id: "sp-graph".into(),
                    scopes: vec!["Mail.Send".into()],
                    consent_type: "Principal".into(),
                },
            ],
            app_roles: vec![],
            ..Default::default()
        };
        let rows = permission_rows(Some(&app), &detail);
        let summary: Vec<(&str, &str, String)> = rows
            .iter()
            .map(|r| (r.permission.as_str(), r.kind, r.status()))
            .collect();
        assert_eq!(
            summary,
            [
                ("User.Read.All", "application", "not granted".to_string()),
                ("Mail.Send", "delegated", "granted by 2 users".to_string()),
                (
                    "offline_access",
                    "delegated",
                    "granted (admin consent), not requested".to_string()
                ),
                (
                    "User.Read",
                    "delegated",
                    "granted (admin consent)".to_string()
                ),
            ]
        );
        assert_eq!(rows[0].description, "User.Read.All description");

        // An application permission granted: admin consent.
        let detail = AppDetail {
            app_roles: vec![RoleGrant {
                resource_id: "sp-graph".into(),
                app_role_id: "r1".into(),
                ..Default::default()
            }],
            ..detail
        };
        let rows = permission_rows(Some(&app), &detail);
        assert!(rows
            .iter()
            .any(|r| r.permission == "User.Read.All" && r.consent == Consent::Admin));
    }
}
