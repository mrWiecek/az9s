/// Command mode parser and executor
///
/// Handles k9s-like colon commands for quick navigation:
/// Command names are always singular (k9s style); plural forms are not accepted.
/// - `:sub` / `:subscription` - Go to subscriptions view
/// - `:rg <sub>` / `:resourcegroup <sub>` - Go to resource groups for a subscription
/// - `:rg all` - Go to resource groups across all subscriptions
/// - `:res <sub>` / `:resource <sub>` - Go to resources for a subscription
/// - `:res all` - Go to resources across all subscriptions
#[derive(Debug, Clone)]
pub struct Command {
    pub kind: CommandKind,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CommandKind {
    /// Navigate to subscriptions view
    Subscriptions,
    /// Navigate to resource groups view
    ResourceGroups { scope: Scope },
    /// Navigate to resources view
    Resources { scope: Scope },
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
            "res" | "resource" => {
                let scope = if parts.len() > 1 {
                    if parts[1].to_lowercase() == "all" {
                        Scope::All
                    } else {
                        Scope::Subscription(parts[1..].join(" "))
                    }
                } else {
                    Scope::All
                };
                Self {
                    kind: CommandKind::Resources { scope },
                }
            }
            _ => Self {
                kind: CommandKind::Unknown(input.to_string()),
            },
        }
    }

    /// Check if this is a valid command
    pub fn is_valid(&self) -> bool {
        !matches!(self.kind, CommandKind::Unknown(_))
    }

    /// Get a user-friendly error message for invalid commands
    pub fn error_message(&self) -> Option<String> {
        match &self.kind {
            CommandKind::Unknown(cmd) if !cmd.is_empty() => {
                Some(format!(
                    "Unknown command: '{}'\n\nAvailable commands:\n  :sub - Subscriptions\n  :rg [all|<sub>] - Resource groups\n  :res [all|<sub>] - Resources",
                    cmd
                ))
            }
            CommandKind::Unknown(_) => Some("No command entered".to_string()),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(cmd.kind, CommandKind::Resources { scope: Scope::All });
        assert!(cmd.is_valid());

        let cmd = Command::parse(":resource my-subscription");
        assert_eq!(
            cmd.kind,
            CommandKind::Resources {
                scope: Scope::Subscription("my-subscription".to_string())
            }
        );
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
