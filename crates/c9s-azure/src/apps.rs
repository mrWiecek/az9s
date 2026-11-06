//! Applications from Microsoft Graph: app registrations (`/applications`)
//! and enterprise applications (service principals, `/servicePrincipals`),
//! which share an app ID. A registration declares what the app asks for and
//! exposes; its enterprise application holds what is granted in this
//! tenant and who may use it.

use crate::directory::{principal_from_item, DirectoryClient, Principal};
use crate::{timing, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;

/// The tenant Microsoft's own (first-party) applications belong to.
pub const MICROSOFT_TENANT: &str = "f8cdef31-a31e-4b4a-93e4-5f571e91255a";

/// A client secret or certificate.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Credential {
    pub name: String,
    /// "secret" or "certificate"
    pub kind: String,
    pub start: String,
    /// ISO 8601; empty if it never expires
    pub end: String,
}

/// A delegated permission (scope) or application permission (app role)
/// that an API defines.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiPermission {
    pub id: String,
    /// What clients ask for, e.g. "User.Read.All"
    pub value: String,
    /// Its description, e.g. "Read all users' full profiles"
    pub display: String,
    /// Scopes: "Admin" or "User" (who can consent). App roles: who can be
    /// given it ("Application", "User", or both).
    pub audience: String,
    pub enabled: bool,
}

/// What a registration asks for from one API.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequiredResource {
    pub resource_app_id: String,
    /// Delegated permission IDs
    pub scopes: Vec<String>,
    /// Application permission IDs
    pub roles: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Application {
    /// Object ID of the registration
    pub id: String,
    pub app_id: String,
    pub display_name: String,
    pub sign_in_audience: String,
    pub created: String,
    pub identifier_uris: Vec<String>,
    pub redirect_uris: Vec<String>,
    pub credentials: Vec<Credential>,
    pub required: Vec<RequiredResource>,
    /// Delegated permissions it exposes
    pub scopes: Vec<ApiPermission>,
    /// Application permissions (app roles) it defines
    pub roles: Vec<ApiPermission>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServicePrincipal {
    /// Object ID of the enterprise application
    pub id: String,
    pub app_id: String,
    pub display_name: String,
    /// "Application", "ManagedIdentity", "Legacy", …
    pub sp_type: String,
    pub enabled: bool,
    pub assignment_required: bool,
    /// The tenant the app is registered in
    pub owner_tenant: String,
    pub credentials: Vec<Credential>,
    /// For a managed identity: `isExplicit=True|False` (user- or
    /// system-assigned) and the Azure resource ID it belongs to.
    #[serde(default)]
    pub alternative_names: Vec<String>,
}

/// Where a managed identity lives in Azure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagedIdentity {
    /// User-assigned (a `userAssignedIdentities` resource of its own), or
    /// system-assigned (part of the resource it identifies); `None` when
    /// Graph doesn't say.
    pub user_assigned: Option<bool>,
    /// The ARM ID: the user-assigned identity resource, or the resource a
    /// system-assigned identity belongs to (a VM, an App Service...).
    pub resource_id: Option<String>,
}

/// A delegated permission grant: scopes the app may use on an API, for
/// everyone (admin consent) or one user.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DelegatedGrant {
    /// The API's service principal
    pub resource_id: String,
    pub scopes: Vec<String>,
    /// "AllPrincipals" (admin consent) or "Principal" (one user)
    pub consent_type: String,
}

/// An app role assignment: an application permission granted to the app,
/// or a user, group or app assigned to it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleGrant {
    pub resource_id: String,
    pub resource_name: String,
    pub app_role_id: String,
    pub principal_id: String,
    pub principal_name: String,
    pub principal_type: String,
}

/// An API's permissions, to name requested and granted ones.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceApi {
    /// Its service principal's object ID
    pub id: String,
    pub app_id: String,
    pub display_name: String,
    pub scopes: Vec<ApiPermission>,
    pub roles: Vec<ApiPermission>,
}

