use crate::db::Database;
use crate::ingest::canonical;
use crate::Error;
use actix_web::{get, web, HttpRequest, HttpResponse};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio_postgres::types::ToSql;
use uuid::Uuid;

#[derive(Debug, PartialEq, Serialize)]
pub struct ChannelInfo {
    pub key: String,
    pub label: String,
    pub unit: Option<String>,
    pub unit_family: Option<String>,
    pub canonical_unit: Option<String>,
    pub display_units: Vec<String>,
    pub default_display_unit: Option<String>,
    pub category: String,
    pub sensitivity: String,
    pub rollup: bool,
    pub public_cohort: bool,
    pub derived_preferred: bool,
    pub has_numeric: bool,
    pub has_text: bool,
    pub reading_count: i64,
}

#[derive(Debug, Default, Deserialize)]
struct ChannelQuery {
    vehicle_id: Option<String>,
    start: Option<DateTime<Utc>>,
    end: Option<DateTime<Utc>>,
}

#[derive(Debug)]
struct ValidatedChannelQuery {
    vehicle_id: Option<Uuid>,
    start: Option<DateTime<Utc>>,
    end: Option<DateTime<Utc>>,
}

#[derive(Debug)]
struct ChannelStats {
    key: String,
    label: String,
    unit: Option<String>,
    has_numeric: bool,
    has_text: bool,
    reading_count: i64,
}

