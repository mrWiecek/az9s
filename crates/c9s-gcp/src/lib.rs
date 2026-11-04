//! Google Cloud for c9s: the gcloud CLI's configurations (contexts) and
//! tokens, projects from Cloud Resource Manager, and resources from Cloud
//! Asset Inventory (`searchAllResources`), GCP's counterpart of Azure
//! Resource Graph. Plain REST over reqwest; no Google SDK.

use serde_json::Value;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::process::Command;
use tokio::sync::Mutex;

pub const RESOURCE_MANAGER: &str = "https://cloudresourcemanager.googleapis.com";
pub const CLOUD_ASSET: &str = "https://cloudasset.googleapis.com";
/// gcloud's tokens last an hour; refresh well before.
const TOKEN_LIFETIME: Duration = Duration::from_secs(45 * 60);
const PAGE_SIZE: u32 = 500;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error(pub String);

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;

/// A gcloud configuration: an account and a default project.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Configuration {
    pub name: String,
    pub active: bool,
    pub account: String,
    pub project: String,
}

/// A GCP project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Project {
    /// e.g. "billing-prod"
    pub id: String,
    pub name: String,
    /// "ACTIVE", "DELETE_REQUESTED", …
    pub state: String,
    /// e.g. "organizations/123" or "folders/456"
    pub parent: String,
}

/// A folder or organization: the tree above projects.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    /// e.g. "folders/456" or "organizations/123"
    pub name: String,
    pub display_name: String,
    /// The parent folder or organization; empty for an organization.
    pub parent: String,
}

/// A resource from Cloud Asset Inventory.
#[derive(Debug, Clone, PartialEq)]
pub struct Asset {
    /// Full resource name, e.g.
    /// "//compute.googleapis.com/projects/p/zones/z/instances/vm-1"
    pub name: String,
    pub display_name: String,
    /// e.g. "compute.googleapis.com/Instance"
    pub asset_type: String,
    pub location: String,
    pub labels: Vec<(String, String)>,
    /// The search result as returned, for the detail view
    pub document: Value,
}

/// Runs the gcloud CLI (`gcloud.cmd` on Windows, through `cmd /C`).
fn gcloud(program: &str, args: &[&str]) -> Command {
    let mut command = if cfg!(windows) && program == "gcloud" {
        let mut c = Command::new("cmd");
        c.args(["/C", "gcloud"]);
        c
    } else {
        Command::new(program)
    };
    command.args(args);
    command
}

async fn run(program: &str, args: &[&str]) -> Result<String> {
    let output = gcloud(program, args).output().await.map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            Error("the Google Cloud CLI (gcloud) was not found; install it and run `gcloud auth login`".into())
        } else {
            Error(format!("could not run gcloud: {e}"))
        }
    })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let first = stderr
            .lines()
            .find(|l| !l.trim().is_empty())
            .unwrap_or("unknown error")
            .trim();
        return Err(Error(format!("gcloud failed: {first}")));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// The gcloud CLI's configurations (`gcloud config configurations list`).
pub async fn configurations(program: &str) -> Result<Vec<Configuration>> {
    let text = run(
        program,
        &["config", "configurations", "list", "--format=json"],
    )
    .await?;
    let items: Value =
        serde_json::from_str(&text).map_err(|e| Error(format!("unreadable gcloud output: {e}")))?;
    Ok(items
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|c| {
            let core = c.pointer("/properties/core");
            let text =
                |v: Option<&Value>| v.and_then(Value::as_str).unwrap_or_default().to_string();
            Some(Configuration {
                name: c.get("name")?.as_str()?.to_string(),
                active: c.get("is_active").and_then(Value::as_bool).unwrap_or(false),
                account: text(core.and_then(|c| c.get("account"))),
                project: text(core.and_then(|c| c.get("project"))),
            })
        })
        .collect())
}

/// Access tokens from gcloud for one configuration, cached until shortly
/// before they expire.
pub struct Tokens {
    program: String,
    configuration: Option<String>,
    cached: Mutex<Option<(String, Instant)>>,
}

impl Tokens {
    pub fn new(configuration: Option<String>) -> Self {
        Self::with_program("gcloud", configuration)
    }

    /// Tests point `program` at a fake gcloud.
    pub fn with_program(program: impl Into<String>, configuration: Option<String>) -> Self {
        Self {
            program: program.into(),
            configuration,
            cached: Mutex::new(None),
        }
    }

