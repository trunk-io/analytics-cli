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
            Some(DateType::DateTime) => self
                .parse_date_time(date_str)
                .or_else(|| self.parse_naive_date(date_str)),
            _ => self
                .parse_naive_date(date_str)
                .or_else(|| self.parse_date_time(date_str)),
        };

        Self::convert_to_chrono_date_time(timestamp_and_offset.unwrap_or_default())
    }

    fn parse_date_time<T: AsRef<str>>(&mut self, date_str: T) -> Option<TimestampAndOffset> {
        SpeedateDateTime::parse_str(date_str.as_ref())
            .ok()
            .map(|dt| {
                self.date_type = Some(DateType::DateTime);
                TimestampAndOffset {
                    timestamp_secs_micros: Some((dt.timestamp(), dt.time.microsecond)),
                    offset_secs: dt.time.tz_offset,
                }
            })
    }

    fn parse_naive_date<T: AsRef<str>>(&mut self, date_str: T) -> Option<TimestampAndOffset> {
        SpeedateDate::parse_str(date_str.as_ref()).ok().map(|d| {
            self.date_type = Some(DateType::NaiveDate);
            TimestampAndOffset {
                timestamp_secs_micros: Some((d.timestamp(), 0)),
                offset_secs: None,
            }
        })
    }

    fn convert_to_chrono_date_time(
        TimestampAndOffset {
            timestamp_secs_micros,
            offset_secs,
        }: TimestampAndOffset,
    ) -> Option<ChronoDateTime<FixedOffset>> {
        match (
            timestamp_secs_micros.and_then(|(secs, micros)| {
                let duration = Duration::from_micros(micros.into());
                ChronoDateTime::from_timestamp(
                    secs - offset_secs.unwrap_or(0) as i64,
                    duration.as_nanos().try_into().unwrap_or_default(),
                )
            }),
            offset_secs.and_then(FixedOffset::east_opt),
        ) {
            (Some(chrono_date_time), Some(fixed_offset)) => {
                Some(chrono_date_time.with_timezone(&fixed_offset))
            }
            (Some(chrono_date_time), None) => Some(chrono_date_time.fixed_offset()),
            (None, None) | (None, Some(..)) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::junit::date_parser::JunitDateParser;

    #[test]
    fn test_parse_date() {
        let mut date_parser = JunitDateParser::default();
        pretty_assertions::assert_eq!(None, date_parser.parse_date("not a date"));
        pretty_assertions::assert_eq!(
            1704819148443565,
            date_parser
                .parse_date("2024-01-09T16:52:28.443565")
                .unwrap()
                .timestamp_micros()
        );
        pretty_assertions::assert_eq!(
            1704758400000000,
            date_parser
                .parse_date("2024-01-09")
                .unwrap()
                .timestamp_micros()
        );
        pretty_assertions::assert_eq!(
            1721743937587000,
            date_parser
                .parse_date("2024-07-23T14:12:17.587Z")
                .unwrap()
                .timestamp_micros()
        );
        pretty_assertions::assert_eq!(
            1721745659000000,
            date_parser
                .parse_date("2024-07-23T14:40:59+00:00")
                .unwrap()
                .timestamp_micros()
        );
        pretty_assertions::assert_eq!(
            1758837152774867,
            date_parser
                .parse_date("2025-09-25T14:52:32.774867-07:00")
                .unwrap()
                .timestamp_micros()
        );
        pretty_assertions::assert_eq!(
            1758861384602000,
            date_parser
                .parse_date("2025-09-25T21:36:24.602-07:00")
                .unwrap()
                .timestamp_micros()
        );
        pretty_assertions::assert_eq!(
            1758823584602567,
            date_parser
                .parse_date("2025-09-25T21:36:24.602567+03:30")
                .unwrap()
                .timestamp_micros()
        );
    }
}
