//! Names of directory principals (users, groups, service principals) from
//! Microsoft Graph, for role assignments, which only carry principal IDs.
//!
//! One call resolves up to 1000 IDs:
//! `POST {endpoint}/v1.0/directoryObjects/getByIds`. Reading them needs the
//! default directory read permission members have; guests may not, and then
//! the IDs stay as they are.

use crate::resource_graph::TokenSource;
use crate::{timing, AzureError, Result};
use serde_json::{json, Value};
use std::sync::Arc;

pub const DEFAULT_GRAPH_ENDPOINT: &str = "https://graph.microsoft.com";
/// The token audience for Microsoft Graph.
pub const GRAPH_RESOURCE: &str = "https://graph.microsoft.com/";
const MAX_IDS_PER_REQUEST: usize = 1000;

/// A directory object behind a principal ID.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Principal {
    pub id: String,
    pub display_name: String,
    /// "user", "group" or "servicePrincipal"
    pub kind: String,
    /// A user's sign-in name, or an application's app ID
    pub detail: String,
}

pub struct DirectoryClient {
    http: reqwest::Client,
    endpoint: String,
    tokens: Arc<dyn TokenSource>,
}

impl DirectoryClient {
    pub fn new(tokens: Arc<dyn TokenSource>) -> Self {
        Self::with_endpoint(DEFAULT_GRAPH_ENDPOINT, tokens)
    }

    pub fn with_endpoint(endpoint: impl Into<String>, tokens: Arc<dyn TokenSource>) -> Self {
        Self {
            http: reqwest::Client::new(),
            endpoint: endpoint.into().trim_end_matches('/').to_string(),
            tokens,
        }
    }

    /// The principals behind `ids`. IDs the directory doesn't know (deleted
    /// principals) are left out.
    pub async fn principals(&self, ids: &[String]) -> Result<Vec<Principal>> {
        let _t = timing::timer("azure.principals");
        let mut found = Vec::new();
        for batch in ids.chunks(MAX_IDS_PER_REQUEST) {
            let token = self.tokens.token().await?;
            let response = self
                .http
                .post(format!("{}/v1.0/directoryObjects/getByIds", self.endpoint))
                .bearer_auth(token)
                .header("content-type", "application/json")
                .body(
                    json!({"ids": batch, "types": ["user", "group", "servicePrincipal"]})
                        .to_string(),
                )
                .send()
                .await
                .map_err(|e| {
                    AzureError::ApiError(format!("Microsoft Graph request failed: {e}"))
                })?;
            let status = response.status();
            let text = response
                .text()
                .await
                .map_err(|e| AzureError::ApiError(format!("Microsoft Graph response: {e}")))?;
            if !status.is_success() {
                let message = serde_json::from_str::<Value>(&text)
                    .ok()
                    .and_then(|v| v.pointer("/error/message")?.as_str().map(str::to_string))
                    .unwrap_or_else(|| text.chars().take(300).collect());
                return Err(AzureError::Http {
                    status: status.as_u16(),
                    message,
                });
            }
            let body: Value = serde_json::from_str(&text).map_err(|e| {
                AzureError::ApiError(format!("Invalid Microsoft Graph response: {e}"))
            })?;
            found.extend(
                body.get("value")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(principal_from_item),
            );
        }
        Ok(found)
    }
}

