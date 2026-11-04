//! Browsing AWS: the fetcher for an AWS context (an AWS CLI profile).
//! The accounts are the profile's own, or, when the profile can read AWS
//! Organizations (the management account or a delegated administrator),
//! every account in the organization, with its units as the tree. Another
//! account is reached through a CLI profile for it (an SSO profile with
//! its `sso_account_id`, say), else by assuming a role in it
//! (`OrganizationAccountAccessRole` unless the config names another).
//! Resources come from Resource Explorer (or the Tagging API in the
//! profile's region), with their region standing in for a container.

use crate::loader::{ChunkSink, FetchOutcome, Fetcher, LoadRequest, LoadedData};
use crate::models::{Resource, Subscription};
use crate::scope_tree::{GroupNode, ScopeTree, SubscriptionNode};
pub use c9s_aws::{profiles, Profile};
use c9s_aws::{AwsClient, Organization, Session, Source};
use futures::future::BoxFuture;
use futures::{stream, FutureExt, StreamExt};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// The role assumed in other accounts unless the config names another:
/// the one AWS Organizations creates in accounts it creates.
pub const DEFAULT_ROLE: &str = "OrganizationAccountAccessRole";

/// Accounts searched for resources at once.
const CONCURRENT_ACCOUNTS: usize = 8;

pub struct AwsFetcher {
    inner: Arc<Inner>,
}

struct Inner {
    /// The profile's own client.
    client: Arc<AwsClient>,
    /// Role name assumed in accounts without a profile.
    role: String,
    /// CLI profiles, to find one for an account.
    profiles: Vec<Profile>,
    own_account: tokio::sync::Mutex<Option<String>>,
    organization: Mutex<Option<Organization>>,
    clients: Mutex<HashMap<String, Arc<AwsClient>>>,
    /// Documents of the resources last listed, by ARN: Resource Explorer
    /// has no lookup by ARN, so the detail view shows what the list found.
    documents: Mutex<HashMap<String, serde_json::Value>>,
}

impl AwsFetcher {
    /// Ask AWS as CLI profile `profile`, in its region (else us-east-1);
    /// other accounts through `role` (else [`DEFAULT_ROLE`]).
    pub fn new(profile: &str, role: Option<&str>) -> Self {
        let profiles = c9s_aws::profiles();
        let region = profiles
            .iter()
            .find(|p| p.name == profile)
            .map(|p| p.region.clone())
            .unwrap_or_default();
        Self::with_client(AwsClient::new(Arc::new(Session::new(profile)), &region))
            .with_profiles(profiles)
            .with_role(role.unwrap_or(DEFAULT_ROLE))
    }

    pub fn with_client(client: AwsClient) -> Self {
        Self {
            inner: Arc::new(Inner {
                client: Arc::new(client),
                role: DEFAULT_ROLE.into(),
                profiles: vec![],
                own_account: Default::default(),
                organization: Default::default(),
                clients: Default::default(),
                documents: Default::default(),
            }),
        }
    }

    fn inner_mut(&mut self) -> &mut Inner {
        Arc::get_mut(&mut self.inner).expect("configured before use")
    }

    pub fn with_profiles(mut self, profiles: Vec<Profile>) -> Self {
        self.inner_mut().profiles = profiles;
        self
    }

    pub fn with_role(mut self, role: &str) -> Self {
        let role = role.trim();
        if !role.is_empty() {
            self.inner_mut().role = role.to_string();
        }
        self
    }
}

impl Inner {
    /// The profile's own account (asked once).
    async fn own_account(&self) -> Result<String, String> {
        let mut own = self.own_account.lock().await;
        if let Some(id) = own.as_ref() {
            return Ok(id.clone());
        }
        let id = self
            .client
            .session()
            .identity()
            .await
            .map_err(|e| e.to_string())?
            .account;
        *own = Some(id.clone());
        Ok(id)
    }

    /// How to reach `account`: the profile itself, a CLI profile for that
    /// account, else the role assumed in it.
    fn client_for(&self, account: &str, own: &str) -> Arc<AwsClient> {
        if account == own {
            return self.client.clone();
        }
        let mut clients = self.clients.lock().unwrap();
        clients
            .entry(account.to_string())
            .or_insert_with(|| {
                let profile = self
                    .profiles
                    .iter()
                    .filter(|p| p.account_id == account)
                    // SSO profiles first: they need no role in the account.
                    .min_by_key(|p| p.kind != "sso");
                let session = match profile {
                    Some(p) => self.client.session().sibling(p.name.clone()),
                    None => self
                        .client
                        .assume_session(&format!("arn:aws:iam::{account}:role/{}", self.role)),
                };
                Arc::new(self.client.with_session(Arc::new(session)))
            })
            .clone()
    }

    /// The organization, asked once per context.
    async fn organization(&self) -> Result<Organization, String> {
        if let Some(org) = self.organization.lock().unwrap().clone() {
            return Ok(org);
        }
        let org = self.client.organization().await.map_err(|e| {
            let e = e.to_string();
            if e.contains("AccessDenied") || e.contains("NotInUse") || e.contains("HTTP 403") {
                "this profile can't read AWS Organizations (it needs the management \
                 account or a delegated administrator)"
                    .to_string()
            } else {
                e
            }
        })?;
        *self.organization.lock().unwrap() = Some(org.clone());
        Ok(org)
    }
}

