// Batched dashboard data endpoint.
// Query params: ?view=summary&limit=200&channel_limit=20&vehicle_id=UUID&start=RFC3339&end=RFC3339&bucket=1d&channels=a,b

use crate::db::Database;
use crate::Error;
use actix_web::{get, web, HttpRequest, HttpResponse};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio_postgres::types::ToSql;
use uuid::Uuid;

const MAX_CHANNEL_LIMIT: i64 = 50;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DashboardView {
    Raw,
    Summary,
}

impl DashboardView {
    fn as_str(self) -> &'static str {
        match self {
            DashboardView::Raw => "raw",
            DashboardView::Summary => "summary",
        }
    }
}

#[derive(Debug, Deserialize)]
struct DashboardQuery {
    #[serde(default = "default_view")]
    view: String,
    #[serde(default = "super::query::default_limit")]
    limit: i64,
    channel_limit: Option<i64>,
    vehicle_id: Option<String>,
    start: Option<DateTime<Utc>>,
    end: Option<DateTime<Utc>>,
    #[serde(default = "super::query::default_bucket")]
    bucket: String,
    channels: Option<String>,
}

impl Default for DashboardQuery {
    fn default() -> Self {
        Self {
            view: default_view(),
            limit: super::query::default_limit(),
            channel_limit: None,
            vehicle_id: None,
            start: None,
            end: None,
            bucket: super::query::default_bucket(),
            channels: None,
        }
    }
}

#[derive(Debug)]
struct ValidatedDashboardQuery {
    view: DashboardView,
    limit: i64,
    channel_limit: Option<i64>,
    vehicle_id: Option<Uuid>,
    start: Option<DateTime<Utc>>,
    end: Option<DateTime<Utc>>,
    bucket: super::query::SummaryBucket,
    channels: Option<Vec<String>>,
}

#[derive(Debug, PartialEq, Serialize)]
struct DashboardResponse<T> {
    view: &'static str,
    series: Vec<DashboardSeries<T>>,
}

#[derive(Debug, PartialEq, Serialize)]
struct DashboardSeries<T> {
    key: String,
    label: String,
    points: Vec<T>,
}

#[derive(Debug, PartialEq, Serialize)]
struct RawPoint {
    time: String,
    value: Option<f64>,
    text_value: Option<String>,
}

#[derive(Debug, PartialEq, Serialize)]
struct SummaryPoint {
    bucket: String,
    avg: f64,
    min: f64,
    max: f64,
    count: i64,
}

#[derive(Debug)]
struct RawDashboardRow {
    key: String,
    label: String,
    time: DateTime<Utc>,
    value: Option<f64>,
    text_value: Option<String>,
}

#[derive(Debug)]
struct SummaryDashboardRow {
    key: String,
    label: String,
    bucket: DateTime<Utc>,
    avg: f64,
    min: f64,
    max: f64,
    count: i64,
}

struct DashboardFilters {
    selected: String,
    reading: String,
    param_index: usize,
}

fn default_view() -> String {
    "raw".into()
}

#[get("/analysis/dashboard")]
async fn dashboard(
    db: web::Data<Database>,
    req: HttpRequest,
    query: web::Query<DashboardQuery>,
) -> Result<HttpResponse, Error> {
    let query = validate_query(&query)?;
    let client = db.get().await?;
    let account_id = super::privacy::account_id(&client, &req).await?;

    match query.view {
        DashboardView::Raw => {
            let rows = query_raw_dashboard(&client, account_id, &query).await?;
            Ok(HttpResponse::Ok().json(DashboardResponse {
                view: query.view.as_str(),
                series: group_raw_rows(rows),
            }))
        }
        DashboardView::Summary => {
            let rows = query_summary_dashboard(&client, account_id, &query).await?;
            let rows = if rows.is_empty() {
                query_summary_dashboard_from_raw(&client, account_id, &query).await?
            } else {
                rows
            };
            Ok(HttpResponse::Ok().json(DashboardResponse {
                view: query.view.as_str(),
                series: group_summary_rows(rows),
            }))
        }
    }
}

