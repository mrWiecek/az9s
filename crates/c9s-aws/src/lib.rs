//! Browsing AWS for c9s over plain HTTPS (no AWS SDK): profiles from the
//! AWS CLI's config files, credentials from `aws configure
//! export-credentials`, the account from `aws sts get-caller-identity`, and
//! resources from Resource Explorer (or, without it, the Resource Groups
//! Tagging API in the profile's region). Requests are signed with SigV4.

pub mod sigv4;

use serde_json::{json, Value};
use sigv4::Credentials;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::process::Command;
use tokio::sync::Mutex;

/// Credentials without an expiry are asked for again after this long.
const CREDENTIALS_LIFETIME: Duration = Duration::from_secs(10 * 60);
/// Resource Explorer's largest page; a search returns 1,000 results at most.
const SEARCH_PAGE: u32 = 1000;
/// The region used when a profile names none.
pub const DEFAULT_REGION: &str = "us-east-1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error(pub String);

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;

/// An AWS CLI profile: a context in `:ctx`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Profile {
    pub name: String,
    /// The profile's region, if it names one.
    pub region: String,
    /// "sso" for IAM Identity Center profiles, "role" for assumed roles,
    /// "keys" for access keys.
    pub kind: String,
}

/// The account behind a profile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identity {
    pub account: String,
    pub arn: String,
}

/// A resource from Resource Explorer or the Tagging API.
#[derive(Debug, Clone, PartialEq)]
pub struct AwsResource {
    pub arn: String,
    pub name: String,
    /// e.g. "ec2:instance", "s3:bucket"
    pub resource_type: String,
    /// The region, or "global"
    pub region: String,
    pub tags: Vec<(String, String)>,
    /// The result as returned, for the detail view
    pub document: Value,
}

/// Resources found, and whether that is all of them.
#[derive(Debug, Clone, PartialEq)]
pub struct Found {
    pub resources: Vec<AwsResource>,
    /// Resource Explorer returns at most 1,000 results for a search.
    pub complete: bool,
    /// Where they came from, for the status bar.
    pub source: Source,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    ResourceExplorer,
    /// Tagging API in one region (no Resource Explorer index).
    Tagging,
}

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

fn aws_file(variable: &str, name: &str) -> Option<PathBuf> {
    std::env::var_os(variable)
        .map(PathBuf::from)
        .or_else(|| home().map(|h| h.join(".aws").join(name)))
}

/// The profiles in the AWS CLI's config and credentials files
/// (`AWS_CONFIG_FILE`, `AWS_SHARED_CREDENTIALS_FILE`, else `~/.aws/`).
/// No files: no profiles.
pub fn profiles() -> Vec<Profile> {
    let read = |p: Option<PathBuf>| {
        p.and_then(|p| std::fs::read_to_string(p).ok())
            .unwrap_or_default()
    };
    parse_profiles(
        &read(aws_file("AWS_CONFIG_FILE", "config")),
        &read(aws_file("AWS_SHARED_CREDENTIALS_FILE", "credentials")),
    )
}

/// Profiles from the text of the config file (`[default]`, `[profile x]`)
/// and the credentials file (`[x]`), in that order, each once.
pub fn parse_profiles(config: &str, credentials: &str) -> Vec<Profile> {
    let mut out: Vec<Profile> = Vec::new();
    let mut parse = |text: &str, in_config: bool| {
        let mut current: Option<usize> = None;
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
                continue;
            }
            if let Some(section) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
                let section = section.trim();
                let name = if in_config {
                    match section.strip_prefix("profile ") {
                        Some(n) => Some(n.trim()),
                        None if section == "default" => Some("default"),
                        // [sso-session x], [services x]: not profiles.
                        None => None,
                    }
                } else {
                    Some(section)
                };
                current = name.map(|name| {
                    out.iter().position(|p| p.name == name).unwrap_or_else(|| {
                        out.push(Profile {
                            name: name.to_string(),
                            region: String::new(),
                            kind: "keys".into(),
                        });
                        out.len() - 1
                    })
                });
                continue;
            }
            let (Some(i), Some((key, value))) = (current, line.split_once('=')) else {
                continue;
            };
            let (key, value) = (key.trim(), value.trim());
            match key {
                "region" if out[i].region.is_empty() => out[i].region = value.to_string(),
                "sso_session" | "sso_start_url" => out[i].kind = "sso".into(),
                "role_arn" if out[i].kind != "sso" => out[i].kind = "role".into(),
                _ => {}
            }
        }
    };
    parse(config, true);
    parse(credentials, false);
    out
}

