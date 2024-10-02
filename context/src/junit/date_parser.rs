use std::time::Duration;

use chrono::{DateTime as ChronoDateTime, FixedOffset};
use speedate::{Date as SpeedateDate, DateTime as SpeedateDateTime};

#[derive(Debug, Copy, Clone)]
enum DateType {
    DateTime,
    NaiveDate,
}

#[derive(Debug, Clone, Default)]
struct TimestampAndOffset {
    timestamp_secs_micros: Option<(i64, u32)>,
    offset_secs: Option<i32>,
}

#[derive(Debug, Clone, Default)]
pub struct JunitDateParser {
    date_type: Option<DateType>,
}

impl JunitDateParser {
    pub fn parse_date<T: AsRef<str>>(
        &mut self,
        date_str: T,
    ) -> Option<ChronoDateTime<FixedOffset>> {
        let date_str = date_str.as_ref();

        let timestamp_and_offset = match self.date_type {
            Some(DateType::DateTime) => Self::parse_date_time(date_str),
            Some(DateType::NaiveDate) => Self::parse_naive_date(date_str),
            None => Self::parse_date_time(date_str).or_else(|| Self::parse_naive_date(date_str)),
        };

        self.convert_to_chrono_date_time(timestamp_and_offset.unwrap_or_default())
    }

    fn parse_date_time<T: AsRef<str>>(date_str: T) -> Option<TimestampAndOffset> {
        SpeedateDateTime::parse_str(date_str.as_ref())
            .ok()
            .map(|dt| TimestampAndOffset {
                timestamp_secs_micros: Some((dt.timestamp(), dt.time.microsecond)),
                offset_secs: dt.time.tz_offset,
            })
    }

    fn parse_naive_date<T: AsRef<str>>(date_str: T) -> Option<TimestampAndOffset> {
        SpeedateDate::parse_str(date_str.as_ref())
            .ok()
            .map(|d| TimestampAndOffset {
                timestamp_secs_micros: Some((d.timestamp(), 0)),
                offset_secs: None,
            })
    }

    fn convert_to_chrono_date_time(
        &mut self,
        TimestampAndOffset {
            timestamp_secs_micros,
            offset_secs,
        }: TimestampAndOffset,
    ) -> Option<ChronoDateTime<FixedOffset>> {
        match (
            timestamp_secs_micros.and_then(|(secs, micros)| {
                let duration = Duration::from_micros(micros.into());
                ChronoDateTime::from_timestamp(
                    secs,
                    duration.as_nanos().try_into().unwrap_or_default(),
                )
            }),
            offset_secs.and_then(|secs| FixedOffset::east_opt(secs)),
        ) {
            (Some(chrono_date_time), Some(fixed_offset)) => {
                self.date_type = Some(DateType::DateTime);
                Some(chrono_date_time.with_timezone(&fixed_offset))
            }
            (Some(chrono_date_time), None) => {
                self.date_type = Some(DateType::NaiveDate);
                Some(chrono_date_time.fixed_offset())
            }
            (None, None) | (None, Some(..)) => None,
        }
    }
}
