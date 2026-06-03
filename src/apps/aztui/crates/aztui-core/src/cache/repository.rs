use crate::{models::*, Result};
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::HashMap;
use tracing::debug;

/// Repository pattern for cache data access
pub struct CacheRepository<'a> {
    conn: &'a Connection,
}

impl<'a> CacheRepository<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    // ==================== Subscriptions ====================

    /// Cache a single subscription
    pub fn cache_subscription(&self, subscription: &Subscription) -> Result<()> {
        let now = chrono::Utc::now().timestamp();
        
        self.conn.execute(
            "INSERT OR REPLACE INTO subscriptions (id, name, state, tenant_id, cached_at) 
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![subscription.id, subscription.name, subscription.state, subscription.tenant_id, now],
        )?;

        Ok(())
    }

    /// Cache multiple subscriptions
    pub fn cache_subscriptions(&self, subscriptions: &[Subscription]) -> Result<()> {
        let now = chrono::Utc::now().timestamp();
        
        let tx = self.conn.unchecked_transaction()?;
        
        for sub in subscriptions {
            tx.execute(
                "INSERT OR REPLACE INTO subscriptions (id, name, state, tenant_id, cached_at) 
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![sub.id, sub.name, sub.state, sub.tenant_id, now],
            )?;
        }
        
        tx.commit()?;
        debug!("Cached {} subscriptions", subscriptions.len());
        Ok(())
    }

    /// Get all cached subscriptions
    pub fn get_subscriptions(&self) -> Result<Vec<Subscription>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, state, tenant_id FROM subscriptions ORDER BY name"
        )?;

        let subscriptions = stmt
            .query_map([], |row| {
                Ok(Subscription {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    state: row.get(2)?,
                    tenant_id: row.get(3)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(subscriptions)
    }

    /// Get a subscription by ID
    pub fn get_subscription(&self, id: &str) -> Result<Option<Subscription>> {
        let result = self.conn.query_row(
            "SELECT id, name, state, tenant_id FROM subscriptions WHERE id = ?1",
            [id],
            |row| {
                Ok(Subscription {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    state: row.get(2)?,
                    tenant_id: row.get(3)?,
                })
            },
        ).optional()?;

        Ok(result)
    }

    // ==================== Resource Groups ====================

    /// Cache a single resource group
    pub fn cache_resource_group(&self, rg: &ResourceGroup) -> Result<()> {
        let now = chrono::Utc::now().timestamp();
        let tags_json = serde_json::to_string(&rg.tags)?;
        
        self.conn.execute(
            "INSERT OR REPLACE INTO resource_groups 
             (id, name, location, subscription_id, tags, cached_at) 
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![rg.id, rg.name, rg.location, rg.subscription_id, tags_json, now],
        )?;

        Ok(())
    }

    /// Cache multiple resource groups
    pub fn cache_resource_groups(&self, resource_groups: &[ResourceGroup]) -> Result<()> {
        let now = chrono::Utc::now().timestamp();
        
        let tx = self.conn.unchecked_transaction()?;
        
        for rg in resource_groups {
            let tags_json = serde_json::to_string(&rg.tags)?;
            
            tx.execute(
                "INSERT OR REPLACE INTO resource_groups 
                 (id, name, location, subscription_id, tags, cached_at) 
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![rg.id, rg.name, rg.location, rg.subscription_id, tags_json, now],
            )?;
        }
        
        tx.commit()?;
        debug!("Cached {} resource groups", resource_groups.len());
        Ok(())
    }

    /// Get resource groups for a subscription
    pub fn get_resource_groups(&self, subscription_id: &str) -> Result<Vec<ResourceGroup>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, location, subscription_id, tags 
             FROM resource_groups 
             WHERE subscription_id = ?1 
             ORDER BY name"
        )?;

        let resource_groups = stmt
            .query_map([subscription_id], |row| {
                let tags_json: String = row.get(4)?;
                let tags = serde_json::from_str(&tags_json).unwrap_or_default();
                
                Ok(ResourceGroup {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    location: row.get(2)?,
                    subscription_id: row.get(3)?,
                    tags,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(resource_groups)
    }

    /// Get a resource group by name
    pub fn get_resource_group(&self, subscription_id: &str, name: &str) -> Result<Option<ResourceGroup>> {
        let result = self.conn.query_row(
            "SELECT id, name, location, subscription_id, tags 
             FROM resource_groups 
             WHERE subscription_id = ?1 AND name = ?2",
            params![subscription_id, name],
            |row| {
                let tags_json: String = row.get(4)?;
                let tags = serde_json::from_str(&tags_json).unwrap_or_default();
                
                Ok(ResourceGroup {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    location: row.get(2)?,
                    subscription_id: row.get(3)?,
                    tags,
                })
            },
        ).optional()?;

        Ok(result)
    }

    // ==================== Resources ====================

    /// Cache a single resource
    pub fn cache_resource(&self, resource: &Resource) -> Result<()> {
        let now = chrono::Utc::now().timestamp();
        let tags_json = serde_json::to_string(&resource.tags)?;
        
        self.conn.execute(
            "INSERT OR REPLACE INTO resources 
             (id, name, resource_type, location, resource_group, subscription_id, tags, cached_at) 
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                resource.id,
                resource.name,
                resource.resource_type,
                resource.location,
                resource.resource_group,
                resource.subscription_id,
                tags_json,
                now
            ],
        )?;

        Ok(())
    }

    /// Cache multiple resources
    pub fn cache_resources(&self, resources: &[Resource]) -> Result<()> {
        let now = chrono::Utc::now().timestamp();
        
        let tx = self.conn.unchecked_transaction()?;
        
        for resource in resources {
            let tags_json = serde_json::to_string(&resource.tags)?;
            
            tx.execute(
                "INSERT OR REPLACE INTO resources 
                 (id, name, resource_type, location, resource_group, subscription_id, tags, cached_at) 
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    resource.id,
                    resource.name,
                    resource.resource_type,
                    resource.location,
                    resource.resource_group,
                    resource.subscription_id,
                    tags_json,
                    now
                ],
            )?;
        }
        
        tx.commit()?;
        debug!("Cached {} resources", resources.len());
        Ok(())
    }

    /// Get resources with optional filtering
    pub fn get_resources(&self, filter: &ResourceFilter) -> Result<Vec<Resource>> {
        let mut query = String::from(
            "SELECT id, name, resource_type, location, resource_group, subscription_id, tags 
             FROM resources WHERE 1=1"
        );
        let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        if let Some(sub_id) = &filter.subscription_id {
            query.push_str(" AND subscription_id = ?");
            params.push(Box::new(sub_id.clone()));
        }

        if let Some(rg) = &filter.resource_group {
            query.push_str(" AND resource_group = ?");
            params.push(Box::new(rg.clone()));
        }

        if let Some(rtype) = &filter.resource_type {
            query.push_str(" AND resource_type = ?");
            params.push(Box::new(rtype.clone()));
        }

        if let Some(location) = &filter.location {
            query.push_str(" AND location = ?");
            params.push(Box::new(location.clone()));
        }

        if let Some(name_filter) = &filter.name_contains {
            query.push_str(" AND name LIKE ?");
            params.push(Box::new(format!("%{}%", name_filter)));
        }

        query.push_str(" ORDER BY name");

        if let Some(limit) = filter.limit {
            query.push_str(&format!(" LIMIT {}", limit));
        }

        let mut stmt = self.conn.prepare(&query)?;
        
        let param_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();
        
        let resources = stmt
            .query_map(param_refs.as_slice(), |row| {
                let tags_json: String = row.get(6)?;
                let tags = serde_json::from_str(&tags_json).unwrap_or_default();
                
                Ok(Resource {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    resource_type: row.get(2)?,
                    location: row.get(3)?,
                    resource_group: row.get(4)?,
                    subscription_id: row.get(5)?,
                    tags,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(resources)
    }

    /// Get a resource by ID
    pub fn get_resource(&self, id: &str) -> Result<Option<Resource>> {
        let result = self.conn.query_row(
            "SELECT id, name, resource_type, location, resource_group, subscription_id, tags 
             FROM resources 
             WHERE id = ?1",
            [id],
            |row| {
                let tags_json: String = row.get(6)?;
                let tags = serde_json::from_str(&tags_json).unwrap_or_default();
                
                Ok(Resource {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    resource_type: row.get(2)?,
                    location: row.get(3)?,
                    resource_group: row.get(4)?,
                    subscription_id: row.get(5)?,
                    tags,
                })
            },
        ).optional()?;

        Ok(result)
    }

    /// Get resource count by type for a subscription
    pub fn get_resource_type_counts(&self, subscription_id: &str) -> Result<HashMap<String, usize>> {
        let mut stmt = self.conn.prepare(
            "SELECT resource_type, COUNT(*) 
             FROM resources 
             WHERE subscription_id = ?1 
             GROUP BY resource_type 
             ORDER BY COUNT(*) DESC"
        )?;

        let counts = stmt
            .query_map([subscription_id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as usize))
            })?
            .collect::<std::result::Result<HashMap<_, _>, _>>()?;

        Ok(counts)
    }

    /// Get unique locations for a subscription
    pub fn get_locations(&self, subscription_id: &str) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT DISTINCT location 
             FROM resources 
             WHERE subscription_id = ?1 
             ORDER BY location"
        )?;

        let locations = stmt
            .query_map([subscription_id], |row| row.get(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(locations)
    }

    // ==================== Resource Details ====================

    /// Cache resource detail
    pub fn cache_resource_detail(&self, detail: &ResourceDetail) -> Result<()> {
        let now = chrono::Utc::now().timestamp();
        let properties_json = serde_json::to_string(&detail.properties)?;
        
        self.conn.execute(
            "INSERT OR REPLACE INTO resource_details 
             (resource_id, properties, sku, kind, cached_at) 
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                detail.resource.id,
                properties_json,
                detail.sku,
                detail.kind,
                now
            ],
        )?;

        // Also cache the base resource
        self.cache_resource(&detail.resource)?;

        Ok(())
    }

    /// Get resource detail
    pub fn get_resource_detail(&self, resource_id: &str) -> Result<Option<ResourceDetail>> {
        // First get the resource
        let resource = match self.get_resource(resource_id)? {
            Some(r) => r,
            None => return Ok(None),
        };

        // Then get the details
        let result = self.conn.query_row(
            "SELECT properties, sku, kind 
             FROM resource_details 
             WHERE resource_id = ?1",
            [resource_id],
            |row| {
                let properties_json: String = row.get(0)?;
                let properties = serde_json::from_str(&properties_json).unwrap_or(serde_json::Value::Null);
                
                Ok(ResourceDetail {
                    resource: resource.clone(),
                    properties,
                    sku: row.get(1)?,
                    kind: row.get(2)?,
                })
            },
        ).optional()?;

        Ok(result)
    }
}

/// Filter for querying resources
#[derive(Debug, Default, Clone)]
pub struct ResourceFilter {
    pub subscription_id: Option<String>,
    pub resource_group: Option<String>,
    pub resource_type: Option<String>,
    pub location: Option<String>,
    pub name_contains: Option<String>,
    pub limit: Option<usize>,
}

impl ResourceFilter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn subscription(mut self, id: impl Into<String>) -> Self {
        self.subscription_id = Some(id.into());
        self
    }

    pub fn resource_group(mut self, name: impl Into<String>) -> Self {
        self.resource_group = Some(name.into());
        self
    }

    pub fn resource_type(mut self, rtype: impl Into<String>) -> Self {
        self.resource_type = Some(rtype.into());
        self
    }

    pub fn location(mut self, loc: impl Into<String>) -> Self {
        self.location = Some(loc.into());
        self
    }

    pub fn name_contains(mut self, name: impl Into<String>) -> Self {
        self.name_contains = Some(name.into());
        self
    }

    pub fn limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use crate::cache::migrations;

    fn setup_test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        migrations::run_migrations(&conn).unwrap();
        conn
    }

    #[test]
    fn test_subscription_cache() {
        let conn = setup_test_db();
        let repo = CacheRepository::new(&conn);

        let sub = Subscription {
            id: "sub-1".to_string(),
            name: "Test Sub".to_string(),
            state: "Enabled".to_string(),
            tenant_id: "tenant-1".to_string(),
        };

        repo.cache_subscription(&sub).unwrap();
        let subs = repo.get_subscriptions().unwrap();
        assert_eq!(subs.len(), 1);
        assert_eq!(subs[0].name, "Test Sub");
    }

    #[test]
    fn test_resource_filter() {
        let conn = setup_test_db();
        let repo = CacheRepository::new(&conn);

        // First create the subscription (required by foreign key)
        let sub = Subscription {
            id: "sub-1".to_string(),
            name: "Test Sub".to_string(),
            state: "Enabled".to_string(),
            tenant_id: "tenant-1".to_string(),
        };
        repo.cache_subscription(&sub).unwrap();

        // Now create the resource
        let resource = Resource {
            id: "res-1".to_string(),
            name: "test-vm".to_string(),
            resource_type: "Microsoft.Compute/virtualMachines".to_string(),
            location: "eastus".to_string(),
            resource_group: "rg-1".to_string(),
            subscription_id: "sub-1".to_string(),
            tags: HashMap::new(),
        };

        repo.cache_resource(&resource).unwrap();

        let filter = ResourceFilter::new()
            .subscription("sub-1")
            .resource_type("Microsoft.Compute/virtualMachines");

        let resources = repo.get_resources(&filter).unwrap();
        assert_eq!(resources.len(), 1);
        assert_eq!(resources[0].name, "test-vm");
    }
}