impl DirectoryClient {
    /// The groups `principal_id` is a member of, directly or through other
    /// groups: `GET {endpoint}/v1.0/directoryObjects/{id}/transitiveMemberOf`,
    /// paged by `@odata.nextLink`.
    pub async fn groups_of(&self, principal_id: &str) -> Result<Vec<Principal>> {
        let _t = timing::timer("azure.principal_groups");
        let mut url = format!(
            "{}/v1.0/directoryObjects/{}/transitiveMemberOf/microsoft.graph.group\
             ?$select=id,displayName&$top=999",
            self.endpoint, principal_id
        );
        let mut groups = Vec::new();
        loop {
            let token = self.tokens.token().await?;
            let response = self
                .http
                .get(&url)
                .bearer_auth(token)
                .send()
                .await
                .map_err(|e| {
                    AzureError::ApiError(format!("Microsoft Graph request failed: {e}"))
                })?;
            let body = read_body(response).await?;
            groups.extend(
                body.get("value")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(|item| {
                        // The cast path returns groups without @odata.type.
                        let mut p = principal_from_item(item)?;
                        p.kind = "group".into();
                        Some(p)
                    }),
            );
            match body.get("@odata.nextLink").and_then(Value::as_str) {
                Some(next) if !next.is_empty() => {
                    url = crate::client::next_page(next, &self.endpoint)?
                }
                _ => break,
            }
        }
        Ok(groups)
    }
}

/// A directory group.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Group {
    pub id: String,
    pub display_name: String,
    pub description: String,
    pub mail: String,
    /// Microsoft 365 group (`groupTypes` has "Unified"); else a security
    /// or distribution group
    pub unified: bool,
    pub security_enabled: bool,
    /// Members come from a rule (`groupTypes` has "DynamicMembership")
    pub dynamic: bool,
    /// Can be given Entra ID roles
    pub role_assignable: bool,
    pub created: String,
}

impl Group {
    fn from_item(item: &Value) -> Option<Self> {
        let text = |key: &str| {
            item.get(key)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        let flag = |key: &str| item.get(key).and_then(Value::as_bool).unwrap_or(false);
        let types: Vec<&str> = item
            .get("groupTypes")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();
        let id = text("id");
        if id.is_empty() {
            return None;
        }
        Some(Self {
            display_name: Some(text("displayName"))
                .filter(|n| !n.is_empty())
                .unwrap_or_else(|| id.clone()),
            id,
            description: text("description"),
            mail: text("mail"),
            unified: types.iter().any(|t| t.eq_ignore_ascii_case("Unified")),
            security_enabled: flag("securityEnabled"),
            dynamic: types
                .iter()
                .any(|t| t.eq_ignore_ascii_case("DynamicMembership")),
            role_assignable: flag("isAssignableToRole"),
            created: text("createdDateTime"),
        })
    }

    /// "Microsoft 365", "security" or "distribution".
    pub fn kind_label(&self) -> &'static str {
        if self.unified {
            "Microsoft 365"
        } else if self.security_enabled {
            "security"
        } else {
            "distribution"
        }
    }
}

impl DirectoryClient {
    /// Every group in the directory, paged.
    pub async fn groups(&self) -> Result<Vec<Group>> {
        let _t = timing::timer("azure.groups");
        let url = format!(
            "{}/v1.0/groups?$select=id,displayName,description,mail,groupTypes,\
             securityEnabled,isAssignableToRole,createdDateTime&$top=999",
            self.endpoint
        );
        Ok(self
            .get_all(url)
            .await?
            .iter()
            .filter_map(Group::from_item)
            .collect())
    }

    /// A group's members: direct, or with `transitive` also the members of
    /// its member groups (and theirs).
    pub async fn members(&self, group_id: &str, transitive: bool) -> Result<Vec<Principal>> {
        let _t = timing::timer("azure.group_members");
        let relation = if transitive {
            "transitiveMembers"
        } else {
            "members"
        };
        let url = format!(
            "{}/v1.0/groups/{group_id}/{relation}\
             ?$select=id,displayName,userPrincipalName,appId,mail&$top=999",
            self.endpoint
        );
        Ok(self
            .get_all(url)
            .await?
            .iter()
            .filter_map(principal_from_item)
            .collect())
    }