fn validate_query(query: &DashboardQuery) -> Result<ValidatedDashboardQuery, Error> {
    let view = match query.view.as_str() {
        "raw" => DashboardView::Raw,
        "summary" => DashboardView::Summary,
        _ => return Err(Error::BadRequest("view must be raw or summary".into())),
    };

    super::query::check_time_range(query.start.as_ref(), query.end.as_ref())?;

    let vehicle_id = query
        .vehicle_id
        .as_deref()
        .map(super::query::parse_vehicle_id)
        .transpose()?;

    Ok(ValidatedDashboardQuery {
        view,
        limit: query.limit.clamp(1, super::query::MAX_LIMIT),
        channel_limit: query
            .channel_limit
            .map(|channel_limit| channel_limit.clamp(1, MAX_CHANNEL_LIMIT)),
        vehicle_id,
        start: query.start,
        end: query.end,
        bucket: super::query::parse_summary_bucket(&query.bucket)?,
        channels: parse_channels(query.channels.as_deref()),
    })
}

async fn query_raw_dashboard(
    client: &tokio_postgres::Client,
    account_id: Uuid,
    query: &ValidatedDashboardQuery,
) -> Result<Vec<RawDashboardRow>, Error> {
    let mut params: Vec<&(dyn ToSql + Sync)> = vec![&account_id];
    let mut filters = dashboard_filters(&mut params, 2, query, "r.vehicle_id", "r.time", false);
    let channel_limit_clause = push_channel_limit(
        &mut params,
        &mut filters.param_index,
        query.channel_limit.as_ref(),
    );
    let limit_param = filters.param_index;
    params.push(&query.limit);
    let selected_filters = filters.selected;
    let reading_filters = filters.reading;

    let sql = format!(
        "WITH selected_keys AS (
            SELECT m.key, MAX(m.label) AS label
            FROM obd2_metric m
            JOIN obd2_metric_reading r
              ON r.metric_id = m.id
            JOIN account_vehicle_upload avu ON avu.upload_id = r.upload_id
            WHERE avu.account_id = $1
              AND avu.private_access
              {selected_filters}
              {reading_filters}
            GROUP BY m.key
            ORDER BY m.key
            {channel_limit_clause}
         ),
         selected_metrics AS (
            SELECT m.id, m.key, sk.label
            FROM obd2_metric m
            JOIN selected_keys sk ON sk.key = m.key
         ),
         candidate_readings AS (
            SELECT sm.key,
                   sm.label,
                   r.time,
                   r.value,
                   r.text_value
            FROM selected_metrics sm
            JOIN LATERAL (
                SELECT r.time, r.value, r.text_value
                FROM obd2_metric_reading r
                JOIN account_vehicle_upload avu ON avu.upload_id = r.upload_id
                WHERE r.metric_id = sm.id
                  AND avu.account_id = $1
                  AND avu.private_access
                  {reading_filters}
                ORDER BY r.time DESC
                LIMIT ${limit_param}
            ) r ON TRUE
         ),
         ranked AS (
            SELECT *,
                   row_number() OVER (PARTITION BY key ORDER BY time DESC) AS rn
            FROM candidate_readings
         )
         SELECT key, label, time, value, text_value
         FROM ranked
         WHERE rn <= ${limit_param}
         ORDER BY key, time DESC"
    );

    let rows = client
        .query(&sql, &params)
        .await
        .map_err(|_| Error::Database)?;

    Ok(rows
        .iter()
        .map(|row| RawDashboardRow {
            key: row.get(0),
            label: row.get(1),
            time: row.get(2),
            value: row.get(3),
            text_value: row.get(4),
        })
        .collect())
}