/// What an application's tabs show beyond its two list objects.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppDetail {
    pub app_owners: Vec<Principal>,
    pub sp_owners: Vec<Principal>,
    pub delegated: Vec<DelegatedGrant>,
    /// Application permissions granted to the app
    pub app_roles: Vec<RoleGrant>,
    /// Users, groups and apps assigned to the app
    pub assigned: Vec<RoleGrant>,
    /// The APIs it asks for or is granted
    pub resources: Vec<ResourceApi>,
    /// The two documents, for the JSON tab
    pub application_json: Value,
    pub service_principal_json: Value,
}

fn text(item: &Value, key: &str) -> String {
    item.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn strings(item: Option<&Value>) -> Vec<String> {
    item.and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn credentials(item: &Value) -> Vec<Credential> {
    let mut out = Vec::new();
    for (key, kind) in [
        ("passwordCredentials", "secret"),
        ("keyCredentials", "certificate"),
    ] {
        for c in item
            .get(key)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            out.push(Credential {
                name: Some(text(c, "displayName"))
                    .filter(|n| !n.is_empty())
                    .unwrap_or_else(|| text(c, "keyId")),
                kind: kind.into(),
                start: text(c, "startDateTime"),
                end: text(c, "endDateTime"),
            });
        }
    }
    out
}

fn permissions(items: Option<&Value>, scope: bool) -> Vec<ApiPermission> {
    items
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|p| ApiPermission {
            id: text(p, "id").to_ascii_lowercase(),
            value: text(p, "value"),
            display: if scope {
                text(p, "adminConsentDisplayName")
            } else {
                text(p, "displayName")
            },
            audience: if scope {
                text(p, "type")
            } else {
                strings(p.get("allowedMemberTypes")).join(", ")
            },
            enabled: p.get("isEnabled").and_then(Value::as_bool).unwrap_or(true),
        })
        .collect()
}

impl Application {
    pub fn from_item(item: &Value) -> Option<Self> {
        let id = text(item, "id");
        if id.is_empty() {
            return None;
        }
        let mut redirect_uris = Vec::new();
        for key in ["web", "spa", "publicClient"] {
            redirect_uris.extend(strings(item.get(key).and_then(|v| v.get("redirectUris"))));
        }
        let required = item
            .get("requiredResourceAccess")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(|r| {
                let access: Vec<&Value> = r
                    .get("resourceAccess")
                    .and_then(Value::as_array)
                    .map(|a| a.iter().collect())
                    .unwrap_or_default();
                let of = |kind: &str| -> Vec<String> {
                    access
                        .iter()
                        .filter(|a| text(a, "type").eq_ignore_ascii_case(kind))
                        .map(|a| text(a, "id").to_ascii_lowercase())
                        .collect()
                };
                RequiredResource {
                    resource_app_id: text(r, "resourceAppId").to_ascii_lowercase(),
                    scopes: of("Scope"),
                    roles: of("Role"),
                }
            })
            .collect();
        Some(Self {
            app_id: text(item, "appId").to_ascii_lowercase(),
            display_name: text(item, "displayName"),
            sign_in_audience: text(item, "signInAudience"),
            created: text(item, "createdDateTime"),
            identifier_uris: strings(item.get("identifierUris")),
            redirect_uris,
            credentials: credentials(item),
            required,
            scopes: permissions(
                item.get("api")
                    .and_then(|a| a.get("oauth2PermissionScopes")),
                true,
            ),
            roles: permissions(item.get("appRoles"), false),
            id,
        })
    }
}

