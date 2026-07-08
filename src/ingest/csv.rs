use std::collections::HashMap;
use std::fmt::Write as _;
use std::io::Read;

use chrono::{DateTime, Duration, TimeZone, Utc};
use csv::StringRecord;
use tokio_postgres::types::ToSql;

use crate::db::Database;
use crate::ingest::canonical::{self, NumericTransform};
use crate::ingest::model::{self, RawMetricReading, ValueKind};
use crate::ingest::vin;
use crate::Error;

#[derive(Debug, PartialEq)]
struct RawMetricColumn {
    key: String,
    label: String,
    col: usize,
    unit: Option<String>,
    transform: Option<NumericTransform>,
}

#[derive(Clone, Copy)]
enum TimeSource {
    Elapsed {
        col: usize,
        start_time: DateTime<Utc>,
    },
    Timestamp {
        col: usize,
    },
}

struct ParsedHeader {
    time_source: TimeSource,
    raw_columns: Vec<RawMetricColumn>,
}

struct ParsedReadings {
    raw_readings: Vec<RawMetricReading>,
    skipped_rows: usize,
}

const INSERT_BATCH_SIZE: usize = 1_000;

#[derive(Clone, Copy)]
struct IngestOptions {
    advisory_lock: bool,
    update_daily_rollups: bool,
    insert_batch_size: usize,
}

const HTTP_INGEST_OPTIONS: IngestOptions = IngestOptions {
    advisory_lock: true,
    update_daily_rollups: true,
    insert_batch_size: INSERT_BATCH_SIZE,
};

#[derive(Clone, Copy)]
struct MetricCacheEntry {
    id: i64,
    value_kind: ValueKind,
}

#[derive(Default)]
struct MetricCache {
    ids: HashMap<String, MetricCacheEntry>,
}

pub async fn ingest_reader<R: Read>(
    reader: R,
    vin: &str,
    upload_id: uuid::Uuid,
    db: &Database,
) -> Result<usize, Error> {
    ingest_reader_with_options(
        reader,
        vin,
        upload_id,
        db,
        &mut MetricCache::default(),
        HTTP_INGEST_OPTIONS,
    )
    .await
}

async fn ingest_reader_with_options<R: Read>(
    reader: R,
    vin: &str,
    upload_id: uuid::Uuid,
    db: &Database,
    cache: &mut MetricCache,
    options: IngestOptions,
) -> Result<usize, Error> {
    let vehicle_id = model::vin_to_uuid(vin);
    let parsed = parse_readings(reader, vin, upload_id)?;
    validate_upload_value_kinds(&parsed.raw_readings)?;
    let metadata = vin::decode(vin);

    let mut client = db.get().await?;
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

    let txn = client.transaction().await.map_err(|_| Error::Database)?;
    if options.advisory_lock {
        let vehicle_key = vehicle_id.to_string();
        txn.execute(
            "SELECT pg_advisory_xact_lock(hashtextextended($1, 0))",
            &[&vehicle_key],
        )
        .await
        .map_err(|_| Error::Database)?;
    }
    // ponytail: raw metrics are the only write path.
    let mut count = 0usize;
    for chunk in parsed.raw_readings.chunks(options.insert_batch_size) {
        count += insert_raw_metric_batch(&txn, chunk, cache, options).await?;
    }

    txn.commit().await.map_err(|_| Error::Database)?;
    client
        .execute(
            "UPDATE vehicle SET updated_at = NOW() WHERE id = $1",
            &[&vehicle_id],
        )
        .await
        .map_err(|_| Error::Database)?;
    tracing::info!(
        "Ingested {count} new readings for VIN {vin} (skipped {} rows)",
        parsed.skipped_rows
    );
    Ok(count)
}

