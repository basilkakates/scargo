use crate::db::Database;
use crate::ingest::{model, vin};
use crate::Error;
use actix_web::{post, web, HttpRequest, HttpResponse};
use chrono::{DateTime, Duration, Utc};
use reqwest::{StatusCode, Url};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use uuid::Uuid;

const VPIC_API_BASE: &str = "https://vpic.nhtsa.dot.gov/api/vehicles/DecodeVinValuesExtended";

#[derive(Deserialize)]
struct IngestQuery {
    vin: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CsvIngestResult {
    pub upload_id: Uuid,
    pub rows_ingested: i64,
    pub duplicate: bool,
    pub vehicle_id: Uuid,
    pub content_hash: String,
}

#[post("/ingest/csv")]
pub(crate) async fn upload_csv(
    db: web::Data<Database>,
    req: HttpRequest,
    query: web::Query<IngestQuery>,
    body: web::Bytes,
) -> Result<HttpResponse, Error> {
    let vin = query.vin.trim();
    if vin.is_empty() {
        return Err(Error::BadRequest("missing ?vin=VIN query parameter".into()));
    }

    let content_type = req
        .headers()
        .get(actix_web::http::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");
    let client = db.get().await?;
    let account_id = super::privacy::account_id(&client, &req).await?;
    let result = ingest_csv_for_account(&db, account_id, vin, body.as_ref(), content_type).await?;
    Ok(HttpResponse::Ok().json(result))
}

pub async fn ingest_csv_for_account(
    db: &Database,
    account_id: Uuid,
    vin: &str,
    body: &[u8],
    content_type: &str,
) -> Result<CsvIngestResult, Error> {
    let vin = vin.trim();
    if vin.is_empty() {
        return Err(Error::BadRequest("missing vin".into()));
    }

    let vehicle_id = model::vin_to_uuid(vin);
    let metadata = vin::decode(vin);
    let content_hash = packet_hash(body);
    let bytes = body.len() as i64;
    let client = db.get().await?;
    client
        .execute(
            "INSERT INTO vehicle (id, vin, make, model, engine_family, year)
             VALUES ($1, $2, $3, $4, $5, $6)
             ON CONFLICT (id) DO UPDATE SET
                make = CASE
                    WHEN EXCLUDED.make <> '' THEN EXCLUDED.make
                    ELSE vehicle.make
                END,
                model = CASE
                    WHEN EXCLUDED.model <> '' THEN EXCLUDED.model
                    ELSE vehicle.model
                END,
                engine_family = CASE
                    WHEN EXCLUDED.engine_family <> '' THEN EXCLUDED.engine_family
                    ELSE vehicle.engine_family
                END,
                year = CASE
                    WHEN EXCLUDED.year > 0 THEN EXCLUDED.year
                    ELSE vehicle.year
                END,
                updated_at = NOW()",
            &[
                &vehicle_id,
                &vin,
                &metadata.make,
                &metadata.model,
                &metadata.engine_family,
                &metadata.year,
            ],
        )
        .await
        .map_err(|_| Error::Database)?;
    drop(client);
    enrich_vehicle_metadata(db, vehicle_id, vin).await?;

    let client = db.get().await?;
    let upload_id = Uuid::new_v4();
    let upload = client
        .query_one(
            "WITH inserted AS (
                INSERT INTO ingest_upload
                    (id, vehicle_id, content_hash, content_type, bytes)
                VALUES ($1, $2, $3, $4, $5)
                ON CONFLICT (vehicle_id, content_hash) DO NOTHING
                RETURNING id, TRUE AS inserted
             )
             SELECT id, inserted FROM inserted
             UNION ALL
             SELECT id, FALSE AS inserted
             FROM ingest_upload
             WHERE vehicle_id = $2 AND content_hash = $3
               AND NOT EXISTS (SELECT 1 FROM inserted)",
            &[
                &upload_id,
                &vehicle_id,
                &content_hash,
                &content_type,
                &bytes,
            ],
        )
        .await
        .map_err(|_| Error::Database)?;
    let upload_id: uuid::Uuid = upload.get(0);
    let inserted: bool = upload.get(1);
    super::privacy::link_upload_to_account(&client, account_id, upload_id, vehicle_id).await?;
    if !inserted {
        return Ok(CsvIngestResult {
            upload_id,
            rows_ingested: 0,
            duplicate: true,
            vehicle_id,
            content_hash,
        });
    }

    let n = match crate::ingest::ingest_reader(body, vin, upload_id, db).await {
        Ok(n) => n,
        Err(e) => {
            let _ = client
                .execute(
                    "DELETE FROM ingest_upload
                     WHERE id = $1",
                    &[&upload_id],
                )
                .await;
            eprintln!("ingest error: {e:?}");
            return Err(e);
        }
    };
    let rows_ingested = n as i64;
    client
        .execute(
            "UPDATE ingest_upload
             SET rows_ingested = $2, completed_at = NOW()
             WHERE id = $1",
            &[&upload_id, &rows_ingested],
        )
        .await
        .map_err(|_| Error::Database)?;

    Ok(CsvIngestResult {
        upload_id,
        rows_ingested,
        duplicate: false,
        vehicle_id,
        content_hash,
    })
}

pub(crate) fn packet_hash(body: &[u8]) -> String {
    let digest = Sha256::digest(body);
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        write!(&mut out, "{byte:02x}").expect("writing to string cannot fail");
    }
    out
}