    pub async fn token(&self) -> Result<String> {
        let mut cached = self.cached.lock().await;
        if let Some((token, at)) = cached.as_ref() {
            if at.elapsed() < TOKEN_LIFETIME {
                return Ok(token.clone());
            }
        }
        let flag = self
            .configuration
            .as_ref()
            .map(|c| format!("--configuration={c}"));
        let mut args = vec!["auth", "print-access-token"];
        if let Some(flag) = &flag {
            args.push(flag);
        }
        let token = run(&self.program, &args).await?;
        if token.is_empty() {
            return Err(Error(
                "gcloud returned an empty token; run `gcloud auth login`".into(),
            ));
        }
        *cached = Some((token.clone(), Instant::now()));
        Ok(token)
    }
}

/// Google Cloud REST calls for c9s.
pub struct GcpClient {
    http: reqwest::Client,
    tokens: Arc<Tokens>,
    resource_manager: String,
    cloud_asset: String,
}

impl GcpClient {
    pub fn new(tokens: Arc<Tokens>) -> Self {
        Self::with_endpoints(tokens, RESOURCE_MANAGER, CLOUD_ASSET)
    }

    pub fn with_endpoints(tokens: Arc<Tokens>, resource_manager: &str, cloud_asset: &str) -> Self {
        Self {
            http: reqwest::Client::new(),
            tokens,
            resource_manager: resource_manager.trim_end_matches('/').to_string(),
            cloud_asset: cloud_asset.trim_end_matches('/').to_string(),
        }
    }

    async fn get(&self, url: &str) -> Result<Value> {
        let token = self.tokens.token().await?;
        let response = self
            .http
            .get(url)
            .bearer_auth(token)
            .send()
            .await
            .map_err(|e| Error(format!("Google Cloud request failed: {e}")))?;
        let status = response.status();
        let text = response
            .text()
            .await
            .map_err(|e| Error(format!("Google Cloud response: {e}")))?;
        if !status.is_success() {
            let message = serde_json::from_str::<Value>(&text)
                .ok()
                .and_then(|v| v.pointer("/error/message")?.as_str().map(str::to_string))
                .unwrap_or_else(|| text.chars().take(300).collect());
            return Err(Error(format!("HTTP {}: {message}", status.as_u16())));
        }
        serde_json::from_str(&text)
            .map_err(|e| Error(format!("invalid Google Cloud response: {e}")))
    }

    /// Every page of a list: `key` holds the items, `nextPageToken` the next page.
    async fn get_all(&self, base: &str, key: &str) -> Result<Vec<Value>> {
        let mut items = Vec::new();
        let mut token: Option<String> = None;
        loop {
            let sep = if base.contains('?') { '&' } else { '?' };
            let url = match &token {
                Some(t) => format!("{base}{sep}pageToken={t}"),
                None => base.to_string(),
            };
            let page = self.get(&url).await?;
            if let Some(Value::Array(found)) = page.get(key) {
                items.extend(found.iter().cloned());
            }
            match page.get("nextPageToken").and_then(Value::as_str) {
                Some(t) if !t.is_empty() => token = Some(t.to_string()),
                _ => return Ok(items),
            }
        }
    }

    /// Organizations the login can see (`organizations/123`, display name
    /// usually the domain).
    pub async fn organizations(&self) -> Result<Vec<Node>> {
        let url = format!(
            "{}/v3/organizations:search?pageSize={PAGE_SIZE}",
            self.resource_manager
        );
        Ok(nodes(self.get_all(&url, "organizations").await?))
    }

    /// Active folders the login can see, with their parent folder or
    /// organization.
    pub async fn folders(&self) -> Result<Vec<Node>> {
        let url = format!(
            "{}/v3/folders:search?pageSize={PAGE_SIZE}",
            self.resource_manager
        );
        Ok(nodes(self.get_all(&url, "folders").await?))
    }