async fn insert_raw_metric_batch(
    txn: &tokio_postgres::Transaction<'_>,
    readings: &[RawMetricReading],
    cache: &mut MetricCache,
    options: IngestOptions,
) -> Result<usize, Error> {
    if readings.is_empty() {
        return Ok(0);
    }

    let mut sql = String::from(
        "INSERT INTO obd2_metric_reading
         (upload_id, vehicle_id, metric_id, time, value, text_value) VALUES ",
    );
    let metric_ids = metric_ids(txn, readings, cache).await?;
    let mut params: Vec<&(dyn ToSql + Sync)> = Vec::with_capacity(readings.len() * 6);

    for (i, reading) in readings.iter().enumerate() {
        if i > 0 {
            sql.push_str(", ");
        }
        let base = i * 6;
        write!(
            sql,
            "(${}, ${}, ${}, ${}, ${}, ${})",
            base + 1,
            base + 2,
            base + 3,
            base + 4,
            base + 5,
            base + 6
        )
        .map_err(|_| Error::Internal)?;

        params.push(&reading.upload_id);
        params.push(&reading.vehicle_id);
        params.push(&metric_ids[i]);
        params.push(&reading.time);
        params.push(&reading.value);
        params.push(&reading.text_value);
    }

    let inserted = txn.execute(&sql, &params).await.map_err(|e| {
        tracing::warn!("Raw metric batch insert failed: {e:?}");
        Error::Database
    })?;
    if options.update_daily_rollups {
        upsert_daily_rollups(txn, readings, &metric_ids).await?;
    }
    Ok(inserted as usize)
}

#[derive(Debug, Clone, Copy)]
struct DayRollup {
    value_sum: f64,
    min_value: f64,
    max_value: f64,
    reading_count: i64,
}

async fn upsert_daily_rollups(
    txn: &tokio_postgres::Transaction<'_>,
    readings: &[RawMetricReading],
    metric_ids: &[i64],
) -> Result<(), Error> {
    let mut rollups = HashMap::<(uuid::Uuid, uuid::Uuid, i64, DateTime<Utc>), DayRollup>::new();

    for (reading, metric_id) in readings.iter().zip(metric_ids.iter()) {
        if !should_roll_up_metric(&reading.key) {
            continue;
        }
        let Some(value) = reading.value else {
            continue;
        };
        let bucket_day = Utc.from_utc_datetime(
            &reading
                .time
                .date_naive()
                .and_hms_opt(0, 0, 0)
                .ok_or(Error::Internal)?,
        );
        rollups
            .entry((
                reading.upload_id,
                reading.vehicle_id,
                *metric_id,
                bucket_day,
            ))
            .and_modify(|existing| {
                existing.value_sum += value;
                existing.min_value = existing.min_value.min(value);
                existing.max_value = existing.max_value.max(value);
                existing.reading_count += 1;
            })
            .or_insert(DayRollup {
                value_sum: value,
                min_value: value,
                max_value: value,
                reading_count: 1,
            });
    }

    if rollups.is_empty() {
        return Ok(());
    }

    let mut sql = String::from(
        "INSERT INTO vehicle_metric_day
         (bucket_day, upload_id, vehicle_id, metric_id, value_sum, min_value, max_value, reading_count)
         VALUES ",
    );
    let mut params: Vec<&(dyn ToSql + Sync)> = Vec::with_capacity(rollups.len() * 8);

    for (i, ((upload_id, vehicle_id, metric_id, bucket_day), rollup)) in rollups.iter().enumerate()
    {
        if i > 0 {
            sql.push_str(", ");
        }
        let base = i * 8;
        write!(
            sql,
            "(${}, ${}, ${}, ${}, ${}, ${}, ${}, ${})",
            base + 1,
            base + 2,
            base + 3,
            base + 4,
            base + 5,
            base + 6,
            base + 7,
            base + 8
        )
        .map_err(|_| Error::Internal)?;

        params.push(bucket_day);
        params.push(upload_id);
        params.push(vehicle_id);
        params.push(metric_id);
        params.push(&rollup.value_sum);
        params.push(&rollup.min_value);
        params.push(&rollup.max_value);
        params.push(&rollup.reading_count);
    }

    sql.push_str(
        " ON CONFLICT (bucket_day, upload_id, vehicle_id, metric_id) DO UPDATE SET
            value_sum = vehicle_metric_day.value_sum + EXCLUDED.value_sum,
            min_value = LEAST(vehicle_metric_day.min_value, EXCLUDED.min_value),
            max_value = GREATEST(vehicle_metric_day.max_value, EXCLUDED.max_value),
            reading_count = vehicle_metric_day.reading_count + EXCLUDED.reading_count",
    );

    txn.execute(&sql, &params)
        .await
        .map_err(|_| Error::Database)?;
    Ok(())
}

fn should_roll_up_metric(key: &str) -> bool {
    canonical::metric_policy(key).rollup
}