async fn enrich_vehicle_metadata(db: &Database, vehicle_id: Uuid, vin: &str) -> Result<(), Error> {
    if !vin::is_valid_vin(vin) {
        return Ok(());
    }

    let client = db.get().await?;
    let current = load_vehicle_metadata(&client, vehicle_id).await?;
    if has_public_metadata(&current) {
        return Ok(());
    }
    if let Some(metadata) = cached_complete_metadata(&client, vin).await? {
        update_vehicle_metadata(&client, vehicle_id, &metadata).await?;
        return Ok(());
    }
    if let Some(metadata) = infer_vehicle_metadata(&client, vin, current.year).await? {
        update_vehicle_metadata(&client, vehicle_id, &metadata).await?;
        return Ok(());
    }
    let retry = cache_retry_state(&client, vin).await?;
    if retry.next_retry_after.is_some_and(|next| next > Utc::now()) {
        return Ok(());
    }
    if !acquire_vpic_throttle(&client).await? {
        return Ok(());
    }
    drop(client);

    match fetch_vpic_metadata(vin, current.year).await {
        Ok(metadata) => {
            let client = db.get().await?;
            record_vpic_result(&client, vin, &metadata).await?;
            if has_public_metadata(&metadata) {
                update_vehicle_metadata(&client, vehicle_id, &metadata).await?;
            }
        }
        Err(error) => {
            tracing::warn!("vPIC lookup failed for {vin}: {error}");
            let client = db.get().await?;
            record_vpic_error(&client, vin, retry.attempt_count + 1, &error).await?;
        }
    }
    Ok(())
}

#[derive(Default)]
struct RetryState {
    attempt_count: i32,
    next_retry_after: Option<DateTime<Utc>>,
}

fn has_public_metadata(metadata: &vin::VinMetadata) -> bool {
    !metadata.model.is_empty() && !metadata.engine_family.is_empty()
}

async fn load_vehicle_metadata(
    client: &tokio_postgres::Client,
    vehicle_id: Uuid,
) -> Result<vin::VinMetadata, Error> {
    let row = client
        .query_one(
            "SELECT year, make, model, engine_family
             FROM vehicle
             WHERE id = $1",
            &[&vehicle_id],
        )
        .await
        .map_err(|_| Error::Database)?;
    Ok(vin::VinMetadata {
        year: row.get(0),
        make: row.get(1),
        model: row.get(2),
        engine_family: row.get(3),
        ..Default::default()
    })
}

async fn update_vehicle_metadata(
    client: &tokio_postgres::Client,
    vehicle_id: Uuid,
    metadata: &vin::VinMetadata,
) -> Result<(), Error> {
    client
        .execute(
            "UPDATE vehicle
             SET year = CASE WHEN $2 > 0 THEN $2 ELSE year END,
                 make = CASE WHEN $3 <> '' THEN $3 ELSE make END,
                 model = CASE WHEN $4 <> '' THEN $4 ELSE model END,
                 engine_family = CASE WHEN $5 <> '' THEN $5 ELSE engine_family END,
                 updated_at = NOW()
             WHERE id = $1",
            &[
                &vehicle_id,
                &metadata.year,
                &metadata.make,
                &metadata.model,
                &metadata.engine_family,
            ],
        )
        .await
        .map_err(|_| Error::Database)?;
    Ok(())
}

