// ── GET /api/analysis/summary/{channel} ─────────────────────
// Returns daily/weekly/monthly aggregates (avg, min, max, count) for
// a channel from vehicle_metric_day.
// Query params: ?vehicle_id=UUID&bucket=1d|1w|1mon&limit=100&start=RFC3339&end=RFC3339
// ────────────────────────────────────────────────────────────

use crate::db::Database;
use crate::Error;
use actix_web::{get, web, HttpRequest, HttpResponse};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use tokio_postgres::types::ToSql;

#[derive(Deserialize)]
struct SummaryQuery {
    #[serde(default = "super::query::default_bucket")]
    bucket: String, // e.g. "1d", "1w", "1mon"
    #[serde(default = "super::query::default_limit")]
    limit: i64,
    vehicle_id: Option<String>,
    start: Option<DateTime<Utc>>,
    end: Option<DateTime<Utc>>,
}

#[get("/analysis/summary/{channel}")]
async fn summary(
    db: web::Data<Database>,
    req: HttpRequest,
    path: web::Path<String>,
    query: web::Query<SummaryQuery>,
) -> Result<HttpResponse, Error> {
    let channel = path.into_inner();
    let limit = query.limit.clamp(1, super::query::MAX_LIMIT);
    super::query::check_time_range(query.start.as_ref(), query.end.as_ref())?;
    let bucket = super::query::parse_summary_bucket(&query.bucket)?;
    let vehicle_id = query
        .vehicle_id
        .as_deref()
        .map(super::query::parse_vehicle_id)
        .transpose()?;
    let client = db.get().await?;
    let account_id = super::privacy::account_id(&client, &req).await?;

    let mut sql = format!(
        "SELECT {} AS bucket,
                (SUM(d.value_sum) / SUM(d.reading_count)::DOUBLE PRECISION)::DOUBLE PRECISION AS avg_val,
                MIN(d.min_value) AS min_val,
                MAX(d.max_value) AS max_val,
                SUM(d.reading_count)::BIGINT AS cnt
         FROM vehicle_metric_day d
         JOIN obd2_metric m ON m.id = d.metric_id
         JOIN account_vehicle_upload avu ON avu.upload_id = d.upload_id
         WHERE m.key = $1
           AND avu.account_id = $2
           AND avu.private_access
           AND d.reading_count > 0",
        bucket.bucket_expr(),
    );
    let mut params: Vec<&(dyn ToSql + Sync)> = vec![&channel, &account_id];
    let mut param_index = 3;

    if let Some(vid) = vehicle_id.as_ref() {
        sql.push_str(&format!(" AND d.vehicle_id = ${param_index}::uuid"));
        params.push(vid);
        param_index += 1;
    }
    if let Some(start) = query.start.as_ref() {
        sql.push_str(&format!(
            " AND d.bucket_day >= date_trunc('day', ${param_index}::timestamptz)"
        ));
        params.push(start);
        param_index += 1;
    }
    if let Some(end) = query.end.as_ref() {
        sql.push_str(&format!(
            " AND d.bucket_day <= date_trunc('day', ${param_index}::timestamptz)"
        ));
        params.push(end);
        param_index += 1;
    }
    sql.push_str(" GROUP BY bucket ORDER BY bucket DESC");
    sql.push_str(&format!(" LIMIT ${param_index}"));
    params.push(&limit);

    let rows = client
        .query(&sql, &params)
        .await
        .map_err(|_| Error::Database)?;
    let rows = if rows.is_empty() {
        query_summary_from_raw(
            &client,
            RawSummaryQuery {
                channel: &channel,
                account_id,
                vehicle_id,
                start: query.start,
                end: query.end,
                bucket,
                limit,
            },
        )
        .await?
    } else {
        rows
    };

    let out: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            serde_json::json!({
                "bucket": r.get::<_, chrono::DateTime<chrono::Utc>>(0).to_rfc3339(),
                "avg": r.get::<_, f64>(1),
                "min": r.get::<_, f64>(2),
                "max": r.get::<_, f64>(3),
                "count": r.get::<_, i64>(4),
            })
        })
        .collect();

    Ok(HttpResponse::Ok().json(out))
}

struct RawSummaryQuery<'a> {
    channel: &'a str,
    account_id: uuid::Uuid,
    vehicle_id: Option<uuid::Uuid>,
    start: Option<DateTime<Utc>>,
    end: Option<DateTime<Utc>>,
    bucket: super::query::SummaryBucket,
    limit: i64,
}

async fn query_summary_from_raw(
    client: &tokio_postgres::Client,
    query: RawSummaryQuery<'_>,
) -> Result<Vec<tokio_postgres::Row>, Error> {
    let bucket_expr = match query.bucket {
        super::query::SummaryBucket::Day => "date_trunc('day', r.time)",
        super::query::SummaryBucket::Week => "date_trunc('week', r.time)",
        super::query::SummaryBucket::Month => "date_trunc('month', r.time)",
    };

    let mut sql = format!(
        "SELECT {} AS bucket,
                AVG(r.value)::DOUBLE PRECISION AS avg_val,
                MIN(r.value) AS min_val,
                MAX(r.value) AS max_val,
                COUNT(*)::BIGINT AS cnt
         FROM obd2_metric_reading r
         JOIN obd2_metric m ON m.id = r.metric_id
         JOIN account_vehicle_upload avu ON avu.upload_id = r.upload_id
         WHERE m.key = $1
           AND avu.account_id = $2
           AND avu.private_access
           AND r.value IS NOT NULL",
        bucket_expr,
    );
    let mut params: Vec<&(dyn ToSql + Sync)> = vec![&query.channel, &query.account_id];
    let mut param_index = 3;

    if let Some(vid) = query.vehicle_id.as_ref() {
        sql.push_str(&format!(" AND r.vehicle_id = ${param_index}::uuid"));
        params.push(vid);
        param_index += 1;
    }
    if let Some(start) = query.start.as_ref() {
        sql.push_str(&format!(" AND r.time >= ${param_index}::timestamptz"));
        params.push(start);
        param_index += 1;
    }
    if let Some(end) = query.end.as_ref() {
        sql.push_str(&format!(" AND r.time <= ${param_index}::timestamptz"));
        params.push(end);
        param_index += 1;
    }
    sql.push_str(" GROUP BY bucket ORDER BY bucket DESC");
    sql.push_str(&format!(" LIMIT ${param_index}"));
    params.push(&query.limit);

    client
        .query(&sql, &params)
        .await
        .map_err(|_| Error::Database)
}