async fn metric_ids(
    txn: &tokio_postgres::Transaction<'_>,
    readings: &[RawMetricReading],
    cache: &mut MetricCache,
) -> Result<Vec<i64>, Error> {
    let mut ids = HashMap::<String, i64>::new();

    for reading in readings {
        if ids.contains_key(&reading.key) {
            continue;
        }

        let value_kind = reading.value_kind();
        if let Some(entry) = cache.ids.get(&reading.key) {
            ensure_value_kind_pair(&reading.key, entry.value_kind, value_kind)?;
            ids.insert(reading.key.clone(), entry.id);
            continue;
        }
        let row = txn
            .query_one(
                "WITH upsert AS (
                    INSERT INTO obd2_metric (key, label, unit, value_kind)
                    VALUES ($1, $2, $3, $4)
                    ON CONFLICT (key) DO UPDATE SET
                    label = EXCLUDED.label,
                    unit = EXCLUDED.unit
                    WHERE obd2_metric.value_kind = EXCLUDED.value_kind
                    RETURNING id, value_kind
                 )
                 SELECT id, value_kind FROM upsert
                 UNION ALL
                 SELECT id, value_kind
                 FROM obd2_metric
                 WHERE key = $1
                   AND NOT EXISTS (SELECT 1 FROM upsert)",
                &[
                    &reading.key,
                    &reading.label,
                    &reading.unit,
                    &value_kind.as_str(),
                ],
            )
            .await
            .map_err(|_| Error::Database)?;
        ensure_value_kind_matches(&reading.key, value_kind, row.get::<_, String>(1).as_str())?;
        let id = row.get(0);
        ids.insert(reading.key.clone(), id);
        cache
            .ids
            .insert(reading.key.clone(), MetricCacheEntry { id, value_kind });
    }

    readings
        .iter()
        .map(|reading| ids.get(&reading.key).copied().ok_or(Error::Database))
        .collect()
}

fn validate_upload_value_kinds(readings: &[RawMetricReading]) -> Result<(), Error> {
    let mut seen = HashMap::<&str, ValueKind>::new();

    for reading in readings {
        if let Some(existing) = seen.insert(&reading.key, reading.value_kind()) {
            ensure_value_kind_pair(&reading.key, existing, reading.value_kind())?;
        }
    }

    Ok(())
}

fn ensure_value_kind_matches(key: &str, incoming: ValueKind, existing: &str) -> Result<(), Error> {
    let existing = match existing {
        "numeric" => ValueKind::Numeric,
        "text" => ValueKind::Text,
        _ => return Err(Error::Database),
    };
    ensure_value_kind_pair(key, existing, incoming)
}

fn ensure_value_kind_pair(
    key: &str,
    existing: ValueKind,
    incoming: ValueKind,
) -> Result<(), Error> {
    if existing == incoming {
        return Ok(());
    }

    Err(Error::BadRequest(format!(
        "metric key '{key}' conflicts: existing {} vs incoming {}",
        existing.as_str(),
        incoming.as_str()
    )))
}

fn parse_readings<R: Read>(
    reader: R,
    vin: &str,
    upload_id: uuid::Uuid,
) -> Result<ParsedReadings, Error> {
    let vehicle_id = model::vin_to_uuid(vin);
    let mut csv_reader = csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .trim(csv::Trim::All)
        .from_reader(reader);

    let mut start_time = None;
    let mut parsed_header = None;
    let mut raw_readings = Vec::new();
    let mut skipped_rows = 0usize;

    for record in csv_reader.records() {
        let record = record.map_err(|_| Error::CsvParse)?;
        if empty_record(&record) {
            continue;
        }

        if parsed_header.is_none() {
            if let Some(parsed_start_time) = start_time_from_record(&record)? {
                start_time = Some(parsed_start_time);
                continue;
            }

            if let Some(header) = parse_header(&record, start_time)? {
                parsed_header = Some(header);
            }
            continue;
        }

        let header = parsed_header.as_ref().expect("header parsed above");
        let Some(time) = record_time(&record, header.time_source) else {
            skipped_rows += 1;
            continue;
        };

        let raw_before = raw_readings.len();
        for mapping in &header.raw_columns {
            let Some(raw) = record
                .get(mapping.col)
                .map(str::trim)
                .filter(|v| !v.is_empty())
            else {
                continue;
            };
            let value = parse_number(raw).map(|value| {
                mapping
                    .transform
                    .map(|transform| transform.apply(value))
                    .unwrap_or(value)
            });
            raw_readings.push(RawMetricReading {
                upload_id,
                vehicle_id,
                time,
                key: mapping.key.clone(),
                label: mapping.label.clone(),
                unit: mapping.unit.clone(),
                value,
                text_value: value.is_none().then(|| raw.to_string()),
            });
        }
        if raw_readings.len() == raw_before {
            skipped_rows += 1;
        }
    }

    if parsed_header.is_none() {
        return Err(Error::BadRequest(
            "missing CSV header with timestamp and data columns".into(),
        ));
    }

    Ok(ParsedReadings {
        raw_readings,
        skipped_rows,
    })
}

