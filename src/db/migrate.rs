// Clean schema bootstrap for the current Scargo database shape.
// TimescaleDB is required: readings are stored in a hypertable.

use crate::db::Database;
use crate::ingest::canonical;
use crate::Error;

const RESET_DDL: &[&str] = &[
    "DROP MATERIALIZED VIEW IF EXISTS obd2_metric_hourly;",
    "DROP TABLE IF EXISTS vehicle_metric_day;",
    "DROP TABLE IF EXISTS obd2_metric_reading;",
    "DROP TABLE IF EXISTS dropbox_ingest_file;",
    "DROP TABLE IF EXISTS dropbox_oauth_state;",
    "DROP TABLE IF EXISTS dropbox_connection;",
    "DROP TABLE IF EXISTS account_vehicle_upload;",
    "DROP TABLE IF EXISTS account_vehicle_profile;",
    "DROP TABLE IF EXISTS ingest_upload;",
    "DROP TABLE IF EXISTS vehicle_ownership;",
    "DROP TABLE IF EXISTS obd2_metric;",
    "DROP TABLE IF EXISTS account_api_token;",
    "DROP TABLE IF EXISTS account_session;",
    "DROP TABLE IF EXISTS account;",
    "DROP TABLE IF EXISTS vehicle;",
];

const CORE_DDL: &[&str] = &[
    "CREATE EXTENSION IF NOT EXISTS timescaledb;",
    "CREATE TABLE IF NOT EXISTS vehicle (
        id            UUID PRIMARY KEY,
        vin           TEXT NOT NULL UNIQUE,
        make          TEXT NOT NULL DEFAULT '',
        model         TEXT NOT NULL DEFAULT '',
        engine_family TEXT NOT NULL DEFAULT '',
        year          INT4 NOT NULL DEFAULT 0,
        created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
        updated_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
    );",
    "CREATE TABLE IF NOT EXISTS account (
        id            UUID PRIMARY KEY,
        username      TEXT UNIQUE,
        label         TEXT NOT NULL DEFAULT '',
        display_name  TEXT NOT NULL DEFAULT '',
        password_hash TEXT,
        is_guest      BOOLEAN NOT NULL DEFAULT FALSE,
        created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
        updated_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
    );",
    "CREATE TABLE IF NOT EXISTS account_session (
        token_hash TEXT PRIMARY KEY,
        account_id UUID NOT NULL REFERENCES account(id) ON DELETE CASCADE,
        created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
        expires_at TIMESTAMPTZ NOT NULL
    );",
    "CREATE TABLE IF NOT EXISTS account_api_token (
        token_hash   TEXT PRIMARY KEY,
        account_id   UUID NOT NULL REFERENCES account(id) ON DELETE CASCADE,
        label        TEXT NOT NULL DEFAULT '',
        created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
        last_used_at TIMESTAMPTZ,
        revoked_at   TIMESTAMPTZ
    );",
    "CREATE TABLE IF NOT EXISTS dropbox_connection (
        id                         UUID PRIMARY KEY,
        account_id                 UUID NOT NULL UNIQUE REFERENCES account(id) ON DELETE CASCADE,
        dropbox_account_id         TEXT NOT NULL,
        root_path                  TEXT NOT NULL DEFAULT '/OBD Fusion/CsvLogs',
        encrypted_refresh_token    BYTEA NOT NULL,
        encrypted_access_token     BYTEA,
        access_token_expires_at    TIMESTAMPTZ,
        cursor                     TEXT,
        status                     TEXT NOT NULL DEFAULT 'active',
        last_sync_at               TIMESTAMPTZ,
        last_success_at            TIMESTAMPTZ,
        latest_error               TEXT,
        created_at                 TIMESTAMPTZ NOT NULL DEFAULT NOW(),
        updated_at                 TIMESTAMPTZ NOT NULL DEFAULT NOW(),
        CHECK (status IN ('active', 'paused', 'error'))
    );",
    "CREATE TABLE IF NOT EXISTS dropbox_oauth_state (
        state_hash    TEXT PRIMARY KEY,
        account_id    UUID NOT NULL REFERENCES account(id) ON DELETE CASCADE,
        redirect_path TEXT NOT NULL,
        created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
        expires_at    TIMESTAMPTZ NOT NULL
    );",
    "CREATE TABLE IF NOT EXISTS dropbox_ingest_file (
        id              UUID PRIMARY KEY,
        connection_id   UUID NOT NULL REFERENCES dropbox_connection(id) ON DELETE CASCADE,
        account_id      UUID NOT NULL REFERENCES account(id) ON DELETE CASCADE,
        dropbox_file_id TEXT,
        path_lower      TEXT NOT NULL,
        rev             TEXT,
        content_hash    TEXT,
        vin             TEXT,
        upload_id       UUID REFERENCES ingest_upload(id) ON DELETE SET NULL,
        status          TEXT NOT NULL DEFAULT 'pending',
        rows_ingested   BIGINT NOT NULL DEFAULT 0,
        duplicate       BOOLEAN NOT NULL DEFAULT FALSE,
        latest_error    TEXT,
        seen_at         TIMESTAMPTZ NOT NULL DEFAULT NOW(),
        ingested_at     TIMESTAMPTZ,
        CHECK (status IN ('pending', 'ingested', 'duplicate', 'skipped', 'deleted', 'error'))
    );",
    "CREATE TABLE IF NOT EXISTS ingest_upload (
        id            UUID PRIMARY KEY,
        vehicle_id    UUID NOT NULL REFERENCES vehicle(id),
        content_hash  TEXT NOT NULL,
        content_type  TEXT NOT NULL DEFAULT '',
        bytes         BIGINT NOT NULL DEFAULT 0,
        rows_ingested BIGINT NOT NULL DEFAULT 0,
        approved_exact_vin_at TIMESTAMPTZ,
        approved_cohort_at TIMESTAMPTZ,
        created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
        completed_at  TIMESTAMPTZ,
        UNIQUE (vehicle_id, content_hash)
    );",
    "CREATE TABLE IF NOT EXISTS account_vehicle_profile (
        account_id    UUID NOT NULL REFERENCES account(id),
        vehicle_id    UUID NOT NULL REFERENCES vehicle(id),
        exact_vin_share_enabled BOOLEAN NOT NULL DEFAULT FALSE,
        created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
        updated_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
        PRIMARY KEY (account_id, vehicle_id)
    );",
    "CREATE TABLE IF NOT EXISTS account_vehicle_upload (
        account_id    UUID NOT NULL REFERENCES account(id),
        upload_id     UUID NOT NULL REFERENCES ingest_upload(id),
        vehicle_id    UUID NOT NULL REFERENCES vehicle(id),
        private_access BOOLEAN NOT NULL DEFAULT TRUE,
        exact_vin_share_enabled BOOLEAN NOT NULL DEFAULT FALSE,
        linked_at     TIMESTAMPTZ NOT NULL DEFAULT NOW(),
        access_revoked_at TIMESTAMPTZ,
        PRIMARY KEY (account_id, upload_id)
    );",
    "INSERT INTO account (id, username, label, display_name, is_guest)
        VALUES ('889705d1-e9c0-53ca-9415-37f0afc024ff', 'guest', 'local-dev', 'Guest', TRUE)
        ON CONFLICT (id) DO UPDATE SET
            username = COALESCE(account.username, EXCLUDED.username),
            label = EXCLUDED.label,
            display_name = COALESCE(NULLIF(account.display_name, ''), EXCLUDED.display_name),
            is_guest = TRUE;",
    "CREATE TABLE IF NOT EXISTS obd2_metric (
        id          BIGSERIAL PRIMARY KEY,
        key         TEXT NOT NULL UNIQUE,
        label       TEXT NOT NULL,
        unit        TEXT,
        value_kind  TEXT NOT NULL CHECK (value_kind IN ('numeric', 'text')),
        created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
    );",
    "CREATE TABLE IF NOT EXISTS obd2_metric_reading (
        time        TIMESTAMPTZ NOT NULL,
        upload_id   UUID NOT NULL REFERENCES ingest_upload(id),
        vehicle_id  UUID NOT NULL,
        metric_id   BIGINT NOT NULL,
        value       DOUBLE PRECISION,
        text_value  TEXT,
        CHECK (
            (value IS NOT NULL AND text_value IS NULL)
            OR (value IS NULL AND text_value IS NOT NULL)
        )
    );",
    "CREATE TABLE IF NOT EXISTS vehicle_metric_day (
        bucket_day    TIMESTAMPTZ NOT NULL,
        upload_id     UUID NOT NULL REFERENCES ingest_upload(id),
        vehicle_id    UUID NOT NULL REFERENCES vehicle(id),
        metric_id     BIGINT NOT NULL REFERENCES obd2_metric(id),
        value_sum     DOUBLE PRECISION NOT NULL,
        min_value     DOUBLE PRECISION NOT NULL,
        max_value     DOUBLE PRECISION NOT NULL,
        reading_count BIGINT NOT NULL,
        PRIMARY KEY (bucket_day, upload_id, vehicle_id, metric_id)
    );",
    "SELECT create_hypertable(
        'obd2_metric_reading',
        'time',
        if_not_exists => true,
        migrate_data => false
    );",
];

