//! Pure file-name pattern expander for generated changelogs and migrations. No clock, no file system: the caller
//! passes the current date, the existing sequence and the project version in.
//!
//! Documented originals (JetBrains help, "Changelog File Name Macros" and the Flyway "Version pattern"):
//! `#increment(<start>, <step>, <decimalFormat>)`, `#date(<simpleDateFormat>)` and `${semVer.<getterMethod>}` with
//! `getRawVersion()`, `getMajor()`, `getMinor()`, `getPatch()`, `getPreRelease()`. The help gives no example output
//! for `#increment` or `#date`, so the choices below are ours and are marked:
//! - `#increment`: the first file gets `start`, later ones the highest existing number plus `step`; `decimalFormat`
//!   is only runs of `0` (zero padding), nothing else of Java's DecimalFormat.
//! - `#date`: only `yyyy MM dd HH mm ss` with `-`, `_`, `.` between them (a small subset of SimpleDateFormat).
//! Our own tokens (not from the originals): `{seq}`, `{description}`, `{date}` (= `#date(yyyyMMdd)`).
//! Everything else is refused: unknown tokens, literals outside `A-Za-z0-9_.-`, output with `..`, a leading `.`,
//! characters outside that set, or more than `MAX_LEN` characters. Nothing is sanitized silently, with one documented
//! exception: the project version inserted by `${semVer.*}` keeps its SemVer build-metadata `+` (the help's own example
//! `1.2.3-SNAPSHOT+build4` yields `getRawVersion()` = the full string) and any other character outside the set is
//! replaced by `_` in that version component only, so such a version is usable instead of refused.

pub const MAX_LEN: usize = 120;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DateTime {
    pub year: u32,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
}

