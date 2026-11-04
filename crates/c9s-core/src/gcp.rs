//! Browsing Google Cloud: the fetcher for a GCP context (a gcloud
//! configuration). Projects are the accounts; resources come from Cloud
//! Asset Inventory, with their location standing in for a container.

use crate::loader::{ChunkSink, FetchOutcome, Fetcher, LoadRequest, LoadedData};
use crate::models::{Resource, Subscription};
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
}