/// The organization's accounts as rows; suspended ones as disabled.
fn accounts(org: &Organization) -> Vec<Subscription> {
    org.accounts
        .iter()
        .map(|a| Subscription {
            id: a.id.clone(),
            name: if a.name.is_empty() {
                a.id.clone()
            } else {
                a.name.clone()
            },
            state: if a.status.is_empty() || a.status.eq_ignore_ascii_case("ACTIVE") {
                "Enabled".into()
            } else {
                "Disabled".into()
            },
            tenant_id: String::new(),
        })
        .collect()
}

/// The organization as the tree: its root, units and accounts.
fn tree(org: &Organization) -> ScopeTree {
    let root = org.roots.first();
    let root_id = root.map(|r| r.id.clone()).unwrap_or_default();
    ScopeTree {
        root: root_id.clone(),
        root_display_name: root
            .map(|r| format!("{} (organization)", r.name))
            .unwrap_or_else(|| "Organization".into()),
        groups: org
            .units
            .iter()
            .map(|u| GroupNode {
                name: u.id.clone(),
                display_name: u.name.clone(),
                parent: u.parent.clone(),
            })
            .collect(),
        subscriptions: org
            .accounts
            .iter()
            .map(|a| SubscriptionNode {
                id: a.id.to_ascii_lowercase(),
                display_name: if a.name.is_empty() {
                    a.id.clone()
                } else {
                    a.name.clone()
                },
                parent: a.parent.clone(),
            })
            .collect(),
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
        let inner = self.inner.clone();
        async move {
            match request {
                LoadRequest::Subscriptions => {
                    let own = match inner.own_account().await {
                        Ok(own) => own,
                        Err(e) => return outcome(Err(e)),
                    };
                    let mut rows = match inner.organization().await {
                        Ok(org) => accounts(&org),
                        Err(e) => {
                            tracing::debug!("No AWS Organizations: {e}");
                            vec![]
                        }
                    };
                    // The profile's own account, named after the profile
                    // when the organization isn't readable.
                    if !rows.iter().any(|r| r.id == own) {
                        rows.insert(
                            0,
                            Subscription {
                                id: own,
                                name: inner.client.session().profile.clone(),
                                state: "Enabled".into(),
                                tenant_id: String::new(),
                            },
                        );
                    }
                    outcome(Ok(LoadedData::Subscriptions(rows)))
                }
                LoadRequest::ScopeTree => {
                    outcome(inner.organization().await.map(|org| LoadedData::ScopeTree(tree(&org))))
                }
                LoadRequest::Resources {
                    subscription_ids,
                    resource_group,
                } => {
                    let own = match inner.own_account().await {
                        Ok(own) => own,
                        Err(e) => return outcome(Err(e)),
                    };
                    let ids = if subscription_ids.is_empty() {
                        vec![own.clone()]
                    } else {
                        subscription_ids
                    };
                    let results: Vec<(String, Result<c9s_aws::Found, String>)> =
                        stream::iter(ids)
                            .map(|account| {
                                let client = inner.client_for(&account, &own);
                                async move {
                                    let found =
                                        client.resources().await.map_err(|e| e.to_string());
                                    (account, found)
                                }
                            })
                            .buffer_unordered(CONCURRENT_ACCOUNTS)
                            .collect()
                            .await;
                    let asked = results.len();
                    let mut resources = Vec::new();
                    let mut failed = Vec::new();
                    let mut first_error = None;
                    let mut tagging = 0;
                    let mut capped = 0;
                    {
                        let mut docs = inner.documents.lock().unwrap();
                        docs.clear();
                        for (account, found) in results {
                            match found {
                                Ok(found) => {
                                    tagging += usize::from(found.source == Source::Tagging);
                                    capped += usize::from(!found.complete);
                                    for r in found.resources {
                                        docs.insert(r.arn.clone(), r.document.clone());
                                        resources.push(resource(&account, r));
                                    }
                                }
                                Err(e) => {
                                    tracing::warn!("AWS account {account}: {e}");
                                    first_error.get_or_insert(e);
                                    failed.push(account);
                                }
                            }
                        }
                    }
                    // A region picked as the container narrows the list.
                    if let Some(region) = resource_group {
                        resources.retain(|r| r.location.eq_ignore_ascii_case(&region));
                    }
                    let region = inner.client.region().to_string();
                    let notice = if capped > 0 {
                        Some(
                            "Resource Explorer returns 1,000 resources at most per account: \
                             some are not shown"
                                .to_string(),
                        )
                    } else if tagging == 1 && asked == 1 {
                        Some(format!(
                            "Resource Explorer isn't set up for this account: showing \
                             resources in {region} from the Tagging API (untagged ones may be missing)"
                        ))
                    } else if tagging > 0 {
                        Some(format!(
                            "{tagging} of {asked} accounts without Resource Explorer: their \
                             resources in {region} come from the Tagging API (untagged ones may be missing)"
                        ))
                    } else {
                        None
                    };
                    let data = match (resources.is_empty(), first_error) {
                        (true, Some(e)) if !failed.is_empty() => Err(e),
                        _ => Ok(LoadedData::Resources(resources)),
                    };
                    FetchOutcome {
                        data,
                        failed_subscriptions: failed,
                        notice,
                    }
                }
                LoadRequest::ResourceDetail { resource_id, .. } => {
                    let doc = inner.documents.lock().unwrap().get(&resource_id).cloned();
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