async fn query_summary_dashboard(
    client: &tokio_postgres::Client,
    account_id: Uuid,
    query: &ValidatedDashboardQuery,
) -> Result<Vec<SummaryDashboardRow>, Error> {
    let mut params: Vec<&(dyn ToSql + Sync)> = vec![&account_id];
    let mut filters =
        dashboard_filters(&mut params, 2, query, "d.vehicle_id", "d.bucket_day", true);
    let channel_limit_clause = push_channel_limit(
        &mut params,
        &mut filters.param_index,
        query.channel_limit.as_ref(),
    );
    let limit_param = filters.param_index;
    params.push(&query.limit);
    let selected_filters = filters.selected;
    let reading_filters = filters.reading;

    let sql = format!(
        "WITH selected_keys AS (
            SELECT m.key, MAX(m.label) AS label
            FROM obd2_metric m
            JOIN vehicle_metric_day d
              ON d.metric_id = m.id
            JOIN account_vehicle_upload avu ON avu.upload_id = d.upload_id
            WHERE avu.account_id = $1
              AND avu.private_access
              {selected_filters}
              {reading_filters}
            GROUP BY m.key
            ORDER BY m.key
            {channel_limit_clause}
         ),
         selected_metrics AS (
            SELECT m.id, m.key, sk.label
            FROM obd2_metric m
            JOIN selected_keys sk ON sk.key = m.key
         ),
         bucketed AS (
            SELECT sm.key,
                   sm.label,
                   {bucket_expr} AS bucket,
                   (SUM(d.value_sum) / SUM(d.reading_count)::DOUBLE PRECISION)::DOUBLE PRECISION AS avg_val,
                   MIN(d.min_value) AS min_val,
                   MAX(d.max_value) AS max_val,
                   SUM(d.reading_count)::BIGINT AS cnt
            FROM selected_metrics sm
            JOIN vehicle_metric_day d
              ON d.metric_id = sm.id
            JOIN account_vehicle_upload avu ON avu.upload_id = d.upload_id
            WHERE d.reading_count > 0
              AND avu.account_id = $1
              AND avu.private_access
              {reading_filters}
            GROUP BY sm.key, sm.label, bucket
         ),
         ranked AS (
            SELECT *,
                   row_number() OVER (PARTITION BY key ORDER BY bucket DESC) AS rn
            FROM bucketed
         )
         SELECT key, label, bucket, avg_val, min_val, max_val, cnt
         FROM ranked
         WHERE rn <= ${limit_param}
         ORDER BY key, bucket DESC"
    ,
        bucket_expr = query.bucket.bucket_expr(),
    );

    let rows = client
        .query(&sql, &params)
        .await
        .map_err(|_| Error::Database)?;

    Ok(rows
        .iter()
        .map(|row| SummaryDashboardRow {
            key: row.get(0),
            label: row.get(1),
            bucket: row.get(2),
            avg: row.get(3),
            min: row.get(4),
            max: row.get(5),
            count: row.get(6),
        })
        .collect())
}

async fn query_summary_dashboard_from_raw(
    client: &tokio_postgres::Client,
    account_id: Uuid,
    query: &ValidatedDashboardQuery,
) -> Result<Vec<SummaryDashboardRow>, Error> {
    let mut params: Vec<&(dyn ToSql + Sync)> = vec![&account_id];
    let mut filters = dashboard_filters(&mut params, 2, query, "r.vehicle_id", "r.time", false);
    let channel_limit_clause = push_channel_limit(
        &mut params,
        &mut filters.param_index,
        query.channel_limit.as_ref(),
    );
    let limit_param = filters.param_index;
    params.push(&query.limit);
    let selected_filters = filters.selected;
    let reading_filters = filters.reading;

    let bucket_expr = match query.bucket {
        super::query::SummaryBucket::Day => "date_trunc('day', r.time)",
        super::query::SummaryBucket::Week => "date_trunc('week', r.time)",
        super::query::SummaryBucket::Month => "date_trunc('month', r.time)",
    };

    let sql = format!(
        "WITH selected_keys AS (
            SELECT m.key, MAX(m.label) AS label
            FROM obd2_metric m
            JOIN obd2_metric_reading r
              ON r.metric_id = m.id
            JOIN account_vehicle_upload avu ON avu.upload_id = r.upload_id
            WHERE avu.account_id = $1
              AND avu.private_access
              {selected_filters}
              AND r.value IS NOT NULL
              {reading_filters}
            GROUP BY m.key
            ORDER BY m.key
            {channel_limit_clause}
         ),
         selected_metrics AS (
            SELECT m.id, m.key, sk.label
            FROM obd2_metric m
            JOIN selected_keys sk ON sk.key = m.key
         ),
         bucketed AS (
            SELECT sm.key,
                   sm.label,
                   {bucket_expr} AS bucket,
                   AVG(r.value)::DOUBLE PRECISION AS avg_val,
                   MIN(r.value) AS min_val,
                   MAX(r.value) AS max_val,
                   COUNT(*)::BIGINT AS cnt
            FROM selected_metrics sm
            JOIN obd2_metric_reading r
              ON r.metric_id = sm.id
            JOIN account_vehicle_upload avu ON avu.upload_id = r.upload_id
            WHERE r.value IS NOT NULL
              AND avu.account_id = $1
              AND avu.private_access
              {reading_filters}
            GROUP BY sm.key, sm.label, bucket
         ),
         ranked AS (
            SELECT *,
                   row_number() OVER (PARTITION BY key ORDER BY bucket DESC) AS rn
            FROM bucketed
         )
         SELECT key, label, bucket, avg_val, min_val, max_val, cnt
         FROM ranked
         WHERE rn <= ${limit_param}
         ORDER BY key, bucket DESC",
        bucket_expr = bucket_expr,
    );

    let rows = client
        .query(&sql, &params)
        .await
        .map_err(|_| Error::Database)?;

    Ok(rows
        .iter()
        .map(|row| SummaryDashboardRow {
            key: row.get(0),
            label: row.get(1),
            bucket: row.get(2),
            avg: row.get(3),
            min: row.get(4),
            max: row.get(5),
            count: row.get(6),
        })
        .collect())
}