const RUNTIME_DDL: &[&str] = &[
    "CREATE INDEX IF NOT EXISTS idx_metric_reading_vehicle_metric_time
        ON obd2_metric_reading (vehicle_id, metric_id, time DESC);",
    "CREATE INDEX IF NOT EXISTS idx_metric_reading_upload_metric_time
        ON obd2_metric_reading (upload_id, metric_id, time DESC);",
    "CREATE INDEX IF NOT EXISTS idx_metric_key
        ON obd2_metric (key);",
    "CREATE INDEX IF NOT EXISTS idx_metric_key_dashboard
        ON obd2_metric (key) INCLUDE (id, label, unit, value_kind);",
    "CREATE INDEX IF NOT EXISTS idx_vehicle_metric_day_upload_bucket
        ON vehicle_metric_day (upload_id, bucket_day DESC, metric_id);",
    "CREATE INDEX IF NOT EXISTS idx_vehicle_metric_day_metric_bucket
        ON vehicle_metric_day (metric_id, bucket_day DESC, vehicle_id);",
    "CREATE INDEX IF NOT EXISTS idx_vehicle_metric_day_vehicle_bucket
        ON vehicle_metric_day (vehicle_id, bucket_day DESC, metric_id);",
    "CREATE INDEX IF NOT EXISTS idx_account_vehicle_profile_vehicle
        ON account_vehicle_profile (vehicle_id, exact_vin_share_enabled);",
    "CREATE INDEX IF NOT EXISTS idx_account_vehicle_upload_account_vehicle
        ON account_vehicle_upload (account_id, vehicle_id, private_access, linked_at DESC);",
    "CREATE INDEX IF NOT EXISTS idx_account_vehicle_upload_public_exact
        ON account_vehicle_upload (vehicle_id, exact_vin_share_enabled, upload_id);",
    "CREATE INDEX IF NOT EXISTS idx_ingest_upload_vehicle_time
        ON ingest_upload (vehicle_id, created_at DESC);",
    "CREATE INDEX IF NOT EXISTS idx_account_session_account_expires
        ON account_session (account_id, expires_at DESC);",
    "CREATE INDEX IF NOT EXISTS idx_account_api_token_account
        ON account_api_token (account_id, created_at DESC)
        WHERE revoked_at IS NULL;",
    "CREATE INDEX IF NOT EXISTS idx_dropbox_connection_status
        ON dropbox_connection (status, updated_at DESC);",
    "CREATE INDEX IF NOT EXISTS idx_dropbox_oauth_state_account_expires
        ON dropbox_oauth_state (account_id, expires_at DESC);",
    "CREATE UNIQUE INDEX IF NOT EXISTS idx_dropbox_ingest_file_conn_path_rev
        ON dropbox_ingest_file (connection_id, path_lower, COALESCE(rev, ''));",
    "CREATE INDEX IF NOT EXISTS idx_dropbox_ingest_file_conn_file
        ON dropbox_ingest_file (connection_id, dropbox_file_id);",
    "ALTER TABLE obd2_metric_reading SET (
        timescaledb.compress,
        timescaledb.compress_segmentby = 'vehicle_id, metric_id',
        timescaledb.compress_orderby = 'time DESC'
    );",
    "SELECT add_compression_policy(
        'obd2_metric_reading',
        INTERVAL '7 days',
        if_not_exists => true
    );",
    "CREATE MATERIALIZED VIEW IF NOT EXISTS obd2_metric_hourly
        WITH (timescaledb.continuous) AS
        SELECT time_bucket('1 hour', r.time) AS bucket,
               r.vehicle_id,
               r.metric_id,
               AVG(r.value)::DOUBLE PRECISION AS avg_value,
               MIN(r.value) AS min_value,
               MAX(r.value) AS max_value,
               COUNT(*)::BIGINT AS reading_count
        FROM obd2_metric_reading r
        WHERE r.value IS NOT NULL
        GROUP BY bucket, r.vehicle_id, r.metric_id
        WITH NO DATA;",
];

