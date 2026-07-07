use chrono::{DateTime, NaiveDate, NaiveDateTime, TimeZone, Utc};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum ParseError {
    #[error("invalid start time: {0}")]
    InvalidStartTime(String),
}

pub fn parse_start_time(line: &str) -> Result<DateTime<Utc>, ParseError> {
    let ts_str = line
        .trim_start_matches('\u{feff}')
        .trim_start_matches('#')
        .trim()
        .trim_start_matches("StartTime")
        .trim()
        .trim_start_matches('=')
        .trim();

    let naive = NaiveDateTime::parse_from_str(ts_str, "%m/%d/%Y %I:%M:%S%.f %p")
        .map_err(|_| ParseError::InvalidStartTime(ts_str.to_string()))?;

    use chrono::FixedOffset;
    let local_offset = FixedOffset::west_opt(5 * 3600).unwrap();
    let local = local_offset
        .from_local_datetime(&naive)
        .single()
        .ok_or_else(|| ParseError::InvalidStartTime(ts_str.to_string()))?;
    Ok(local.with_timezone(&Utc))
}

pub fn parse_timestamp(value: &str) -> Result<DateTime<Utc>, ParseError> {
    let trimmed = value.trim();
    if let Ok(parsed) = DateTime::parse_from_rfc3339(trimmed) {
        return Ok(parsed.with_timezone(&Utc));
    }

    for fmt in [
        "%Y-%m-%d %H:%M:%S%.f",
        "%m/%d/%Y %I:%M:%S%.f %p",
        "%m/%d/%Y %H:%M:%S%.f",
    ] {
        if let Ok(naive) = NaiveDateTime::parse_from_str(trimmed, fmt) {
            return naive_to_utc(naive, trimmed);
        }
    }

    for fmt in ["%Y-%m-%d", "%m/%d/%Y"] {
        if let Ok(date) = NaiveDate::parse_from_str(trimmed, fmt) {
            let naive = date
                .and_hms_opt(0, 0, 0)
                .ok_or_else(|| ParseError::InvalidStartTime(trimmed.to_string()))?;
            return naive_to_utc(naive, trimmed);
        }
    }

    Err(ParseError::InvalidStartTime(trimmed.to_string()))
}

fn naive_to_utc(naive: NaiveDateTime, original: &str) -> Result<DateTime<Utc>, ParseError> {
    use chrono::FixedOffset;
    let local_offset = FixedOffset::west_opt(5 * 3600).unwrap();
    let local = local_offset
        .from_local_datetime(&naive)
        .single()
        .ok_or_else(|| ParseError::InvalidStartTime(original.to_string()))?;
    Ok(local.with_timezone(&Utc))
}

#[derive(Debug, Clone)]
pub struct RawMetricReading {
    pub upload_id: Uuid,
    pub vehicle_id: Uuid,
    pub time: DateTime<Utc>,
    pub key: String,
    pub label: String,
    pub unit: Option<String>,
    pub value: Option<f64>,
    pub text_value: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ValueKind {
    Numeric,
    Text,
}

impl ValueKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Numeric => "numeric",
            Self::Text => "text",
        }
    }
}

impl RawMetricReading {
    pub fn value_kind(&self) -> ValueKind {
        if self.value.is_some() {
            ValueKind::Numeric
        } else {
            ValueKind::Text
        }
    }
}

pub fn vin_to_uuid(vin: &str) -> Uuid {
    Uuid::new_v5(&Uuid::NAMESPACE_DNS, vin.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    pub fn parse_start_time_works() {
        let t = parse_start_time("# StartTime = 03/27/2026 06:54:01.3973 PM").unwrap();
        assert_eq!(t.format("%Y-%m-%d").to_string(), "2026-03-27");
    }

    #[test]
    fn parse_timestamp_works() {
        let t = parse_timestamp("2026-03-27T23:54:01Z").unwrap();
        assert_eq!(
            t.format("%Y-%m-%d %H:%M:%S").to_string(),
            "2026-03-27 23:54:01"
        );
    }

    #[test]
    fn vin_to_uuid_is_stable() {
        let u1 = vin_to_uuid("DEMO-HONDA-ACCORD");
        let u2 = vin_to_uuid("DEMO-HONDA-ACCORD");
        assert_eq!(u1, u2);
    }
}
