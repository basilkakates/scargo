use crate::db::Database;
use crate::ingest::{canonical, model};
use crate::Error;
use actix_web::{delete, get, post, web, HttpRequest, HttpResponse};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
struct Query {
    #[serde(default = "default_limit")]
    limit: i64,
}

#[derive(Deserialize)]
struct ExactVinSharingRequest {
    enabled: bool,
}

#[derive(Serialize)]
struct VehicleListItem {
    id: String,
    handle: String,
    make: String,
    model: String,
    engine_family: String,
    year: i32,
    updated_at: Option<String>,
    reading_count: i64,
    upload_count: i64,
    exact_vin_share_enabled: bool,
    exact_vin_pending_approval_count: i64,
    cohort_pending_approval_count: i64,
    exact_vin_public_status: &'static str,
    cohort_public_status: &'static str,
}

#[derive(Serialize)]
struct ApprovalResponse {
    vehicle_id: uuid::Uuid,
    approval: &'static str,
    approved_upload_count: i64,
    already_approved_upload_count: i64,
}

fn default_limit() -> i64 {
    50
}

#[get("/vehicles")]
pub(crate) async fn list_vehicles(
    db: web::Data<Database>,
    req: HttpRequest,
    query: web::Query<Query>,
) -> Result<HttpResponse, Error> {
    let client = db.get().await?;
    let account = super::privacy::resolve_account(&client, &req).await?;

    let rows = client
        .query(
            "SELECT v.id,
                    v.make,
                    v.model,
                    v.engine_family,
                    v.year,
                    v.updated_at,
                    COUNT(DISTINCT avu.upload_id)::BIGINT AS upload_count,
                    COALESCE(SUM(iu.rows_ingested), 0)::BIGINT AS reading_count,
                    BOOL_OR(avu.exact_vin_share_enabled) AS exact_vin_share_enabled,
                    COUNT(DISTINCT avu.upload_id) FILTER (
                        WHERE avu.exact_vin_share_enabled
                          AND iu.approved_exact_vin_at IS NULL
                    )::BIGINT AS exact_vin_pending_approval_count,
                    COUNT(DISTINCT avu.upload_id) FILTER (
                        WHERE iu.approved_cohort_at IS NULL
                    )::BIGINT AS cohort_pending_approval_count,
                    BOOL_OR(avu.exact_vin_share_enabled AND iu.approved_exact_vin_at IS NOT NULL) AS exact_vin_public,
                    BOOL_OR(iu.approved_cohort_at IS NOT NULL) AS cohort_public
             FROM vehicle v
             JOIN account_vehicle_upload avu
               ON avu.vehicle_id = v.id
             JOIN ingest_upload iu
               ON iu.id = avu.upload_id
             WHERE avu.account_id = $2
               AND avu.private_access
             GROUP BY v.id, v.make, v.model, v.engine_family, v.year, v.updated_at
             ORDER BY v.updated_at DESC NULLS LAST
             LIMIT $1",
            &[&query.limit, &account.id],
        )
        .await
        .map_err(|_| Error::Database)?;

    let vehicles: Vec<VehicleListItem> = rows
        .iter()
        .map(|r| {
            let exact_enabled: bool = r.get(8);
            let exact_public: bool = r.get(11);
            let cohort_public: bool = r.get(12);
            VehicleListItem {
                id: r.get::<_, uuid::Uuid>(0).to_string(),
                handle: r.get::<_, uuid::Uuid>(0).to_string(),
                make: r.get(1),
                model: r.get(2),
                engine_family: r.get(3),
                year: r.get(4),
                updated_at: r
                    .get::<_, Option<chrono::DateTime<chrono::Utc>>>(5)
                    .map(|t| t.to_rfc3339()),
                upload_count: r.get(6),
                reading_count: r.get(7),
                exact_vin_share_enabled: exact_enabled,
                exact_vin_pending_approval_count: r.get(9),
                cohort_pending_approval_count: r.get(10),
                exact_vin_public_status: if !exact_enabled {
                    "private"
                } else if exact_public {
                    "public"
                } else {
                    "pending"
                },
                cohort_public_status: if cohort_public { "public" } else { "pending" },
            }
        })
        .collect();

    Ok(HttpResponse::Ok().json(vehicles))
}

