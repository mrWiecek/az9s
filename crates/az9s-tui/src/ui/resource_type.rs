//! Resource type display utilities
//!
//! Provides concise, meaningful type labels for Azure resources while keeping
//! full type information accessible.

use once_cell::sync::Lazy;
use std::collections::HashMap;

/// Map of Azure resource types to short, readable labels
static TYPE_LABELS: Lazy<HashMap<&'static str, &'static str>> = Lazy::new(|| {
    let mut m = HashMap::new();

    // Compute
    m.insert("Microsoft.Compute/virtualMachines", "vm");
    m.insert("Microsoft.Compute/virtualMachineScaleSets", "vmss");
    m.insert("Microsoft.Compute/disks", "disk");
    m.insert("Microsoft.Compute/snapshots", "snapshot");
    m.insert("Microsoft.Compute/images", "image");
    m.insert("Microsoft.Compute/availabilitySets", "avail-set");

    // Networking
    m.insert("Microsoft.Network/virtualNetworks", "vnet");
    m.insert("Microsoft.Network/networkInterfaces", "nic");
    m.insert("Microsoft.Network/publicIPAddresses", "public-ip");
    m.insert("Microsoft.Network/loadBalancers", "lb");
    m.insert("Microsoft.Network/networkSecurityGroups", "nsg");
    m.insert("Microsoft.Network/applicationGateways", "app-gw");
    m.insert("Microsoft.Network/virtualNetworkGateways", "vpn-gw");
    m.insert("Microsoft.Network/localNetworkGateways", "local-gw");
    m.insert("Microsoft.Network/connections", "connection");
    m.insert("Microsoft.Network/routeTables", "route-table");
    m.insert("Microsoft.Network/privateDnsZones", "private-dns");
    m.insert("Microsoft.Network/dnsZones", "dns-zone");
    m.insert("Microsoft.Network/trafficManagerProfiles", "traffic-mgr");
    m.insert("Microsoft.Network/frontDoors", "front-door");
    m.insert("Microsoft.Network/bastionHosts", "bastion");
    m.insert("Microsoft.Network/natGateways", "nat-gw");

    // Storage
    m.insert("Microsoft.Storage/storageAccounts", "storage");
    m.insert("Microsoft.Storage/storageAccounts/blobServices", "blob-svc");
    m.insert("Microsoft.Storage/storageAccounts/fileServices", "file-svc");
    m.insert(
        "Microsoft.Storage/storageAccounts/queueServices",
        "queue-svc",
    );
    m.insert(
        "Microsoft.Storage/storageAccounts/tableServices",
        "table-svc",
    );

    // Databases
    m.insert("Microsoft.Sql/servers", "sql-server");
    m.insert("Microsoft.Sql/servers/databases", "sql-db");
    m.insert("Microsoft.DBforPostgreSQL/servers", "postgres");
    m.insert("Microsoft.DBforMySQL/servers", "mysql");
    m.insert("Microsoft.DocumentDB/databaseAccounts", "cosmos-db");
    m.insert("Microsoft.Cache/Redis", "redis");

    // Containers
    m.insert("Microsoft.ContainerService/managedClusters", "aks");
    m.insert("Microsoft.ContainerRegistry/registries", "acr");
    m.insert("Microsoft.ContainerInstance/containerGroups", "aci");
    m.insert("Microsoft.App/containerApps", "container-app");

    // Web & App Services
    m.insert("Microsoft.Web/sites", "app-service");
    m.insert("Microsoft.Web/serverFarms", "app-plan");
    m.insert("Microsoft.Web/staticSites", "static-web");
    m.insert("Microsoft.ApiManagement/service", "api-mgmt");

    // Key Vault & Security
    m.insert("Microsoft.KeyVault/vaults", "key-vault");
    m.insert(
        "Microsoft.ManagedIdentity/userAssignedIdentities",
        "managed-id",
    );
    m.insert("Microsoft.Security/securityContacts", "security-contact");

    // Monitoring & Management
    m.insert("Microsoft.Insights/components", "app-insights");
    m.insert("Microsoft.OperationalInsights/workspaces", "log-analytics");
    m.insert("Microsoft.Insights/actionGroups", "action-group");
    m.insert("Microsoft.Insights/metricAlerts", "metric-alert");
    m.insert("Microsoft.Insights/activityLogAlerts", "activity-alert");

    // Messaging
    m.insert("Microsoft.ServiceBus/namespaces", "service-bus");
    m.insert("Microsoft.EventHub/namespaces", "event-hub");
    m.insert("Microsoft.EventGrid/topics", "event-grid");

    // Functions & Logic
    m.insert("Microsoft.Web/sites/functions", "function");
    m.insert("Microsoft.Logic/workflows", "logic-app");

    // AI & ML
    m.insert("Microsoft.CognitiveServices/accounts", "cognitive-svc");
    m.insert(
        "Microsoft.MachineLearningServices/workspaces",
        "ml-workspace",
    );

    // IoT
    m.insert("Microsoft.Devices/IotHubs", "iot-hub");
    m.insert("Microsoft.Devices/provisioningServices", "iot-dps");

    // Backup & Recovery
    m.insert("Microsoft.RecoveryServices/vaults", "recovery-vault");
    m.insert("Microsoft.DataProtection/backupVaults", "backup-vault");

    m
});