impl ServicePrincipal {
    pub fn from_item(item: &Value) -> Option<Self> {
        let id = text(item, "id");
        if id.is_empty() {
            return None;
        }
        Some(Self {
            app_id: text(item, "appId").to_ascii_lowercase(),
            display_name: text(item, "displayName"),
            sp_type: text(item, "servicePrincipalType"),
            enabled: item
                .get("accountEnabled")
                .and_then(Value::as_bool)
                .unwrap_or(true),
            assignment_required: item
                .get("appRoleAssignmentRequired")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            owner_tenant: text(item, "appOwnerOrganizationId").to_ascii_lowercase(),
            credentials: credentials(item),
            alternative_names: item
                .get("alternativeNames")
                .and_then(Value::as_array)
                .map(|names| {
                    names
                        .iter()
                        .filter_map(|n| n.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default(),
            id,
        })
    }

    pub fn is_managed_identity(&self) -> bool {
        self.sp_type.eq_ignore_ascii_case("ManagedIdentity")
    }

    /// For a managed identity: user- or system-assigned, and its Azure
    /// resource (from `alternativeNames`).
    pub fn managed_identity(&self) -> Option<ManagedIdentity> {
        if !self.is_managed_identity() {
            return None;
        }
        let user_assigned = self.alternative_names.iter().find_map(|n| {
            let value = n.strip_prefix("isExplicit=")?;
            Some(value.eq_ignore_ascii_case("true"))
        });
        let resource_id = self
            .alternative_names
            .iter()
            .find(|n| n.to_ascii_lowercase().starts_with("/subscriptions/"))
            .cloned();
        Some(ManagedIdentity {
            user_assigned: user_assigned.or_else(|| {
                // Older identities may lack isExplicit: the resource type tells.
                resource_id.as_ref().map(|id| {
                    id.to_ascii_lowercase()
                        .contains("/providers/microsoft.managedidentity/userassignedidentities/")
                })
            }),
            resource_id,
        })
    }

    /// One of Microsoft's own applications.
    pub fn is_microsoft(&self) -> bool {
        self.owner_tenant == MICROSOFT_TENANT
    }
}

fn resource_api(item: &Value) -> Option<ResourceApi> {
    let id = text(item, "id");
    (!id.is_empty()).then(|| ResourceApi {
        app_id: text(item, "appId").to_ascii_lowercase(),
        display_name: text(item, "displayName"),
        scopes: permissions(item.get("oauth2PermissionScopes"), true),
        roles: permissions(item.get("appRoles"), false),
        id: id.to_ascii_lowercase(),
    })
}

fn role_grant(item: &Value) -> RoleGrant {
    RoleGrant {
        resource_id: text(item, "resourceId").to_ascii_lowercase(),
        resource_name: text(item, "resourceDisplayName"),
        app_role_id: text(item, "appRoleId").to_ascii_lowercase(),
        principal_id: text(item, "principalId").to_ascii_lowercase(),
        principal_name: text(item, "principalDisplayName"),
        principal_type: text(item, "principalType"),
    }
}

const APP_SELECT: &str = "id,appId,displayName,signInAudience,createdDateTime,identifierUris,\
     passwordCredentials,keyCredentials,requiredResourceAccess,api,appRoles,web,spa,publicClient";
const SP_SELECT: &str = "id,appId,displayName,servicePrincipalType,accountEnabled,\
     appRoleAssignmentRequired,appOwnerOrganizationId,passwordCredentials,keyCredentials,\
     alternativeNames";
const RESOURCE_SELECT: &str = "id,appId,displayName,oauth2PermissionScopes,appRoles";

impl DirectoryClient {
    /// Every app registration and every enterprise application.
    pub async fn applications(&self) -> Result<(Vec<Application>, Vec<ServicePrincipal>)> {
        let _t = timing::timer("azure.applications");
        let base = self.endpoint().to_string();
        let (apps, sps) = futures::join!(
            self.get_all(format!(
                "{base}/v1.0/applications?$select={APP_SELECT}&$top=999"
            )),
            self.get_all(format!(
                "{base}/v1.0/servicePrincipals?$select={SP_SELECT}&$top=999"
            )),
        );
        Ok((
            apps?.iter().filter_map(Application::from_item).collect(),
            sps?.iter()
                .filter_map(ServicePrincipal::from_item)
                .collect(),
        ))
    }

    /// One application's owners, grants, assignments and the APIs it uses.
    /// Either object may be missing (no registration in this tenant, or no
    /// enterprise application yet).
    pub async fn app_detail(
        &self,
        application_id: Option<&str>,
        service_principal_id: Option<&str>,
    ) -> Result<AppDetail> {
        let _t = timing::timer("azure.app_detail");
        let base = self.endpoint().to_string();
        let owners = "owners?$select=id,displayName,userPrincipalName,appId&$top=999";
        let mut detail = AppDetail::default();
        let mut resource_app_ids: Vec<String> = Vec::new();
        if let Some(id) = application_id {
            let doc = self
                .get_one(&format!("{base}/v1.0/applications/{id}"))
                .await?;
            if let Some(app) = Application::from_item(&doc) {
                resource_app_ids.extend(app.required.iter().map(|r| r.resource_app_id.clone()));
            }
            detail.application_json = doc;
            detail.app_owners = self
                .get_all(format!("{base}/v1.0/applications/{id}/{owners}"))
                .await?
                .iter()
                .filter_map(principal_from_item)
                .collect();
        }
        let mut resource_ids: Vec<String> = Vec::new();
        if let Some(id) = service_principal_id {
            let sp = format!("{base}/v1.0/servicePrincipals/{id}");
            let (doc, sp_owners, grants, roles, assigned) = futures::join!(
                self.get_one(&sp),
                self.get_all(format!("{sp}/{owners}")),
                self.get_all(format!("{sp}/oauth2PermissionGrants?$top=999")),
                self.get_all(format!("{sp}/appRoleAssignments?$top=999")),
                self.get_all(format!("{sp}/appRoleAssignedTo?$top=999")),
            );
            detail.service_principal_json = doc?;
            detail.sp_owners = sp_owners?.iter().filter_map(principal_from_item).collect();
            detail.delegated = grants?
                .iter()
                .map(|g| DelegatedGrant {
                    resource_id: text(g, "resourceId").to_ascii_lowercase(),
                    scopes: text(g, "scope")
                        .split_whitespace()
                        .map(str::to_string)
                        .collect(),
                    consent_type: text(g, "consentType"),
                })
                .collect();
            detail.app_roles = roles?.iter().map(role_grant).collect();
            detail.assigned = assigned?.iter().map(role_grant).collect();
            resource_ids.extend(detail.delegated.iter().map(|g| g.resource_id.clone()));
            resource_ids.extend(detail.app_roles.iter().map(|g| g.resource_id.clone()));
        }
        // The APIs, to name their permissions: by app ID (requested) and by
        // object ID (granted), each once.
        let mut seen = HashSet::new();
        resource_app_ids.retain(|a| seen.insert(format!("app:{a}")));
        resource_ids.retain(|r| seen.insert(format!("id:{r}")));
        for app_id in resource_app_ids {
            let url = format!(
                "{base}/v1.0/servicePrincipals(appId='{app_id}')?$select={RESOURCE_SELECT}"
            );
            // An API without an enterprise application here can't be named.
            if let Ok(doc) = self.get_one(&url).await {
                detail.resources.extend(resource_api(&doc));
            }
        }
        for id in resource_ids {
            if detail.resources.iter().any(|r| r.id == id) {
                continue;
            }
            let url = format!("{base}/v1.0/servicePrincipals/{id}?$select={RESOURCE_SELECT}");
            if let Ok(doc) = self.get_one(&url).await {
                detail.resources.extend(resource_api(&doc));
            }
        }
        Ok(detail)
    }
}

#[cfg(test)]
mod managed_identity_tests {
    use super::*;

    fn mi(names: &[&str]) -> ServicePrincipal {
        ServicePrincipal {
            sp_type: "ManagedIdentity".into(),
            alternative_names: names.iter().map(|n| n.to_string()).collect(),
            ..Default::default()
        }
    }

    #[test]
    fn user_and_system_assigned_identities_and_their_resources() {
        let uami = "/subscriptions/s1/resourcegroups/rg-id/providers/Microsoft.ManagedIdentity/userAssignedIdentities/id-deploy";
        assert_eq!(
            mi(&["isExplicit=True", uami]).managed_identity(),
            Some(ManagedIdentity {
                user_assigned: Some(true),
                resource_id: Some(uami.into())
            })
        );
        let vm =
            "/subscriptions/s1/resourceGroups/rg/providers/Microsoft.Compute/virtualMachines/vm-1";
        let system = mi(&["isExplicit=False", vm]).managed_identity().unwrap();
        assert_eq!(system.user_assigned, Some(false));
        assert_eq!(system.resource_id.as_deref(), Some(vm));
        // Without isExplicit, the resource type says which it is.
        assert_eq!(
            mi(&[uami]).managed_identity().unwrap().user_assigned,
            Some(true)
        );
        assert_eq!(mi(&[]).managed_identity().unwrap().user_assigned, None);
        let app = ServicePrincipal {
            sp_type: "Application".into(),
            ..Default::default()
        };
        assert_eq!(app.managed_identity(), None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{ok, serve_paths, StaticToken};
    use serde_json::json;
    use std::sync::Arc;

    const GRAPH: &str = "00000003-0000-0000-c000-000000000000";

    fn graph_api() -> Value {
        json!({"id": "sp-graph", "appId": GRAPH, "displayName": "Microsoft Graph",
               "oauth2PermissionScopes": [{"id": "S1", "value": "User.Read",
                   "adminConsentDisplayName": "Sign in and read user profile", "type": "User", "isEnabled": true}],
               "appRoles": [{"id": "R1", "value": "User.Read.All", "displayName": "Read all users",
                   "allowedMemberTypes": ["Application"], "isEnabled": true}]})
    }

    fn registration() -> Value {
        json!({"id": "app-obj", "appId": "APP-1", "displayName": "billing-api",
               "signInAudience": "AzureADMyOrg",
               "identifierUris": ["api://billing"],
               "web": {"redirectUris": ["https://billing/cb"]}, "spa": {"redirectUris": []},
               "passwordCredentials": [{"displayName": "ci", "keyId": "k1", "endDateTime": "2027-01-01T00:00:00Z"}],
               "keyCredentials": [{"keyId": "k2", "endDateTime": "2026-12-01T00:00:00Z"}],
               "requiredResourceAccess": [{"resourceAppId": GRAPH, "resourceAccess": [
                   {"id": "S1", "type": "Scope"}, {"id": "R1", "type": "Role"}]}],
               "api": {"oauth2PermissionScopes": [{"id": "x", "value": "Invoices.Read",
                   "adminConsentDisplayName": "Read invoices", "type": "Admin", "isEnabled": true}]},
               "appRoles": [{"id": "y", "value": "Billing.Admin", "displayName": "Billing admins",
                   "allowedMemberTypes": ["User", "Application"], "isEnabled": true}]})
    }

    #[tokio::test]
    async fn applications_and_enterprise_apps_parse() {
        let (endpoint, _) = serve_paths(|path, _| {
            if path.starts_with("/v1.0/applications?") {
                ok(json!({"value": [registration()]}))
            } else {
                ok(json!({"value": [
                    {"id": "sp-1", "appId": "app-1", "displayName": "billing-api",
                     "servicePrincipalType": "Application", "accountEnabled": true,
                     "appRoleAssignmentRequired": true, "appOwnerOrganizationId": "T"},
                    {"id": "sp-mi", "appId": "mi-1", "displayName": "vm-identity",
                     "servicePrincipalType": "ManagedIdentity",
                     "alternativeNames": ["isExplicit=False",
                        "/subscriptions/s1/resourceGroups/rg/providers/Microsoft.Compute/virtualMachines/vm-1"]},
                    {"id": "sp-ms", "appId": "ms-1", "displayName": "Office 365",
                     "appOwnerOrganizationId": MICROSOFT_TENANT}
                ]}))
            }
        })
        .await;
        let client = DirectoryClient::with_endpoint(endpoint, Arc::new(StaticToken("t".into())));
        let (apps, sps) = client.applications().await.unwrap();
        assert_eq!(
            sps[1].managed_identity().unwrap().user_assigned,
            Some(false),
            "alternativeNames are read"
        );
        let app = &apps[0];
        assert_eq!(app.app_id, "app-1", "app IDs lowercase, to join the two");
        assert_eq!(app.redirect_uris, ["https://billing/cb"]);
        assert_eq!(
            app.credentials
                .iter()
                .map(|c| (c.name.as_str(), c.kind.as_str()))
                .collect::<Vec<_>>(),
            [("ci", "secret"), ("k2", "certificate")]
        );
        assert_eq!(app.required[0].scopes, ["s1"]);
        assert_eq!(app.required[0].roles, ["r1"]);
        assert_eq!(
            (
                app.scopes[0].value.as_str(),
                app.scopes[0].audience.as_str()
            ),
            ("Invoices.Read", "Admin")
        );
        assert_eq!(app.roles[0].audience, "User, Application");
        assert!(sps[0].assignment_required && !sps[0].is_microsoft());
        assert!(sps[1].is_managed_identity() && sps[2].is_microsoft());
    }

    #[tokio::test]
    async fn app_detail_names_the_apis_it_uses() {
        let (endpoint, requests) = serve_paths(|path, _| {
            let p = path.split('?').next().unwrap();
            match p {
                "/v1.0/applications/app-obj" => ok(registration()),
                "/v1.0/applications/app-obj/owners" => ok(json!({"value": [
                    {"@odata.type": "#microsoft.graph.user", "id": "u1", "displayName": "Ada",
                     "userPrincipalName": "ada@contoso.com"}]})),
                "/v1.0/servicePrincipals/sp-1" => ok(json!({"id": "sp-1", "appId": "app-1"})),
                "/v1.0/servicePrincipals/sp-1/owners" => ok(json!({"value": []})),
                "/v1.0/servicePrincipals/sp-1/oauth2PermissionGrants" => ok(json!({"value": [
                    {"resourceId": "SP-GRAPH", "scope": " User.Read  offline_access", "consentType": "AllPrincipals"}]})),
                "/v1.0/servicePrincipals/sp-1/appRoleAssignments" => ok(json!({"value": []})),
                "/v1.0/servicePrincipals/sp-1/appRoleAssignedTo" => ok(json!({"value": [
                    {"principalId": "g1", "principalDisplayName": "Finance", "principalType": "Group",
                     "appRoleId": "y", "resourceId": "sp-1"}]})),
                _ if p.starts_with("/v1.0/servicePrincipals(appId=") => ok(graph_api()),
                _ => crate::testing::error(404, "unexpected"),
            }
        })
        .await;
        let client = DirectoryClient::with_endpoint(endpoint, Arc::new(StaticToken("t".into())));
        let detail = client
            .app_detail(Some("app-obj"), Some("sp-1"))
            .await
            .unwrap();
        assert_eq!(detail.app_owners[0].detail, "ada@contoso.com");
        assert_eq!(detail.delegated[0].scopes, ["User.Read", "offline_access"]);
        assert_eq!(detail.assigned[0].principal_name, "Finance");
        assert_eq!(detail.resources.len(), 1, "Graph looked up once, by app ID");
        assert_eq!(detail.resources[0].roles[0].value, "User.Read.All");
        assert_eq!(detail.application_json["displayName"], "billing-api");
        assert!(requests.lock().unwrap().len() >= 7);
    }
}