async fn cached_complete_metadata(
    client: &tokio_postgres::Client,
    vin: &str,
) -> Result<Option<vin::VinMetadata>, Error> {
    let row = client
        .query_opt(
            "SELECT year, make, model, engine_family
             FROM vin_decode_cache
             WHERE vin = $1
               AND lookup_status = 'ok'
               AND model <> ''
               AND engine_family <> ''",
            &[&vin],
        )
        .await
        .map_err(|_| Error::Database)?;
    Ok(row.map(row_metadata))
}

async fn infer_vehicle_metadata(
    client: &tokio_postgres::Client,
    vin: &str,
    year: i32,
) -> Result<Option<vin::VinMetadata>, Error> {
    let Some((prefix, year)) = vin::pattern_key(vin, year) else {
        return Ok(None);
    };
    let rows = client
        .query(
            "SELECT make, model, engine_family
             FROM (
                SELECT make, model, engine_family
                FROM vehicle
                WHERE SUBSTRING(vin FROM 1 FOR 8) = $1
                  AND year = $2
                  AND model <> ''
                  AND engine_family <> ''
                UNION
                SELECT make, model, engine_family
                FROM vin_decode_cache
                WHERE SUBSTRING(vin FROM 1 FOR 8) = $1
                  AND year = $2
                  AND lookup_status = 'ok'
                  AND model <> ''
                  AND engine_family <> ''
             ) candidate
             GROUP BY make, model, engine_family",
            &[&prefix, &year],
        )
        .await
        .map_err(|_| Error::Database)?;
    if rows.len() != 1 {
        return Ok(None);
    }
    let row = &rows[0];
    Ok(Some(vin::VinMetadata {
        year,
        make: row.get(0),
        model: row.get(1),
        engine_family: row.get(2),
        ..Default::default()
    }))
}

async fn cache_retry_state(
    client: &tokio_postgres::Client,
    vin: &str,
) -> Result<RetryState, Error> {
    let row = client
        .query_opt(
            "SELECT attempt_count, next_retry_after
             FROM vin_decode_cache
             WHERE vin = $1",
            &[&vin],
        )
        .await
        .map_err(|_| Error::Database)?;
    Ok(row
        .map(|row| RetryState {
            attempt_count: row.get(0),
            next_retry_after: row.get(1),
        })
        .unwrap_or_default())
}

async fn acquire_vpic_throttle(client: &tokio_postgres::Client) -> Result<bool, Error> {
    let row = client
        .query_opt(
            "INSERT INTO external_lookup_throttle (lookup_key, last_request_at)
             VALUES ('vpic', NOW())
             ON CONFLICT (lookup_key) DO UPDATE SET
                last_request_at = EXCLUDED.last_request_at
             WHERE external_lookup_throttle.last_request_at <= NOW() - INTERVAL '1 minute'
             RETURNING last_request_at",
            &[],
        )
        .await
        .map_err(|_| Error::Database)?;
    Ok(row.is_some())
}

async fn fetch_vpic_metadata(vin: &str, year: i32) -> Result<vin::VinMetadata, String> {
    let mut url = Url::parse(VPIC_API_BASE).map_err(|error| error.to_string())?;
    url.path_segments_mut()
        .map_err(|_| "invalid vPIC URL".to_string())?
        .push(vin);
    url.query_pairs_mut().append_pair("format", "json");
    if year > 0 {
        url.query_pairs_mut()
            .append_pair("modelyear", &year.to_string());
    }
    let response = reqwest::Client::new()
        .get(url)
        .header("User-Agent", "scargo-vin-enrichment/1.0")
        .send()
        .await
        .map_err(|error| error.to_string())?;
    if response.status() != StatusCode::OK {
        return Err(format!("status {}", response.status().as_u16()));
    }
    let body = response.bytes().await.map_err(|error| error.to_string())?;
    let payload: Value = serde_json::from_slice(&body).map_err(|error| error.to_string())?;
    let row = payload
        .get("Results")
        .and_then(Value::as_array)
        .and_then(|results| results.first())
        .cloned()
        .unwrap_or(Value::Null);
    Ok(vin::map_vpic_result(vin, year, &row))
}