/// Get a short, readable label for a resource type
///
/// Returns a concise label like "vm", "nic", "storage" for common types.
/// Falls back to intelligent truncation for unknown types.
pub fn short_label(resource_type: &str) -> String {
    // Check if we have a predefined label
    if let Some(label) = TYPE_LABELS.get(resource_type) {
        return label.to_string();
    }

    // Fallback: extract the last segment after the last slash
    // e.g., "Microsoft.Compute/virtualMachines" -> "virtualMachines"
    if let Some(last_segment) = resource_type.rsplit('/').next() {
        // Convert camelCase to kebab-case for readability
        let kebab = camel_to_kebab(last_segment);

        // Truncate if still too long, but keep meaningful parts
        if kebab.len() > 20 {
            // Try to keep the first meaningful word
            if let Some(first_word) = kebab.split('-').next() {
                if first_word.len() >= 4 {
                    return format!("{}...", &first_word[..first_word.len().min(15)]);
                }
            }
            format!("{}...", &kebab[..17])
        } else {
            kebab
        }
    } else {
        // Last resort: truncate the full type
        if resource_type.len() > 20 {
            format!("{}...", &resource_type[..17])
        } else {
            resource_type.to_string()
        }
    }
}

/// Get a display string with both short label and full type
///
/// Format: "vm (Microsoft.Compute/virtualMachines)"
pub fn display_with_full(resource_type: &str) -> String {
    let short = short_label(resource_type);
    if short == resource_type {
        resource_type.to_string()
    } else {
        format!("{} ({})", short, resource_type)
    }
}

/// Convert camelCase to kebab-case
fn camel_to_kebab(s: &str) -> String {
    let mut result = String::with_capacity(s.len() + 5);
    let chars = s.chars();

    for c in chars {
        if c.is_uppercase() && !result.is_empty() {
            // Add hyphen before uppercase letter (unless it's the start)
            result.push('-');
            result.push(c.to_lowercase().next().unwrap());
        } else {
            result.push(c.to_lowercase().next().unwrap());
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_short_label_known_types() {
        assert_eq!(short_label("Microsoft.Compute/virtualMachines"), "vm");
        assert_eq!(short_label("Microsoft.Network/networkInterfaces"), "nic");
        assert_eq!(short_label("Microsoft.Storage/storageAccounts"), "storage");
        assert_eq!(short_label("Microsoft.KeyVault/vaults"), "key-vault");
    }

    #[test]
    fn test_short_label_unknown_types() {
        // Should extract last segment and convert to kebab-case
        let result = short_label("Microsoft.Custom/someNewResource");
        assert!(result.contains("some") || result.contains("new"));
    }

    #[test]
    fn test_camel_to_kebab() {
        assert_eq!(camel_to_kebab("virtualMachines"), "virtual-machines");
        assert_eq!(camel_to_kebab("networkInterfaces"), "network-interfaces");
        assert_eq!(camel_to_kebab("storageAccounts"), "storage-accounts");
        assert_eq!(camel_to_kebab("simple"), "simple");
    }

    #[test]
    fn test_display_with_full() {
        let result = display_with_full("Microsoft.Compute/virtualMachines");
        assert!(result.contains("vm"));
        assert!(result.contains("Microsoft.Compute/virtualMachines"));
    }
}
