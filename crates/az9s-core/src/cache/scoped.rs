//! Scope-based cache reads and writes for cache-first rendering.
//!
//! A *scope* is what a view shows: a set of subscriptions, optionally narrowed
//! to one resource group. Reads return the rows plus the oldest `cached_at`,
//! so the UI can say how old the data is. Writes replace the whole scope in
//! one transaction, so resources deleted in Azure disappear from the cache.
//!
//! Rows whose subscription is not cached are skipped rather than failing the
//! whole write (the schema has foreign keys to `subscriptions`).
//!
//! Subscription IDs are passed as one JSON array parameter (`json_each`), so
//! scopes of any size avoid SQLite's bound-parameter limit.

use super::repository::CacheRepository;
use crate::{models::*, Result};
use rusqlite::params;

/// Cached rows and the time (unix seconds) the oldest of them was written.
#[derive(Debug, Default)]
pub struct Cached<T> {
    pub items: Vec<T>,
    pub oldest: Option<i64>,
}

fn ids_json(subscription_ids: &[String]) -> Result<String> {
    let lower: Vec<String> = subscription_ids
        .iter()
        .map(|s| s.to_ascii_lowercase())
        .collect();
    Ok(serde_json::to_string(&lower)?)
}

const IN_SCOPE: &str = "lower(subscription_id) IN (SELECT value FROM json_each(?1))";

