//! Browsing Google Cloud: the fetcher for a GCP context (a gcloud
//! configuration). Projects are the accounts; resources come from Cloud
//! Asset Inventory, with their location standing in for a container.

use crate::loader::{ChunkSink, FetchOutcome, Fetcher, LoadRequest, LoadedData};
use crate::models::{Resource, Subscription};
use crate::scope_tree::{GroupNode, ScopeTree, SubscriptionNode};
use c9s_gcp::{GcpClient, Tokens};
use futures::future::BoxFuture;
use futures::{stream, FutureExt, StreamExt};
use std::sync::Arc;

/// Projects searched for resources at once.
const CONCURRENT_PROJECTS: usize = 8;

pub struct GcpFetcher {
    client: Arc<GcpClient>,
}

impl GcpFetcher {
    /// Ask Google Cloud as gcloud configuration `configuration` (`None`:
    /// gcloud's active one).
    pub fn new(configuration: Option<String>) -> Self {
        Self::with_client(GcpClient::new(Arc::new(Tokens::new(configuration))))
    }

    pub fn with_client(client: GcpClient) -> Self {
        Self {
            client: Arc::new(client),
        }
    }
}

/// A project as an account row.
fn account(p: c9s_gcp::Project) -> Subscription {
    Subscription {
        state: if p.state.eq_ignore_ascii_case("ACTIVE") || p.state.is_empty() {
            "Enabled".into()
        } else {
            p.state
        },
        id: p.id,
        name: p.name,
        tenant_id: String::new(),
    }
}

/// An asset as a resource row; its location is its container.
fn resource(project: &str, a: c9s_gcp::Asset) -> Resource {
    Resource {
        id: a.name,
        name: a.display_name,
        resource_type: a.asset_type,
        resource_group: a.location.clone(),
        location: a.location,
        subscription_id: project.to_string(),
        tags: a.labels.into_iter().collect(),
    }
}

/// Root of the tree when the login sees no organization, or several.
pub const ALL_ORGANIZATIONS: &str = "~gcp";

/// The tree above projects: one organization is the root; with none or
/// several, a root above them holds each organization and the projects
/// outside any.
fn tree(
    organizations: Vec<c9s_gcp::Node>,
    folders: Vec<c9s_gcp::Node>,
    projects: &[c9s_gcp::Project],
) -> ScopeTree {
    let (root, root_display_name, groups_from_orgs) = match organizations.as_slice() {
        [only] => (only.name.clone(), only.display_name.clone(), vec![]),
        _ => (
            ALL_ORGANIZATIONS.to_string(),
            if organizations.is_empty() {
                "No organization".to_string()
            } else {
                "All organizations".to_string()
            },
            organizations,
        ),
    };
    let mut known: std::collections::HashSet<String> = folders
        .iter()
        .chain(&groups_from_orgs)
        .map(|n| n.name.clone())
        .collect();
    known.insert(root.clone());
    // A parent the login can't see hangs off the root, so nothing is lost.
    let parent_of = |parent: &str| {
        if known.contains(parent) {
            parent.to_string()
        } else {
            root.clone()
        }
    };
    let groups = groups_from_orgs
        .iter()
        .map(|o| GroupNode {
            name: o.name.clone(),
            display_name: o.display_name.clone(),
            parent: root.clone(),
        })
        .chain(folders.iter().map(|f| GroupNode {
            name: f.name.clone(),
            display_name: f.display_name.clone(),
            parent: parent_of(&f.parent),
        }))
        .collect();
    let subscriptions = projects
        .iter()
        .map(|p| SubscriptionNode {
            id: p.id.to_ascii_lowercase(),
            display_name: p.name.clone(),
            parent: parent_of(&p.parent),
        })
        .collect();
    ScopeTree {
        root,
        root_display_name,
        groups,
        subscriptions,
    }
}

fn outcome(data: Result<LoadedData, String>) -> FetchOutcome {
    FetchOutcome {
        data,
        failed_subscriptions: vec![],
        notice: None,
    }
}