/// Runs the AWS CLI (through `cmd /C` on Windows).
fn aws(program: &str, args: &[&str]) -> Command {
    let mut command = if cfg!(windows) && program == "aws" {
        let mut c = Command::new("cmd");
        c.args(["/C", "aws"]);
        c
    } else {
        Command::new(program)
    };
    command.args(args);
    command
}

async fn run(program: &str, args: &[&str]) -> Result<String> {
    let output = aws(program, args).output().await.map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            Error("the AWS CLI (aws) was not found; install AWS CLI v2 and sign in (`aws sso login` or `aws configure`)".into())
        } else {
            Error(format!("could not run aws: {e}"))
        }
    })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let first = stderr
            .lines()
            .find(|l| !l.trim().is_empty())
            .unwrap_or("unknown error")
            .trim();
        return Err(Error(format!("aws failed: {first}")));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn text(v: &Value, k: &str) -> String {
    v.get(k)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// One profile's credentials and account, from the AWS CLI. Credentials are
/// cached until shortly before they expire.
pub struct Session {
    program: String,
    pub profile: String,
    cached: Mutex<Option<(Credentials, Instant, Duration)>>,
}

impl Session {
    pub fn new(profile: impl Into<String>) -> Self {
        Self::with_program("aws", profile)
    }

    /// Tests point `program` at a fake aws.
    pub fn with_program(program: impl Into<String>, profile: impl Into<String>) -> Self {
        Self {
            program: program.into(),
            profile: profile.into(),
            cached: Mutex::new(None),
        }
    }

    pub async fn credentials(&self) -> Result<Credentials> {
        let mut cached = self.cached.lock().await;
        if let Some((c, at, lifetime)) = cached.as_ref() {
            if at.elapsed() < *lifetime {
                return Ok(c.clone());
            }
        }
        let out = run(
            &self.program,
            &[
                "configure",
                "export-credentials",
                "--profile",
                &self.profile,
                "--format",
                "process",
            ],
        )
        .await?;
        let v: Value =
            serde_json::from_str(&out).map_err(|e| Error(format!("unreadable aws output: {e}")))?;
        let credentials = Credentials {
            access_key_id: text(&v, "AccessKeyId"),
            secret_access_key: text(&v, "SecretAccessKey"),
            session_token: Some(text(&v, "SessionToken")).filter(|t| !t.is_empty()),
        };
        if credentials.access_key_id.is_empty() || credentials.secret_access_key.is_empty() {
            return Err(Error(format!(
                "no credentials for profile {}; sign in with `aws sso login --profile {0}` or `aws configure`",
                self.profile
            )));
        }
        // Until five minutes before they expire.
        let lifetime = chrono::DateTime::parse_from_rfc3339(&text(&v, "Expiration"))
            .ok()
            .and_then(|at| {
                (at.with_timezone(&chrono::Utc) - chrono::Utc::now() - chrono::Duration::minutes(5))
                    .to_std()
                    .ok()
            })
            .map_or(CREDENTIALS_LIFETIME, |d| d.min(CREDENTIALS_LIFETIME * 6));
        *cached = Some((credentials.clone(), Instant::now(), lifetime));
        Ok(credentials)
    }

    /// The account and principal behind the profile.
    pub async fn identity(&self) -> Result<Identity> {
        let out = run(
            &self.program,
            &[
                "sts",
                "get-caller-identity",
                "--profile",
                &self.profile,
                "--output",
                "json",
            ],
        )
        .await?;
        let v: Value =
            serde_json::from_str(&out).map_err(|e| Error(format!("unreadable aws output: {e}")))?;
        Ok(Identity {
            account: text(&v, "Account"),
            arn: text(&v, "Arn"),
        })
    }
}

/// The parts of an ARN: `arn:partition:service:region:account:resource`.
pub fn arn_parts(arn: &str) -> Option<(&str, &str, &str, &str)> {
    let mut parts = arn.splitn(6, ':');
    (parts.next()? == "arn").then_some(())?;
    let _partition = parts.next()?;
    let service = parts.next()?;
    let region = parts.next()?;
    let account = parts.next()?;
    let resource = parts.next()?;
    Some((service, region, account, resource))
}

/// A resource's name: the last part of the ARN's resource.
pub fn name_of(arn: &str) -> String {
    let resource = arn_parts(arn).map_or(arn, |(_, _, _, r)| r);
    resource
        .rsplit(['/', ':'])
        .next()
        .unwrap_or(resource)
        .to_string()
}

/// `service:type` from an ARN (`ec2:instance`, `s3:bucket`).
fn type_of(arn: &str) -> String {
    match arn_parts(arn) {
        Some((service, _, _, resource)) => {
            let kind = match resource.split_once(['/', ':']) {
                Some((kind, _)) => kind,
                // S3 buckets and the like: the ARN's resource is the name.
                None if service == "s3" => "bucket",
                None => resource,
            };
            format!("{service}:{kind}")
        }
        None => String::new(),
    }
}

fn region_or_global(region: &str) -> String {
    if region.is_empty() {
        "global".into()
    } else {
        region.to_string()
    }
}

/// Signed calls to AWS for one profile.
pub struct AwsClient {
    http: reqwest::Client,
    session: Arc<Session>,
    region: String,
    /// Tests send every call here instead of AWS.
    endpoint: Option<String>,
}

impl AwsClient {
    pub fn new(session: Arc<Session>, region: &str) -> Self {
        Self {
            http: reqwest::Client::new(),
            session,
            region: if region.is_empty() {
                DEFAULT_REGION.into()
            } else {
                region.into()
            },
            endpoint: None,
        }
    }

    pub fn with_endpoint(session: Arc<Session>, region: &str, endpoint: &str) -> Self {
        let mut client = Self::new(session, region);
        client.endpoint = Some(endpoint.trim_end_matches('/').to_string());
        client
    }

    pub fn region(&self) -> &str {
        &self.region
    }

    pub fn session(&self) -> &Session {
        &self.session
    }

    /// POST `body` to `host` + `path`, signed for `service`.
    async fn post(
        &self,
        service: &str,
        host: &str,
        path: &str,
        content_type: &str,
        target: Option<&str>,
        body: &Value,
    ) -> Result<Value> {
        let credentials = self.session.credentials().await?;
        let body = body.to_string();
        let (base, sign_host) = match &self.endpoint {
            Some(e) => (e.clone(), e.split("://").nth(1).unwrap_or(e).to_string()),
            None => (format!("https://{host}"), host.to_string()),
        };
        let mut headers = vec![
            ("content-type".to_string(), content_type.to_string()),
            ("host".to_string(), sign_host),
        ];
        if let Some(target) = target {
            headers.push(("x-amz-target".into(), target.into()));
        }
        let amz_date = chrono::Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
        let signed = sigv4::sign(
            &sigv4::Request {
                method: "POST",
                path,
                query: "",
                headers,
                body: body.as_bytes(),
            },
            &credentials,
            &self.region,
            service,
            &amz_date,
        );
        let mut request = self.http.post(format!("{base}{path}")).body(body);
        for (k, v) in signed.iter().filter(|(k, _)| k != "host") {
            request = request.header(k.as_str(), v.as_str());
        }
        let response = request
            .send()
            .await
            .map_err(|e| Error(format!("AWS request failed: {e}")))?;
        let status = response.status();
        let text = response
            .text()
            .await
            .map_err(|e| Error(format!("AWS response: {e}")))?;
        if !status.is_success() {
            let v = serde_json::from_str::<Value>(&text).unwrap_or(Value::Null);
            let message = ["message", "Message"]
                .iter()
                .find_map(|k| v.get(*k).and_then(Value::as_str))
                .map(str::to_string)
                .unwrap_or_else(|| text.chars().take(300).collect());
            let kind = v
                .get("__type")
                .and_then(Value::as_str)
                .map(|t| format!("{} ", t.rsplit('#').next().unwrap_or(t)))
                .unwrap_or_default();
            return Err(Error(format!("HTTP {}: {kind}{message}", status.as_u16())));
        }
        if text.trim().is_empty() {
            return Ok(Value::Null);
        }
        serde_json::from_str(&text).map_err(|e| Error(format!("invalid AWS response: {e}")))
    }

    /// Resource Explorer search in the profile's region (its default view):
    /// every region when that region holds the aggregator index.
    pub async fn search(&self, query: &str) -> Result<Found> {
        let host = format!("resource-explorer-2.{}.api.aws", self.region);
        let mut resources = Vec::new();
        let mut next: Option<String> = None;
        let mut complete = true;
        loop {
            let mut body = json!({"QueryString": query, "MaxResults": SEARCH_PAGE});
            if let Some(t) = &next {
                body["NextToken"] = json!(t);
            }
            let page = self
                .post(
                    "resource-explorer-2",
                    &host,
                    "/Search",
                    "application/json",
                    None,
                    &body,
                )
                .await?;
            for r in page
                .get("Resources")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let arn = text(r, "Arn");
                if arn.is_empty() {
                    continue;
                }
                let tags = r
                    .get("Properties")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter(|p| p.get("Name").and_then(Value::as_str) == Some("tags"))
                    .filter_map(|p| p.get("Data").and_then(Value::as_array))
                    .flatten()
                    .map(|t| (text(t, "Key"), text(t, "Value")))
                    .collect();
                resources.push(AwsResource {
                    name: name_of(&arn),
                    resource_type: Some(text(r, "ResourceType"))
                        .filter(|t| !t.is_empty())
                        .unwrap_or_else(|| type_of(&arn)),
                    region: region_or_global(&text(r, "Region")),
                    tags,
                    document: r.clone(),
                    arn,
                });
            }
            if page.pointer("/Count/Complete").and_then(Value::as_bool) == Some(false) {
                complete = false;
            }
            match page.get("NextToken").and_then(Value::as_str) {
                Some(t) if !t.is_empty() => next = Some(t.to_string()),
                _ => break,
            }
        }
        Ok(Found {
            resources,
            complete,
            source: Source::ResourceExplorer,
        })
    }

    /// Tagged (and some untagged) resources in the profile's region, from
    /// the Resource Groups Tagging API.
    pub async fn tagged(&self) -> Result<Found> {
        let host = format!("tagging.{}.amazonaws.com", self.region);
        let mut resources = Vec::new();
        let mut token = String::new();
        loop {
            let body = json!({"ResourcesPerPage": 100, "PaginationToken": token});
            let page = self
                .post(
                    "tagging",
                    &host,
                    "/",
                    "application/x-amz-json-1.1",
                    Some("ResourceGroupsTaggingAPI_20170126.GetResources"),
                    &body,
                )
                .await?;
            for r in page
                .get("ResourceTagMappingList")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let arn = text(r, "ResourceARN");
                if arn.is_empty() {
                    continue;
                }
                let region = arn_parts(&arn).map_or("", |(_, region, _, _)| region);
                resources.push(AwsResource {
                    name: name_of(&arn),
                    resource_type: type_of(&arn),
                    region: region_or_global(region),
                    tags: r
                        .get("Tags")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                        .map(|t| (text(t, "Key"), text(t, "Value")))
                        .collect(),
                    document: r.clone(),
                    arn,
                });
            }
            token = text(&page, "PaginationToken");
            if token.is_empty() {
                break;
            }
        }
        Ok(Found {
            resources,
            complete: true,
            source: Source::Tagging,
        })
    }

    /// Resource Explorer, else (no index or no access) the Tagging API in
    /// the profile's region; `Err` only when both fail.
    pub async fn resources(&self) -> Result<Found> {
        match self.search("").await {
            Ok(found) => Ok(found),
            Err(explorer) => {
                tracing::info!("Resource Explorer unavailable ({explorer}); using the Tagging API");
                self.tagged()
                    .await
                    .map_err(|tagging| Error(format!("{explorer}; Tagging API: {tagging}")))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    use c9s_azure::testing::{error, ok, serve_paths};

    #[test]
    fn profiles_come_from_config_and_credentials() {
        let config = "\
[default]
region = eu-west-1

[profile prod]
sso_session = acme
sso_account_id = 111122223333
region = eu-central-1

[sso-session acme]
sso_region = eu-west-1

# a comment
[profile audit]
role_arn = arn:aws:iam::444455556666:role/Audit
source_profile = default
";
        let credentials = "\
[default]
aws_access_key_id = AKIA
[ci]
aws_access_key_id = AKIB
";
        let profiles = parse_profiles(config, credentials);
        let view: Vec<(&str, &str, &str)> = profiles
            .iter()
            .map(|p| (p.name.as_str(), p.region.as_str(), p.kind.as_str()))
            .collect();
        assert_eq!(
            view,
            [
                ("default", "eu-west-1", "keys"),
                ("prod", "eu-central-1", "sso"),
                ("audit", "", "role"),
                ("ci", "", "keys"),
            ],
            "sso-session sections are not profiles; default is listed once"
        );
    }

    #[test]
    fn arns_give_names_types_and_regions() {
        let vm = "arn:aws:ec2:eu-west-1:111122223333:instance/i-0abc";
        assert_eq!(
            arn_parts(vm),
            Some(("ec2", "eu-west-1", "111122223333", "instance/i-0abc"))
        );
        assert_eq!(name_of(vm), "i-0abc");
        assert_eq!(type_of(vm), "ec2:instance");
        assert_eq!(type_of("arn:aws:s3:::logs-eu"), "s3:bucket");
        assert_eq!(name_of("arn:aws:s3:::logs-eu"), "logs-eu");
        assert_eq!(
            name_of("arn:aws:lambda:eu-west-1:1:function:resize"),
            "resize"
        );
        assert_eq!(arn_parts("not-an-arn"), None);
    }

    #[cfg(unix)]
    fn fake_aws(dir: &std::path::Path, body: &str) -> String {
        use std::os::unix::fs::PermissionsExt;
        let path = dir.join("aws");
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path.to_string_lossy().into_owned()
    }

    #[cfg(unix)]
    const FAKE: &str = r#"case "$*" in
  "configure export-credentials --profile prod --format process") echo '{"Version":1,"AccessKeyId":"ASIA","SecretAccessKey":"s","SessionToken":"t","Expiration":"2099-01-01T00:00:00+00:00"}' ;;
  "sts get-caller-identity --profile prod --output json") echo '{"Account":"111122223333","Arn":"arn:aws:sts::111122223333:assumed-role/Admin/ada"}' ;;
  *) echo "The config profile ($*) could not be found" >&2; exit 255 ;;