fn group_raw_rows(rows: Vec<RawDashboardRow>) -> Vec<DashboardSeries<RawPoint>> {
    let mut series: Vec<DashboardSeries<RawPoint>> = Vec::new();
    for row in rows {
        let point = RawPoint {
            time: row.time.to_rfc3339(),
            value: row.value,
            text_value: row.text_value,
        };
        if let Some(existing) = series.iter_mut().find(|item| item.key == row.key) {
            existing.points.push(point);
        } else {
            series.push(DashboardSeries {
                key: row.key,
                label: row.label,
                points: vec![point],
            });
        }
    }
    series
}

fn group_summary_rows(rows: Vec<SummaryDashboardRow>) -> Vec<DashboardSeries<SummaryPoint>> {
    let mut series: Vec<DashboardSeries<SummaryPoint>> = Vec::new();
    for row in rows {
        let point = SummaryPoint {
            bucket: row.bucket.to_rfc3339(),
            avg: row.avg,
            min: row.min,
            max: row.max,
            count: row.count,
        };
        if let Some(existing) = series.iter_mut().find(|item| item.key == row.key) {
            existing.points.push(point);
        } else {
            series.push(DashboardSeries {
                key: row.key,
                label: row.label,
                points: vec![point],
            });
        }
    }
    series
}

fn dashboard_filters<'a>(
    params: &mut Vec<&'a (dyn ToSql + Sync)>,
    mut param_index: usize,
    query: &'a ValidatedDashboardQuery,
    vehicle_column: &str,
    time_column: &str,
    day_bucketed: bool,
) -> DashboardFilters {
    let mut selected = String::new();
    let mut reading = String::new();

    if let Some(vehicle_id) = query.vehicle_id.as_ref() {
        reading.push_str(&format!(" AND {vehicle_column} = ${param_index}::uuid"));
        params.push(vehicle_id);
        param_index += 1;
    }
    if let Some(channels) = query.channels.as_ref() {
        selected.push_str(&format!(" AND m.key = ANY(${param_index}::text[])"));
        params.push(channels);
        param_index += 1;
    }
    if let Some(start) = query.start.as_ref() {
        if day_bucketed {
            reading.push_str(&format!(
                " AND {time_column} >= date_trunc('day', ${param_index}::timestamptz)"
            ));
        } else {
            reading.push_str(&format!(
                " AND {time_column} >= ${param_index}::timestamptz"
            ));
        }
        params.push(start);
        param_index += 1;
    }
    if let Some(end) = query.end.as_ref() {
        if day_bucketed {
            reading.push_str(&format!(
                " AND {time_column} <= date_trunc('day', ${param_index}::timestamptz)"
            ));
        } else {
            reading.push_str(&format!(
                " AND {time_column} <= ${param_index}::timestamptz"
            ));
        }
        params.push(end);
        param_index += 1;
    }

    DashboardFilters {
        selected,
        reading,
        param_index,
    }
}

fn push_channel_limit<'a>(
    params: &mut Vec<&'a (dyn ToSql + Sync)>,
    param_index: &mut usize,
    channel_limit: Option<&'a i64>,
) -> String {
    if let Some(channel_limit) = channel_limit {
        let channel_limit_param = *param_index;
        params.push(channel_limit);
        *param_index += 1;
        format!("LIMIT ${channel_limit_param}")
    } else {
        String::new()
    }
}

