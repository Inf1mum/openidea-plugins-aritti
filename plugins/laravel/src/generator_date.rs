//! Bounded Gregorian UTC SimpleDateFormat-compatible formatting. English US
//! symbols/week rules are fixed so generated source never depends on UI locale.
#[derive(Clone, Copy, Debug)]
struct Date {
    year: i32,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
    second: u32,
}
fn leap(year: i32) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}
fn month_days(year: i32, month: u32) -> u32 {
    match month {
        2 => {
            if leap(year) {
                29
            } else {
                28
            }
        }
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}
fn days(year: i32, month: u32, day: u32) -> i64 {
    let y = i64::from(year) - i64::from(month <= 2);
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let m = i64::from(month);
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + i64::from(day) - 1;
    era * 146097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719468
}
fn civil(day: i64) -> (i32, u32, u32) {
    let z = day + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    ((y + i64::from(m <= 2)) as i32, m as u32, d as u32)
}
impl Date {
    fn parse(text: &str) -> Result<Self, String> {
        let b = text.as_bytes();
        if b.len() != 19
            || b[4] != b'-'
            || b[7] != b'-'
            || b[10] != b' '
            || b[13] != b':'
            || b[16] != b':'
            || b.iter()
                .enumerate()
                .any(|(i, b)| ![4, 7, 10, 13, 16].contains(&i) && !b.is_ascii_digit())
        {
            return Err("Date inputs must use yyyy-MM-dd HH:mm:ss in UTC".into());
        }
        let number = |range: std::ops::Range<usize>| text[range].parse::<u32>().unwrap();
        let date = Self {
            year: number(0..4) as i32,
            month: number(5..7),
            day: number(8..10),
            hour: number(11..13),
            minute: number(14..16),
            second: number(17..19),
        };
        if !(1..=9999).contains(&date.year)
            || !(1..=12).contains(&date.month)
            || date.day == 0
            || date.day > month_days(date.year, date.month)
            || date.hour > 23
            || date.minute > 59
            || date.second > 59
        {
            return Err("Invalid Gregorian UTC date".into());
        }
        Ok(date)
    }
    fn weekday(self) -> u32 {
        (days(self.year, self.month, self.day) + 4).rem_euclid(7) as u32
    }
    fn day_of_year(self) -> u32 {
        (days(self.year, self.month, self.day) - days(self.year, 1, 1) + 1) as u32
    }
    fn week(self) -> (i32, u32) {
        let day = days(self.year, self.month, self.day);
        let year_start = |year| {
            let jan = days(year, 1, 1);
            jan - (jan + 4).rem_euclid(7)
        };
        let next = year_start(self.year + 1);
        if day >= next {
            (self.year + 1, 1)
        } else {
            (self.year, ((day - year_start(self.year)) / 7 + 1) as u32)
        }
    }
}
pub(super) fn validate(text: &str) -> Result<(), String> {
    Date::parse(text).map(|_| ())
}
pub(super) fn captured_now() -> Result<String, String> {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| "System UTC clock predates the supported epoch")?
        .as_secs();
    if seconds > 253402300799 {
        return Err("System UTC clock exceeds year 9999".into());
    }
    let (year, month, day) = civil((seconds / 86400) as i64);
    let hour = seconds / 3600 % 24;
    let minute = seconds / 60 % 60;
    let second = seconds % 60;
    Ok(format!(
        "{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}:{second:02}"
    ))
}
pub(super) fn format(text: &str, pattern: &str) -> Result<String, String> {
    if pattern.len() > 1024 {
        return Err("Date patterns exceed the 1024-byte limit".into());
    }
    let date = Date::parse(text)?;
    let chars = pattern.chars().collect::<Vec<_>>();
    let mut at = 0;
    let mut quoted = false;
    let mut result = String::new();
    const MONTHS: [&str; 12] = [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ];
    const WEEKDAYS: [&str; 7] = [
        "Sunday",
        "Monday",
        "Tuesday",
        "Wednesday",
        "Thursday",
        "Friday",
        "Saturday",
    ];
    while at < chars.len() {
        let ch = chars[at];
        if ch == '\'' {
            if chars.get(at + 1) == Some(&'\'') {
                result.push('\'');
                at += 2;
                continue;
            }
            quoted = !quoted;
            at += 1;
            continue;
        }
        if quoted || !ch.is_ascii_alphabetic() {
            result.push(ch);
            at += 1;
            continue;
        }
        let start = at;
        while chars.get(at) == Some(&ch) {
            at += 1;
        }
        let width = at - start;
        if width > 64 {
            return Err("Repeated date pattern fields exceed the 64-character limit".into());
        }
        let number = |value: u32| format!("{value:0width$}");
        let part = match ch {
            'G' => "AD".into(),
            'y' | 'Y' => {
                let year = if ch == 'Y' { date.week().0 } else { date.year } as u32;
                number(if width == 2 { year % 100 } else { year })
            }
            'M' | 'L' if width >= 3 => {
                let name = MONTHS[(date.month - 1) as usize];
                if width == 3 {
                    name[..3].into()
                } else {
                    name.into()
                }
            }
            'M' | 'L' => number(date.month),
            'w' => number(date.week().1),
            'W' => {
                let first = (days(date.year, date.month, 1) + 4).rem_euclid(7) as u32;
                number((date.day + first - 1) / 7 + 1)
            }
            'D' => number(date.day_of_year()),
            'd' => number(date.day),
            'F' => number((date.day - 1) / 7 + 1),
            'E' => {
                let name = WEEKDAYS[date.weekday() as usize];
                if width >= 4 {
                    name.into()
                } else {
                    name[..3].into()
                }
            }
            'u' => number((date.weekday() + 6) % 7 + 1),
            'a' => {
                if date.hour < 12 {
                    "AM".into()
                } else {
                    "PM".into()
                }
            }
            'H' => number(date.hour),
            'k' => number(if date.hour == 0 { 24 } else { date.hour }),
            'K' => number(date.hour % 12),
            'h' => number(if date.hour % 12 == 0 {
                12
            } else {
                date.hour % 12
            }),
            'm' => number(date.minute),
            's' => number(date.second),
            'S' => number(0),
            'z' => {
                if width >= 4 {
                    "Coordinated Universal Time".into()
                } else {
                    "UTC".into()
                }
            }
            'Z' => "+0000".into(),
            'X' if width <= 3 => "Z".into(),
            _ => return Err(format!("Unsupported SimpleDateFormat pattern field {ch}")),
        };
        result.push_str(&part);
        if result.len() > 4096 {
            return Err("Formatted date exceeds 4096 bytes".into());
        }
    }
    if quoted {
        return Err("Unclosed quoted date pattern literal".into());
    }
    Ok(result)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn utc_examples_cover_java_symbols_padding_quotes_and_week_boundaries() {
        assert_eq!(
            format(
                "2004-02-29 00:07:09",
                "yyyy-MM-dd'T'HH:mm:ss.SSSXXX EEEE MMMM G"
            )
            .unwrap(),
            "2004-02-29T00:07:09.000Z Sunday February AD"
        );
        assert_eq!(
            format(
                "2001-07-04 12:08:56",
                "EEE, MMM d, ''yy | hh 'o''clock' a, zzzz"
            )
            .unwrap(),
            "Wed, Jul 4, '01 | 12 o'clock PM, Coordinated Universal Time"
        );
        assert_eq!(
            format(
                "2021-12-26 23:59:58",
                "yyyy YYYY ww W D F u k K h Z X XX XXX"
            )
            .unwrap(),
            "2021 2022 01 5 360 4 7 23 11 11 +0000 Z Z Z"
        );
        assert_eq!(
            format("2024-01-01 00:00:00", "y yy yyyyy M MM MMM MMMMM ddd k K h").unwrap(),
            "2024 24 02024 1 01 Jan January 001 24 0 12"
        );
    }
    #[test]
    fn malformed_dates_patterns_and_work_growth_are_refused() {
        for date in [
            "2023-02-29 00:00:00",
            "2000-13-01 00:00:00",
            "2024-01-01 24:00:00",
            "0000-01-01 00:00:00",
            "2024-01-01T00:00:00",
        ] {
            assert!(format(date, "yyyy").is_err());
        }
        for pattern in ["yyyy 'open", "XXXX", "q"] {
            assert!(format("2024-01-01 00:00:00", pattern).is_err());
        }
        assert!(format("2024-01-01 00:00:00", &"y".repeat(1025)).is_err());
        assert!(captured_now().and_then(|text| validate(&text)).is_ok());
    }
    #[test]
    #[ignore = "requires installed Java source launcher; compares independent SimpleDateFormat UTC/US results"]
    fn installed_java_simple_date_format_matches_all_supported_fields() {
        let directory = std::env::temp_dir().join(format!(
            "laravel-date-oracle-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&directory).unwrap();
        let source = directory.join("DateOracle.java");
        std::fs::write(&source,r#"import java.text.*;import java.util.*;class DateOracle {public static void main(String[] args)throws Exception{TimeZone zone=TimeZone.getTimeZone("UTC");SimpleDateFormat input=new SimpleDateFormat("yyyy-MM-dd HH:mm:ss",Locale.US);input.setTimeZone(zone);input.setLenient(false);for(int i=0;i<args.length;i+=2){SimpleDateFormat format=new SimpleDateFormat(args[i+1],Locale.US);format.setTimeZone(zone);System.out.println(format.format(input.parse(args[i])));}}}"#).unwrap();
        let dates = [
            "1999-12-31 23:59:59",
            "2000-02-29 00:07:09",
            "2004-07-04 12:08:56",
            "2021-12-26 23:59:58",
            "2022-01-01 00:00:00",
            "2023-01-01 00:00:00",
            "2024-12-31 11:05:06",
        ];
        let mut cases = Vec::new();
        for date in dates {
            for width in 1..=5 {
                let pattern = "GyYMLwWDdFEuaHkKhmsSzZX"
                    .chars()
                    .filter(|ch| *ch != 'X' || width <= 3)
                    .map(|ch| ch.to_string().repeat(width))
                    .collect::<Vec<_>>()
                    .join(" / ");
                cases.push((date, pattern));
            }
            cases.push((
                date,
                "yyyy-MM-dd'T'HH:mm:ss.SSSXXX 'o''clock' '' GGGG zzzz".into(),
            ));
        }
        let mut command = std::process::Command::new("java");
        command.arg(&source);
        for (date, pattern) in &cases {
            command.arg(date).arg(pattern);
        }
        let output = command
            .output()
            .expect("Install a Java source launcher for this opt-in oracle");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let text = String::from_utf8(output.stdout).unwrap();
        let actual = text.lines().collect::<Vec<_>>();
        assert_eq!(actual.len(), cases.len());
        for ((date, pattern), expected) in cases.iter().zip(actual) {
            assert_eq!(
                format(date, pattern).unwrap(),
                expected,
                "{date} :: {pattern}"
            );
        }
        std::fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn calendar_roundtrips_leap_centuries_and_epoch() {
        for text in [
            "0001-01-01 00:00:00",
            "1900-03-01 00:00:00",
            "2000-02-29 00:00:00",
            "1970-01-01 00:00:00",
            "9999-12-31 00:00:00",
        ] {
            let d = Date::parse(text).unwrap();
            assert_eq!(
                civil(days(d.year, d.month, d.day)),
                (d.year, d.month, d.day)
            );
        }
    }
}
