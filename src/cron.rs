use chrono::{DateTime, Datelike, Duration, TimeZone, Timelike};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    WrongFieldCount(usize),
    BadValue(String),
    BadRange(String),
    BadStep(String),
    OutOfBounds(String),
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::WrongFieldCount(n) => {
                write!(f, "expected 5 fields, got {n}")
            }
            ParseError::BadValue(s) => write!(f, "bad value: {s}"),
            ParseError::BadRange(s) => write!(f, "bad range: {s}"),
            ParseError::BadStep(s) => write!(f, "bad step: {s}"),
            ParseError::OutOfBounds(s) => write!(f, "value out of bounds: {s}"),
        }
    }
}

impl std::error::Error for ParseError {}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Field {
    Any,
    Values(Vec<u32>),
}

impl Field {
    fn contains(&self, v: u32) -> bool {
        match self {
            Field::Any => true,
            Field::Values(vs) => vs.binary_search(&v).is_ok(),
        }
    }

    fn is_any(&self) -> bool {
        matches!(self, Field::Any)
    }
}

fn parse_field(raw: &str, min: u32, max: u32) -> Result<Field, ParseError> {
    let mut values: Vec<u32> = Vec::new();
    for part in raw.split(',') {
        if part.is_empty() {
            return Err(ParseError::BadValue(raw.to_string()));
        }
        let (range_part, step) = match part.split_once('/') {
            Some((r, s)) => {
                let step: u32 = s
                    .parse()
                    .map_err(|_| ParseError::BadStep(part.to_string()))?;
                if step == 0 {
                    return Err(ParseError::BadStep(part.to_string()));
                }
                (r, Some(step))
            }
            None => (part, None),
        };

        let (lo, hi) = if range_part == "*" {
            (min, max)
        } else if let Some((a, b)) = range_part.split_once('-') {
            let lo: u32 = a
                .parse()
                .map_err(|_| ParseError::BadRange(part.to_string()))?;
            let hi: u32 = b
                .parse()
                .map_err(|_| ParseError::BadRange(part.to_string()))?;
            if lo > hi {
                return Err(ParseError::BadRange(part.to_string()));
            }
            (lo, hi)
        } else {
            let v: u32 = range_part
                .parse()
                .map_err(|_| ParseError::BadValue(part.to_string()))?;
            if step.is_some() {
                (v, max)
            } else {
                (v, v)
            }
        };

        if lo < min || hi > max {
            return Err(ParseError::OutOfBounds(part.to_string()));
        }

        let step = step.unwrap_or(1);
        let mut v = lo;
        while v <= hi {
            values.push(v);
            v += step;
        }
    }

    values.sort_unstable();
    values.dedup();

    if values.len() as u32 == (max - min + 1) && step_covers_all(&values, min, max) {
        Ok(Field::Any)
    } else {
        Ok(Field::Values(values))
    }
}