fn days_in_month(year: u32, month: u32) -> u32 {
    match month {
        2 if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

impl DateTime {
    pub fn new(year: u32, month: u32, day: u32, hour: u32, minute: u32, second: u32) -> Option<Self> {
        let valid = (1000..=9999).contains(&year) && (1..=12).contains(&month) && (1..=days_in_month(year, month)).contains(&day) && hour < 24 && minute < 60 && second < 60;
        valid.then_some(Self { year, month, day, hour, minute, second })
    }

    /// `yyyyMMdd` (midnight) or `yyyyMMddHHmmss`, digits only.
    pub fn parse_compact(text: &str) -> Option<Self> {
        if !text.bytes().all(|b| b.is_ascii_digit()) || !matches!(text.len(), 8 | 14) {
            return None;
        }
        let part = |from: usize, to: usize| text.get(from..to).map_or(Some(0), |digits| digits.parse().ok());
        Self::new(text[0..4].parse().ok()?, text[4..6].parse().ok()?, text[6..8].parse().ok()?, part(8, 10)?, part(10, 12)?, part(12, 14)?)
    }

    /// UTC civil time from seconds since the Unix epoch (Howard Hinnant's days-to-civil algorithm).
    pub fn from_unix(seconds: u64) -> Option<Self> {
        let days = (seconds / 86_400) as i64 + 719_468;
        let rest = (seconds % 86_400) as u32;
        let era = days.div_euclid(146_097);
        let day_of_era = days.rem_euclid(146_097);
        let year_of_era = (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
        let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
        let shifted_month = (5 * day_of_year + 2) / 153;
        let day = (day_of_year - (153 * shifted_month + 2) / 5 + 1) as u32;
        let month = if shifted_month < 10 { shifted_month + 3 } else { shifted_month - 9 } as u32;
        let year = (year_of_era + era * 400 + i64::from(month <= 2)) as u32;
        Self::new(year, month, day, rest / 3_600, rest % 3_600 / 60, rest % 60)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DescriptionStyle {
    /// Words joined by `_`, case kept (the behavior before patterns existed).
    Keep,
    LowerSnake,
    Kebab,
}

impl DescriptionStyle {
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "keep" => Some(Self::Keep),
            "lower_snake" => Some(Self::LowerSnake),
            "kebab" => Some(Self::Kebab),
            _ => None,
        }
    }
}

/// ASCII letters and digits of `text` as words (at most 100 characters in all), joined per `style`.
pub fn style_description(text: &str, style: DescriptionStyle) -> String {
    let words: Vec<&str> = text.split(|c: char| !c.is_ascii_alphanumeric()).filter(|w| !w.is_empty()).collect();
    let joined = words.join(if style == DescriptionStyle::Kebab { "-" } else { "_" });
    let joined = if style == DescriptionStyle::Keep { joined } else { joined.to_ascii_lowercase() };
    joined.chars().take(100).collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DatePart {
    Year,
    Month,
    Day,
    Hour,
    Minute,
    Second,
    Separator(char),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SemVerPart {
    Raw,
    Major,
    Minor,
    Patch,
    PreRelease,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Token {
    Literal(String),
    Seq,
    Description,
    Date(Vec<DatePart>),
    Increment { start: u64, step: u64, width: usize },
    SemVer(SemVerPart),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NamingError {
    Empty,
    TooLong,
    Unclosed(&'static str),
    UnknownToken(String),
    BadArguments { token: &'static str, reason: &'static str },
    UnsafeLiteral(char),
    Missing(&'static str),
    UnsafeOutput(&'static str),
}

impl std::fmt::Display for NamingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => write!(f, "the pattern is empty"),
            Self::TooLong => write!(f, "the pattern or the name is longer than {MAX_LEN} characters"),
            Self::Unclosed(what) => write!(f, "{what} is not closed"),
            Self::UnknownToken(token) => write!(f, "unknown token {token:?}"),
            Self::BadArguments { token, reason } => write!(f, "{token}: {reason}"),
            Self::UnsafeLiteral(c) => write!(f, "{c:?} is not allowed in a name (only ASCII letters, digits, '.', '_' and '-')"),
            Self::Missing(what) => write!(f, "{what} is needed by the pattern but is not available"),
            Self::UnsafeOutput(why) => write!(f, "the expanded name is not usable: {why}"),
        }
    }
}

/// What a pattern needs from its caller.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Needs {
    pub seq: bool,
    pub increment: bool,
    pub description: bool,
    pub date: bool,
    pub sem_ver: bool,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Inputs<'a> {
    /// Value of `{seq}`, already computed by the caller's own convention.
    pub seq: Option<&'a str>,
    /// Highest number already used, for `#increment`; `None` when there is none.
    pub highest: Option<u64>,
    /// Value of `{description}`, already styled.
    pub description: Option<&'a str>,
    pub now: Option<DateTime>,
    /// The project version for `${semVer.*}`.
    pub sem_ver: Option<&'a str>,
}

fn name_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-')
}

fn date_parts(format: &str) -> Result<Vec<DatePart>, NamingError> {
    let bad = |reason| Err(NamingError::BadArguments { token: "date", reason });
    let mut parts = Vec::new();
    let mut rest = format;
    while !rest.is_empty() {
        let (part, len) = match &rest[..rest.len().min(4)] {
            s if s.starts_with("yyyy") => (DatePart::Year, 4),
            s if s.starts_with("MM") => (DatePart::Month, 2),
            s if s.starts_with("dd") => (DatePart::Day, 2),
            s if s.starts_with("HH") => (DatePart::Hour, 2),
            s if s.starts_with("mm") => (DatePart::Minute, 2),
            s if s.starts_with("ss") => (DatePart::Second, 2),
            s if s.starts_with(['-', '_', '.']) => (DatePart::Separator(s.chars().next().expect("non-empty")), 1),
            _ => return bad("only yyyy, MM, dd, HH, mm, ss and the separators - _ . are supported"),
        };
        if parts.contains(&part) && !matches!(part, DatePart::Separator(_)) {
            return bad("each field may appear once");
        }
        parts.push(part);
        rest = &rest[len..];
    }
    if !parts.iter().any(|part| !matches!(part, DatePart::Separator(_))) {
        return bad("the format has no date field");
    }
    Ok(parts)
}

fn increment(arguments: &str) -> Result<Token, NamingError> {
    let bad = |reason| Err(NamingError::BadArguments { token: "increment", reason });
    let fields: Vec<&str> = arguments.split(',').map(str::trim).collect();
    let [start, step, format] = fields.as_slice() else {
        return bad("expected (start, step, decimalFormat)");
    };
    let number = |text: &str| text.len() <= 9 && !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit());
    if !number(start) || !number(step) {
        return bad("start and step are plain non-negative numbers of at most 9 digits");
    }
    let (start, step): (u64, u64) = (start.parse().expect("digits"), step.parse().expect("digits"));
    if step == 0 {
        return bad("step must be at least 1");
    }
    if format.is_empty() || format.len() > 12 || !format.bytes().all(|b| b == b'0') {
        return bad("decimalFormat is only zeros, 1 to 12 of them (zero padding)");
    }
    Ok(Token::Increment { start, step, width: format.len() })
}

fn parse(pattern: &str) -> Result<Vec<Token>, NamingError> {
    if pattern.is_empty() {
        return Err(NamingError::Empty);
    }
    if pattern.chars().count() > MAX_LEN {
        return Err(NamingError::TooLong);
    }
    let mut tokens = Vec::new();
    let mut rest = pattern;
    while let Some(c) = rest.chars().next() {
        if let Some(after) = rest.strip_prefix('{') {
            let end = after.find('}').ok_or(NamingError::Unclosed("{"))?;
            tokens.push(match &after[..end] {
                "seq" => Token::Seq,
                "description" => Token::Description,
                "date" => Token::Date(date_parts("yyyyMMdd")?),
                other => return Err(NamingError::UnknownToken(format!("{{{other}}}"))),
            });
            rest = &after[end + 1..];
        } else if let Some(after) = rest.strip_prefix("#date(") {
            let end = after.find(')').ok_or(NamingError::Unclosed("#date("))?;
            tokens.push(Token::Date(date_parts(&after[..end])?));
            rest = &after[end + 1..];
        } else if let Some(after) = rest.strip_prefix("#increment(") {
            let end = after.find(')').ok_or(NamingError::Unclosed("#increment("))?;
            tokens.push(increment(&after[..end])?);
            rest = &after[end + 1..];
        } else if let Some(after) = rest.strip_prefix("${semVer.") {
            let end = after.find('}').ok_or(NamingError::Unclosed("${semVer."))?;
            tokens.push(Token::SemVer(match &after[..end] {
                "getRawVersion()" => SemVerPart::Raw,
                "getMajor()" => SemVerPart::Major,
                "getMinor()" => SemVerPart::Minor,
                "getPatch()" => SemVerPart::Patch,
                "getPreRelease()" => SemVerPart::PreRelease,
                other => return Err(NamingError::UnknownToken(format!("${{semVer.{other}}}"))),
            }));
            rest = &after[end + 1..];
        } else if matches!(c, '#' | '$' | '{' | '}') {
            let token: String = rest.chars().take_while(|c| !c.is_whitespace()).take(24).collect();
            return Err(NamingError::UnknownToken(token));
        } else if name_char(c) {
            let end = rest.find(|c: char| !name_char(c)).unwrap_or(rest.len());
            tokens.push(Token::Literal(rest[..end].to_owned()));
            rest = &rest[end..];
        } else {
            return Err(NamingError::UnsafeLiteral(c));
        }
    }
    Ok(tokens)
}

/// Validates the pattern and reports what it needs; no values are involved.
pub fn needs(pattern: &str) -> Result<Needs, NamingError> {
    let mut needs = Needs::default();
    for token in parse(pattern)? {
        match token {
            Token::Seq => needs.seq = true,
            Token::Description => needs.description = true,
            Token::Date(_) => needs.date = true,
            Token::Increment { .. } => needs.increment = true,
            Token::SemVer(_) => needs.sem_ver = true,
            Token::Literal(_) => {}
        }
    }
    Ok(needs)
}

fn sanitized_version_text(text: &str) -> String {
    text.chars().map(|c| if name_char(c) || c == '+' { c } else { '_' }).collect()
}

fn sem_ver_part(version: &str, part: SemVerPart) -> Result<String, NamingError> {
    if part == SemVerPart::Raw {
        return Ok(sanitized_version_text(version));
    }
    let bad = NamingError::Missing("a semantic version (major.minor.patch with optional -pre-release and +build)");
    let (core, _build) = version.split_once('+').unwrap_or((version, ""));
    let (numbers, pre) = core.split_once('-').map_or((core, None), |(n, p)| (n, Some(p)));
    let numbers: Vec<&str> = numbers.split('.').collect();
    let [major, minor, patch] = numbers.as_slice() else {
        return Err(bad);
    };
    if ![major, minor, patch].iter().all(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit())) {
        return Err(bad);
    }
    match part {
        SemVerPart::Major => Ok((*major).to_owned()),
        SemVerPart::Minor => Ok((*minor).to_owned()),
        SemVerPart::Patch => Ok((*patch).to_owned()),
        SemVerPart::PreRelease => pre.map(str::to_owned).ok_or(NamingError::Missing("a pre-release part in the project version")),
        SemVerPart::Raw => unreachable!("handled above"),
    }
}

/// Expands `pattern`; deterministic for the same inputs.
pub fn expand(pattern: &str, inputs: &Inputs) -> Result<String, NamingError> {
    let mut out = String::new();
    let tokens = parse(pattern)?;
    let plus_allowed = tokens.iter().any(|token| matches!(token, Token::SemVer(SemVerPart::Raw)));
    for token in tokens {
        match token {
            Token::Literal(text) => out.push_str(&text),
            Token::Seq => out.push_str(inputs.seq.ok_or(NamingError::Missing("a sequence"))?),
            Token::Description => out.push_str(inputs.description.ok_or(NamingError::Missing("a description"))?),
            Token::Date(parts) => {
                let now = inputs.now.ok_or(NamingError::Missing("the date"))?;
                for part in parts {
                    match part {
                        DatePart::Year => out.push_str(&format!("{:04}", now.year)),
                        DatePart::Month => out.push_str(&format!("{:02}", now.month)),
                        DatePart::Day => out.push_str(&format!("{:02}", now.day)),
                        DatePart::Hour => out.push_str(&format!("{:02}", now.hour)),
                        DatePart::Minute => out.push_str(&format!("{:02}", now.minute)),
                        DatePart::Second => out.push_str(&format!("{:02}", now.second)),
                        DatePart::Separator(c) => out.push(c),
                    }
                }
            }
            Token::Increment { start, step, width } => {
                let value = inputs.highest.map_or(Some(start), |highest| highest.checked_add(step)).ok_or(NamingError::UnsafeOutput("the sequence overflows"))?;
                out.push_str(&format!("{value:0width$}"));
            }
            Token::SemVer(part) => out.push_str(&sem_ver_part(inputs.sem_ver.ok_or(NamingError::Missing("the project version"))?, part)?),
        }
    }
    let unsafe_output = |why| Err(NamingError::UnsafeOutput(why));
    if out.is_empty() {
        return unsafe_output("it is empty");
    }
    if out.chars().count() > MAX_LEN {
        return Err(NamingError::TooLong);
    }
    if !out.chars().all(|c| name_char(c) || (plus_allowed && c == '+')) {
        return unsafe_output("it contains a character outside ASCII letters, digits, '.', '_' and '-'");
    }
    if out.starts_with('.') || out.contains("..") {
        return unsafe_output("it starts with '.' or contains '..'");
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn when() -> DateTime {
        DateTime::new(2025, 3, 9, 4, 5, 6).unwrap()
    }

    fn inputs() -> Inputs<'static> {
        Inputs { seq: Some("003"), highest: Some(7), description: Some("add_email"), now: Some(when()), sem_ver: Some("1.2.3-SNAPSHOT+build4") }
    }

    fn expanded(pattern: &str) -> String {
        expand(pattern, &inputs()).unwrap()
    }

    #[test]
    fn default_style_patterns() {
        assert_eq!(expanded("{seq}-{description}.xml"), "003-add_email.xml");
        assert_eq!(expanded("V{seq}__{description}.sql"), "V003__add_email.sql");
    }

    #[test]
    fn date_tokens() {
        assert_eq!(expanded("{date}"), "20250309");
        assert_eq!(expanded("#date(yyyyMMdd)"), "20250309");
        assert_eq!(expanded("#date(yyyyMMddHHmmss)"), "20250309040506");
        assert_eq!(expanded("V#date(yyyy.MM.dd_HH.mm)__x"), "V2025.03.09_04.05__x");
        assert_eq!(expanded("#date(yyyy-MM-dd)-{description}"), "2025-03-09-add_email");
        for bad in ["#date()", "#date(yyyyMMdd", "#date(YYYY)", "#date(yyyy/MM)", "#date(-)", "#date(yy)", "#date(yyyy MM)", "#date(MMMM)"] {
            assert!(expand(bad, &inputs()).is_err(), "{bad}");
        }
        let none = Inputs { now: None, ..inputs() };
        assert_eq!(expand("{date}", &none), Err(NamingError::Missing("the date")));
    }

    #[test]
    fn increment_token() {
        assert_eq!(expanded("#increment(1, 1, 000)"), "008");
        assert_eq!(expanded("#increment(10,10,0)"), "17");
        let first = Inputs { highest: None, ..inputs() };
        assert_eq!(expand("#increment(100, 5, 0000)", &first).unwrap(), "0100");
        for bad in ["#increment", "#increment()", "#increment(1,1)", "#increment(1,0,0)", "#increment(a,1,0)", "#increment(1,1,#.##)", "#increment(1,1,)", "#increment(1,1,0,0)", "#increment(-1,1,0)", "#increment(1234567890,1,0)", "#increment(1,1,0000000000000)"] {
            assert!(expand(bad, &inputs()).is_err(), "{bad}");
        }
        let top = Inputs { highest: Some(u64::MAX), ..inputs() };
        assert!(expand("#increment(1,1,0)", &top).is_err());
    }

    #[test]
    fn sem_ver_tokens() {
        assert_eq!(expanded("${semVer.getMajor()}"), "1");
        assert_eq!(expanded("${semVer.getMinor()}"), "2");
        assert_eq!(expanded("V${semVer.getMajor()}.${semVer.getMinor()}.${semVer.getPatch()}"), "V1.2.3");
        assert_eq!(expanded("${semVer.getPreRelease()}"), "SNAPSHOT");
        assert_eq!(expanded("${semVer.getRawVersion()}"), "1.2.3-SNAPSHOT+build4", "build metadata is kept");
        assert_eq!(expanded("${semVer.getRawVersion()}_${semVer.getMajor()}_${semVer.getPatch()}_${semVer.getPreRelease()}"), "1.2.3-SNAPSHOT+build4_1_3_SNAPSHOT");
        let odd_chars = Inputs { sem_ver: Some("1.2.3 beta/2"), ..inputs() };
        assert_eq!(expand("${semVer.getRawVersion()}", &odd_chars).unwrap(), "1.2.3_beta_2");
        assert!(expand("a+b", &inputs()).is_err(), "a literal '+' stays refused");
        assert!(expand("${semVer.getRawVersion()}", &Inputs { sem_ver: Some("../x"), ..inputs() }).is_err(), "traversal stays refused");
        let plain = Inputs { sem_ver: Some("2.0.1"), ..inputs() };
        assert_eq!(expand("${semVer.getRawVersion()}", &plain).unwrap(), "2.0.1");
        assert_eq!(expand("${semVer.getPreRelease()}", &plain), Err(NamingError::Missing("a pre-release part in the project version")));
        for version in ["1.2", "a.b.c", "1.2.3.4", "", "1..3"] {
            let odd = Inputs { sem_ver: Some(version), ..inputs() };
            assert!(expand("${semVer.getMajor()}", &odd).is_err(), "{version}");
        }
        let none = Inputs { sem_ver: None, ..inputs() };
        assert!(expand("${semVer.getMajor()}", &none).is_err());
        for bad in ["${semVer.getMajor}", "${semVer.getMajor()", "${semVer.getBuild()}", "${semVer}", "${other}", "${semVer.getMajor().x}"] {
            assert!(expand(bad, &inputs()).is_err(), "{bad}");
        }
    }

    #[test]
    fn unknown_tokens_and_unsafe_literals_are_refused() {
        assert_eq!(expand("{nope}", &inputs()), Err(NamingError::UnknownToken("{nope}".into())));
        assert!(matches!(expand("#hash", &inputs()), Err(NamingError::UnknownToken(_))));
        assert!(matches!(expand("a}b", &inputs()), Err(NamingError::UnknownToken(_))));
        assert!(matches!(expand("{seq", &inputs()), Err(NamingError::Unclosed(_))));
        for bad in ["a/b", "a\\b", "a b", "a\nb", "a\0b", "é", "a:b", "a*b", "a~b"] {
            assert!(matches!(expand(bad, &inputs()), Err(NamingError::UnsafeLiteral(_))), "{bad:?}");
        }
        assert_eq!(expand("", &inputs()), Err(NamingError::Empty));
    }

    #[test]
    fn output_is_bounded_and_filename_safe() {
        assert!(matches!(expand("a..b", &inputs()), Err(NamingError::UnsafeOutput(_))));
        assert!(matches!(expand(".hidden", &inputs()), Err(NamingError::UnsafeOutput(_))));
        assert!(matches!(expand("{seq}..{seq}", &inputs()), Err(NamingError::UnsafeOutput(_))));
        assert_eq!(expand(&"a".repeat(121), &inputs()), Err(NamingError::TooLong));
        let long = Inputs { description: Some(&"d".repeat(100)), ..inputs() };
        assert_eq!(expand("{description}{description}", &long), Err(NamingError::TooLong));
        let sneaky = Inputs { description: Some("a/b"), ..inputs() };
        assert!(matches!(expand("{description}", &sneaky), Err(NamingError::UnsafeOutput(_))));
        let control = Inputs { seq: Some("1\n2"), ..inputs() };
        assert!(expand("{seq}", &control).is_err());
        assert_eq!(expand("{seq}", &Inputs { seq: None, ..inputs() }), Err(NamingError::Missing("a sequence")));
        assert_eq!(expanded("{seq}-{description}"), expanded("{seq}-{description}"));
    }

    #[test]
    fn needs_reports_each_token_kind() {
        let n = needs("#date(yyyy)-{seq}-{description}-#increment(1,1,0)-${semVer.getMajor()}").unwrap();
        assert!(n.date && n.seq && n.description && n.increment && n.sem_ver);
        assert_eq!(needs("V1__x.sql").unwrap(), Needs::default());
        assert!(needs("{x}").is_err());
    }

    #[test]
    fn dates_and_description_styles() {
        assert_eq!(DateTime::parse_compact("20240229"), DateTime::new(2024, 2, 29, 0, 0, 0));
        assert_eq!(DateTime::parse_compact("20240229235959"), DateTime::new(2024, 2, 29, 23, 59, 59));
        for bad in ["20230229", "20241301", "2024022", "20240229240000", "2024-02-29", "20240229235960", "00000101", "20240229 12"] {
            assert_eq!(DateTime::parse_compact(bad), None, "{bad}");
        }
        assert_eq!(DateTime::from_unix(0), DateTime::new(1970, 1, 1, 0, 0, 0));
        assert_eq!(DateTime::from_unix(1_709_251_199), DateTime::new(2024, 2, 29, 23, 59, 59));
        assert_eq!(DateTime::from_unix(1_741_493_106), DateTime::new(2025, 3, 9, 4, 5, 6));
        assert_eq!(style_description("Add customer E-mail!", DescriptionStyle::Keep), "Add_customer_E_mail");
        assert_eq!(style_description("Add customer E-mail!", DescriptionStyle::LowerSnake), "add_customer_e_mail");
        assert_eq!(style_description("Add customer E-mail!", DescriptionStyle::Kebab), "add-customer-e-mail");
        assert_eq!(style_description("!!!", DescriptionStyle::Kebab), "");
        assert_eq!(style_description(&"x".repeat(300), DescriptionStyle::Keep).len(), 100);
        assert_eq!(DescriptionStyle::parse("kebab"), Some(DescriptionStyle::Kebab));
        assert_eq!(DescriptionStyle::parse("Kebab"), None);
    }
}