impl CacheRepository<'_> {
    /// All cached subscriptions, ordered by name.
    pub fn cached_subscriptions(&self) -> Result<Cached<Subscription>> {
        let mut stmt = self.conn().prepare(
            "SELECT id, name, state, tenant_id, cached_at FROM subscriptions ORDER BY name",
        )?;
        let mut oldest: Option<i64> = None;
        let items = stmt
            .query_map([], |row| {
                let cached_at: i64 = row.get(4)?;
                oldest = Some(oldest.map_or(cached_at, |o| o.min(cached_at)));
                Ok(Subscription {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    state: row.get(2)?,
                    tenant_id: row.get(3)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(Cached { items, oldest })
    }

    /// Cached resource groups of these subscriptions, ordered by name.
    pub fn cached_resource_groups(
        &self,
        subscription_ids: &[String],
    ) -> Result<Cached<ResourceGroup>> {
        let mut stmt = self.conn().prepare(&format!(
            "SELECT id, name, location, subscription_id, tags, cached_at
             FROM resource_groups WHERE {IN_SCOPE} ORDER BY name"
        ))?;
        let mut oldest: Option<i64> = None;
        let items = stmt
            .query_map([ids_json(subscription_ids)?], |row| {
                let cached_at: i64 = row.get(5)?;
                oldest = Some(oldest.map_or(cached_at, |o| o.min(cached_at)));
                let tags: Option<String> = row.get(4)?;
                Ok(ResourceGroup {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    location: row.get(2)?,
                    subscription_id: row.get(3)?,
                    tags: tags
                        .and_then(|t| serde_json::from_str(&t).ok())
                        .unwrap_or_default(),
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(Cached { items, oldest })
    }

    /// Cached resources of these subscriptions (optionally one resource
    /// group, case-insensitive), ordered by name.
    pub fn cached_resources(
        &self,
        subscription_ids: &[String],
        resource_group: Option<&str>,
    ) -> Result<Cached<Resource>> {
        let mut stmt = self.conn().prepare(&format!(
            "SELECT id, name, resource_type, location, resource_group, subscription_id, tags, cached_at
             FROM resources
             WHERE {IN_SCOPE} AND (?2 IS NULL OR resource_group = ?2 COLLATE NOCASE)
             ORDER BY name"
        ))?;
        let mut oldest: Option<i64> = None;
        let items = stmt
            .query_map(
                params![ids_json(subscription_ids)?, resource_group],
                |row| {
                    let cached_at: i64 = row.get(7)?;
                    oldest = Some(oldest.map_or(cached_at, |o| o.min(cached_at)));
                    let tags: Option<String> = row.get(6)?;
                    Ok(Resource {
                        id: row.get(0)?,
                        name: row.get(1)?,
                        resource_type: row.get(2)?,
                        location: row.get(3)?,
                        resource_group: row.get(4)?,
                        subscription_id: row.get(5)?,
                        tags: tags
                            .and_then(|t| serde_json::from_str(&t).ok())
                            .unwrap_or_default(),
                    })
                },
            )?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(Cached { items, oldest })
    }

    /// Replace the cached subscription list with `items`.
    ///
    /// Subscriptions that are gone are deleted, and their resource groups and
    /// resources go with them (ON DELETE CASCADE). The others are updated in
    /// place: never `INSERT OR REPLACE` a parent row, because SQLite implements
    /// REPLACE as delete + insert, which cascades and wipes the children.
    pub fn replace_subscriptions(&self, items: &[Subscription]) -> Result<()> {
        let now = chrono::Utc::now().timestamp();
        let ids: Vec<String> = items.iter().map(|s| s.id.clone()).collect();
        let tx = self.conn().unchecked_transaction()?;
        tx.execute(
            "DELETE FROM subscriptions WHERE lower(id) NOT IN (SELECT value FROM json_each(?1))",
            [ids_json(&ids)?],
        )?;
        let mut insert = tx.prepare(
            "INSERT INTO subscriptions (id, name, state, tenant_id, cached_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(id) DO UPDATE SET name = excluded.name, state = excluded.state,
                   tenant_id = excluded.tenant_id, cached_at = excluded.cached_at",
        )?;
        for s in items {
            insert.execute(params![s.id, s.name, s.state, s.tenant_id, now])?;
        }
        drop(insert);
        tx.commit()?;
        Ok(())
    }

    /// Replace the cached resource groups of these subscriptions with `items`.
    pub fn replace_resource_groups(
        &self,
        subscription_ids: &[String],
        items: &[ResourceGroup],
    ) -> Result<()> {
        let now = chrono::Utc::now().timestamp();
        let tx = self.conn().unchecked_transaction()?;
        tx.execute(
            &format!("DELETE FROM resource_groups WHERE {IN_SCOPE}"),
            [ids_json(subscription_ids)?],
        )?;
        let mut insert = tx.prepare(
            "INSERT INTO resource_groups
                 (id, name, location, subscription_id, tags, cached_at)
                 SELECT ?1, ?2, ?3, ?4, ?5, ?6
                 WHERE EXISTS (SELECT 1 FROM subscriptions WHERE id = ?4)
                 ON CONFLICT(id) DO UPDATE SET name = excluded.name, location = excluded.location,
                   subscription_id = excluded.subscription_id, tags = excluded.tags,
                   cached_at = excluded.cached_at",
        )?;
        for rg in items {
            insert.execute(params![
                rg.id,
                rg.name,
                rg.location,
                rg.subscription_id,
                serde_json::to_string(&rg.tags)?,
                now
            ])?;
        }
        drop(insert);
        tx.commit()?;
        Ok(())
    }

    /// Replace the cached resources of this scope with `items`.
    pub fn replace_resources(
        &self,
        subscription_ids: &[String],
        resource_group: Option<&str>,
        items: &[Resource],
    ) -> Result<()> {
        let now = chrono::Utc::now().timestamp();
        let tx = self.conn().unchecked_transaction()?;
        tx.execute(
            &format!(
                "DELETE FROM resources
                 WHERE {IN_SCOPE} AND (?2 IS NULL OR resource_group = ?2 COLLATE NOCASE)"
            ),
            params![ids_json(subscription_ids)?, resource_group],
        )?;
        let mut insert = tx.prepare(
                "INSERT INTO resources
                 (id, name, resource_type, location, resource_group, subscription_id, tags, cached_at)
                 SELECT ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8
                 WHERE EXISTS (SELECT 1 FROM subscriptions WHERE id = ?6)
                 ON CONFLICT(id) DO UPDATE SET name = excluded.name,
                   resource_type = excluded.resource_type, location = excluded.location,
                   resource_group = excluded.resource_group,
                   subscription_id = excluded.subscription_id, tags = excluded.tags,
                   cached_at = excluded.cached_at",
        )?;
        for r in items {
            insert.execute(params![
                r.id,
                r.name,
                r.resource_type,
                r.location,
                r.resource_group,
                r.subscription_id,
                serde_json::to_string(&r.tags)?,
                now
            ])?;
        }
        drop(insert);
        tx.commit()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::cache::Cache;
    use crate::models::*;
    use std::collections::HashMap;
    use std::path::Path;

    fn rg(sub: &str, name: &str) -> ResourceGroup {
        ResourceGroup {
            id: format!("/subscriptions/{sub}/resourceGroups/{name}"),
            name: name.into(),
            location: "westeurope".into(),
            subscription_id: sub.into(),
            tags: HashMap::from([("env".to_string(), "prod".to_string())]),
        }
    }

    fn res(sub: &str, group: &str, name: &str) -> Resource {
        Resource {
            id: format!("/subscriptions/{sub}/resourceGroups/{group}/providers/x/{name}"),
            name: name.into(),
            resource_type: "x".into(),
            location: "westeurope".into(),
            resource_group: group.into(),
            subscription_id: sub.into(),
            tags: HashMap::new(),
        }
    }

    fn ids(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    fn sub(id: &str) -> Subscription {
        Subscription {
            id: id.into(),
            name: id.into(),
            state: "Enabled".into(),
            tenant_id: "t".into(),
        }
    }

    /// Cache with these subscriptions (children need their parent row).
    fn cache_with(subs: &[&str]) -> Cache {
        let cache = Cache::new(Path::new(":memory:")).unwrap();
        let subs: Vec<_> = subs.iter().map(|s| sub(s)).collect();
        cache.repository().replace_subscriptions(&subs).unwrap();
        cache
    }

    #[test]
    fn resource_groups_round_trip_by_scope_case_insensitively() {
        let cache = cache_with(&["a", "b"]);
        let repo = cache.repository();
        repo.replace_resource_groups(
            &ids(&["a", "b"]),
            &[rg("a", "rg-2"), rg("a", "rg-1"), rg("b", "rg-3")],
        )
        .unwrap();

        let got = repo.cached_resource_groups(&ids(&["A"])).unwrap();
        let names: Vec<_> = got.items.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, ["rg-1", "rg-2"]);
        assert_eq!(
            got.items[0].tags.get("env").map(String::as_str),
            Some("prod")
        );
        assert!(got.oldest.is_some());
        assert!(repo
            .cached_resource_groups(&ids(&["none"]))
            .unwrap()
            .items
            .is_empty());
        assert!(repo
            .cached_resource_groups(&ids(&["none"]))
            .unwrap()
            .oldest
            .is_none());
    }

    #[test]
    fn replacing_a_scope_drops_deleted_rows_but_keeps_other_scopes() {
        let cache = cache_with(&["a", "b"]);
        let repo = cache.repository();
        repo.replace_resource_groups(&ids(&["a", "b"]), &[rg("a", "old"), rg("b", "keep")])
            .unwrap();
        // Refresh of subscription a: "old" was deleted in Azure, "new" was created.
        repo.replace_resource_groups(&ids(&["a"]), &[rg("a", "new")])
            .unwrap();

        let a = repo.cached_resource_groups(&ids(&["a"])).unwrap();
        assert_eq!(
            a.items.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(),
            ["new"]
        );
        let b = repo.cached_resource_groups(&ids(&["b"])).unwrap();
        assert_eq!(b.items.len(), 1, "other subscriptions untouched");
    }

    #[test]
    fn resources_scope_to_one_resource_group() {
        let cache = cache_with(&["s"]);
        let repo = cache.repository();
        repo.replace_resources(
            &ids(&["s"]),
            None,
            &[res("s", "RG-App", "vm-1"), res("s", "rg-db", "sql-1")],
        )
        .unwrap();

        let app = repo.cached_resources(&ids(&["s"]), Some("rg-app")).unwrap();
        assert_eq!(app.items.len(), 1);
        assert_eq!(app.items[0].name, "vm-1");

        // Refreshing one group must not delete the other group's rows.
        repo.replace_resources(&ids(&["s"]), Some("RG-APP"), &[])
            .unwrap();
        assert!(repo
            .cached_resources(&ids(&["s"]), Some("rg-app"))
            .unwrap()
            .items
            .is_empty());
        assert_eq!(
            repo.cached_resources(&ids(&["s"]), None)
                .unwrap()
                .items
                .len(),
            1
        );
    }

    #[test]
    fn rows_for_unknown_subscriptions_are_skipped_not_fatal() {
        let cache = cache_with(&["a"]);
        let repo = cache.repository();
        repo.replace_resource_groups(&ids(&["a", "ghost"]), &[rg("a", "rg"), rg("ghost", "x")])
            .unwrap();
        assert_eq!(
            repo.cached_resource_groups(&ids(&["a", "ghost"]))
                .unwrap()
                .items
                .len(),
            1
        );
    }

    #[test]
    fn large_scopes_work_without_hitting_parameter_limits() {
        let subs: Vec<String> = (0..2000).map(|i| format!("sub-{i}")).collect();
        let names: Vec<&str> = subs.iter().map(String::as_str).collect();
        let cache = cache_with(&names);
        let repo = cache.repository();
        let rgs: Vec<_> = subs.iter().map(|s| rg(s, "rg")).collect();
        repo.replace_resource_groups(&subs, &rgs).unwrap();
        assert_eq!(
            repo.cached_resource_groups(&subs).unwrap().items.len(),
            2000
        );
    }

    #[test]
    fn replacing_subscriptions_drops_removed_ones_and_their_children_only() {
        let cache = cache_with(&["a", "b"]);
        let repo = cache.repository();
        repo.replace_resource_groups(&ids(&["a", "b"]), &[rg("a", "rg-a"), rg("b", "rg-b")])
            .unwrap();

        repo.replace_subscriptions(&[sub("b")]).unwrap();

        let got = repo.get_subscriptions().unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].id, "b");
        assert!(repo
            .cached_resource_groups(&ids(&["a"]))
            .unwrap()
            .items
            .is_empty());
        assert_eq!(
            repo.cached_resource_groups(&ids(&["b"]))
                .unwrap()
                .items
                .len(),
            1
        );
    }

    /// Regression: refreshing subscriptions used `INSERT OR REPLACE`, which
    /// SQLite runs as delete + insert, cascading to every cached resource
    /// group and resource. Every app start wiped the cache.
    #[test]
    fn refreshing_subscriptions_keeps_cached_children() {
        let cache = cache_with(&["a"]);
        let repo = cache.repository();
        repo.replace_resource_groups(&ids(&["a"]), &[rg("a", "rg")])
            .unwrap();
        repo.replace_resources(&ids(&["a"]), None, &[res("a", "rg", "vm")])
            .unwrap();

        repo.replace_subscriptions(&[Subscription {
            name: "renamed".into(),
            ..sub("a")
        }])
        .unwrap();
        repo.cache_subscriptions(&[sub("a")]).unwrap();

        assert_eq!(
            repo.cached_resource_groups(&ids(&["a"]))
                .unwrap()
                .items
                .len(),
            1
        );
        assert_eq!(
            repo.cached_resources(&ids(&["a"]), None)
                .unwrap()
                .items
                .len(),
            1
        );
    }
}

#[cfg(test)]
mod perf {
    use crate::cache::Cache;
    use crate::models::*;
    use std::time::Instant;

    /// Cache read/write cost for a large scope. Run with
    /// `cargo test --release -p az9s-core --lib cache_scope_perf -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn cache_scope_perf() {
        let dir = std::env::temp_dir().join(format!("az9s-perf-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let cache = Cache::new(&dir.join("perf.db")).unwrap();
        let repo = cache.repository();
        let subs: Vec<Subscription> = (0..400)
            .map(|i| Subscription {
                id: format!("sub-{i:04}"),
                name: format!("sub {i}"),
                state: "Enabled".into(),
                tenant_id: "t".into(),
            })
            .collect();
        repo.replace_subscriptions(&subs).unwrap();
        let ids: Vec<String> = subs.iter().map(|s| s.id.clone()).collect();
        for n in [20_000usize, 100_000] {
            let rows: Vec<Resource> = (0..n)
                .map(|i| Resource {
                    id: format!("/subscriptions/sub-{:04}/resourceGroups/rg-{}/providers/Microsoft.Compute/virtualMachines/vm-{i}", i % 400, i % 1000),
                    name: format!("vm-{i:06}"),
                    resource_type: "microsoft.compute/virtualmachines".into(),
                    location: "westeurope".into(),
                    resource_group: format!("rg-{}", i % 1000),
                    subscription_id: format!("sub-{:04}", i % 400),
                    tags: [("env".to_string(), "prod".to_string())].into(),
                })
                .collect();
            let t = Instant::now();
            repo.replace_resources(&ids, None, &rows).unwrap();
            let write = t.elapsed();
            let t = Instant::now();
            let got = repo.cached_resources(&ids, None).unwrap();
            let read = t.elapsed();
            let t = Instant::now();
            let one_rg = repo.cached_resources(&ids[..1], Some("rg-0")).unwrap();
            let read_rg = t.elapsed();
            println!(
                "{n} resources: write {write:?}, read all {read:?} ({}), read one RG {read_rg:?} ({})",
                got.items.len(),
                one_rg.items.len()
            );
        }
        std::fs::remove_dir_all(&dir).ok();
    }
}