fn step_covers_all(values: &[u32], min: u32, max: u32) -> bool {
    values.first() == Some(&min) && values.last() == Some(&max) && values.len() as u32 == max - min + 1
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CronExpr {
    minute: Field,
    hour: Field,
    dom: Field,
    month: Field,
    dow: Field,
}

impl CronExpr {
    pub fn parse(expr: &str) -> Result<CronExpr, ParseError> {
        let fields: Vec<&str> = expr.split_whitespace().collect();
        if fields.len() != 5 {
            return Err(ParseError::WrongFieldCount(fields.len()));
        }
        let minute = parse_field(fields[0], 0, 59)?;
        let hour = parse_field(fields[1], 0, 23)?;
        let dom = parse_field(fields[2], 1, 31)?;
        let month = parse_field(fields[3], 1, 12)?;
        let dow_raw = fields[4].replace('7', "0");
        let dow = parse_field(&dow_raw, 0, 6)?;
        Ok(CronExpr {
            minute,
            hour,
            dom,
            month,
            dow,
        })
    }

    fn day_matches(&self, day: u32, weekday: u32) -> bool {
        match (self.dom.is_any(), self.dow.is_any()) {
            (true, true) => true,
            (true, false) => self.dow.contains(weekday),
            (false, true) => self.dom.contains(day),
            (false, false) => self.dom.contains(day) || self.dow.contains(weekday),
        }
    }

    fn matches<Tz: TimeZone>(&self, dt: &DateTime<Tz>) -> bool {
        self.minute.contains(dt.minute())
            && self.hour.contains(dt.hour())
            && self.month.contains(dt.month())
            && self.day_matches(dt.day(), dt.weekday().num_days_from_sunday())
    }

    /// Finds the next fire time strictly after `from`, truncated to the minute.
    /// Searches minute by minute up to ~4 years ahead.
    pub fn next_after<Tz: TimeZone>(&self, from: &DateTime<Tz>) -> Option<DateTime<Tz>> {
        let start = from.clone() + Duration::minutes(1);
        let start = start
            .with_second(0)
            .and_then(|d| d.with_nanosecond(0))
            .unwrap_or(start);

        let max_minutes: i64 = 4 * 366 * 24 * 60;
        let mut candidate = start;
        for _ in 0..max_minutes {
            if self.matches(&candidate) {
                return Some(candidate);
            }
            candidate = candidate + Duration::minutes(1);
        }
        None
    }

    /// Returns the next `count` fire times after `from`.
    pub fn next_n<Tz: TimeZone>(&self, from: &DateTime<Tz>, count: usize) -> Vec<DateTime<Tz>> {
        let mut out = Vec::with_capacity(count);
        let mut cur = from.clone();
        for _ in 0..count {
            match self.next_after(&cur) {
                Some(next) => {
                    out.push(next.clone());
                    cur = next;
                }
                None => break,
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    fn dt(y: i32, mo: u32, d: u32, h: u32, mi: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(y, mo, d, h, mi, 0).unwrap()
    }

    #[test]
    fn every_15_minutes_from_07() {
        let e = CronExpr::parse("*/15 * * * *").unwrap();
        let from = dt(2026, 1, 1, 0, 7);
        let next = e.next_after(&from).unwrap();
        assert_eq!(next, dt(2026, 1, 1, 0, 15));
    }

    #[test]
    fn daily_midnight() {
        let e = CronExpr::parse("0 0 * * *").unwrap();
        let from = dt(2026, 1, 1, 13, 45);
        let next = e.next_after(&from).unwrap();
        assert_eq!(next, dt(2026, 1, 2, 0, 0));
    }

    #[test]
    fn daily_midnight_exactly_at_midnight() {
        let e = CronExpr::parse("0 0 * * *").unwrap();
        let from = dt(2026, 1, 1, 0, 0);
        let next = e.next_after(&from).unwrap();
        assert_eq!(next, dt(2026, 1, 2, 0, 0));
    }

    #[test]
    fn weekly_monday_9am() {
        // 2026-01-01 is a Thursday.
        let e = CronExpr::parse("0 9 * * 1").unwrap();
        let from = dt(2026, 1, 1, 10, 0);
        let next = e.next_after(&from).unwrap();
        assert_eq!(next, dt(2026, 1, 5, 9, 0));
        assert_eq!(next.weekday().num_days_from_sunday(), 1);
    }

    #[test]
    fn range_with_step() {
        let e = CronExpr::parse("1-30/2 * * * *").unwrap();
        let from = dt(2026, 1, 1, 0, 0);
        let next = e.next_after(&from).unwrap();
        assert_eq!(next, dt(2026, 1, 1, 0, 1));
        let next2 = e.next_after(&next).unwrap();
        assert_eq!(next2, dt(2026, 1, 1, 0, 3));
    }

    #[test]
    fn list_values() {
        let e = CronExpr::parse("0,15,45 * * * *").unwrap();
        let from = dt(2026, 1, 1, 0, 16);
        let next = e.next_after(&from).unwrap();
        assert_eq!(next, dt(2026, 1, 1, 0, 45));
    }

    #[test]
    fn dom_or_dow_semantics_both_restricted() {
        // "1st of month OR Friday": matches if either is satisfied.
        let e = CronExpr::parse("0 0 1 * 5").unwrap();
        // 2026-01-02 is a Friday.
        let from = dt(2026, 1, 1, 0, 0);
        let next = e.next_after(&from).unwrap();
        assert_eq!(next, dt(2026, 1, 2, 0, 0));
    }

    #[test]
    fn dom_restricted_dow_any() {
        let e = CronExpr::parse("0 0 15 * *").unwrap();
        let from = dt(2026, 1, 1, 0, 0);
        let next = e.next_after(&from).unwrap();
        assert_eq!(next, dt(2026, 1, 15, 0, 0));
    }

    #[test]
    fn month_restriction() {
        let e = CronExpr::parse("0 0 1 6 *").unwrap();
        let from = dt(2026, 1, 1, 0, 0);
        let next = e.next_after(&from).unwrap();
        assert_eq!(next, dt(2026, 6, 1, 0, 0));
    }

    #[test]
    fn dow_7_is_sunday() {
        let a = CronExpr::parse("0 0 * * 7").unwrap();
        let b = CronExpr::parse("0 0 * * 0").unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn next_n_returns_multiple() {
        let e = CronExpr::parse("*/15 * * * *").unwrap();
        let from = dt(2026, 1, 1, 0, 0);
        let times = e.next_n(&from, 3);
        assert_eq!(
            times,
            vec![dt(2026, 1, 1, 0, 15), dt(2026, 1, 1, 0, 30), dt(2026, 1, 1, 0, 45)]
        );
    }

    #[test]
    fn wrong_field_count_rejected() {
        assert!(matches!(
            CronExpr::parse("* * * *"),
            Err(ParseError::WrongFieldCount(4))
        ));
    }

    #[test]
    fn out_of_range_rejected() {
        assert!(CronExpr::parse("60 * * * *").is_err());
        assert!(CronExpr::parse("* 24 * * *").is_err());
        assert!(CronExpr::parse("* * 32 * *").is_err());
        assert!(CronExpr::parse("* * * 13 *").is_err());
        assert!(CronExpr::parse("* * * * 8").is_err());
    }

    #[test]
    fn bad_range_rejected() {
        assert!(CronExpr::parse("5-1 * * * *").is_err());
    }

    #[test]
    fn zero_step_rejected() {
        assert!(CronExpr::parse("*/0 * * * *").is_err());
    }

    #[test]
    fn garbage_rejected() {
        assert!(CronExpr::parse("abc * * * *").is_err());
    }
}
