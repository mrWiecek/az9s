use az9s_core::query::ResourceQuery;

/// Command mode parser and executor
///
/// Handles k9s-like colon commands for quick navigation:
/// Command names are always singular (k9s style); plural forms are not accepted.
/// - `:mg` / `:managementgroup` - Go to the management group tree
/// - `:sub` / `:subscription` - Go to subscriptions view
/// - `:rg <sub>` / `:resourcegroup <sub>` - Go to resource groups for a subscription
/// - `:rg all` - Go to resource groups across all subscriptions
/// - `:res <sub>` / `:resource <sub>` - Go to resources for a subscription
/// - `:res all` - Go to resources across all subscriptions
/// - `:res [all|<sub>] type=… loc=… tag=…` - Resources narrowed by filters
/// - `:type [all|<sub>] [filters]` - Resource counts per type
#[derive(Debug, Clone)]
pub struct Command {
    pub kind: CommandKind,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CommandKind {
    /// Navigate to the management group tree
    ManagementGroups,
    /// Navigate to subscriptions view
    Subscriptions,
    /// Navigate to resource groups view
    ResourceGroups { scope: Scope },
    /// Navigate to resources view
    Resources { scope: Scope, query: ResourceQuery },
    /// Resource counts per type
    ResourceTypes { scope: Scope, query: ResourceQuery },
    /// A known command with bad arguments (the message says why)
    Invalid(String),
    /// Unknown/invalid command
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Scope {
    /// All subscriptions
    All,
    /// Specific subscription by name or ID
    Subscription(String),
}

impl Command {
    /// Parse a command string
    pub fn parse(input: &str) -> Self {
        let input = input.trim();

        // Remove leading colon if present
        let input = input.strip_prefix(':').unwrap_or(input);

        let parts: Vec<&str> = input.split_whitespace().collect();

        if parts.is_empty() {
            return Self {
                kind: CommandKind::Unknown(String::new()),
            };
        }

        let cmd = parts[0].to_lowercase();

        match cmd.as_str() {
            "mg" | "managementgroup" | "management-group" => Self {
                kind: CommandKind::ManagementGroups,
            },
            "sub" | "subscription" => Self {
                kind: CommandKind::Subscriptions,
            },
            "rg" | "resourcegroup" | "resource-group" => {
                let scope = if parts.len() > 1 {
                    if parts[1].to_lowercase() == "all" {
                        Scope::All
                    } else {
                        Scope::Subscription(parts[1..].join(" "))
                    }
                } else {
                    // Default to current subscription if available
                    Scope::All
                };
                Self {
                    kind: CommandKind::ResourceGroups { scope },
                }
            }
            "res" | "resource" | "type" => {
                let (filters, scope_terms): (Vec<&str>, Vec<&str>) =
                    parts[1..].iter().partition(|t| ResourceQuery::is_filter(t));
                let query = match ResourceQuery::parse(filters) {
                    Ok(query) => query,
                    Err(e) => {
                        return Self {
                            kind: CommandKind::Invalid(format!(":{cmd}: {e}")),
                        }
                    }
                };
                let scope = match scope_terms.first() {
                    None => Scope::All,
                    Some(t) if t.eq_ignore_ascii_case("all") => Scope::All,
                    Some(_) => Scope::Subscription(scope_terms.join(" ")),
                };
                let kind = if cmd == "type" {
                    CommandKind::ResourceTypes { scope, query }
                } else {
                    CommandKind::Resources { scope, query }
                };
                Self { kind }
            }
            _ => Self {
                kind: CommandKind::Unknown(input.to_string()),
            },
        }
    }

    /// Check if this is a valid command
    pub fn is_valid(&self) -> bool {
        !matches!(self.kind, CommandKind::Unknown(_) | CommandKind::Invalid(_))
    }

    /// Get a user-friendly error message for invalid commands
    pub fn error_message(&self) -> Option<String> {
        match &self.kind {
            CommandKind::Unknown(cmd) if !cmd.is_empty() => {
                Some(format!(
                    "Unknown command: '{}'\n\nAvailable commands:\n  :mg - Management group tree\n  :sub - Subscriptions\n  :rg [all|<sub>] - Resource groups\n  :res [all|<sub>] [type=… loc=… tag=…] - Resources\n  :type [all|<sub>] - Resource counts per type",
                    cmd
                ))
            }
            CommandKind::Unknown(_) => Some("No command entered".to_string()),
            CommandKind::Invalid(message) => Some(message.clone()),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_management_groups() {
        for input in [":mg", "managementgroup", ":management-group"] {
            assert_eq!(Command::parse(input).kind, CommandKind::ManagementGroups);
        }
        assert!(matches!(
            Command::parse(":mgs").kind,
            CommandKind::Unknown(_)
        ));
    }

    #[test]
    fn test_parse_subscriptions() {
        let cmd = Command::parse(":sub");
        assert_eq!(cmd.kind, CommandKind::Subscriptions);
        assert!(cmd.is_valid());

        let cmd = Command::parse("subscription");
        assert_eq!(cmd.kind, CommandKind::Subscriptions);
    }

    #[test]
    fn test_parse_resource_groups() {
        let cmd = Command::parse(":rg all");
        assert_eq!(cmd.kind, CommandKind::ResourceGroups { scope: Scope::All });
        assert!(cmd.is_valid());

        let cmd = Command::parse(":resourcegroup my-subscription");
        assert_eq!(
            cmd.kind,
            CommandKind::ResourceGroups {
                scope: Scope::Subscription("my-subscription".to_string())
            }
        );
    }

    #[test]
    fn test_parse_resources() {
        let cmd = Command::parse(":res all");
        assert_eq!(
            cmd.kind,
            CommandKind::Resources {
                scope: Scope::All,
                query: ResourceQuery::default()
            }
        );
        assert!(cmd.is_valid());

        let cmd = Command::parse(":resource my-subscription");
        assert_eq!(
            cmd.kind,
            CommandKind::Resources {
                scope: Scope::Subscription("my-subscription".to_string()),
                query: ResourceQuery::default()
            }
        );
    }

    #[test]
    fn test_parse_filters_and_type_summary() {
        let cmd = Command::parse(":res my sub type=vm,disks loc=westeurope");
        let CommandKind::Resources { scope, query } = cmd.kind else {
            panic!("{:?}", cmd.kind)
        };
        assert_eq!(scope, Scope::Subscription("my sub".into()));
        assert_eq!(query.types, ["vm", "disks"]);
        assert_eq!(query.locations, ["westeurope"]);

        let cmd = Command::parse(":res tag=env:prod");
        assert!(matches!(
            cmd.kind,
            CommandKind::Resources {
                scope: Scope::All,
                ..
            }
        ));

        assert_eq!(
            Command::parse(":type").kind,
            CommandKind::ResourceTypes {
                scope: Scope::All,
                query: ResourceQuery::default()
            }
        );
        assert!(!Command::parse(":types").is_valid(), "singular only");

        let bad = Command::parse(":res size=big");
        assert!(!bad.is_valid());
        assert!(bad.error_message().unwrap().contains("unknown filter"));
    }

    #[test]
    fn test_plural_commands_are_rejected() {
        for input in [
            ":subs",
            ":subscriptions",
            ":rgs all",
            ":resourcegroups",
            ":resource-groups",
            ":resources all",
        ] {
            let cmd = Command::parse(input);
            assert!(
                matches!(cmd.kind, CommandKind::Unknown(_)),
                "{input} should be rejected"
            );
        }
    }

    #[test]
    fn test_parse_unknown() {
        let cmd = Command::parse(":invalid");
        assert!(matches!(cmd.kind, CommandKind::Unknown(_)));
        assert!(!cmd.is_valid());
        assert!(cmd.error_message().is_some());
    }

    #[test]
    fn test_parse_empty() {
        let cmd = Command::parse("");
        assert!(matches!(cmd.kind, CommandKind::Unknown(_)));
        assert!(!cmd.is_valid());
    }
}
