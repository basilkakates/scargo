use crate::db::Database;
use crate::ingest::{model, vin};
use crate::Error;
use actix_web::{post, web, HttpRequest, HttpResponse};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

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
            "INSERT INTO vehicle (id, vin, make, model, year)
             VALUES ($1, $2, $3, $4, $5)
             ON CONFLICT (id) DO UPDATE SET
                make = CASE
                    WHEN EXCLUDED.make <> '' THEN EXCLUDED.make
                    ELSE vehicle.make
                END,
                model = CASE
                    WHEN EXCLUDED.model <> '' THEN EXCLUDED.model
                    ELSE vehicle.model
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
                &metadata.year,
            ],
        )
        .await
        .map_err(|_| Error::Database)?;

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

fn packet_hash(body: &[u8]) -> String {
    let digest = Sha256::digest(body);
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        write!(&mut out, "{byte:02x}").expect("writing to string cannot fail");
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn packet_hash_is_stable() {
        assert_eq!(super::packet_hash(b"same"), super::packet_hash(b"same"));
        assert_ne!(
            super::packet_hash(b"same"),
            super::packet_hash(b"different")
        );
    }
}
