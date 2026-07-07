use crate::Error;
use chrono::{DateTime, Utc};
use uuid::Uuid;

pub(super) const DEFAULT_LIMIT: i64 = 200;
pub(super) const MAX_LIMIT: i64 = 10_000;

pub(super) fn default_limit() -> i64 {
    DEFAULT_LIMIT
}

pub(super) fn parse_vehicle_id(raw: &str) -> Result<Uuid, Error> {
    Uuid::parse_str(raw).map_err(|_| Error::BadRequest("invalid vehicle_id".into()))
}

pub(super) fn check_time_range(
    start: Option<&DateTime<Utc>>,
    end: Option<&DateTime<Utc>>,
) -> Result<(), Error> {
    if let (Some(start), Some(end)) = (start, end) {
        if start > end {
            return Err(Error::BadRequest("start must be before end".into()));
        }
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SummaryBucket {
    Day,
    Week,
    Month,
}

impl SummaryBucket {
    pub(super) fn bucket_expr(self) -> &'static str {
        match self {
            SummaryBucket::Day => "d.bucket_day",
            SummaryBucket::Week => "date_trunc('week', d.bucket_day)",
            SummaryBucket::Month => "date_trunc('month', d.bucket_day)",
        }
    }

    pub(super) fn label(self) -> &'static str {
        match self {
            SummaryBucket::Day => "1d",
            SummaryBucket::Week => "1w",
            SummaryBucket::Month => "1mon",
        }
    }
}

pub(super) fn default_bucket() -> String {
    SummaryBucket::Day.label().into()
}

pub(super) fn parse_summary_bucket(raw: &str) -> Result<SummaryBucket, Error> {
    match raw.trim() {
        "1d" => Ok(SummaryBucket::Day),
        "1w" => Ok(SummaryBucket::Week),
        "1mon" => Ok(SummaryBucket::Month),
        _ => Err(Error::BadRequest("bucket must be 1d, 1w, or 1mon".into())),
    }
}