    /// Every project the login can see.
    pub async fn projects(&self) -> Result<Vec<Project>> {
        let url = format!(
            "{}/v3/projects:search?pageSize={PAGE_SIZE}",
            self.resource_manager
        );
        let text = |v: &Value, k: &str| {
            v.get(k)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        Ok(self
            .get_all(&url, "projects")
            .await?
            .iter()
            .filter_map(|p| {
                let id = text(p, "projectId");
                (!id.is_empty()).then(|| Project {
                    name: Some(text(p, "displayName"))
                        .filter(|n| !n.is_empty())
                        .unwrap_or_else(|| id.clone()),
                    state: text(p, "state"),
                    parent: text(p, "parent"),
                    id,
                })
            })
            .collect())
    }

    /// Resources in a project, optionally narrowed by a Cloud Asset query
    /// (e.g. `name="//compute.googleapis.com/…"`).
    pub async fn assets(&self, project: &str, query: Option<&str>) -> Result<Vec<Asset>> {
        let mut url = format!(
            "{}/v1/projects/{project}:searchAllResources?pageSize={PAGE_SIZE}",
            self.cloud_asset
        );
        if let Some(q) = query {
            url.push_str("&query=");
            url.push_str(&encode(q));
        }
        let text = |v: &Value, k: &str| {
            v.get(k)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        Ok(self
            .get_all(&url, "results")
            .await?
            .into_iter()
            .filter_map(|r| {
                let name = text(&r, "name");
                if name.is_empty() {
                    return None;
                }
                let labels = r
                    .get("labels")
                    .and_then(Value::as_object)
                    .map(|m| {
                        m.iter()
                            .map(|(k, v)| (k.clone(), v.as_str().unwrap_or_default().to_string()))
                            .collect()
                    })
                    .unwrap_or_default();
                Some(Asset {
                    display_name: Some(text(&r, "displayName"))
                        .filter(|n| !n.is_empty())
                        .unwrap_or_else(|| name.rsplit('/').next().unwrap_or(&name).to_string()),
                    asset_type: text(&r, "assetType"),
                    location: text(&r, "location"),
                    labels,
                    name,
                    document: r,
                })
            })
            .collect())
    }
}

/// Percent-encode a query string value.
/// Folders or organizations from Resource Manager, skipping deleted ones.
fn nodes(items: Vec<Value>) -> Vec<Node> {
    let text = |v: &Value, k: &str| {
        v.get(k)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    items
        .iter()
        .filter(|n| {
            matches!(
                n.get("state").and_then(Value::as_str),
                None | Some("ACTIVE")
            )
        })
        .filter_map(|n| {
            let name = text(n, "name");
            (!name.is_empty()).then(|| Node {
                display_name: Some(text(n, "displayName"))
                    .filter(|d| !d.is_empty())
                    .unwrap_or_else(|| name.clone()),
                parent: text(n, "parent"),
                name,
            })
        })
        .collect()
}

fn encode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// The project a full resource name belongs to, e.g. "p" from
/// "//compute.googleapis.com/projects/p/zones/z/instances/i".
pub fn project_of(name: &str) -> Option<&str> {
    let rest = name.split("/projects/").nth(1)?;
    rest.split('/').next()
}

#[cfg(test)]
mod tests {
    use super::*;
    use c9s_azure::testing::{error, ok, serve_paths};
    use serde_json::json;

    #[cfg(unix)]
    fn fake_gcloud(dir: &std::path::Path, body: &str) -> String {
        use std::os::unix::fs::PermissionsExt;
        let path = dir.join("gcloud");
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path.to_string_lossy().into_owned()
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn configurations_and_tokens_come_from_gcloud() {
        let dir = tempfile::tempdir().unwrap();
        let program = fake_gcloud(
            dir.path(),
            r#"case "$*" in
  "config configurations list --format=json") echo '[{"name":"work","is_active":true,"properties":{"core":{"account":"ada@acme.com","project":"billing-prod"}}},{"name":"lab","is_active":false,"properties":{"core":{}}}]' ;;
  "auth print-access-token --configuration=work") echo "token-for-work" ;;
  *) echo "unexpected: $*" >&2; exit 1 ;;
esac"#,
        );
        let configs = configurations(&program).await.unwrap();
        assert_eq!(
            configs[0],
            Configuration {
                name: "work".into(),
                active: true,
                account: "ada@acme.com".into(),
                project: "billing-prod".into()
            }
        );
        assert_eq!(configs[1].project, "", "missing properties are empty");
        let tokens = Tokens::with_program(program.clone(), Some("work".into()));
        assert_eq!(tokens.token().await.unwrap(), "token-for-work");
        let missing = Tokens::with_program(dir.path().join("nope").to_string_lossy(), None);
        assert!(missing.token().await.unwrap_err().0.contains("not found"));
    }

    fn client(endpoint: &str, dir: &std::path::Path) -> GcpClient {
        #[cfg(unix)]
        let program = fake_gcloud(dir, "echo t");
        #[cfg(not(unix))]
        let program = {
            let _ = dir;
            "gcloud".to_string()
        };
        GcpClient::with_endpoints(
            Arc::new(Tokens::with_program(program, None)),
            endpoint,
            endpoint,
        )
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn projects_and_assets_are_listed_across_pages() {
        let (endpoint, _) = serve_paths(|path, _| {
            if path.starts_with("/v3/projects:search") {
                return ok(json!({"projects": [
                    {"projectId": "billing-prod", "displayName": "Billing", "state": "ACTIVE", "parent": "folders/1"},
                    {"projectId": "lab", "state": "ACTIVE"}
                ]}));
            }
            if path.starts_with("/v1/projects/billing-prod:searchAllResources") {
                if path.contains("pageToken=p2") {
                    return ok(json!({"results": [
                        {"name": "//storage.googleapis.com/billing-logs", "assetType": "storage.googleapis.com/Bucket",
                         "location": "eu"}
                    ]}));
                }
                assert!(!path.contains("query=") || path.contains("query=name%3D%22"), "{path}");
                return ok(json!({"results": [
                    {"name": "//compute.googleapis.com/projects/billing-prod/zones/europe-west1-b/instances/vm-1",
                     "displayName": "vm-1", "assetType": "compute.googleapis.com/Instance",
                     "location": "europe-west1-b", "labels": {"env": "prod"}}
                ], "nextPageToken": "p2"}));
            }
            error(404, "unexpected")
        })
        .await;
        let dir = tempfile::tempdir().unwrap();
        let client = client(&endpoint, dir.path());
        let projects = client.projects().await.unwrap();
        assert_eq!(
            projects.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(),
            ["Billing", "lab"]
        );
        let assets = client.assets("billing-prod", None).await.unwrap();
        assert_eq!(assets.len(), 2, "both pages");
        assert_eq!(assets[0].labels, [("env".to_string(), "prod".to_string())]);
        assert_eq!(
            assets[1].display_name, "billing-logs",
            "name's last part without a display name"
        );
        assert_eq!(project_of(&assets[0].name), Some("billing-prod"));
        let err = client.assets("other", None).await.unwrap_err();
        assert!(err.0.contains("404"), "{err}");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn organizations_and_active_folders_are_listed() {
        let (endpoint, _) = serve_paths(|path, _| {
            if path.starts_with("/v3/organizations:search") {
                return ok(json!({"organizations": [
                    {"name": "organizations/1", "displayName": "acme.com", "state": "ACTIVE"}
                ]}));
            }
            if path.starts_with("/v3/folders:search") {
                if path.contains("pageToken=f2") {
                    return ok(json!({"folders": [
                        {"name": "folders/3", "displayName": "Old", "parent": "folders/2",
                         "state": "DELETE_REQUESTED"}
                    ]}));
                }
                return ok(json!({"folders": [
                    {"name": "folders/2", "displayName": "Prod", "parent": "organizations/1",
                     "state": "ACTIVE"},
                    {"name": "folders/4", "parent": "folders/2"}
                ], "nextPageToken": "f2"}));
            }
            error(404, "unexpected")
        })
        .await;
        let dir = tempfile::tempdir().unwrap();
        let client = client(&endpoint, dir.path());
        let orgs = client.organizations().await.unwrap();
        assert_eq!(
            orgs,
            [Node {
                name: "organizations/1".into(),
                display_name: "acme.com".into(),
                parent: String::new()
            }]
        );
        let folders = client.folders().await.unwrap();
        assert_eq!(
            folders
                .iter()
                .map(|f| (f.name.as_str(), f.display_name.as_str(), f.parent.as_str()))
                .collect::<Vec<_>>(),
            [
                ("folders/2", "Prod", "organizations/1"),
                ("folders/4", "folders/4", "folders/2")
            ],
            "deleted folders are left out; a missing display name falls back to the name"
        );
    }

    #[test]
    fn queries_are_encoded() {
        assert_eq!(encode(r#"name="//a/b""#), "name%3D%22%2F%2Fa%2Fb%22");
        assert_eq!(project_of("//x/projects/p1/y"), Some("p1"));
        assert_eq!(project_of("//storage.googleapis.com/bucket"), None);
    }
}