    /// Every item of a paged Graph list (`value`, `@odata.nextLink`).
    pub(crate) async fn get_all(&self, mut url: String) -> Result<Vec<Value>> {
        let mut items = Vec::new();
        loop {
            let token = self.tokens.token().await?;
            let response = self
                .http
                .get(&url)
                .bearer_auth(token)
                .send()
                .await
                .map_err(|e| {
                    AzureError::ApiError(format!("Microsoft Graph request failed: {e}"))
                })?;
            let body = read_body(response).await?;
            if let Some(Value::Array(page)) = body.get("value") {
                items.extend(page.iter().cloned());
            }
            match body.get("@odata.nextLink").and_then(Value::as_str) {
                Some(next) if !next.is_empty() => {
                    url = crate::client::next_page(next, &self.endpoint)?
                }
                _ => return Ok(items),
            }
        }
    }
}

impl DirectoryClient {
    /// The Graph endpoint (for building URLs in sibling modules).
    pub(crate) fn endpoint(&self) -> &str {
        &self.endpoint
    }

    /// One Graph object.
    pub(crate) async fn get_one(&self, url: &str) -> Result<Value> {
        let token = self.tokens.token().await?;
        let response = self
            .http
            .get(url)
            .bearer_auth(token)
            .send()
            .await
            .map_err(|e| AzureError::ApiError(format!("Microsoft Graph request failed: {e}")))?;
        read_body(response).await
    }
}

/// A Microsoft Graph response's JSON, or its error message.
async fn read_body(response: reqwest::Response) -> Result<Value> {
    let status = response.status();
    let text = response
        .text()
        .await
        .map_err(|e| AzureError::ApiError(format!("Microsoft Graph response: {e}")))?;
    if !status.is_success() {
        let message = serde_json::from_str::<Value>(&text)
            .ok()
            .and_then(|v| v.pointer("/error/message")?.as_str().map(str::to_string))
            .unwrap_or_else(|| text.chars().take(300).collect());
        return Err(AzureError::Http {
            status: status.as_u16(),
            message,
        });
    }
    serde_json::from_str(&text)
        .map_err(|e| AzureError::ApiError(format!("Invalid Microsoft Graph response: {e}")))
}