async fn record_vpic_result(
    client: &tokio_postgres::Client,
    vin: &str,
    metadata: &vin::VinMetadata,
) -> Result<(), Error> {
    let status = vin::lookup_status(metadata);
    let next_retry_after = next_retry_after(status, 1);
    client
        .execute(
            "INSERT INTO vin_decode_cache (
                vin, year, make, model, engine_family, powertrain, displacement_l,
                cylinders, engine_configuration, aspiration, body_class, trim,
                lookup_status, source, attempt_count, last_attempt_at,
                next_retry_after, latest_error, decoded_at
             )
             VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12,
                $13, 'vpic', 1, NOW(), $14, NULL, NOW()
             )
             ON CONFLICT (vin) DO UPDATE SET
                year = EXCLUDED.year,
                make = EXCLUDED.make,
                model = EXCLUDED.model,
                engine_family = EXCLUDED.engine_family,
                powertrain = EXCLUDED.powertrain,
                displacement_l = EXCLUDED.displacement_l,
                cylinders = EXCLUDED.cylinders,
                engine_configuration = EXCLUDED.engine_configuration,
                aspiration = EXCLUDED.aspiration,
                body_class = EXCLUDED.body_class,
                trim = EXCLUDED.trim,
                lookup_status = EXCLUDED.lookup_status,
                source = EXCLUDED.source,
                attempt_count = vin_decode_cache.attempt_count + 1,
                last_attempt_at = NOW(),
                next_retry_after = EXCLUDED.next_retry_after,
                latest_error = NULL,
                decoded_at = NOW(),
                updated_at = NOW()",
            &[
                &vin,
                &metadata.year,
                &metadata.make,
                &metadata.model,
                &metadata.engine_family,
                &metadata.powertrain,
                &metadata.displacement_l,
                &metadata.cylinders,
                &metadata.engine_configuration,
                &metadata.aspiration,
                &metadata.body_class,
                &metadata.trim,
                &status,
                &next_retry_after,
            ],
        )
        .await
        .map_err(|_| Error::Database)?;
    Ok(())
}

async fn record_vpic_error(
    client: &tokio_postgres::Client,
    vin: &str,
    attempt_count: i32,
    error: &str,
) -> Result<(), Error> {
    let next_retry_after = next_retry_after("error", attempt_count);
    client
        .execute(
            "INSERT INTO vin_decode_cache (
                vin, lookup_status, source, attempt_count, last_attempt_at,
                next_retry_after, latest_error
             )
             VALUES ($1, 'error', 'vpic', 1, NOW(), $2, $3)
             ON CONFLICT (vin) DO UPDATE SET
                lookup_status = 'error',
                source = 'vpic',
                attempt_count = vin_decode_cache.attempt_count + 1,
                last_attempt_at = NOW(),
                next_retry_after = $2,
                latest_error = $3,
                updated_at = NOW()",
            &[&vin, &next_retry_after, &error],
        )
        .await
        .map_err(|_| Error::Database)?;
    Ok(())
}

fn row_metadata(row: tokio_postgres::Row) -> vin::VinMetadata {
    vin::VinMetadata {
        year: row.get(0),
        make: row.get(1),
        model: row.get(2),
        engine_family: row.get(3),
        ..Default::default()
    }
}

fn next_retry_after(status: &str, attempt_count: i32) -> Option<DateTime<Utc>> {
    match status {
        "ok" => None,
        "incomplete" => Some(Utc::now() + Duration::days(7)),
        _ => {
            let capped_attempts = attempt_count.clamp(1, 5) - 1;
            Some(Utc::now() + Duration::hours(24 * 2_i64.pow(capped_attempts as u32)))
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::Duration;

    #[test]
    fn packet_hash_is_stable() {
        assert_eq!(super::packet_hash(b"same"), super::packet_hash(b"same"));
        assert_ne!(
            super::packet_hash(b"same"),
            super::packet_hash(b"different")
        );
    }

    #[test]
    fn retry_backoff_never_hammers_vpic() {
        let first = super::next_retry_after("error", 1).unwrap();
        assert!(first - chrono::Utc::now() >= Duration::hours(23));

        let incomplete = super::next_retry_after("incomplete", 1).unwrap();
        assert!(incomplete - chrono::Utc::now() >= Duration::days(6));

        assert!(super::next_retry_after("ok", 1).is_none());
    }
}
