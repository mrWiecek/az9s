//! Browsing AWS: the fetcher for an AWS context (an AWS CLI profile). The
//! profile's account is the one account; resources come from Resource
//! Explorer (or the Tagging API in the profile's region), with their region
//! standing in for a container.

use crate::loader::{ChunkSink, FetchOutcome, Fetcher, LoadRequest, LoadedData};
use crate::models::{Resource, Subscription};
pub use c9s_aws::{profiles, Profile};
use c9s_aws::{AwsClient, Session, Source};
use futures::future::BoxFuture;
use futures::FutureExt;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

pub struct AwsFetcher {
    client: Arc<AwsClient>,
    /// Documents of the resources last listed, by ARN: Resource Explorer
    /// has no lookup by ARN, so the detail view shows what the list found.
    documents: Arc<Mutex<HashMap<String, serde_json::Value>>>,
}

impl AwsFetcher {
    /// Ask AWS as CLI profile `profile`, in its region (else us-east-1).
    pub fn new(profile: &str) -> Self {
        let region = c9s_aws::profiles()
            .into_iter()
            .find(|p| p.name == profile)
            .map(|p| p.region)
            .unwrap_or_default();
        Self::with_client(AwsClient::new(Arc::new(Session::new(profile)), &region))
    }

    pub fn with_client(client: AwsClient) -> Self {
        Self {
            client: Arc::new(client),
            documents: Default::default(),
        }
    }
}

/// A resource as a row; its region is its container.
fn resource(account: &str, r: c9s_aws::AwsResource) -> Resource {
    let owner = c9s_aws::arn_parts(&r.arn)
        .map(|(_, _, account, _)| account)
        .filter(|a| !a.is_empty())
        .unwrap_or(account)
        .to_string();
    Resource {
        id: r.arn,
        name: r.name,
        resource_type: r.resource_type,
        resource_group: r.region.clone(),
        location: r.region,
        subscription_id: owner,
        tags: r.tags.into_iter().collect(),
    }
}

fn outcome(data: Result<LoadedData, String>) -> FetchOutcome {
    FetchOutcome {
        data,
        failed_subscriptions: vec![],
        notice: None,
    }
}

impl Fetcher for AwsFetcher {
    fn fetch(&self, request: LoadRequest, _sink: ChunkSink) -> BoxFuture<'static, FetchOutcome> {
        let client = self.client.clone();
        let documents = self.documents.clone();
        async move {
            match request {
                LoadRequest::Subscriptions => {
                    let profile = client.session().profile.clone();
                    outcome(
                        client
                            .session()
                            .identity()
                            .await
                            .map(|id| {
                                LoadedData::Subscriptions(vec![Subscription {
                                    id: id.account,
                                    name: profile,
                                    state: "Enabled".into(),
                                    tenant_id: String::new(),
                                }])
                            })
                            .map_err(|e| e.to_string()),
                    )
                }
                LoadRequest::Resources {
                    subscription_ids,
                    resource_group,
                } => {
                    let account = subscription_ids.first().cloned().unwrap_or_default();
                    let found = match client.resources().await {
                        Ok(found) => found,
                        Err(e) => return outcome(Err(e.to_string())),
                    };
                    let notice = match (found.source, found.complete) {
                        (Source::Tagging, _) => Some(format!(
                            "Resource Explorer isn't set up for this account: showing \
                             resources in {} from the Tagging API (untagged ones may be missing)",
                            client.region()
                        )),
                        (_, false) => Some(
                            "Resource Explorer returns 1,000 resources at most: \
                             some are not shown"
                                .to_string(),
                        ),
                        _ => None,
                    };
                    {
                        let mut docs = documents.lock().unwrap();
                        docs.clear();
                        docs.extend(
                            found
                                .resources
                                .iter()
                                .map(|r| (r.arn.clone(), r.document.clone())),
                        );
                    }
                    let mut resources: Vec<Resource> = found
                        .resources
                        .into_iter()
                        .map(|r| resource(&account, r))
                        .collect();
                    // A region picked as the container narrows the list.
                    if let Some(region) = resource_group {
                        resources.retain(|r| r.location.eq_ignore_ascii_case(&region));
                    }
                    FetchOutcome {
                        data: Ok(LoadedData::Resources(resources)),
                        failed_subscriptions: vec![],
                        notice,
                    }
                }
                LoadRequest::ResourceDetail { resource_id, .. } => {
                    let doc = documents.lock().unwrap().get(&resource_id).cloned();
                    outcome(doc.map(LoadedData::ResourceDetail).ok_or_else(|| {
                        "not in the last resource list; list the resources again (r)".to_string()
                    }))
                }
                LoadRequest::CloudContexts => outcome(
                    crate::loader::cloud_contexts()
                        .await
                        .map(LoadedData::CloudContexts),
                ),
                other => outcome(Err(format!(
                    "not available for AWS yet ({})",
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
    fn resources_map_to_rows_owned_by_their_account() {
        let r = resource(
            "111122223333",
            c9s_aws::AwsResource {
                arn: "arn:aws:ec2:eu-west-1:444455556666:instance/i-0abc".into(),
                name: "i-0abc".into(),
                resource_type: "ec2:instance".into(),
                region: "eu-west-1".into(),
                tags: vec![("env".into(), "prod".into())],
                document: serde_json::Value::Null,
            },
        );
        assert_eq!(r.resource_group, "eu-west-1", "the region is the container");
        assert_eq!(r.subscription_id, "444455556666", "the ARN's account");
        let bucket = resource(
            "111122223333",
            c9s_aws::AwsResource {
                arn: "arn:aws:s3:::logs".into(),
                name: "logs".into(),
                resource_type: "s3:bucket".into(),
                region: "global".into(),
                tags: vec![],
                document: serde_json::Value::Null,
            },
        );
        assert_eq!(
            bucket.subscription_id, "111122223333",
            "no account in the ARN"
        );
    }
}