impl From<ChannelStats> for ChannelInfo {
    fn from(stats: ChannelStats) -> Self {
        let metadata = canonical::channel_unit_metadata(&stats.key);
        let policy = canonical::metric_policy(&stats.key);
        Self {
            key: stats.key,
            label: stats.label,
            unit: stats.unit,
            unit_family: metadata.map(|value| value.unit_family.to_string()),
            canonical_unit: metadata.map(|value| value.canonical_unit.to_string()),
            display_units: metadata
                .map(|value| {
                    value
                        .display_units
                        .iter()
                        .map(|unit| unit.to_string())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default(),
            default_display_unit: metadata.map(|value| value.default_display_unit.to_string()),
            category: policy.category.to_string(),
            sensitivity: policy.sensitivity.to_string(),
            rollup: policy.rollup,
            public_cohort: policy.public_cohort,
            derived_preferred: policy.derived_preferred,
            has_numeric: stats.has_numeric,
            has_text: stats.has_text,
            reading_count: stats.reading_count,
        }
    }
}

#[get("/channels")]
async fn list_channels(
    db: web::Data<Database>,
    req: HttpRequest,
    query: web::Query<ChannelQuery>,
) -> Result<HttpResponse, Error> {
    let query = validate_query(&query)?;
    let client = db.get().await?;
    let account_id = super::privacy::account_id(&client, &req).await?;

    let mut params: Vec<&(dyn ToSql + Sync)> = vec![&account_id];
    let mut filters = String::new();
    let mut param_index = 2;

    if let Some(vehicle_id) = query.vehicle_id.as_ref() {
        filters.push_str(&format!(" AND r.vehicle_id = ${param_index}::uuid"));
        params.push(vehicle_id);
        param_index += 1;
    }
    if let Some(start) = query.start.as_ref() {
        filters.push_str(&format!(" AND r.time >= ${param_index}::timestamptz"));
        params.push(start);
        param_index += 1;
    }
    if let Some(end) = query.end.as_ref() {
        filters.push_str(&format!(" AND r.time <= ${param_index}::timestamptz"));
        params.push(end);
    }

    let sql = format!(
        "SELECT m.key,
                MAX(m.label) AS label,
                MAX(m.unit) FILTER (WHERE m.unit IS NOT NULL) AS unit,
                BOOL_OR(r.value IS NOT NULL) AS has_numeric,
                BOOL_OR(r.text_value IS NOT NULL) AS has_text,
                COUNT(*)::BIGINT AS reading_count
         FROM obd2_metric m
         JOIN obd2_metric_reading r
           ON r.metric_id = m.id
         JOIN account_vehicle_upload avu ON avu.upload_id = r.upload_id
         WHERE avu.account_id = $1
           AND avu.private_access
           {filters}
         GROUP BY m.key
         ORDER BY m.key"
    );

    let rows = client
        .query(&sql, &params)
        .await
        .map_err(|_| Error::Database)?;

    let channels = rows
        .iter()
        .map(|row| {
            ChannelInfo::from(ChannelStats {
                key: row.get(0),
                label: row.get(1),
                unit: row.get(2),
                has_numeric: row.get(3),
                has_text: row.get(4),
                reading_count: row.get(5),
            })
        })
        .collect::<Vec<_>>();

    Ok(HttpResponse::Ok().json(channels))
}

fn validate_query(query: &ChannelQuery) -> Result<ValidatedChannelQuery, Error> {
    super::query::check_time_range(query.start.as_ref(), query.end.as_ref())?;

    let vehicle_id = query
        .vehicle_id
        .as_deref()
        .map(super::query::parse_vehicle_id)
        .transpose()?;

    Ok(ValidatedChannelQuery {
        vehicle_id,
        start: query.start,
        end: query.end,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use serde_json::json;

    #[test]
    fn validate_query_rejects_invalid_uuid() {
        let query = ChannelQuery {
            vehicle_id: Some("not-a-uuid".into()),
            ..ChannelQuery::default()
        };

        assert!(matches!(
            validate_query(&query),
            Err(Error::BadRequest(message)) if message == "invalid vehicle_id"
        ));
    }

    #[test]
    fn validate_query_rejects_reversed_time_range() {
        let query = ChannelQuery {
            start: Some(Utc.with_ymd_and_hms(2026, 3, 28, 0, 0, 0).unwrap()),
            end: Some(Utc.with_ymd_and_hms(2026, 3, 27, 0, 0, 0).unwrap()),
            ..ChannelQuery::default()
        };

        assert!(matches!(
            validate_query(&query),
            Err(Error::BadRequest(message)) if message == "start must be before end"
        ));
    }

    #[test]
    fn channel_metadata_serializes_numeric_only_shape() {
        let channel = ChannelInfo::from(ChannelStats {
            key: "engine_rpm".into(),
            label: "Engine RPM (RPM)".into(),
            unit: Some("rpm".into()),
            has_numeric: true,
            has_text: false,
            reading_count: 12,
        });

        assert_eq!(
            serde_json::to_value(channel).unwrap(),
            json!({
                "key": "engine_rpm",
                "label": "Engine RPM (RPM)",
                "unit": "rpm",
                "unit_family": null,
                "canonical_unit": null,
                "display_units": [],
                "default_display_unit": null,
                "category": "sae_pid",
                "sensitivity": "public_vehicle",
                "rollup": true,
                "public_cohort": true,
                "derived_preferred": false,
                "has_numeric": true,
                "has_text": false,
                "reading_count": 12
            })
        );
    }

    #[test]
    fn channel_metadata_serializes_text_only_shape() {
        let channel = ChannelInfo::from(ChannelStats {
            key: "fuel_status".into(),
            label: "Fuel system 1 status".into(),
            unit: None,
            has_numeric: false,
            has_text: true,
            reading_count: 3,
        });

        assert_eq!(
            serde_json::to_value(channel).unwrap(),
            json!({
                "key": "fuel_status",
                "label": "Fuel system 1 status",
                "unit": null,
                "unit_family": null,
                "canonical_unit": null,
                "display_units": [],
                "default_display_unit": null,
                "category": "unknown",
                "sensitivity": "owner_only",
                "rollup": false,
                "public_cohort": false,
                "derived_preferred": false,
                "has_numeric": false,
                "has_text": true,
                "reading_count": 3
            })
        );
    }

    #[test]
    fn channel_metadata_serializes_unit_options() {
        let channel = ChannelInfo::from(ChannelStats {
            key: "vehicle_speed".into(),
            label: "Vehicle speed".into(),
            unit: Some("mph".into()),
            has_numeric: true,
            has_text: false,
            reading_count: 8,
        });

        assert_eq!(
            serde_json::to_value(channel).unwrap(),
            json!({
                "key": "vehicle_speed",
                "label": "Vehicle speed",
                "unit": "mph",
                "unit_family": "speed",
                "canonical_unit": "mph",
                "display_units": ["mph", "km/h"],
                "default_display_unit": "mph",
                "category": "sae_pid",
                "sensitivity": "public_vehicle",
                "rollup": true,
                "public_cohort": true,
                "derived_preferred": false,
                "has_numeric": true,
                "has_text": false,
                "reading_count": 8
            })
        );
    }
}
