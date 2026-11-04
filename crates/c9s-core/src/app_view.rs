//! The tabs of an application, as lines of toned text: the TUI colours the
//! tones, `c` copies the text.

use crate::apps::{credential_expiry, permission_rows, AppRow, Consent};
use crate::policy::{expiry_label, Expiry, EXPIRY_WARNING_DAYS};
use crate::state::{AppState, AppTab};

/// How a piece of text should look.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Heading,
    Label,
    Text,
    Muted,
    Accent,
    Good,
    Warn,
    Bad,
}

/// A line: pieces of text, each with its tone.
pub type Line = Vec<(String, Tone)>;

fn heading(text: &str) -> Line {
    vec![(text.to_string(), Tone::Heading)]
}

fn muted(text: &str) -> Line {
    vec![(text.to_string(), Tone::Muted)]
}

/// "label: value" rows of a section, labels padded to line up.
fn rows(out: &mut Vec<Line>, title: &str, rows: &[(&str, String)]) {
    if !out.is_empty() {
        out.push(vec![]);
    }
    out.push(heading(title));
    let width = rows.iter().map(|(l, _)| l.len()).max().unwrap_or(0);
    for (label, value) in rows {
        out.push(vec![
            (format!("  {label:<width$}  "), Tone::Label),
            (value.clone(), Tone::Text),
        ]);
    }
}

/// A list under a heading, or "(none)".
fn list(out: &mut Vec<Line>, title: &str, items: Vec<Line>) {
    if !out.is_empty() {
        out.push(vec![]);
    }
    out.push(heading(title));
    if items.is_empty() {
        out.push(muted("  (none)"));
    }
    for mut item in items {
        item.insert(0, ("  ".into(), Tone::Text));
        out.push(item);
    }
}

fn pad(text: &str, width: usize) -> String {
    let shown: String = text.chars().take(width.saturating_sub(1)).collect();
    format!("{shown:<width$}")
}

fn yes_no(b: bool) -> String {
    if b { "yes" } else { "no" }.into()
}

fn expiry_tone(e: Expiry) -> Tone {
    match e {
        Expiry::Expired(_) => Tone::Bad,
        Expiry::In(d) if d <= EXPIRY_WARNING_DAYS => Tone::Warn,
        Expiry::In(_) => Tone::Good,
        Expiry::Never => Tone::Muted,
    }
}