#[post("/vehicles/{vehicle_id}/exact-vin-sharing")]
pub(crate) async fn set_exact_vin_sharing(
    db: web::Data<Database>,
    req: HttpRequest,
    path: web::Path<String>,
    body: web::Json<ExactVinSharingRequest>,
) -> Result<HttpResponse, Error> {
    let vehicle_id = super::query::parse_vehicle_id(&path.into_inner())?;
    let client = db.get().await?;
    let account = super::privacy::session_account(&client, &req).await?;
    if account.is_guest {
        return Err(Error::BadRequest(
            "guest accounts cannot manage vehicle sharing".into(),
        ));
    }
    if !super::privacy::can_access_vehicle(&client, vehicle_id, account.id).await? {
        return Err(Error::NotFound("vehicle".into()));
    }
    super::privacy::set_vehicle_exact_vin_sharing(&client, account.id, vehicle_id, body.enabled)
        .await?;
    Ok(HttpResponse::Ok().json(serde_json::json!({
        "vehicle_id": vehicle_id,
        "exact_vin_share_enabled": body.enabled,
    })))
}

#[post("/vehicles/{vehicle_id}/approve-exact-vin-sharing")]
pub(crate) async fn approve_exact_vin_sharing(
    db: web::Data<Database>,
    req: HttpRequest,
    path: web::Path<String>,
) -> Result<HttpResponse, Error> {
    approve_vehicle_sharing(db, req, path, "exact_vin").await
}

#[post("/vehicles/{vehicle_id}/approve-cohort-sharing")]
pub(crate) async fn approve_cohort_sharing(
    db: web::Data<Database>,
    req: HttpRequest,
    path: web::Path<String>,
) -> Result<HttpResponse, Error> {
    approve_vehicle_sharing(db, req, path, "cohort").await
}

#[delete("/vehicles/{vehicle_id}")]
pub(crate) async fn drop_vehicle(
    db: web::Data<Database>,
    req: HttpRequest,
    path: web::Path<String>,
) -> Result<HttpResponse, Error> {
    let vehicle_id = super::query::parse_vehicle_id(&path.into_inner())?;
    let client = db.get().await?;
    let account = super::privacy::session_account(&client, &req).await?;
    if account.is_guest {
        return Err(Error::BadRequest(
            "guest accounts cannot manage vehicles".into(),
        ));
    }
    let removed =
        super::privacy::revoke_vehicle_private_access(&client, account.id, vehicle_id).await?;
    if removed == 0 {
        return Err(Error::NotFound("vehicle".into()));
    }
    Ok(HttpResponse::Ok().json(serde_json::json!({
        "removed": true,
        "vehicle_id": vehicle_id,
    })))
}

async fn approve_vehicle_sharing(
    db: web::Data<Database>,
    req: HttpRequest,
    path: web::Path<String>,
    approval: &'static str,
) -> Result<HttpResponse, Error> {
    let vehicle_id = super::query::parse_vehicle_id(&path.into_inner())?;
    let client = db.get().await?;
    let account = super::privacy::session_account(&client, &req).await?;
    if account.is_guest {
        return Err(Error::BadRequest(
            "guest accounts cannot approve public sharing".into(),
        ));
    }
    if !super::privacy::manual_public_approval_enabled() {
        return Err(Error::BadRequest(
            "manual public sharing approval is only available in dev/test mode".into(),
        ));
    }
    if !super::privacy::can_access_vehicle(&client, vehicle_id, account.id).await? {
        return Err(Error::NotFound("vehicle".into()));
    }
    let response = match approval {
        "exact_vin" => approve_exact_vin_uploads(&client, account.id, vehicle_id).await?,
        "cohort" => approve_cohort_uploads(&client, account.id, vehicle_id).await?,
        _ => return Err(Error::Internal),
    };
    Ok(HttpResponse::Ok().json(response))
}

async fn approve_exact_vin_uploads(
    client: &tokio_postgres::Client,
    account_id: uuid::Uuid,
    vehicle_id: uuid::Uuid,
) -> Result<ApprovalResponse, Error> {
    let row = client
        .query_one(
            "WITH eligible AS (
                SELECT iu.id,
                       iu.approved_exact_vin_at IS NOT NULL AS already_approved
                FROM ingest_upload iu
                JOIN account_vehicle_upload avu
                  ON avu.upload_id = iu.id
                WHERE avu.account_id = $1
                  AND avu.vehicle_id = $2
                  AND avu.private_access
                  AND avu.exact_vin_share_enabled
            ),
            updated AS (
                UPDATE ingest_upload iu
                SET approved_exact_vin_at = NOW()
                FROM eligible
                WHERE iu.id = eligible.id
                  AND NOT eligible.already_approved
                RETURNING iu.id
            )
            SELECT
                (SELECT COUNT(*)::BIGINT FROM updated),
                (SELECT COUNT(*)::BIGINT FROM eligible WHERE already_approved)",
            &[&account_id, &vehicle_id],
        )
        .await
        .map_err(|_| Error::Database)?;
    Ok(ApprovalResponse {
        vehicle_id,
        approval: "exact_vin",
        approved_upload_count: row.get(0),
        already_approved_upload_count: row.get(1),
    })
}