fn parse_header(
    headers: &StringRecord,
    start_time: Option<DateTime<Utc>>,
) -> Result<Option<ParsedHeader>, Error> {
    let time_source = match time_source(headers, start_time)? {
        Some(source) => source,
        None => return Ok(None),
    };
    let raw_columns = raw_metric_columns(headers, time_source);

    if raw_columns.is_empty() {
        return Ok(None);
    }

    Ok(Some(ParsedHeader {
        time_source,
        raw_columns,
    }))
}

fn start_time_from_record(record: &StringRecord) -> Result<Option<DateTime<Utc>>, Error> {
    if record.len() > 2 {
        return Ok(None);
    }

    let line = record.iter().collect::<Vec<_>>().join(",");
    let normalized = normalize_header(&line);
    if !normalized.starts_with("starttime") && !normalized.starts_with("start time") {
        return Ok(None);
    }

    model::parse_start_time(&line)
        .map(Some)
        .map_err(|_| Error::BadRequest(format!("invalid StartTime row: {line}")))
}

fn time_source(
    headers: &StringRecord,
    start_time: Option<DateTime<Utc>>,
) -> Result<Option<TimeSource>, Error> {
    if let Some(col) = elapsed_time_column(headers) {
        let start_time = start_time.ok_or_else(|| {
            Error::BadRequest("elapsed time CSV is missing a # StartTime row".into())
        })?;
        return Ok(Some(TimeSource::Elapsed { col, start_time }));
    }

    if let Some(col) = timestamp_column(headers) {
        return Ok(Some(TimeSource::Timestamp { col }));
    }

    Ok(None)
}

fn elapsed_time_column(headers: &StringRecord) -> Option<usize> {
    headers.iter().position(|header| {
        let info = HeaderInfo::parse(header);
        matches!(
            info.name.as_str(),
            "time" | "elapsed time" | "elapsed seconds" | "seconds" | "time sec" | "time seconds"
        ) && info
            .unit
            .as_deref()
            .map(|unit| {
                matches!(
                    unit,
                    "s" | "sec" | "second" | "seconds" | "ms" | "millisecond" | "milliseconds"
                )
            })
            .unwrap_or(true)
    })
}

fn timestamp_column(headers: &StringRecord) -> Option<usize> {
    headers.iter().position(|header| {
        matches!(
            HeaderInfo::parse(header).name.as_str(),
            "ts" | "timestamp" | "date time" | "datetime" | "date"
        )
    })
}

fn record_time(record: &StringRecord, source: TimeSource) -> Option<DateTime<Utc>> {
    match source {
        TimeSource::Elapsed { col, start_time } => {
            let seconds = parse_f64(record, col)?;
            Some(start_time + Duration::milliseconds((seconds * 1000.0).round() as i64))
        }
        TimeSource::Timestamp { col } => model::parse_timestamp(record.get(col)?.trim()).ok(),
    }
}

fn raw_metric_columns(headers: &StringRecord, time_source: TimeSource) -> Vec<RawMetricColumn> {
    let time_col = match time_source {
        TimeSource::Elapsed { col, .. } | TimeSource::Timestamp { col } => col,
    };
    let mut counts = std::collections::HashMap::<String, usize>::new();

    headers
        .iter()
        .enumerate()
        .filter_map(|(col, header)| {
            if col == time_col {
                return None;
            }

            let raw = header.trim_start_matches('\u{feff}').trim();
            if raw.is_empty() {
                return None;
            }

            let (label, unit) = split_name_unit(raw);
            let base_key = metric_key(&label);
            if base_key.is_empty() {
                return None;
            }
            let normalized_unit = unit.as_ref().map(|value| normalize_unit(value));
            let canonical =
                canonical::canonical_metric(&label, &base_key, normalized_unit.as_deref());
            let count = counts.entry(canonical.key.clone()).or_insert(0);
            *count += 1;
            let key = if *count == 1 {
                canonical.key
            } else {
                format!("{}_{}", canonical.key, *count)
            };

            Some(RawMetricColumn {
                key,
                label: canonical.label,
                col,
                unit: canonical.storage_unit,
                transform: canonical.transform,
            })
        })
        .collect()
}