/// The lines of the open application's current tab.
pub fn app_lines(state: &AppState) -> Vec<Line> {
    let Some(row) = state.app_open.as_ref() else {
        return vec![];
    };
    let detail = state.open_app_detail();
    let loading = || vec![muted("Loading from Microsoft Graph…")];
    let mut out: Vec<Line> = Vec::new();
    match state.app_tab {
        AppTab::Overview => overview(&mut out, row, state),
        AppTab::Permissions => {
            let Some(detail) = detail else {
                return loading();
            };
            let perms = permission_rows(row.app.as_ref(), detail);
            if perms.is_empty() {
                return vec![muted("No API permissions requested or granted.")];
            }
            out.push(vec![(
                format!(
                    "{}{}{}{}Description",
                    pad("Status", 40),
                    pad("Type", 13),
                    pad("API", 26),
                    pad("Permission", 34)
                ),
                Tone::Heading,
            )]);
            for p in perms {
                let tone = match (&p.consent, p.requested) {
                    (Consent::NotGranted, _) => Tone::Bad,
                    (_, false) => Tone::Warn,
                    (Consent::Users(_), _) => Tone::Warn,
                    (Consent::Admin, true) => Tone::Good,
                };
                out.push(vec![
                    (pad(&p.status(), 40), tone),
                    (pad(p.kind, 13), Tone::Accent),
                    (pad(&p.api, 26), Tone::Muted),
                    (pad(&p.permission, 34), Tone::Text),
                    (p.description, Tone::Muted),
                ]);
            }
        }
        AppTab::ExposedApi => {
            let Some(app) = row.app.as_ref() else {
                return vec![muted("No app registration in this tenant: what it exposes is defined in its home tenant.")];
            };
            list(
                &mut out,
                "Identifier URIs",
                app.identifier_uris
                    .iter()
                    .map(|u| vec![(u.clone(), Tone::Text)])
                    .collect(),
            );
            let perm_line = |p: &c9s_azure::apps::ApiPermission| {
                let mut line = vec![
                    (pad(&p.value, 34), Tone::Text),
                    (pad(&p.audience, 22), Tone::Accent),
                    (p.display.clone(), Tone::Muted),
                ];
                if !p.enabled {
                    line.push(("  (disabled)".into(), Tone::Warn));
                }
                line
            };
            list(
                &mut out,
                "Delegated permissions (scopes) · who can consent",
                app.scopes.iter().map(perm_line).collect(),
            );
            list(
                &mut out,
                "App roles · who can be given them",
                app.roles.iter().map(perm_line).collect(),
            );
        }
        AppTab::Owners => {
            let Some(detail) = detail else {
                return loading();
            };
            let owner = |p: &c9s_azure::directory::Principal| {
                vec![
                    (pad(&p.kind, 18), Tone::Accent),
                    (pad(&p.display_name, 34), Tone::Text),
                    (p.detail.clone(), Tone::Muted),
                ]
            };
            if row.app.is_some() {
                list(
                    &mut out,
                    "App registration owners",
                    detail.app_owners.iter().map(owner).collect(),
                );
            }
            if row.sp.is_some() {
                list(
                    &mut out,
                    "Enterprise application owners",
                    detail.sp_owners.iter().map(owner).collect(),
                );
            }
        }
        AppTab::Credentials => {
            let now = chrono::Utc::now().timestamp();
            let mut creds: Vec<(Expiry, &str, &crate::apps::Credential)> = row
                .credentials()
                .into_iter()
                .map(|(on, c)| (credential_expiry(c, now), on, c))
                .collect();
            creds.sort_by_key(|c| c.0);
            if creds.is_empty() {
                return vec![muted("No secrets or certificates.")];
            }
            out.push(vec![(
                format!(
                    "{}{}{}{}Expires on",
                    pad("Expiry", 24),
                    pad("Kind", 13),
                    pad("On", 16),
                    pad("Name", 34)
                ),
                Tone::Heading,
            )]);
            for (e, on, c) in creds {
                out.push(vec![
                    (pad(&expiry_label(e), 24), expiry_tone(e)),
                    (pad(&c.kind, 13), Tone::Accent),
                    (pad(on, 16), Tone::Muted),
                    (pad(&c.name, 34), Tone::Text),
                    (c.end.clone(), Tone::Muted),
                ]);
            }
        }
        AppTab::Assigned => {
            let Some(detail) = detail else {
                return loading();
            };
            if detail.assigned.is_empty() {
                let required = row.sp.as_ref().is_some_and(|s| s.assignment_required);
                return vec![muted(if required {
                    "Nobody is assigned, and assignment is required: nobody can sign in."
                } else {
                    "Nobody is assigned (assignment isn't required: everyone in the tenant can sign in)."
                })];
            }
            let role_name = |id: &str| -> String {
                if id.chars().all(|c| c == '0' || c == '-') {
                    return "default access".into();
                }
                row.app
                    .as_ref()
                    .and_then(|a| a.roles.iter().find(|r| r.id == id))
                    .map_or(id.to_string(), |r| r.value.clone())
            };
            for a in &detail.assigned {
                out.push(vec![
                    (pad(&a.principal_type, 18), Tone::Accent),
                    (pad(&a.principal_name, 40), Tone::Text),
                    (role_name(&a.app_role_id), Tone::Muted),
                ]);
            }
        }
        AppTab::Json => {
            let Some(detail) = detail else {
                return loading();
            };
            for (title, doc) in [
                ("App registration", &detail.application_json),
                ("Enterprise application", &detail.service_principal_json),
            ] {
                if doc.is_null() {
                    continue;
                }
                if !out.is_empty() {
                    out.push(vec![]);
                }
                out.push(heading(title));
                let text = serde_json::to_string_pretty(doc).unwrap_or_default();
                out.extend(text.lines().map(|l| vec![(l.to_string(), Tone::Text)]));
            }
        }
    }
    out
}

fn overview(out: &mut Vec<Line>, row: &AppRow, state: &AppState) {
    let mut main = vec![
        ("Name", row.name.clone()),
        ("Kind", row.kind.label().to_string()),
        ("App ID", row.app_id.clone()),
    ];
    match &row.app {
        Some(app) => {
            main.push(("Registration object ID", app.id.clone()));
            main.push(("Sign-in audience", app.sign_in_audience.clone()));
            main.push(("Created", app.created.clone()));
        }
        None => main.push(("Registration", "none in this tenant".into())),
    }
    rows(out, "Application", &main);
    match &row.sp {
        Some(sp) => {
            let publisher = if sp.is_microsoft() {
                "Microsoft".to_string()
            } else if state
                .scope_tree
                .as_ref()
                .is_some_and(|t| t.root.eq_ignore_ascii_case(&sp.owner_tenant))
                || row.app.is_some()
            {
                "this tenant".to_string()
            } else {
                sp.owner_tenant.clone()
            };
            rows(
                out,
                "Enterprise application",
                &[
                    ("Object ID", sp.id.clone()),
                    ("Type", sp.sp_type.clone()),
                    ("Enabled", yes_no(sp.enabled)),
                    ("Assignment required", yes_no(sp.assignment_required)),
                    ("Publisher tenant", publisher),
                ],
            );
        }
        None => rows(
            out,
            "Enterprise application",
            &[("Object ID", "none yet (nobody has consented)".into())],
        ),
    }
    if let Some(app) = &row.app {
        list(
            out,
            "Redirect URIs",
            app.redirect_uris
                .iter()
                .map(|u| vec![(u.clone(), Tone::Text)])
                .collect(),
        );
    }
    if let Some((e, c)) = &row.soonest {
        rows(
            out,
            "Credentials",
            &[(
                "Soonest",
                format!("{} ({} {})", expiry_label(*e), c.kind, c.name),
            )],
        );
    }
}

/// The current tab as plain text, for `c`.
pub fn app_text(state: &AppState) -> Vec<String> {
    app_lines(state)
        .into_iter()
        .map(|l| {
            l.into_iter()
                .map(|(t, _)| t)
                .collect::<String>()
                .trim_end()
                .to_string()
        })
        .collect()
}
