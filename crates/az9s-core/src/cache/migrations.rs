use rusqlite::{Connection, Result};
use tracing::info;

/// Current schema version
const CURRENT_VERSION: i32 = 1;

/// Run all pending migrations
pub fn run_migrations(conn: &Connection) -> Result<()> {
    // Create migrations table if it doesn't exist
    conn.execute(
        r#"
        CREATE TABLE IF NOT EXISTS schema_migrations (
            version INTEGER PRIMARY KEY,
            applied_at INTEGER NOT NULL
        )
        "#,
        [],
    )?;

    let current_version = get_current_version(conn)?;
    info!("Current schema version: {}", current_version);

    if current_version < CURRENT_VERSION {
        info!(
            "Running migrations from {} to {}",
            current_version, CURRENT_VERSION
        );

        for version in (current_version + 1)..=CURRENT_VERSION {
            apply_migration(conn, version)?;
        }
    }

    Ok(())
}

/// Get the current schema version
fn get_current_version(conn: &Connection) -> Result<i32> {
    let version: Result<i32> =
        conn.query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
            row.get(0)
        });

    match version {
        Ok(v) => Ok(v),
        Err(_) => Ok(0), // No migrations applied yet
    }
}

/// Apply a specific migration
fn apply_migration(conn: &Connection, version: i32) -> Result<()> {
    info!("Applying migration version {}", version);

    match version {
        1 => migration_v1(conn)?,
        _ => panic!("Unknown migration version: {}", version),
    }

    // Record migration
    let now = chrono::Utc::now().timestamp();
    conn.execute(
        "INSERT INTO schema_migrations (version, applied_at) VALUES (?1, ?2)",
        [version, now as i32],
    )?;

    Ok(())
}

/// Initial schema migration
fn migration_v1(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        -- Subscriptions table
        CREATE TABLE IF NOT EXISTS subscriptions (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            state TEXT NOT NULL,
            tenant_id TEXT NOT NULL,
            cached_at INTEGER NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_subscriptions_cached_at 
            ON subscriptions(cached_at);

        -- Resource groups table
        CREATE TABLE IF NOT EXISTS resource_groups (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            location TEXT NOT NULL,
            subscription_id TEXT NOT NULL,
            tags TEXT,
            cached_at INTEGER NOT NULL,
            FOREIGN KEY (subscription_id) REFERENCES subscriptions(id) ON DELETE CASCADE
        );

        CREATE INDEX IF NOT EXISTS idx_resource_groups_subscription 
            ON resource_groups(subscription_id);
        CREATE INDEX IF NOT EXISTS idx_resource_groups_cached_at 
            ON resource_groups(cached_at);
        CREATE INDEX IF NOT EXISTS idx_resource_groups_name 
            ON resource_groups(name);

        -- Resources table
        CREATE TABLE IF NOT EXISTS resources (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            resource_type TEXT NOT NULL,
            location TEXT NOT NULL,
            resource_group TEXT NOT NULL,
            subscription_id TEXT NOT NULL,
            tags TEXT,
            cached_at INTEGER NOT NULL,
            FOREIGN KEY (subscription_id) REFERENCES subscriptions(id) ON DELETE CASCADE
        );

        CREATE INDEX IF NOT EXISTS idx_resources_subscription 
            ON resources(subscription_id);
        CREATE INDEX IF NOT EXISTS idx_resources_rg 
            ON resources(resource_group);
        CREATE INDEX IF NOT EXISTS idx_resources_type 
            ON resources(resource_type);
        CREATE INDEX IF NOT EXISTS idx_resources_location 
            ON resources(location);
        CREATE INDEX IF NOT EXISTS idx_resources_cached_at 
            ON resources(cached_at);
        CREATE INDEX IF NOT EXISTS idx_resources_name 
            ON resources(name);

        -- Resource details table for extended properties
        CREATE TABLE IF NOT EXISTS resource_details (
            resource_id TEXT PRIMARY KEY,
            properties TEXT NOT NULL,
            sku TEXT,
            kind TEXT,
            cached_at INTEGER NOT NULL,
            FOREIGN KEY (resource_id) REFERENCES resources(id) ON DELETE CASCADE
        );

        CREATE INDEX IF NOT EXISTS idx_resource_details_cached_at 
            ON resource_details(cached_at);
        "#,
    )?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    #[test]
    fn test_migrations() {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();

        let version = get_current_version(&conn).unwrap();
        assert_eq!(version, CURRENT_VERSION);
    }

    #[test]
    fn test_idempotent_migrations() {
        let conn = Connection::open_in_memory().unwrap();

        // Run migrations twice
        run_migrations(&conn).unwrap();
        run_migrations(&conn).unwrap();

        let version = get_current_version(&conn).unwrap();
        assert_eq!(version, CURRENT_VERSION);
    }
}
