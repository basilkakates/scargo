// ── GET /api/analysis/latest/{vehicle_id} ───────────────────
// Returns the 50 most recent readings for a vehicle across
// all channels.
// ────────────────────────────────────────────────────────────

use crate::db::Database;
use crate::Error;
use actix_web::{get, web, HttpRequest, HttpResponse};

#[get("/analysis/latest/{vehicle_id}")]
async fn latest(
    db: web::Data<Database>,
    req: HttpRequest,
    path: web::Path<String>,
) -> Result<HttpResponse, Error> {
    let vehicle_id = uuid::Uuid::parse_str(&path.into_inner())
        .map_err(|_| Error::BadRequest("invalid vehicle_id".into()))?;
    let client = db.get().await?;
    let account_id = super::privacy::account_id(&client, &req).await?;
    if !super::privacy::can_access_vehicle(&client, vehicle_id, account_id).await? {
        return Err(Error::NotFound("vehicle".into()));
    }

    let rows = client
        .query(
            "SELECT m.key, r.time, r.value, r.text_value
             FROM obd2_metric_reading r
             JOIN obd2_metric m ON m.id = r.metric_id
             JOIN account_vehicle_upload avu ON avu.upload_id = r.upload_id
             WHERE r.vehicle_id = $1::uuid
               AND avu.account_id = $2
               AND avu.private_access
             ORDER BY r.time DESC LIMIT 50",
            &[&vehicle_id, &account_id],
        )
        .await
        .map_err(|_| Error::Database)?;

    let out: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            serde_json::json!({
                "channel": r.get::<_, String>(0),
                "time": r.get::<_, chrono::DateTime<chrono::Utc>>(1).to_rfc3339(),
                "value": r.get::<_, Option<f64>>(2),
                "text_value": r.get::<_, Option<String>>(3),
            })
        })
        .collect();

    Ok(HttpResponse::Ok().json(out))
}