fn parse_f64(record: &StringRecord, col: usize) -> Option<f64> {
    parse_number(record.get(col)?)
}

fn parse_number(value: &str) -> Option<f64> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("nan") {
        return None;
    }

    if let Ok(value) = trimmed.parse() {
        return Some(value);
    }

    let mut candidate = String::new();
    for ch in trimmed.chars() {
        if ch.is_ascii_digit() || matches!(ch, '-' | '+' | '.' | 'e' | 'E') {
            candidate.push(ch);
        } else if ch == ',' {
            continue;
        } else if !candidate.is_empty() {
            break;
        }
    }

    if candidate.is_empty() {
        return None;
    }
    candidate.parse().ok()
}

fn empty_record(record: &StringRecord) -> bool {
    record.iter().all(|field| field.trim().is_empty())
}

#[derive(Debug)]
struct HeaderInfo {
    name: String,
    unit: Option<String>,
}

impl HeaderInfo {
    fn parse(value: &str) -> Self {
        let raw = value.trim_start_matches('\u{feff}').trim();
        let (name, unit) = split_name_unit(raw);
        Self {
            name: normalize_header(&name),
            unit: unit.map(|u| normalize_unit(&u)),
        }
    }
}

fn split_name_unit(value: &str) -> (String, Option<String>) {
    let trimmed = value.trim();
    let Some(close) = trimmed.rfind(')') else {
        return (trimmed.to_string(), None);
    };
    if close != trimmed.len() - 1 {
        return (trimmed.to_string(), None);
    }
    let Some(open) = trimmed[..close].rfind('(') else {
        return (trimmed.to_string(), None);
    };

    let candidate = trimmed[open + 1..close].trim();
    if !looks_like_unit(candidate) {
        return (trimmed.to_string(), None);
    }

    (
        trimmed[..open].trim().to_string(),
        Some(candidate.to_string()),
    )
}

fn looks_like_unit(value: &str) -> bool {
    let unit = normalize_unit(value);
    matches!(
        unit.as_str(),
        "%" | "c"
            | "f"
            | "k"
            | "km"
            | "m"
            | "ft"
            | "mile"
            | "miles"
            | "knots"
            | "rpm"
            | "mph"
            | "kph"
            | "kmh"
            | "km h"
            | "km hr"
            | "m s"
            | "ft s"
            | "m s 2"
            | "m s2"
            | "ft s 2"
            | "ft s2"
            | "sec"
            | "s"
            | "ms"
            | "min"
            | "hz"
            | "v"
            | "ma"
            | "pa"
            | "kpa"
            | "mpa"
            | "bar"
            | "mbar"
            | "psi"
            | "psig"
            | "inhg"
            | "in hg"
            | "inh2o"
            | "in h2o"
            | "g s"
            | "g"
            | "ut"
            | "mpg"
            | "km l"
            | "kg h"
            | "kg hr"
            | "kg"
            | "lbs"
            | "lb"
            | "lb min"
            | "lb mile"
            | "lb hr"
            | "g km"
            | "gal hr"
            | "gal h"
            | "gal"
            | "l h"
            | "l hr"
            | "l"
            | "hp"
            | "kw"
            | "lb ft"
            | "n m"
            | "deg"
            | "deg s"
            | "degree"
            | "degrees"
    )
}

fn normalize_unit(value: &str) -> String {
    let ascii = value
        .replace("°", "")
        .replace("µ", "u")
        .replace("μ", "u")
        .replace(" per ", "/")
        .replace("hour", "hr");

    let mut normalized = String::new();
    let mut last_was_space = true;

    for ch in ascii.chars() {
        if ch.is_ascii_alphanumeric() || ch == '%' {
            normalized.push(ch.to_ascii_lowercase());
            last_was_space = false;
        } else if matches!(ch, '/' | '-') {
            if !last_was_space {
                normalized.push(' ');
                last_was_space = true;
            }
        } else if !last_was_space {
            normalized.push(' ');
            last_was_space = true;
        }
    }

    normalized.trim().to_string()
}

fn normalize_header(value: &str) -> String {
    let ascii = value
        .replace("°", "")
        .replace("µ", "u")
        .replace("μ", "u")
        .replace('#', " ");
    let mut normalized = String::new();
    let mut last_was_space = true;

    for ch in ascii.chars() {
        if ch.is_ascii_alphanumeric() {
            normalized.push(ch.to_ascii_lowercase());
            last_was_space = false;
        } else if !last_was_space {
            normalized.push(' ');
            last_was_space = true;
        }
    }

    normalized.trim().to_string()
}

