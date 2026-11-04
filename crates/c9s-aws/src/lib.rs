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
    /// The account it signs in to, when the files say (`sso_account_id`,
    /// or the account of `role_arn`).
    pub account_id: String,
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

/// A root or organizational unit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrgNode {
    /// "r-abcd" or "ou-abcd-12345678"
    pub id: String,
    pub name: String,
    /// The parent root or unit; empty for a root.
    pub parent: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrgAccount {
    pub id: String,
    pub name: String,
    /// "ACTIVE", "SUSPENDED", …
    pub status: String,
    pub parent: String,
}

/// An AWS Organizations tree.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Organization {
    pub roots: Vec<OrgNode>,
    pub units: Vec<OrgNode>,
    pub accounts: Vec<OrgAccount>,
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
                            account_id: String::new(),
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
                "sso_account_id" => out[i].account_id = value.to_string(),
                "role_arn" => {
                    if out[i].kind != "sso" {
                        out[i].kind = "role".into();
                    }
                    if out[i].account_id.is_empty() {
                        if let Some((_, _, account, _)) = arn_parts(value) {
                            out[i].account_id = account.to_string();
                        }
                    }
                }
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
    /// The CLI profile, or for an assumed role the role's ARN.
    pub profile: String,
    /// Credentials from assuming a role with another session's.
    assume: Option<Assume>,
    cached: Mutex<Option<(Credentials, Instant, Duration)>>,
}

