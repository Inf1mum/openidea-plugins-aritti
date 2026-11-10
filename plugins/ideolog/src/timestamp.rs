//! A deliberately bounded Java-style timestamp format subset. No locale,
//! named-zone, week-year or lenient date parsing is inferred.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Field {
    Year,
    Month,
    Day,
    Hour,
    Minute,
    Second,
    Millis,
    Zone(u8),
}
#[derive(Debug)]
enum Part {
    Literal(String),
    Field(Field),
}

fn compile(pattern: &str) -> Result<Vec<Part>, String> {
    if pattern.is_empty() || pattern.len() > 256 {
        return Err(
            "Use a timestamp format of 1–256 bytes, or leave it empty for automatic detection."
                .into(),
        );
    }
    let chars: Vec<char> = pattern.chars().collect();
    let (mut parts, mut fields, mut at) = (Vec::new(), Vec::new(), 0);
    while at < chars.len() {
        let ch = chars[at];
        if ch.is_control() {
            return Err("Timestamp formats cannot contain control characters.".into());
        }
        if ch == '\'' {
            at += 1;
            if chars.get(at) == Some(&'\'') {
                parts.push(Part::Literal("'".into()));
                at += 1;
                continue;
            }
            let (mut literal, mut closed) = (String::new(), false);
            while at < chars.len() {
                let c = chars[at];
                at += 1;
                if c == '\'' {
                    if chars.get(at) == Some(&'\'') {
                        literal.push('\'');
                        at += 1;
                    } else {
                        closed = true;
                        break;
                    }
                } else if c.is_control() {
                    return Err("Timestamp formats cannot contain control characters.".into());
                } else {
                    literal.push(c);
                }
            }
            if !closed {
                return Err("Close the quoted literal in the timestamp format.".into());
            }
            parts.push(Part::Literal(literal));
        } else if ch.is_alphabetic() {
            let start = at;
            while chars.get(at) == Some(&ch) {
                at += 1;
            }
            let count = at - start;
            let field = match (ch, count) {
                ('y', 4) => Field::Year, ('M', 2) => Field::Month, ('d', 2) => Field::Day,
                ('H', 2) => Field::Hour, ('m', 2) => Field::Minute, ('s', 2) => Field::Second,
                ('S', 3) => Field::Millis, ('Z', 1) => Field::Zone(0),
                ('X', 1..=3) => Field::Zone(count as u8),
                _ => return Err("Supported fields: yyyy, MM, dd, HH, mm, ss, SSS, Z, X, XX and XXX; quote literal letters. Locale and named-zone formats are unavailable.".into()),
            };
            if fields
                .iter()
                .any(|f| *f == field || matches!((f, field), (Field::Zone(_), Field::Zone(_))))
            {
                return Err("Use each timestamp field only once.".into());
            }
            fields.push(field);
            parts.push(Part::Field(field));
        } else {
            parts.push(Part::Literal(ch.to_string()));
            at += 1;
        }
    }
    if ![Field::Hour, Field::Minute, Field::Second]
        .iter()
        .all(|f| fields.contains(f))
    {
        return Err("Timestamp formats must include HH, mm and ss.".into());
    }
    let has_date = fields
        .iter()
        .any(|f| matches!(f, Field::Year | Field::Month | Field::Day));
    if has_date
        && ![Field::Month, Field::Day]
            .iter()
            .all(|f| fields.contains(f))
    {
        return Err(
            "Include both MM and dd for a date; yyyy is optional (missing year uses 2000).".into(),
        );
    }
    Ok(parts)
}

pub(super) fn validate(pattern: &str) -> Result<(), String> {
    compile(pattern).map(|_| ())
}

fn digits(value: &str, at: &mut usize, count: usize) -> Option<i64> {
    let end = at.checked_add(count)?;
    let part = value.get(*at..end)?;
    if !part.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    *at = end;
    part.parse().ok()
}

