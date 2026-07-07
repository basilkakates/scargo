// ── GET /api/analysis/trends/{channel} ──────────────────────
// Returns readings for a given OBD channel.
// Accepts query params ?limit=N, ?vehicle_id=UUID, ?start=RFC3339, ?end=RFC3339.
// ────────────────────────────────────────────────────────────

use crate::db::Database;
use crate::Error;
use actix_web::{get, web, HttpRequest, HttpResponse};
use chrono::{DateTime, Utc};
use tokio_postgres::types::ToSql;

#[derive(serde::Deserialize)]
struct TrendsQuery {
    #[serde(default = "super::query::default_limit")]
    limit: i64,
    vehicle_id: Option<String>,
    start: Option<DateTime<Utc>>,
    end: Option<DateTime<Utc>>,
}

#[get("/analysis/trends/{channel}")]
async fn trends(
    db: web::Data<Database>,
    req: HttpRequest,
    path: web::Path<String>,
    query: web::Query<TrendsQuery>,
) -> Result<HttpResponse, Error> {
    let channel = path.into_inner();
    let limit = query.limit.clamp(1, super::query::MAX_LIMIT);
    super::query::check_time_range(query.start.as_ref(), query.end.as_ref())?;
    let vehicle_id = query
        .vehicle_id
        .as_deref()
        .map(super::query::parse_vehicle_id)
        .transpose()?;
    let client = db.get().await?;
    let account_id = super::privacy::account_id(&client, &req).await?;

    let mut sql = String::from(
        "SELECT r.time, r.value, r.text_value
         FROM obd2_metric_reading r
         JOIN obd2_metric m ON m.id = r.metric_id
         JOIN account_vehicle_upload avu ON avu.upload_id = r.upload_id
         WHERE m.key = $1
           AND avu.account_id = $2
           AND avu.private_access",
    );
    let mut params: Vec<&(dyn ToSql + Sync)> = vec![&channel, &account_id];
    let mut param_index = 3;

    if let Some(vid) = vehicle_id.as_ref() {
        sql.push_str(&format!(" AND r.vehicle_id = ${param_index}::uuid"));
        params.push(vid);
        param_index += 1;
    }
    if let Some(start) = query.start.as_ref() {
        sql.push_str(&format!(" AND time >= ${param_index}::timestamptz"));
        params.push(start);
        param_index += 1;
    }
    if let Some(end) = query.end.as_ref() {
        sql.push_str(&format!(" AND time <= ${param_index}::timestamptz"));
        params.push(end);
        param_index += 1;
    }

    sql.push_str(&format!(" ORDER BY time DESC LIMIT ${param_index}"));
    params.push(&limit);

    let rows = client
        .query(&sql, &params)
        .await
        .map_err(|_| Error::Database)?;

    let out: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            serde_json::json!({
                "time": r.get::<_, chrono::DateTime<chrono::Utc>>(0).to_rfc3339(),
                "value": r.get::<_, Option<f64>>(1),
                "text_value": r.get::<_, Option<String>>(2),
            })
        })
        .collect();

    Ok(HttpResponse::Ok().json(out))
}