const ANALYZE_SQL: &[&str] = &[
    "ANALYZE vehicle;",
    "ANALYZE account;",
    "ANALYZE account_session;",
    "ANALYZE account_api_token;",
    "ANALYZE dropbox_connection;",
    "ANALYZE dropbox_oauth_state;",
    "ANALYZE dropbox_ingest_file;",
    "ANALYZE ingest_upload;",
    "ANALYZE account_vehicle_profile;",
    "ANALYZE account_vehicle_upload;",
    "ANALYZE obd2_metric;",
    "ANALYZE obd2_metric_reading;",
    "ANALYZE vehicle_metric_day;",
];

pub async fn run(db: &Database) -> Result<(), Error> {
    bootstrap(db, CORE_DDL).await?;
    bootstrap(db, RUNTIME_DDL).await?;
    tracing::info!("Database runtime bootstrap complete");
    Ok(())
}

pub async fn rebuild_for_bulk_load(db: &Database) -> Result<(), Error> {
    let client = db.get().await?;
    for sql in RESET_DDL {
        execute(&client, sql).await?;
    }
    drop(client);
    bootstrap(db, CORE_DDL).await?;
    tracing::info!("Database bulk-load bootstrap complete");
    Ok(())
}

pub async fn finalize_bulk_load(db: &Database) -> Result<(), Error> {
    let client = db.get().await?;
    execute(&client, "TRUNCATE TABLE vehicle_metric_day;").await?;
    let rollup_sql = rollup_sql();
    execute(&client, &rollup_sql).await?;
    for sql in RUNTIME_DDL {
        execute(&client, sql).await?;
    }
    for sql in ANALYZE_SQL {
        execute(&client, sql).await?;
    }
    tracing::info!("Database bulk-load finalize complete");
    Ok(())
}