impl Fetcher for GcpFetcher {
    fn fetch(&self, request: LoadRequest, _sink: ChunkSink) -> BoxFuture<'static, FetchOutcome> {
        let client = self.client.clone();
        async move {
            match request {
                LoadRequest::Subscriptions => outcome(
                    client
                        .projects()
                        .await
                        .map(|p| LoadedData::Subscriptions(p.into_iter().map(account).collect()))
                        .map_err(|e| e.to_string()),
                ),
                LoadRequest::Resources {
                    subscription_ids,
                    resource_group,
                } => {
                    let results: Vec<(String, Result<Vec<Resource>, String>)> =
                        stream::iter(subscription_ids)
                            .map(|project| {
                                let client = client.clone();
                                async move {
                                    let found = client
                                        .assets(&project, None)
                                        .await
                                        .map(|a| {
                                            a.into_iter().map(|a| resource(&project, a)).collect()
                                        })
                                        .map_err(|e| e.to_string());
                                    (project, found)
                                }
                            })
                            .buffer_unordered(CONCURRENT_PROJECTS)
                            .collect()
                            .await;
                    let mut resources = Vec::new();
                    let mut failed = Vec::new();
                    let mut first_error = None;
                    for (project, found) in results {
                        match found {
                            Ok(r) => resources.extend(r),
                            Err(e) => {
                                first_error.get_or_insert(e);
                                failed.push(project);
                            }
                        }
                    }
                    // A location picked as the container narrows the list.
                    if let Some(location) = resource_group {
                        resources.retain(|r| r.location.eq_ignore_ascii_case(&location));
                    }
                    let data = match (resources.is_empty(), first_error) {
                        (true, Some(e)) if !failed.is_empty() => Err(e),
                        _ => Ok(LoadedData::Resources(resources)),
                    };
                    FetchOutcome {
                        data,
                        failed_subscriptions: failed,
                        notice: None,
                    }
                }
                LoadRequest::ResourceDetail {
                    resource_id,
                    subscription_id,
                } => {
                    let project = if subscription_id.is_empty() {
                        c9s_gcp::project_of(&resource_id)
                            .unwrap_or_default()
                            .to_string()
                    } else {
                        subscription_id
                    };
                    let query = format!("name=\"{resource_id}\"");
                    let data = match client.assets(&project, Some(&query)).await {
                        Ok(found) => found
                            .into_iter()
                            .find(|a| a.name == resource_id)
                            .map(|a| LoadedData::ResourceDetail(a.document))
                            .ok_or_else(|| "not found in Cloud Asset Inventory".to_string()),
                        Err(e) => Err(e.to_string()),
                    };
                    outcome(data)
                }
                LoadRequest::ScopeTree => {
                    // Organizations and folders may be hidden from the
                    // login (no permission): projects then hang off the root.
                    let (orgs, folders, projects) =
                        futures::join!(client.organizations(), client.folders(), client.projects());
                    let data = match projects {
                        Ok(projects) => Ok(LoadedData::ScopeTree(tree(
                            orgs.unwrap_or_else(|e| {
                                tracing::warn!("GCP organizations: {e}");
                                vec![]
                            }),
                            folders.unwrap_or_else(|e| {
                                tracing::warn!("GCP folders: {e}");
                                vec![]
                            }),
                            &projects,
                        ))),
                        Err(e) => Err(e.to_string()),
                    };
                    outcome(data)
                }
                LoadRequest::CloudContexts => outcome(
                    c9s_gcp::configurations("gcloud")
                        .await
                        .map(LoadedData::CloudContexts)
                        .map_err(|e| e.to_string()),
                ),
                other => outcome(Err(format!(
                    "not available for Google Cloud yet ({})",
                    other.kind().label()
                ))),
            }
        }
        .boxed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projects_and_assets_map_to_rows() {
        let a = account(c9s_gcp::Project {
            id: "billing-prod".into(),
            name: "Billing".into(),
            state: "ACTIVE".into(),
            parent: String::new(),
        });
        assert_eq!(
            (a.id.as_str(), a.state.as_str()),
            ("billing-prod", "Enabled")
        );
        let r = resource(
            "billing-prod",
            c9s_gcp::Asset {
                name: "//compute.googleapis.com/projects/billing-prod/zones/z/instances/vm-1"
                    .into(),
                display_name: "vm-1".into(),
                asset_type: "compute.googleapis.com/Instance".into(),
                location: "europe-west1-b".into(),
                labels: vec![("env".into(), "prod".into())],
                document: serde_json::Value::Null,
            },
        );
        assert_eq!(
            r.resource_group, "europe-west1-b",
            "the location is the container"
        );
        assert_eq!(r.subscription_id, "billing-prod");
        assert_eq!(r.tags.get("env").map(String::as_str), Some("prod"));
    }

    fn node(name: &str, display: &str, parent: &str) -> c9s_gcp::Node {
        c9s_gcp::Node {
            name: name.into(),
            display_name: display.into(),
            parent: parent.into(),
        }
    }

    fn project(id: &str, parent: &str) -> c9s_gcp::Project {
        c9s_gcp::Project {
            id: id.into(),
            name: id.into(),
            state: "ACTIVE".into(),
            parent: parent.into(),
        }
    }

    #[test]
    fn one_organization_is_the_root_of_folders_and_projects() {
        let t = tree(
            vec![node("organizations/1", "acme.com", "")],
            vec![
                node("folders/2", "Prod", "organizations/1"),
                node("folders/3", "Payments", "folders/2"),
                node("folders/9", "Hidden child", "folders/8"),
            ],
            &[
                project("pay-prod", "folders/3"),
                project("sandbox", "organizations/1"),
                project("orphan", "folders/404"),
            ],
        );
        assert_eq!(
            (t.root.as_str(), t.root_display_name.as_str()),
            ("organizations/1", "acme.com")
        );
        let parent = |name: &str| {
            t.groups
                .iter()
                .find(|g| g.name == name)
                .map(|g| g.parent.clone())
                .or_else(|| {
                    t.subscriptions
                        .iter()
                        .find(|s| s.id == name)
                        .map(|s| s.parent.clone())
                })
                .unwrap()
        };
        assert_eq!(parent("folders/3"), "folders/2");
        assert_eq!(parent("pay-prod"), "folders/3");
        assert_eq!(
            parent("folders/9"),
            "organizations/1",
            "unseen parent: the root"
        );
        assert_eq!(parent("orphan"), "organizations/1");
        assert_eq!(t.subscriptions_under("folders/2", &[]), ["pay-prod"]);
    }

    #[test]
    fn without_one_organization_a_root_holds_them_all() {
        let none = tree(vec![], vec![], &[project("lab", "")]);
        assert_eq!(none.root, ALL_ORGANIZATIONS);
        assert_eq!(none.root_display_name, "No organization");
        assert_eq!(none.subscriptions[0].parent, ALL_ORGANIZATIONS);
        let two = tree(
            vec![
                node("organizations/1", "a.com", ""),
                node("organizations/2", "b.com", ""),
            ],
            vec![],
            &[project("b-prod", "organizations/2")],
        );
        assert_eq!(two.root_display_name, "All organizations");
        assert_eq!(two.groups.len(), 2);
        assert_eq!(two.subscriptions_under("organizations/2", &[]), ["b-prod"]);
    }
}