struct Assume {
    base: Arc<Session>,
    role_arn: String,
    region: String,
    /// Tests send STS calls here instead of AWS.
    endpoint: Option<String>,
    http: reqwest::Client,
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
            assume: None,
            cached: Mutex::new(None),
        }
    }

    /// Another CLI profile, through the same aws program.
    pub fn sibling(&self, profile: impl Into<String>) -> Self {
        Self::with_program(self.program.clone(), profile)
    }

    /// Credentials for `role_arn`, assumed (STS `AssumeRole`, in `region`)
    /// with `base`'s. `endpoint` replaces STS in tests.
    pub fn assume(
        base: Arc<Session>,
        role_arn: impl Into<String>,
        region: &str,
        endpoint: Option<String>,
    ) -> Self {
        let role_arn = role_arn.into();
        Self {
            program: base.program.clone(),
            profile: role_arn.clone(),
            assume: Some(Assume {
                base,
                role_arn,
                region: if region.is_empty() {
                    DEFAULT_REGION.into()
                } else {
                    region.into()
                },
                endpoint,
                http: reqwest::Client::new(),
            }),
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
        if let Some(assume) = &self.assume {
            // Boxed: the base session's credentials are asked for in turn.
            let (credentials, expiration) = Box::pin(assume_role(assume)).await?;
            *cached = Some((
                credentials.clone(),
                Instant::now(),
                lifetime_until(&expiration),
            ));
            return Ok(credentials);
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
        let lifetime = lifetime_until(&text(&v, "Expiration"));
        *cached = Some((credentials.clone(), Instant::now(), lifetime));
        Ok(credentials)
    }

    /// The account and principal behind the profile.
    pub async fn identity(&self) -> Result<Identity> {
        if let Some(assume) = &self.assume {
            let account = arn_parts(&assume.role_arn)
                .map(|(_, _, a, _)| a.to_string())
                .unwrap_or_default();
            return Ok(Identity {
                account,
                arn: assume.role_arn.clone(),
            });
        }
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

/// How long credentials expiring at `expiration` (RFC 3339) may be used:
/// until five minutes before, at most an hour; unknown: ten minutes.
fn lifetime_until(expiration: &str) -> Duration {
    chrono::DateTime::parse_from_rfc3339(expiration)
        .ok()
        .and_then(|at| {
            (at.with_timezone(&chrono::Utc) - chrono::Utc::now() - chrono::Duration::minutes(5))
                .to_std()
                .ok()
        })
        .map_or(CREDENTIALS_LIFETIME, |d| d.min(CREDENTIALS_LIFETIME * 6))
}

/// The text of the first `<tag>` in an XML document (STS answers in XML).
fn xml_text(xml: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let start = xml.find(&open)? + open.len();
    let end = xml[start..].find(&format!("</{tag}>"))? + start;
    Some(xml[start..end].trim().to_string())
}

/// Percent-encode a form value.
fn form_encode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// STS `AssumeRole`: the role's credentials and when they expire.
async fn assume_role(assume: &Assume) -> Result<(Credentials, String)> {
    let base = assume.base.credentials().await?;
    let host = format!("sts.{}.amazonaws.com", assume.region);
    let body = format!(
        "Action=AssumeRole&Version=2011-06-15&RoleSessionName=c9s&DurationSeconds=3600&RoleArn={}",
        form_encode(&assume.role_arn)
    );
    let xml = signed_post(
        &assume.http,
        &base,
        &assume.region,
        "sts",
        assume.endpoint.as_deref(),
        &host,
        "/",
        "application/x-www-form-urlencoded; charset=utf-8",
        None,
        body,
    )
    .await
    .map_err(|e| Error(format!("could not assume {}: {e}", assume.role_arn)))?;
    let field = |tag: &str| xml_text(&xml, tag).unwrap_or_default();
    let credentials = Credentials {
        access_key_id: field("AccessKeyId"),
        secret_access_key: field("SecretAccessKey"),
        session_token: Some(field("SessionToken")).filter(|t| !t.is_empty()),
    };
    if credentials.access_key_id.is_empty() {
        return Err(Error(format!(
            "could not assume {}: no credentials in the answer",
            assume.role_arn
        )));
    }
    Ok((credentials, field("Expiration")))
}

/// POST `body` to `host` + `path` (or `endpoint` + `path` in tests),
/// signed for `service` in `region`; the response text, or an error with
/// AWS's message (JSON or XML).
#[allow(clippy::too_many_arguments)]
async fn signed_post(
    http: &reqwest::Client,
    credentials: &Credentials,
    region: &str,
    service: &str,
    endpoint: Option<&str>,
    host: &str,
    path: &str,
    content_type: &str,
    target: Option<&str>,
    body: String,
) -> Result<String> {
    let (base, sign_host) = match endpoint {
        Some(e) => (
            e.to_string(),
            e.split("://").nth(1).unwrap_or(e).to_string(),
        ),
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
        credentials,
        region,
        service,
        &amz_date,
    );
    let mut request = http.post(format!("{base}{path}")).body(body);
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
    if status.is_success() {
        return Ok(text);
    }
    let v = serde_json::from_str::<Value>(&text).unwrap_or(Value::Null);
    let message = ["message", "Message"]
        .iter()
        .find_map(|k| v.get(*k).and_then(Value::as_str))
        .map(str::to_string)
        .or_else(|| xml_text(&text, "Message"))
        .unwrap_or_else(|| text.chars().take(300).collect());
    let kind = v
        .get("__type")
        .and_then(Value::as_str)
        .map(|t| t.rsplit('#').next().unwrap_or(t).to_string())
        .or_else(|| xml_text(&text, "Code"))
        .map(|t| format!("{t} "))
        .unwrap_or_default();
    Err(Error(format!("HTTP {}: {kind}{message}", status.as_u16())))
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

    /// A client like this one (region, test endpoint) for another session.
    pub fn with_session(&self, session: Arc<Session>) -> Self {
        Self {
            http: self.http.clone(),
            session,
            region: self.region.clone(),
            endpoint: self.endpoint.clone(),
        }
    }

    /// A session assuming `role_arn` with this client's credentials (STS in
    /// the profile's region).
    pub fn assume_session(&self, role_arn: &str) -> Session {
        Session::assume(
            self.session.clone(),
            role_arn,
            &self.region,
            self.endpoint.clone(),
        )
    }

    /// One Organizations call (JSON, signed in us-east-1), every page.
    async fn organizations(&self, action: &str, body: Value, key: &str) -> Result<Vec<Value>> {
        let mut items = Vec::new();
        let mut body = body;
        loop {
            let page = self
                .post(
                    "organizations",
                    Some("us-east-1"),
                    "organizations.us-east-1.amazonaws.com",
                    "/",
                    "application/x-amz-json-1.1",
                    Some(&format!("AWSOrganizationsV20161128.{action}")),
                    &body,
                )
                .await?;
            items.extend(
                page.get(key)
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .cloned(),
            );
            match page.get("NextToken").and_then(Value::as_str) {
                Some(t) if !t.is_empty() => body["NextToken"] = json!(t),
                _ => return Ok(items),
            }
        }
    }

    /// The organization's tree: roots, organizational units and accounts
    /// below each. Needs the management account or a delegated
    /// administrator.
    pub async fn organization(&self) -> Result<Organization> {
        let roots = self.organizations("ListRoots", json!({}), "Roots").await?;
        let node = |v: &Value, parent: &str| OrgNode {
            id: text(v, "Id"),
            name: text(v, "Name"),
            parent: parent.to_string(),
        };
        let mut org = Organization {
            roots: roots.iter().map(|r| node(r, "")).collect(),
            ..Default::default()
        };
        let mut level: Vec<String> = org.roots.iter().map(|r| r.id.clone()).collect();
        // Breadth first, a level of parents at a time.
        while !level.is_empty() {
            let mut next = Vec::new();
            for parent in level {
                let body = json!({"ParentId": parent});
                let ous = self
                    .organizations(
                        "ListOrganizationalUnitsForParent",
                        body.clone(),
                        "OrganizationalUnits",
                    )
                    .await?;
                let accounts = self
                    .organizations("ListAccountsForParent", body, "Accounts")
                    .await?;
                for ou in &ous {
                    let ou = node(ou, &parent);
                    next.push(ou.id.clone());
                    org.units.push(ou);
                }
                org.accounts.extend(accounts.iter().map(|a| OrgAccount {
                    id: text(a, "Id"),
                    name: text(a, "Name"),
                    status: text(a, "Status"),
                    parent: parent.clone(),
                }));
            }
            level = next;
        }
        Ok(org)
    }

    pub fn session(&self) -> &Session {
        &self.session
    }

    /// POST JSON `body` to `host` + `path`, signed for `service` in
    /// `region` (the profile's when `None`).
    #[allow(clippy::too_many_arguments)]
    async fn post(
        &self,
        service: &str,
        region: Option<&str>,
        host: &str,
        path: &str,
        content_type: &str,
        target: Option<&str>,
        body: &Value,
    ) -> Result<Value> {
        let credentials = self.session.credentials().await?;
        let text = signed_post(
            &self.http,
            &credentials,
            region.unwrap_or(&self.region),
            service,
            self.endpoint.as_deref(),
            host,
            path,
            content_type,
            target,
            body.to_string(),
        )
        .await?;
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
                    None,
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
                    None,
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
        let accounts: Vec<&str> = profiles.iter().map(|p| p.account_id.as_str()).collect();
        assert_eq!(accounts, ["", "111122223333", "444455556666", ""]);
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
    async fn organization_tree_and_assumed_roles() {
        let (endpoint, requests) = serve_paths(|path, body| {
            assert_eq!(path, "/");
            // STS: a form body, not JSON.
            if body.is_null() {
                return c9s_azure::testing::Reply {
                    status: 200,
                    headers: vec![],
                    body: "<AssumeRoleResponse><AssumeRoleResult><Credentials>\
                           <AccessKeyId>ASIAROLE</AccessKeyId><SecretAccessKey>rs</SecretAccessKey>\
                           <SessionToken>rt</SessionToken><Expiration>2099-01-01T00:00:00Z</Expiration>\
                           </Credentials></AssumeRoleResult></AssumeRoleResponse>"
                        .into(),
                    delay: Default::default(),
                };
            }
            // Organizations: the mock can't see the target header, so a
            // parent's answer carries both its units and its accounts.
            match body.get("ParentId").and_then(Value::as_str) {
                None => ok(json!({"Roots": [{"Id": "r-1", "Name": "Root"}]})),
                Some("r-1") if body.get("NextToken").is_none() => ok(json!({
                    "OrganizationalUnits": [{"Id": "ou-1", "Name": "Prod"}],
                    "Accounts": [{"Id": "111122223333", "Name": "management", "Status": "ACTIVE"}],
                    "NextToken": "more"
                })),
                Some("r-1") => ok(json!({"OrganizationalUnits": [], "Accounts": []})),
                Some("ou-1") => ok(json!({
                    "OrganizationalUnits": [],
                    "Accounts": [{"Id": "444455556666", "Name": "payments", "Status": "SUSPENDED"}]
                })),
                Some(_) => error(404, "unexpected"),
            }
        })
        .await;
        let dir = tempfile::tempdir().unwrap();
        let session = Arc::new(Session::with_program(fake_aws(dir.path(), FAKE), "prod"));
        let client = AwsClient::with_endpoint(session, "eu-west-1", &endpoint);
        let org = client.organization().await.unwrap();
        assert_eq!(org.roots[0].id, "r-1");
        assert_eq!(
            (org.units[0].id.as_str(), org.units[0].parent.as_str()),
            ("ou-1", "r-1")
        );
        let accounts: Vec<(&str, &str, &str)> = org
            .accounts
            .iter()
            .map(|a| (a.id.as_str(), a.parent.as_str(), a.status.as_str()))
            .collect();
        assert_eq!(
            accounts,
            [
                ("111122223333", "r-1", "ACTIVE"),
                ("444455556666", "ou-1", "SUSPENDED")
            ]
        );

        let role =
            client.assume_session("arn:aws:iam::444455556666:role/OrganizationAccountAccessRole");
        let c = role.credentials().await.unwrap();
        assert_eq!(
            (c.access_key_id.as_str(), c.session_token.as_deref()),
            ("ASIAROLE", Some("rt"))
        );
        assert_eq!(role.identity().await.unwrap().account, "444455556666");
        // Cached: no second STS call.
        role.credentials().await.unwrap();
        let sts_calls = requests
            .lock()
            .unwrap()
            .iter()
            .filter(|b| b.is_null())
            .count();
        assert_eq!(sts_calls, 1);
    }

    #[test]
    fn xml_fields_and_form_values() {
        assert_eq!(
            xml_text("<a><Code>AccessDenied</Code></a>", "Code").as_deref(),
            Some("AccessDenied")
        );
        assert_eq!(xml_text("<a/>", "Code"), None);
        assert_eq!(
            form_encode("arn:aws:iam::1:role/x"),
            "arn%3Aaws%3Aiam%3A%3A1%3Arole%2Fx"
        );
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