fn parse_channels(raw: Option<&str>) -> Option<Vec<String>> {
    let mut channels = Vec::new();
    for channel in raw.unwrap_or("").split(',').map(str::trim) {
        if !channel.is_empty() && !channels.iter().any(|item| item == channel) {
            channels.push(channel.to_owned());
        }
    }
    (!channels.is_empty()).then_some(channels)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn validate_query_rejects_invalid_uuid() {
        let query = DashboardQuery {
            vehicle_id: Some("not-a-uuid".into()),
            ..DashboardQuery::default()
        };

        assert!(matches!(
            validate_query(&query),
            Err(Error::BadRequest(message)) if message == "invalid vehicle_id"
        ));
    }

    #[test]
    fn validate_query_rejects_reversed_time_range() {
        let query = DashboardQuery {
            start: Some(Utc.with_ymd_and_hms(2026, 3, 28, 0, 0, 0).unwrap()),
            end: Some(Utc.with_ymd_and_hms(2026, 3, 27, 0, 0, 0).unwrap()),
            ..DashboardQuery::default()
        };

        assert!(matches!(
            validate_query(&query),
            Err(Error::BadRequest(message)) if message == "start must be before end"
        ));
    }

    #[test]
    fn parse_bucket_normalizes_known_units() {
        assert_eq!(
            crate::api::query::parse_summary_bucket("1d").unwrap(),
            crate::api::query::SummaryBucket::Day
        );
        assert_eq!(
            crate::api::query::parse_summary_bucket("1w").unwrap(),
            crate::api::query::SummaryBucket::Week
        );
        assert_eq!(
            crate::api::query::parse_summary_bucket("1mon").unwrap(),
            crate::api::query::SummaryBucket::Month
        );
        assert!(crate::api::query::parse_summary_bucket("1h").is_err());
    }

    #[test]
    fn validate_query_clamps_limit() {
        let low = validate_query(&DashboardQuery {
            limit: -100,
            ..DashboardQuery::default()
        })
        .unwrap();
        let high = validate_query(&DashboardQuery {
            limit: 50_000,
            ..DashboardQuery::default()
        })
        .unwrap();

        assert_eq!(low.limit, 1);
        assert_eq!(high.limit, crate::api::query::MAX_LIMIT);
    }

    #[test]
    fn validate_query_leaves_absent_channel_limit_uncapped() {
        let query = validate_query(&DashboardQuery::default()).unwrap();

        assert_eq!(query.channel_limit, None);
    }

    #[test]
    fn validate_query_clamps_explicit_channel_limit() {
        let low = validate_query(&DashboardQuery {
            channel_limit: Some(0),
            ..DashboardQuery::default()
        })
        .unwrap();
        let high = validate_query(&DashboardQuery {
            channel_limit: Some(500),
            ..DashboardQuery::default()
        })
        .unwrap();

        assert_eq!(low.channel_limit, Some(1));
        assert_eq!(high.channel_limit, Some(MAX_CHANNEL_LIMIT));
    }

    #[test]
    fn group_raw_rows_builds_series_shape() {
        let rows = vec![
            RawDashboardRow {
                key: "engine_rpm".into(),
                label: "Engine RPM (RPM)".into(),
                time: Utc.with_ymd_and_hms(2026, 3, 27, 23, 54, 6).unwrap(),
                value: Some(1200.0),
                text_value: None,
            },
            RawDashboardRow {
                key: "engine_rpm".into(),
                label: "Engine RPM (RPM)".into(),
                time: Utc.with_ymd_and_hms(2026, 3, 27, 23, 54, 5).unwrap(),
                value: Some(1074.0),
                text_value: None,
            },
            RawDashboardRow {
                key: "status".into(),
                label: "Status".into(),
                time: Utc.with_ymd_and_hms(2026, 3, 27, 23, 54, 5).unwrap(),
                value: None,
                text_value: Some("ok".into()),
            },
        ];

        let series = group_raw_rows(rows);

        assert_eq!(series.len(), 2);
        assert_eq!(series[0].key, "engine_rpm");
        assert_eq!(series[0].points.len(), 2);
        assert_eq!(series[0].points[0].value, Some(1200.0));
        assert_eq!(series[1].key, "status");
        assert_eq!(series[1].points[0].text_value.as_deref(), Some("ok"));
    }

    #[test]
    fn group_summary_rows_builds_series_shape() {
        let rows = vec![
            SummaryDashboardRow {
                key: "engine_rpm".into(),
                label: "Engine RPM (RPM)".into(),
                bucket: Utc.with_ymd_and_hms(2026, 3, 27, 23, 0, 0).unwrap(),
                avg: 1100.0,
                min: 1000.0,
                max: 1200.0,
                count: 3,
            },
            SummaryDashboardRow {
                key: "speed".into(),
                label: "Vehicle speed (MPH)".into(),
                bucket: Utc.with_ymd_and_hms(2026, 3, 27, 23, 0, 0).unwrap(),
                avg: 32.5,
                min: 10.0,
                max: 55.0,
                count: 4,
            },
        ];

        let series = group_summary_rows(rows);

        assert_eq!(series.len(), 2);
        assert_eq!(series[0].key, "engine_rpm");
        assert_eq!(series[0].points[0].bucket, "2026-03-27T23:00:00+00:00");
        assert_eq!(series[0].points[0].count, 3);
        assert_eq!(series[1].points[0].avg, 32.5);
    }
}