fn rollup_sql() -> String {
    let keys = canonical::rollup_metric_keys();
    let key_list = keys
        .iter()
        .map(|key| sql_string_literal(key))
        .collect::<Vec<_>>()
        .join(", ");
    let duplicate_pattern = keys.join("|");

    format!(
        "INSERT INTO vehicle_metric_day
    (bucket_day, upload_id, vehicle_id, metric_id, value_sum, min_value, max_value, reading_count)
SELECT date_trunc('day', r.time) AS bucket_day,
       r.upload_id,
       r.vehicle_id,
       r.metric_id,
       SUM(r.value) AS value_sum,
       MIN(r.value) AS min_value,
       MAX(r.value) AS max_value,
       COUNT(*)::BIGINT AS reading_count
FROM obd2_metric_reading r
JOIN obd2_metric m ON m.id = r.metric_id
WHERE r.value IS NOT NULL
  AND (
      m.key = ANY(ARRAY[{key_list}]::text[])
      OR m.key ~ '^({duplicate_pattern})_[0-9]+$'
  )
GROUP BY 1, 2, 3, 4;"
    )
}

fn sql_string_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

async fn bootstrap(db: &Database, ddl: &[&str]) -> Result<(), Error> {
    let client = db.get().await?;
    for sql in ddl {
        execute(&client, sql).await?;
    }
    Ok(())
}

async fn execute(client: &deadpool_postgres::Client, sql: &str) -> Result<(), Error> {
    client.execute(sql, &[]).await.map_err(|e| {
        tracing::error!("Schema bootstrap failed: {e}\n  SQL: {sql}");
        Error::Database
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn runtime_ddl_is_not_empty() {
        assert!(!super::CORE_DDL.is_empty());
        assert!(!super::RUNTIME_DDL.is_empty());
    }

    #[test]
    fn dropbox_tables_are_bootstrapped() {
        let ddl = super::CORE_DDL.join("\n");
        let runtime = super::RUNTIME_DDL.join("\n");
        assert!(ddl.contains("CREATE TABLE IF NOT EXISTS dropbox_connection"));
        assert!(ddl.contains("CREATE TABLE IF NOT EXISTS dropbox_oauth_state"));
        assert!(ddl.contains("CREATE TABLE IF NOT EXISTS dropbox_ingest_file"));
        assert!(runtime.contains("idx_dropbox_connection_status"));
    }

    #[test]
    fn bulk_rollup_sql_uses_public_allowlist() {
        let sql = super::rollup_sql();

        assert!(sql.contains("JOIN obd2_metric"));
        assert!(sql.contains("'vehicle_speed'"));
        assert!(sql.contains("vehicle_speed"));
        assert!(sql.contains("_[0-9]+"));
        assert!(!sql.contains("'latitude'"));
        assert!(!sql.contains("'accel_x'"));
    }
}
