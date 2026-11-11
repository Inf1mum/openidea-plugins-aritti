//! A deliberately bounded Java-style timestamp format subset. No locale,
//! regional named-zone, week-year or lenient date parsing is inferred.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Field {
    Year,
    Month,
    Day,
    Hour,
    Hour12,
    Hour24OneBased,
    Hour12ZeroBased,
    AmPm,
    Minute,
    Second,
    Millis,
    Zone(u8),
    ZoneNamed,
}
#[derive(Debug)]
enum Part {
    Literal(String),
    Field(Field),
    Variable(Field),
    Millis2,
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
                ('y', 4) => Field::Year, ('M', 1..=2) => Field::Month, ('d', 1..=2) => Field::Day,
                ('H', 1..=2) => Field::Hour, ('h', 1..=2) => Field::Hour12,
                ('k', 1..=2) => Field::Hour24OneBased, ('K', 1..=2) => Field::Hour12ZeroBased,
                ('a', 1) => Field::AmPm, ('m', 1..=2) => Field::Minute, ('s', 1..=2) => Field::Second,
                ('S', 1..=3) => Field::Millis, ('Z', 1..=3) => Field::Zone(0),
                ('X', 1..=3) => Field::Zone(count as u8), ('z', 1..=4) => Field::ZoneNamed,
                _ => return Err("Supported fields: yyyy; M/MM, d/dd, H/HH, h/hh or K/KK with a (AM/PM), k/kk, m/mm, s/ss, S/SS/SSS; Z, X/XX/XXX, z (UTC/GMT or GMT offset). Quote literal letters. Localized names and named regional zones are unavailable.".into()),
            };
            if fields
                .iter()
                .any(|f| *f == field || matches!((f, field), (Field::Zone(_) | Field::ZoneNamed, Field::Zone(_) | Field::ZoneNamed)))
            {
                return Err("Use each timestamp field only once.".into());
            }
            fields.push(field);
            let variable = count == 1 && matches!(field, Field::Month | Field::Day | Field::Hour | Field::Hour12 | Field::Hour24OneBased | Field::Hour12ZeroBased | Field::Minute | Field::Second | Field::Millis);
            parts.push(if variable { Part::Variable(field) } else if field == Field::Millis && count == 2 { Part::Millis2 } else { Part::Field(field) });
        } else {
            parts.push(Part::Literal(ch.to_string()));
            at += 1;
        }
    }
    let hour24 = fields.contains(&Field::Hour) || fields.contains(&Field::Hour24OneBased);
    let hour12 = fields.contains(&Field::Hour12) || fields.contains(&Field::Hour12ZeroBased);
    if hour24 == hour12 || !fields.contains(&Field::Minute) || !fields.contains(&Field::Second) {
        return Err("Timestamp formats must include either HH or hh, with mm and ss.".into());
    }
    if fields.contains(&Field::AmPm) != hour12 {
        return Err("Use hh together with a for AM/PM; do not combine a with HH.".into());
    }
    if fields.iter().filter(|f| matches!(f, Field::Hour | Field::Hour12 | Field::Hour24OneBased | Field::Hour12ZeroBased)).count() != 1 {
        return Err("Use one hour field only.".into());
    }
    for adjacent in parts.windows(2) {
        if matches!(adjacent[0], Part::Variable(_)) && matches!(adjacent[1], Part::Variable(_) | Part::Millis2 | Part::Field(Field::Year | Field::Month | Field::Day | Field::Hour | Field::Hour12 | Field::Hour24OneBased | Field::Hour12ZeroBased | Field::Minute | Field::Second | Field::Millis)) {
            return Err("Separate variable-width numeric fields with a literal separator.".into());
        }
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
    let mut half_day = None;
    let mut hour12 = false;
    let mut zero_based_hour12 = false;
    let mut one_based_hour24 = false;
    for part in parts {
        match part {
            Part::Literal(literal) => {
                if !value.get(at..)?.starts_with(&literal) {
                    return None;
                }
                at += literal.len();
            }
            Part::Field(Field::ZoneNamed) => {
                let remaining = value.get(at..)?;
                let prefix = remaining.get(..3)?;
                if !prefix.eq_ignore_ascii_case("UTC") && !prefix.eq_ignore_ascii_case("GMT") { return None; }
                at += 3;
                if let Some(sign @ (b'+' | b'-')) = value.as_bytes().get(at).copied() {
                    at += 1;
                    let hours = digits(value, &mut at, 2)?;
                    if value.as_bytes().get(at) != Some(&b':') { return None; }
                    at += 1;
                    let minutes = digits(value, &mut at, 2)?;
                    if hours > 23 || minutes > 59 { return None; }
                    zone = (if sign == b'-' { -1 } else { 1 }) * (hours * 3600 + minutes * 60);
                }
            }
            Part::Millis2 => { millis = digits(value, &mut at, 2)?; }
            Part::Variable(field) => {
                let maximum = if field == Field::Millis { 3 } else { 2 };
                let count = value.as_bytes().get(at..)?.iter().take(maximum).take_while(|b| b.is_ascii_digit()).count();
                if count == 0 { return None; }
                let number = digits(value, &mut at, count)?;
                match field {
                    Field::Month => month = number, Field::Day => day = number,
                    Field::Hour => hour = number,
                    Field::Hour12 => { hour = number; hour12 = true; },
                    Field::Hour12ZeroBased => { hour = number; hour12 = true; zero_based_hour12 = true; },
                    Field::Hour24OneBased => { hour = number; one_based_hour24 = true; },
                    Field::Minute => minute = number, Field::Second => second = number, Field::Millis => millis = number,
                    _ => unreachable!(),
                }
            }
            Part::Field(Field::AmPm) => {
                let marker = value.get(at..at.checked_add(2)?)?;
                half_day = Some(if marker.eq_ignore_ascii_case("AM") { 0 } else if marker.eq_ignore_ascii_case("PM") { 12 } else { return None });
                at += 2;
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
                    Field::Hour12 => { hour = number; hour12 = true; },
                    Field::Hour12ZeroBased => { hour = number; hour12 = true; zero_based_hour12 = true; },
                    Field::Hour24OneBased => { hour = number; one_based_hour24 = true; },
                    Field::AmPm | Field::ZoneNamed => unreachable!(),
                    Field::Minute => minute = number,
                    Field::Second => second = number,
                    Field::Millis => millis = number,
                    Field::Zone(_) => unreachable!(),
                }
            }
        }
    }
    if hour12 {
        if !(if zero_based_hour12 { 0..=11 } else { 1..=12 }).contains(&hour) { return None; }
        hour = hour % 12 + half_day?;
    }
    if one_based_hour24 {
        if !(1..=24).contains(&hour) { return None; }
        hour %= 24;
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
    fn twelve_hour_formats_preserve_midnight_noon_and_gap_arithmetic() {
        for (clock, expected) in [("12:00:00 AM", "00:00:00"), ("12:00:00 PM", "12:00:00"), ("01:20:30 pm", "13:20:30"), ("11:59:59 am", "11:59:59")] {
            assert_eq!(seconds(clock, "hh:mm:ss a"), seconds(expected, "HH:mm:ss"));
        }
        assert_eq!(seconds("2024-05-01 12:00:00 AM+02:00", "yyyy-MM-dd hh:mm:ss aXXX"), seconds("2024-04-30 22:00:00Z", "yyyy-MM-dd HH:mm:ssXXX"));
        for clock in ["00:00:00 AM", "13:00:00 PM", "12:00:00 XX", "12:00:00 P", "12:00:00 PM extra"] {
            assert_eq!(seconds(clock, "hh:mm:ss a"), None);
        }
        for pattern in ["hh:mm:ss", "HH:mm:ss a", "HH hh:mm:ss a", "a:mm:ss", "hh:mm:ss a a"] {
            assert!(validate(pattern).is_err(), "{pattern}");
        }
    }
    #[test]
    fn variable_width_numeric_clocks_and_fixed_zones_use_the_same_scale() {
        let baseline = seconds("2024-05-01 08:02:03.005Z", "yyyy-MM-dd HH:mm:ss.SSSXXX");
        for (text, format) in [
            ("2024-5-1 8:2:3.5 UTC", "yyyy-M-d H:m:s.S z"),
            ("2024-5-1 10:2:3.05 GMT+02:00", "yyyy-M-d H:m:s.SS z"),
            ("2024-5-1 3:2:3.005 GMT-05:00", "yyyy-M-d H:m:s.SSS zzzz"),
            ("2024-5-1 8:2:3.5 am UTC", "yyyy-M-d h:m:s.S a z"),
            ("2024-5-1 8:2:3.5 AM UTC", "yyyy-M-d K:m:s.S a z"),
        ] { assert_eq!(seconds(text, format), baseline, "{text} {format}"); }
        assert_eq!(seconds("24:0:0", "k:m:s"), seconds("00:00:00", "HH:mm:ss"));
        assert_eq!(seconds("0:0:0 PM", "K:m:s a"), seconds("12:00:00", "HH:mm:ss"));
        assert_eq!(seconds("0:0:0 AM", "K:m:s a"), seconds("00:00:00", "HH:mm:ss"));
        for (text, format) in [("0:0:0", "k:m:s"), ("12:0:0 AM", "K:m:s a"), ("1:2:3 EST", "H:m:s z"), ("1:2:3 GMT+24:00", "H:m:s z"), ("1:2:3 GMT+01:60", "H:m:s z"), ("1:2:3 GMT+0100", "H:m:s z"), ("1:2:3 UTCjunk", "H:m:s z")] {
            assert_eq!(seconds(text, format), None, "{text}");
        }
        for format in ["yyyyMd H:m:s", "Hms", "H:m:s z Z", "H k:m:s", "K:m:s", "k:m:s a"] {
            assert!(validate(format).is_err(), "{format}");
        }
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
    fn imported_twelve_hour_format_drives_real_event_time() {
        let config = Config::with_custom(&XML.replace("dd/MM/yyyy HH:mm:ss", "dd/MM/yyyy hh:mm:ss a"));
        assert!(config.errors.is_empty());
        let format = config.format_by_id("day-first").unwrap();
        let parsed = Parsed::new(Some(format), "01/05/2024 12:00:01 PM INFO noon");
        assert_eq!(time_seconds(parsed.time(Some(format)).unwrap(), format.time_pattern.as_deref()), crate::log::seconds("2024-05-01T12:00:01Z"));
    }
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
