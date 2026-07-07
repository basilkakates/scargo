use crate::db::Database;
use crate::Error;
use actix_web::{get, web, HttpRequest, HttpResponse};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio_postgres::types::ToSql;
use uuid::Uuid;

#[derive(Debug, Deserialize)]
struct PairQuery {
    x: String,
    y: String,
    #[serde(default = "super::query::default_limit")]
    limit: i64,
    vehicle_id: Option<String>,
    start: Option<DateTime<Utc>>,
    end: Option<DateTime<Utc>>,
}

#[derive(Debug)]
struct ValidatedPairQuery {
    x: String,
    y: String,
    limit: i64,
    vehicle_id: Option<Uuid>,
    start: Option<DateTime<Utc>>,
    end: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize)]
struct PairPoint {
    time: String,
    x: f64,
    y: f64,
}

#[get("/analysis/pairs")]
async fn pairs(
    db: web::Data<Database>,
    req: HttpRequest,
    query: web::Query<PairQuery>,
) -> Result<HttpResponse, Error> {
    let query = validate_query(&query)?;
    let client = db.get().await?;
    let account_id = super::privacy::account_id(&client, &req).await?;

    let mut params: Vec<&(dyn ToSql + Sync)> = vec![&query.x, &query.y, &account_id];
    let mut filters = String::new();
    let mut param_index = 4;

    if let Some(vehicle_id) = query.vehicle_id.as_ref() {
        filters.push_str(&format!(" AND xr.vehicle_id = ${param_index}::uuid"));
        params.push(vehicle_id);
        param_index += 1;
    }
    if let Some(start) = query.start.as_ref() {
        filters.push_str(&format!(" AND xr.time >= ${param_index}::timestamptz"));
        params.push(start);
        param_index += 1;
    }
    if let Some(end) = query.end.as_ref() {
        filters.push_str(&format!(" AND xr.time <= ${param_index}::timestamptz"));
        params.push(end);
        param_index += 1;
    }

    let sql = format!(
        "SELECT xr.time, xr.value AS x, yr.value AS y
         FROM obd2_metric xm
         JOIN obd2_metric ym
           ON ym.key = $2
         JOIN obd2_metric_reading xr
           ON xr.metric_id = xm.id
         JOIN obd2_metric_reading yr
           ON yr.vehicle_id = xr.vehicle_id
          AND yr.upload_id = xr.upload_id
          AND yr.metric_id = ym.id
          AND yr.time = xr.time
         JOIN account_vehicle_upload avu ON avu.upload_id = xr.upload_id
         WHERE xm.key = $1
           AND avu.account_id = $3
           AND avu.private_access
           AND xr.value IS NOT NULL
           AND yr.value IS NOT NULL
           {filters}
         ORDER BY xr.time DESC
         LIMIT ${param_index}"
    );
    params.push(&query.limit);

    let rows = client
        .query(&sql, &params)
        .await
        .map_err(|_| Error::Database)?;

    let points = rows
        .iter()
        .map(|row| PairPoint {
            time: row.get::<_, DateTime<Utc>>(0).to_rfc3339(),
            x: row.get(1),
            y: row.get(2),
        })
        .collect::<Vec<_>>();

    Ok(HttpResponse::Ok().json(points))
}

fn validate_query(query: &PairQuery) -> Result<ValidatedPairQuery, Error> {
    let x = query.x.trim();
    let y = query.y.trim();
    if x.is_empty() || y.is_empty() {
        return Err(Error::BadRequest("x and y are required".into()));
    }
    if x == y {
        return Err(Error::BadRequest("x and y must be different".into()));
    }
    super::query::check_time_range(query.start.as_ref(), query.end.as_ref())?;
    let vehicle_id = query
        .vehicle_id
        .as_deref()
        .map(super::query::parse_vehicle_id)
        .transpose()?;

    Ok(ValidatedPairQuery {
        x: x.to_owned(),
        y: y.to_owned(),
        limit: query.limit.clamp(1, super::query::MAX_LIMIT),
        vehicle_id,
        start: query.start,
        end: query.end,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn query(x: &str, y: &str) -> PairQuery {
        PairQuery {
            x: x.into(),
            y: y.into(),
            limit: 200,
            vehicle_id: None,
            start: None,
            end: None,
        }
    }

    #[test]
    fn validate_query_requires_two_metrics() {
        assert!(matches!(
            validate_query(&query("", "engine_rpm")),
            Err(Error::BadRequest(message)) if message == "x and y are required"
        ));
    }

    #[test]
    fn validate_query_rejects_same_metric() {
        assert!(matches!(
            validate_query(&query("engine_rpm", "engine_rpm")),
            Err(Error::BadRequest(message)) if message == "x and y must be different"
        ));
    }

    #[test]
    fn validate_query_rejects_reversed_time_range() {
        let mut query = query("engine_rpm", "vehicle_speed");
        query.start = Some(Utc.with_ymd_and_hms(2026, 3, 28, 0, 0, 0).unwrap());
        query.end = Some(Utc.with_ymd_and_hms(2026, 3, 27, 0, 0, 0).unwrap());

        assert!(matches!(
            validate_query(&query),
            Err(Error::BadRequest(message)) if message == "start must be before end"
        ));
    }
}
