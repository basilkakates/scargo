// Returns aggregate-only comparison data for similar vehicles.
// Query params: ?year=YYYY&make=...&model=...&engine_family=...&bucket=1d&limit=100&min_vehicles=5

use crate::db::Database;
use crate::ingest::canonical;
use crate::Error;
use actix_web::{get, web, HttpResponse};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use tokio_postgres::types::ToSql;

#[derive(Deserialize)]
struct CohortQuery {
    year: i32,
    make: String,
    model: String,
    engine_family: String,
    #[serde(default = "super::query::default_bucket")]
    bucket: String,
    #[serde(default = "default_limit")]
    limit: i64,
    #[serde(default = "default_min_vehicles")]
    min_vehicles: i64,
    start: Option<DateTime<Utc>>,
    end: Option<DateTime<Utc>>,
}

struct ValidatedCohortQuery {
    year: i32,
    make: String,
    model: String,
    engine_family: String,
    bucket: super::query::SummaryBucket,
    limit: i64,
    min_vehicles: i64,
    start: Option<DateTime<Utc>>,
    end: Option<DateTime<Utc>>,
}

fn default_limit() -> i64 {
    100
}

fn default_min_vehicles() -> i64 {
    5
}

#[get("/analysis/cohort/{channel}")]
async fn cohort(
    db: web::Data<Database>,
    path: web::Path<String>,
    query: web::Query<CohortQuery>,
) -> Result<HttpResponse, Error> {
    let channel = path.into_inner();
    validate_channel(&channel)?;
    let query = validate_query(&query)?;
    let client = db.get().await?;

    let mut sql = format!(
        "SELECT {} AS bucket,
                (SUM(d.value_sum) / SUM(d.reading_count)::DOUBLE PRECISION)::DOUBLE PRECISION AS avg_val,
                MIN(d.min_value) AS min_val,
                MAX(d.max_value) AS max_val,
                SUM(d.reading_count)::BIGINT AS reading_count,
                COUNT(DISTINCT d.vehicle_id)::BIGINT AS vehicle_count
         FROM vehicle_metric_day d
         JOIN obd2_metric m ON m.id = d.metric_id
         JOIN vehicle v ON v.id = d.vehicle_id
         JOIN ingest_upload iu ON iu.id = d.upload_id
         WHERE m.key = $1
           AND d.reading_count > 0
           AND iu.approved_cohort_at IS NOT NULL
           AND v.year = $2
           AND lower(v.make) = lower($3)
           AND lower(v.model) = lower($4)
           AND lower(v.engine_family) = lower($5)",
        query.bucket.bucket_expr(),
    );

    let mut params: Vec<&(dyn ToSql + Sync)> = vec![
        &channel,
        &query.year,
        &query.make,
        &query.model,
        &query.engine_family,
    ];
    let mut param_index = 6;

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

    sql.push_str(&format!(
        " GROUP BY bucket
          HAVING COUNT(DISTINCT d.vehicle_id) >= ${param_index}
          ORDER BY bucket DESC"
    ));
    params.push(&query.min_vehicles);
    param_index += 1;
    sql.push_str(&format!(" LIMIT ${param_index}"));
    params.push(&query.limit);

    let rows = client
        .query(&sql, &params)
        .await
        .map_err(|_| Error::Database)?;

    let out: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            serde_json::json!({
                "bucket": r.get::<_, chrono::DateTime<chrono::Utc>>(0).to_rfc3339(),
                "avg": r.get::<_, f64>(1),
                "min": r.get::<_, f64>(2),
                "max": r.get::<_, f64>(3),
                "reading_count": r.get::<_, i64>(4),
                "vehicle_count": r.get::<_, i64>(5),
            })
        })
        .collect();

    Ok(HttpResponse::Ok().json(out))
}

fn validate_query(query: &CohortQuery) -> Result<ValidatedCohortQuery, Error> {
    let year = query.year;
    let make = query.make.trim();
    let model = query.model.trim();
    let engine_family = query.engine_family.trim();
    if year <= 0 || make.is_empty() || model.is_empty() || engine_family.is_empty() {
        return Err(Error::BadRequest(
            "year, make, model, and engine_family are required".into(),
        ));
    }
    super::query::check_time_range(query.start.as_ref(), query.end.as_ref())?;

    Ok(ValidatedCohortQuery {
        year,
        make: make.into(),
        model: model.into(),
        engine_family: engine_family.into(),
        bucket: super::query::parse_summary_bucket(&query.bucket)?,
        limit: query.limit.clamp(1, 1_000),
        min_vehicles: query.min_vehicles.max(5),
        start: query.start,
        end: query.end,
    })
}

fn validate_channel(channel: &str) -> Result<(), Error> {
    if canonical::metric_policy(channel).public_cohort {
        return Ok(());
    }

    Err(Error::BadRequest(
        "channel is not available for public cohorts".into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cohort_requires_engine_family() {
        assert!(matches!(
            validate_query(&CohortQuery {
                year: 2011,
                make: "Honda".into(),
                model: "Accord".into(),
                engine_family: String::new(),
                bucket: "1d".into(),
                limit: 100,
                min_vehicles: 5,
                start: None,
                end: None,
            }),
            Err(Error::BadRequest(message))
                if message == "year, make, model, and engine_family are required"
        ));
    }

    #[test]
    fn cohort_accepts_week_bucket() {
        let query = validate_query(&CohortQuery {
            year: 2011,
            make: "Honda".into(),
            model: "Accord".into(),
            engine_family: "2.4L I4 NA".into(),
            bucket: "1w".into(),
            limit: 100,
            min_vehicles: 5,
            start: None,
            end: None,
        })
        .unwrap();

        assert_eq!(query.bucket, super::super::query::SummaryBucket::Week);
    }

    #[test]
    fn cohort_accepts_public_channel() {
        assert!(validate_channel("vehicle_speed").is_ok());
    }

    #[test]
    fn cohort_rejects_private_channels() {
        assert!(matches!(
            validate_channel("latitude"),
            Err(Error::BadRequest(message))
                if message == "channel is not available for public cohorts"
        ));
        assert!(validate_channel("accel_x").is_err());
    }
}