pub(super) fn seconds(value: &str, pattern: &str) -> Option<f64> {
    if value.len() > 1024 {
        return None;
    }
    let parts = compile(pattern).ok()?;
    let (mut year, mut month, mut day) = (2000i64, 1i64, 1i64);
    let (mut hour, mut minute, mut second, mut millis, mut zone) = (0, 0, 0, 0, 0);
    let mut at = 0;
    for part in parts {
        match part {
            Part::Literal(literal) => {
                if !value.get(at..)?.starts_with(&literal) {
                    return None;
                }
                at += literal.len();
            }
            Part::Field(Field::Zone(width)) => {
                // ISO X tokens admit Z; Java RFC822 Z admits only a numeric offset.
                if value.get(at..)?.starts_with('Z') && width != 0 {
                    at += 1;
                    continue;
                }
                let sign = match value.as_bytes().get(at)? {
                    b'+' => 1,
                    b'-' => -1,
                    _ => return None,
                };
                at += 1;
                let hours = digits(value, &mut at, 2)?;
                let minutes = if width == 1 {
                    0
                } else {
                    if width == 3 {
                        if value.as_bytes().get(at) != Some(&b':') {
                            return None;
                        }
                        at += 1;
                    }
                    digits(value, &mut at, 2)?
                };
                if hours > 23 || minutes > 59 {
                    return None;
                }
                zone = sign * (hours * 3600 + minutes * 60);
            }
            Part::Field(field) => {
                let width = match field {
                    Field::Year => 4,
                    Field::Millis => 3,
                    _ => 2,
                };
                let number = digits(value, &mut at, width)?;
                match field {
                    Field::Year => year = number,
                    Field::Month => month = number,
                    Field::Day => day = number,
                    Field::Hour => hour = number,
                    Field::Minute => minute = number,
                    Field::Second => second = number,
                    Field::Millis => millis = number,
                    Field::Zone(_) => unreachable!(),
                }
            }
        }
    }
    if at != value.len()
        || year < 1
        || !(1..=12).contains(&month)
        || hour > 23
        || minute > 59
        || second > 59
    {
        return None;
    }
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let maximum = match month {
        2 if leap => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    };
    if day < 1 || day > maximum {
        return None;
    }
    let (y, m) = if month <= 2 {
        (year - 1, month + 9)
    } else {
        (year, month - 3)
    };
    let days = 365 * y + y / 4 - y / 100 + y / 400 + (153 * m + 2) / 5 + day;
    Some((days * 86400 + hour * 3600 + minute * 60 + second - zone) as f64 + millis as f64 / 1000.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn different_orders_quotes_and_zones_share_a_scale() {
        let baseline = crate::log::seconds("2024-05-01T08:00:00Z");
        assert_eq!(
            seconds("01/05/2024 10:00:00 +0200", "dd/MM/yyyy HH:mm:ss Z"),
            baseline
        );
        assert_eq!(
            seconds("2024-05-01T03:00:00-05:00", "yyyy-MM-dd'T'HH:mm:ssXXX"),
            baseline
        );
        assert_eq!(
            seconds("2024-05-01T08:00:00Z", "yyyy-MM-dd'T'HH:mm:ssXXX"),
            baseline
        );
        assert_eq!(
            seconds(
                "2024-05-01 time 08:00:00.250",
                "yyyy-MM-dd 'time' HH:mm:ss.SSS"
            )
            .unwrap()
                - baseline.unwrap(),
            0.25
        );
        assert_eq!(
            seconds("08:00:00'", "HH:mm:ss''"),
            crate::log::seconds("08:00:00")
        );
    }
    #[test]
    fn invalid_values_and_unsupported_formats_never_guess() {
        let pattern = "yyyy-MM-dd HH:mm:ss";
        for text in [
            "2023-02-29 12:00:00",
            "2024-04-31 12:00:00",
            "0000-01-01 12:00:00",
            "2024-01-01 24:00:00",
            "2024-01-01 12:60:00",
            "2024-01-01 12:00:60",
            "2024-01-01 12:00:00 trailing",
            "2024-01-01 12:00:00 ",
            "2024-01-01 12:00:😀",
        ] {
            assert_eq!(seconds(text, pattern), None, "{text}");
        }
        assert!(seconds("2024-02-29 12:00:00", pattern).is_some());
        assert_eq!(seconds("12:00:00+24:00", "HH:mm:ssXXX"), None);
        assert_eq!(seconds("12:00:00Z", "HH:mm:ss Z"), None);
        for pattern in [
            "",
            "yyyy-MM-dd",
            "HH:mm",
            "HH:mm:ss z",
            "HH:mm:ss MMM",
            "HH:mm:ss 'open",
            "HH:mm:ss HH",
            "HH:mm:ss XX Z",
            "dd HH:mm:ss",
            "HH:mm:ss\n",
        ] {
            assert!(validate(pattern).is_err(), "{pattern}");
        }
    }
}

#[cfg(test)]
mod integration_tests {
    use crate::log::{Config, Parsed, time_seconds};
    const XML: &str = r#"<LogParsingPattern enabled="true" name="Day first" pattern="^(.+) (INFO) (.*)$" linePattern="^\d" timeId="0" severityId="1" categoryId="-1" timePattern="dd/MM/yyyy HH:mm:ss" uuid="day-first" />"#;
    #[test]
    fn imported_pattern_drives_real_captured_timestamp_and_unknown_is_reported() {
        let config = Config::with_custom(XML);
        assert!(config.errors.is_empty());
        let format = config.format_by_id("day-first").unwrap();
        let text = "01/05/2024 08:00:00 INFO message";
        let parsed = Parsed::new(Some(format), text);
        let time = parsed.time(Some(format)).unwrap();
        assert_eq!(
            time_seconds(time, format.time_pattern.as_deref()),
            crate::log::seconds("2024-05-01T08:00:00Z")
        );
        assert_eq!(
            crate::log::seconds(time),
            None,
            "this format actually needs its explicit date order"
        );
        let unsupported =
            Config::with_custom(&XML.replace("dd/MM/yyyy HH:mm:ss", "EEE yyyy-MM-dd HH:mm:ss"));
        assert_eq!(unsupported.errors.len(), 1);
        let format = unsupported.format_by_id("day-first").unwrap();
        assert!(
            Parsed::new(Some(format), text).time(Some(format)).is_some(),
            "unknown date formats preserve event parsing"
        );
        assert_eq!(
            time_seconds("2024-05-01T08:00:00Z", format.time_pattern.as_deref()),
            None,
            "explicit unsupported patterns do not auto-fallback"
        );
    }
}