esac"#;

    #[cfg(unix)]
    #[tokio::test]
    async fn credentials_and_identity_come_from_the_cli() {
        let dir = tempfile::tempdir().unwrap();
        let program = fake_aws(dir.path(), FAKE);
        let session = Session::with_program(program.clone(), "prod");
        let c = session.credentials().await.unwrap();
        assert_eq!(
            (c.access_key_id.as_str(), c.session_token.as_deref()),
            ("ASIA", Some("t"))
        );
        assert_eq!(session.identity().await.unwrap().account, "111122223333");
        let missing = Session::with_program(program, "nope");
        let err = missing.credentials().await.unwrap_err();
        assert!(err.0.contains("could not be found"), "{err}");
        let no_cli = Session::with_program(dir.path().join("none").to_string_lossy(), "prod");
        assert!(no_cli
            .credentials()
            .await
            .unwrap_err()
            .0
            .contains("not found"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn resource_explorer_pages_and_tagging_fallback() {
        let (endpoint, requests) = serve_paths(|path, body| {
            if path == "/Search" {
                if body.get("NextToken").is_some() {
                    return ok(json!({"Resources": [
                        {"Arn": "arn:aws:s3:::logs-eu", "ResourceType": "s3:bucket", "Region": ""}
                    ], "Count": {"Complete": true, "TotalResults": 2}}));
                }
                return ok(json!({"Resources": [
                    {"Arn": "arn:aws:ec2:eu-west-1:1:instance/i-0abc", "ResourceType": "ec2:instance",
                     "Region": "eu-west-1",
                     "Properties": [{"Name": "tags", "Data": [{"Key": "env", "Value": "prod"}]}]}
                ], "NextToken": "n2"}));
            }
            error(404, "unexpected")
        })
        .await;
        let dir = tempfile::tempdir().unwrap();
        let session = Arc::new(Session::with_program(fake_aws(dir.path(), FAKE), "prod"));
        let client = AwsClient::with_endpoint(session.clone(), "eu-west-1", &endpoint);
        let found = client.resources().await.unwrap();
        assert_eq!(found.source, Source::ResourceExplorer);
        assert!(found.complete);
        assert_eq!(found.resources.len(), 2, "both pages");
        assert_eq!(found.resources[0].tags, [("env".into(), "prod".into())]);
        assert_eq!(found.resources[1].region, "global");
        assert_eq!(requests.lock().unwrap()[0]["QueryString"], "");

        // No Resource Explorer index: the Tagging API in the region.
        let (endpoint, _) = serve_paths(|path, body| {
            if path == "/Search" {
                return Reply401::reply();
            }
            if path == "/" && body.get("ResourcesPerPage").is_some() {
                if body["PaginationToken"] == "" {
                    return ok(json!({"ResourceTagMappingList": [
                        {"ResourceARN": "arn:aws:lambda:eu-west-1:1:function:resize",
                         "Tags": [{"Key": "team", "Value": "media"}]}
                    ], "PaginationToken": "p2"}));
                }
                return ok(json!({"ResourceTagMappingList": [], "PaginationToken": ""}));
            }
            error(404, "unexpected")
        })
        .await;
        let client = AwsClient::with_endpoint(session, "eu-west-1", &endpoint);
        let found = client.resources().await.unwrap();
        assert_eq!(found.source, Source::Tagging);
        assert_eq!(
            (
                found.resources[0].name.as_str(),
                found.resources[0].resource_type.as_str(),
                found.resources[0].region.as_str()
            ),
            ("resize", "lambda:function", "eu-west-1")
        );
    }

    #[cfg(unix)]
    struct Reply401;
    #[cfg(unix)]
    impl Reply401 {
        fn reply() -> c9s_azure::testing::Reply {
            c9s_azure::testing::Reply {
                status: 401,
                headers: vec![],
                body: json!({"message": "no default view"}).to_string(),
                delay: Default::default(),
            }
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn aws_errors_carry_their_message() {
        let (endpoint, _) = serve_paths(|_, _| {
            let mut r = error(403, "x");
            r.body =
                json!({"__type": "com.amazon#AccessDeniedException", "Message": "not allowed"})
                    .to_string();
            r
        })
        .await;
        let dir = tempfile::tempdir().unwrap();
        let session = Arc::new(Session::with_program(fake_aws(dir.path(), FAKE), "prod"));
        let client = AwsClient::with_endpoint(session, "", &endpoint);
        assert_eq!(client.region(), DEFAULT_REGION);
        let err = client.resources().await.unwrap_err();
        assert!(
            err.0
                .contains("HTTP 403: AccessDeniedException not allowed")
                && err.0.contains("Tagging API"),
            "{err}"
        );
    }
}