pub(crate) fn principal_from_item(item: &Value) -> Option<Principal> {
    let text = |key: &str| item.get(key).and_then(Value::as_str).unwrap_or_default();
    let id = text("id");
    if id.is_empty() {
        return None;
    }
    let kind = text("@odata.type")
        .trim_start_matches("#microsoft.graph.")
        .to_string();
    let detail = match kind.as_str() {
        "user" => text("userPrincipalName"),
        "servicePrincipal" => text("appId"),
        "group" => text("mail"),
        _ => "",
    };
    Some(Principal {
        id: id.to_string(),
        display_name: Some(text("displayName"))
            .filter(|n| !n.is_empty())
            .unwrap_or(id)
            .to_string(),
        kind,
        detail: detail.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{error, ok, serve_paths, StaticToken};

    fn client(endpoint: &str) -> DirectoryClient {
        DirectoryClient::with_endpoint(endpoint, Arc::new(StaticToken("t".into())))
    }

    #[tokio::test]
    async fn principals_are_named_by_id() {
        let (endpoint, requests) = serve_paths(|path, body| {
            assert_eq!(path, "/v1.0/directoryObjects/getByIds");
            assert_eq!(body["types"], json!(["user", "group", "servicePrincipal"]));
            ok(json!({"value": [
                {"@odata.type": "#microsoft.graph.user", "id": "u1",
                 "displayName": "Ada Lovelace", "userPrincipalName": "ada@contoso.com"},
                {"@odata.type": "#microsoft.graph.servicePrincipal", "id": "s1",
                 "displayName": "deploy-pipeline", "appId": "app-1"},
                {"@odata.type": "#microsoft.graph.group", "id": "g1", "displayName": "Platform Admins"}
            ]}))
        })
        .await;
        let ids: Vec<String> = ["u1", "s1", "g1", "gone"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let found = client(&endpoint).principals(&ids).await.unwrap();
        let summary: Vec<(&str, &str, &str)> = found
            .iter()
            .map(|p| (p.display_name.as_str(), p.kind.as_str(), p.detail.as_str()))
            .collect();
        assert_eq!(
            summary,
            [
                ("Ada Lovelace", "user", "ada@contoso.com"),
                ("deploy-pipeline", "servicePrincipal", "app-1"),
                ("Platform Admins", "group", "")
            ]
        );
        assert_eq!(
            requests.lock().unwrap()[0]["ids"].as_array().unwrap().len(),
            4
        );
    }

    #[tokio::test]
    async fn groups_include_nested_ones_across_pages() {
        let base = Arc::new(std::sync::Mutex::new(String::new()));
        let base_in_server = base.clone();
        let (endpoint, _) = serve_paths(move |path, _| {
            assert!(
                path.starts_with(
                    "/v1.0/directoryObjects/u1/transitiveMemberOf/microsoft.graph.group"
                ),
                "{path}"
            );
            if path.contains("page=2") {
                ok(json!({"value": [{"id": "g2", "displayName": "Nested"}]}))
            } else {
                let next = format!("{}{}&page=2", base_in_server.lock().unwrap(), path);
                ok(
                    json!({"value": [{"id": "g1", "displayName": "Platform Admins"}],
                          "@odata.nextLink": next}),
                )
            }
        })
        .await;
        *base.lock().unwrap() = endpoint.clone();
        let groups = client(&endpoint).groups_of("u1").await.unwrap();
        let names: Vec<(&str, &str)> = groups
            .iter()
            .map(|g| (g.display_name.as_str(), g.kind.as_str()))
            .collect();
        assert_eq!(names, [("Platform Admins", "group"), ("Nested", "group")]);
    }

    #[tokio::test]
    async fn groups_and_their_members() {
        let (endpoint, _) = serve_paths(|path, _| {
            if path.starts_with("/v1.0/groups?") {
                ok(json!({"value": [
                    {"id": "g1", "displayName": "Platform Admins", "groupTypes": [],
                     "securityEnabled": true, "isAssignableToRole": true},
                    {"id": "g2", "displayName": "Team Site", "groupTypes": ["Unified", "DynamicMembership"],
                     "securityEnabled": false, "mail": "team@contoso.com"}
                ]}))
            } else if path.starts_with("/v1.0/groups/g1/transitiveMembers?") {
                ok(json!({"value": [
                    {"@odata.type": "#microsoft.graph.user", "id": "u1", "displayName": "Ada",
                     "userPrincipalName": "ada@contoso.com"},
                    {"@odata.type": "#microsoft.graph.device", "id": "d1", "displayName": "laptop-1"}
                ]}))
            } else if path.starts_with("/v1.0/groups/g1/members?") {
                ok(json!({"value": [
                    {"@odata.type": "#microsoft.graph.user", "id": "u1", "displayName": "Ada",
                     "userPrincipalName": "ada@contoso.com"}
                ]}))
            } else {
                error(404, "unexpected")
            }
        })
        .await;
        let client = client(&endpoint);
        let groups = client.groups().await.unwrap();
        assert_eq!(
            groups
                .iter()
                .map(|g| (g.kind_label(), g.dynamic, g.role_assignable))
                .collect::<Vec<_>>(),
            [("security", false, true), ("Microsoft 365", true, false)]
        );
        assert_eq!(client.members("g1", false).await.unwrap().len(), 1);
        let all = client.members("g1", true).await.unwrap();
        assert_eq!(
            all.iter().map(|m| m.kind.as_str()).collect::<Vec<_>>(),
            ["user", "device"]
        );
    }

    #[tokio::test]
    async fn graph_errors_say_why() {
        let (endpoint, _) =
            serve_paths(|_, _| error(403, "Insufficient privileges to complete the operation."))
                .await;
        let e = client(&endpoint)
            .principals(&["u1".to_string()])
            .await
            .unwrap_err()
            .to_string();
        assert!(
            e.contains("403") && e.contains("Insufficient privileges"),
            "{e}"
        );
    }
}