async fn approve_cohort_uploads(
    client: &tokio_postgres::Client,
    account_id: uuid::Uuid,
    vehicle_id: uuid::Uuid,
) -> Result<ApprovalResponse, Error> {
    let row = client
        .query_one(
            "WITH eligible AS (
                SELECT iu.id,
                       iu.approved_cohort_at IS NOT NULL AS already_approved
                FROM ingest_upload iu
                JOIN account_vehicle_upload avu
                  ON avu.upload_id = iu.id
                WHERE avu.account_id = $1
                  AND avu.vehicle_id = $2
                  AND avu.private_access
            ),
            updated AS (
                UPDATE ingest_upload iu
                SET approved_cohort_at = NOW()
                FROM eligible
                WHERE iu.id = eligible.id
                  AND NOT eligible.already_approved
                RETURNING iu.id
            )
            SELECT
                (SELECT COUNT(*)::BIGINT FROM updated),
                (SELECT COUNT(*)::BIGINT FROM eligible WHERE already_approved)",
            &[&account_id, &vehicle_id],
        )
        .await
        .map_err(|_| Error::Database)?;
    Ok(ApprovalResponse {
        vehicle_id,
        approval: "cohort",
        approved_upload_count: row.get(0),
        already_approved_upload_count: row.get(1),
    })
}

#[get("/public/vehicle/{vin}")]
pub(crate) async fn public_vehicle(
    db: web::Data<Database>,
    path: web::Path<String>,
) -> Result<HttpResponse, Error> {
    let vin = path.into_inner();
    let vehicle_id = model::vin_to_uuid(&vin);
    let client = db.get().await?;
    let rows = client
        .query(
            "SELECT v.id,
                    v.make,
                    v.model,
                    v.engine_family,
                    v.year,
                    m.key,
                    MAX(m.label) AS label,
                    (SUM(d.value_sum) / SUM(d.reading_count)::DOUBLE PRECISION)::DOUBLE PRECISION AS avg_val,
                    MIN(d.min_value) AS min_val,
                    MAX(d.max_value) AS max_val,
                    SUM(d.reading_count)::BIGINT AS reading_count
             FROM vehicle v
             JOIN ingest_upload iu
               ON iu.vehicle_id = v.id
             JOIN vehicle_metric_day d
               ON d.upload_id = iu.id
             JOIN obd2_metric m
               ON m.id = d.metric_id
             WHERE v.id = $1
               AND iu.approved_exact_vin_at IS NOT NULL
               AND EXISTS (
                    SELECT 1
                    FROM account_vehicle_upload avu
                    WHERE avu.upload_id = iu.id
                      AND avu.exact_vin_share_enabled
               )
             GROUP BY v.id, v.make, v.model, v.engine_family, v.year, m.key
             ORDER BY m.key",
            &[&vehicle_id],
        )
        .await
        .map_err(|_| Error::Database)?;

    if rows.is_empty() {
        return Err(Error::NotFound("public vehicle stats".into()));
    }

    let mut metrics = Vec::new();
    let mut vehicle_meta = None;
    for row in rows {
        if vehicle_meta.is_none() {
            vehicle_meta = Some((
                row.get::<_, uuid::Uuid>(0).to_string(),
                row.get::<_, String>(1),
                row.get::<_, String>(2),
                row.get::<_, String>(3),
                row.get::<_, i32>(4),
            ));
        }
        let key: String = row.get(5);
        if !canonical::metric_policy(&key).public_cohort {
            continue;
        }
        metrics.push(serde_json::json!({
            "key": key,
            "label": row.get::<_, String>(6),
            "avg": row.get::<_, f64>(7),
            "min": row.get::<_, f64>(8),
            "max": row.get::<_, f64>(9),
            "count": row.get::<_, i64>(10),
        }));
    }

    if metrics.is_empty() {
        return Err(Error::NotFound("public vehicle stats".into()));
    }
    let (vehicle_id, make, model, engine_family, year) =
        vehicle_meta.ok_or(Error::NotFound("public vehicle stats".into()))?;

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "vehicle_id": vehicle_id,
        "vin": vin,
        "make": make,
        "model": model,
        "engine_family": engine_family,
        "year": year,
        "metrics": metrics,
    })))
}