fn metric_key(value: &str) -> String {
    normalize_header(value).replace(' ', "_")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    const VIN: &str = "DEMO-HONDA-ACCORD";

    fn upload_id() -> uuid::Uuid {
        uuid::Uuid::nil()
    }

    fn approx_eq(left: f64, right: f64) -> bool {
        (left - right).abs() < 1e-6
    }

    #[test]
    fn parses_obd_export_by_header_name() {
        let csv = "\u{feff}# StartTime = 03/27/2026 06:54:01.3973 PM\n\
            Time (sec),Engine RPM (RPM),Vehicle speed (MPH),Intake manifold absolute pressure (kPa),Mass air flow rate (g/s),Intake manifold absolute pressure (kPa),Boost (kPa)\n\
            4.406,1074,2.4854848,48,10.66,52,12.5\n";

        let parsed = parse_readings(csv.as_bytes(), VIN, upload_id()).unwrap();

        assert_eq!(parsed.skipped_rows, 0);
        assert!(parsed
            .raw_readings
            .iter()
            .any(|r| r.key == "mass_air_flow_rate" && r.value == Some(10.66)));
        assert!(parsed
            .raw_readings
            .iter()
            .any(|r| r.key == "intake_manifold_absolute_pressure" && r.value == Some(48.0)));
        assert!(parsed
            .raw_readings
            .iter()
            .any(|r| r.key == "intake_manifold_absolute_pressure_2" && r.value == Some(52.0)));
        assert!(parsed
            .raw_readings
            .iter()
            .any(|r| r.key == "boost" && r.value == Some(12.5)));
    }

    #[test]
    fn duplicate_headers_get_stable_suffixes() {
        let headers = StringRecord::from(vec![
            "Time (sec)",
            "Intake manifold absolute pressure (kPa)",
            "Intake manifold absolute pressure (kPa)",
        ]);

        let columns = raw_metric_columns(
            &headers,
            TimeSource::Elapsed {
                col: 0,
                start_time: model::parse_start_time("# StartTime = 03/27/2026 06:54:01 PM")
                    .unwrap(),
            },
        );

        assert_eq!(
            columns
                .iter()
                .map(|column| (column.key.as_str(), column.col))
                .collect::<Vec<_>>(),
            vec![
                ("intake_manifold_absolute_pressure", 1),
                ("intake_manifold_absolute_pressure_2", 2)
            ]
        );
    }

    #[test]
    fn accepts_reordered_headers_and_canonicalizes_units() {
        let csv = "# StartTime = 03/27/2026 06:54:01.3973 PM\n\
            Engine coolant temperature (F),elapsed seconds,Vehicle speed (km/h),MAP (psi)\n\
            212,1.5,100,10\n";

        let parsed = parse_readings(csv.as_bytes(), VIN, upload_id()).unwrap();

        assert_eq!(parsed.raw_readings.len(), 3);
        assert!(parsed
            .raw_readings
            .iter()
            .any(|r| r.key == "engine_coolant_temperature"
                && r.value
                    .map(|value| approx_eq(value, 100.0))
                    .unwrap_or(false)
                && r.unit.as_deref() == Some("c")));
        assert!(parsed.raw_readings.iter().any(|r| r.key == "vehicle_speed"
            && r.value
                .map(|value| approx_eq(value, 62.1371192))
                .unwrap_or(false)
            && r.unit.as_deref() == Some("mph")));
        assert!(parsed
            .raw_readings
            .iter()
            .any(|r| r.key == "intake_manifold_absolute_pressure"
                && r.value
                    .map(|value| approx_eq(value, 68.947572932))
                    .unwrap_or(false)
                && r.unit.as_deref() == Some("kpa")));
    }

    #[test]
    fn accepts_absolute_timestamp_csv() {
        let csv = "timestamp,rpm,speed\n\
            2026-03-27T23:54:01Z,1074,2.4\n";

        let parsed = parse_readings(csv.as_bytes(), VIN, upload_id()).unwrap();

        assert_eq!(parsed.raw_readings.len(), 2);
        assert!(parsed
            .raw_readings
            .iter()
            .any(|r| r.key == "engine_rpm" && r.unit.as_deref() == Some("rpm")));
        assert!(parsed
            .raw_readings
            .iter()
            .any(|r| r.key == "vehicle_speed" && r.unit.as_deref() == Some("mph")));
        assert_eq!(
            parsed.raw_readings[0].time.format("%Y-%m-%d").to_string(),
            "2026-03-27"
        );
    }

    #[test]
    fn converts_supported_unit_variants_into_common_storage_units() {
        let csv = "# StartTime = 03/27/2026 06:54:01.3973 PM\n\
            Time (sec),Vehicle speed (MPH),Vehicle speed (km/h),Fuel Remaining (l),Fuel Remaining (gal),Total CO2 (lbs),Total CO2 (kg)\n\
            1,62.1371192,100,3.785411784,1,2.2046226218,1\n";

        let parsed = parse_readings(csv.as_bytes(), VIN, upload_id()).unwrap();

        assert!(parsed.raw_readings.iter().any(|r| {
            r.key == "vehicle_speed"
                && r.value
                    .map(|value| approx_eq(value, 62.1371192))
                    .unwrap_or(false)
                && r.unit.as_deref() == Some("mph")
        }));
        assert!(parsed.raw_readings.iter().any(|r| {
            r.key == "vehicle_speed_2"
                && r.value
                    .map(|value| approx_eq(value, 62.1371192))
                    .unwrap_or(false)
                && r.unit.as_deref() == Some("mph")
        }));
        assert!(parsed.raw_readings.iter().any(|r| {
            r.key == "fuel_remaining"
                && r.value.map(|value| approx_eq(value, 1.0)).unwrap_or(false)
                && r.unit.as_deref() == Some("gal")
        }));
        assert!(parsed.raw_readings.iter().any(|r| {
            r.key == "fuel_remaining_2"
                && r.value.map(|value| approx_eq(value, 1.0)).unwrap_or(false)
                && r.unit.as_deref() == Some("gal")
        }));
        assert!(parsed.raw_readings.iter().any(|r| {
            r.key == "total_co2"
                && r.value.map(|value| approx_eq(value, 1.0)).unwrap_or(false)
                && r.unit.as_deref() == Some("kg")
        }));
        assert!(parsed.raw_readings.iter().any(|r| {
            r.key == "total_co2_2"
                && r.value.map(|value| approx_eq(value, 1.0)).unwrap_or(false)
                && r.unit.as_deref() == Some("kg")
        }));
    }

    #[test]
    fn converts_acceleration_alias_units_into_common_storage_units() {
        let csv = "# StartTime = 03/27/2026 06:54:01.3973 PM\n\
            Time (sec),Acceleration (m/s),Acceleration (ft/s),Acceleration X (g)\n\
            1,3.5,10,0.5\n";

        let parsed = parse_readings(csv.as_bytes(), VIN, upload_id()).unwrap();

        assert!(parsed.raw_readings.iter().any(|r| {
            r.key == "acceleration"
                && r.value.map(|value| approx_eq(value, 3.5)).unwrap_or(false)
                && r.unit.as_deref() == Some("m s 2")
        }));
        assert!(parsed.raw_readings.iter().any(|r| {
            r.key == "acceleration_2"
                && r.value
                    .map(|value| approx_eq(value, 3.048))
                    .unwrap_or(false)
                && r.unit.as_deref() == Some("m s 2")
        }));
        assert!(parsed.raw_readings.iter().any(|r| {
            r.key == "accel_x"
                && r.value
                    .map(|value| approx_eq(value, 4.903325))
                    .unwrap_or(false)
                && r.unit.as_deref() == Some("m s 2")
        }));
    }

    #[test]
    fn unsupported_units_get_unit_qualified_fallback_keys() {
        let csv = "# StartTime = 03/27/2026 06:54:01.3973 PM\n\
            Time (sec),Vehicle speed (knots)\n\
            1,12\n";

        let parsed = parse_readings(csv.as_bytes(), VIN, upload_id()).unwrap();

        assert!(parsed.raw_readings.iter().any(|r| {
            r.key == "vehicle_speed_knots"
                && r.value == Some(12.0)
                && r.unit.as_deref() == Some("knots")
        }));
    }

    #[test]
    fn logs_similar_speed_columns_separately() {
        let csv = "# StartTime = 03/27/2026 06:54:01.3973 PM\n\
            Time (sec),GPS Speed (MPH),Average Speed (MPH),Vehicle speed (MPH)\n\
            1,45,30,12\n";

        let parsed = parse_readings(csv.as_bytes(), VIN, upload_id()).unwrap();

        assert!(parsed
            .raw_readings
            .iter()
            .any(|r| r.key == "gps_speed" && r.value == Some(45.0)));
        assert!(parsed
            .raw_readings
            .iter()
            .any(|r| r.key == "average_speed" && r.value == Some(30.0)));
        assert!(parsed
            .raw_readings
            .iter()
            .any(|r| r.key == "vehicle_speed" && r.value == Some(12.0)));
    }

    #[test]
    fn logs_unmapped_gps_acceleration_and_status_columns() {
        let csv = "# StartTime = 03/27/2026 06:54:01.3973 PM\n\
            Time (sec),Fuel Status,Latitude,Longitude,Acceleration X (g),Acceleration Y (g),Vehicle speed (MPH)\n\
            1,Closed loop,41.1,-87.2,0.01,-0.02,12\n";

        let parsed = parse_readings(csv.as_bytes(), VIN, upload_id()).unwrap();

        assert!(parsed
            .raw_readings
            .iter()
            .any(|r| r.key == "fuel_status" && r.text_value.as_deref() == Some("Closed loop")));
        assert!(parsed
            .raw_readings
            .iter()
            .any(|r| r.key == "latitude" && r.value == Some(41.1)));
        assert!(parsed
            .raw_readings
            .iter()
            .any(|r| r.key == "longitude" && r.value == Some(-87.2)));
        assert!(parsed.raw_readings.iter().any(|r| r.key == "accel_x"
            && r.value
                .map(|value| approx_eq(value, 0.0980665))
                .unwrap_or(false)
            && r.unit.as_deref() == Some("m s 2")));
        assert!(parsed
            .raw_readings
            .iter()
            .any(|r| r.key == "vehicle_speed" && r.value == Some(12.0)));
    }

    #[test]
    fn rejects_same_key_numeric_text_conflict_within_upload() {
        let time = model::parse_timestamp("2026-03-27T23:54:01Z").unwrap();
        let vehicle_id = model::vin_to_uuid(VIN);
        let readings = vec![
            RawMetricReading {
                upload_id: upload_id(),
                vehicle_id,
                time,
                key: "fuel_status".into(),
                label: "Fuel status".into(),
                unit: None,
                value: None,
                text_value: Some("Closed loop".into()),
            },
            RawMetricReading {
                upload_id: upload_id(),
                vehicle_id,
                time,
                key: "fuel_status".into(),
                label: "Fuel status".into(),
                unit: None,
                value: Some(1.0),
                text_value: None,
            },
        ];

        assert!(matches!(
            validate_upload_value_kinds(&readings),
            Err(Error::BadRequest(message))
                if message == "metric key 'fuel_status' conflicts: existing text vs incoming numeric"
        ));
    }

    #[test]
    fn rejects_existing_metric_value_kind_conflict() {
        assert!(matches!(
            ensure_value_kind_matches("fuel_status", ValueKind::Numeric, "text"),
            Err(Error::BadRequest(message))
                if message == "metric key 'fuel_status' conflicts: existing text vs incoming numeric"
        ));
    }

    #[test]
    fn rollup_policy_keeps_private_metrics_out_of_daily_rollups() {
        assert!(should_roll_up_metric("vehicle_speed"));
        assert!(should_roll_up_metric("vehicle_speed_2"));
        assert!(!should_roll_up_metric("latitude"));
        assert!(!should_roll_up_metric("accel_x"));
        assert!(!should_roll_up_metric("future_ev_metric"));
    }

    #[test]
    fn parses_all_test_data_csv_files() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test_data");
        let mut checked = 0usize;

        for vin_dir in fs::read_dir(root).unwrap() {
            let vin_dir = vin_dir.unwrap();
            if !vin_dir.file_type().unwrap().is_dir() {
                continue;
            }
            let vin = vin_dir.file_name().into_string().unwrap();
            for file in fs::read_dir(vin_dir.path()).unwrap() {
                let file = file.unwrap();
                if file.path().extension().and_then(|ext| ext.to_str()) != Some("csv") {
                    continue;
                }
                let body = fs::read(file.path()).unwrap();
                let parsed = parse_readings(body.as_slice(), &vin, upload_id())
                    .unwrap_or_else(|err| panic!("{}: {err:?}", file.path().display()));
                assert!(
                    !parsed.raw_readings.is_empty(),
                    "{} yielded no readings",
                    file.path().display()
                );
                checked += 1;
            }
        }

        assert!(checked >= 3, "expected concise fixture coverage");
    }
}
