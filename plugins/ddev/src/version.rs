//! Numeric comparison from upstream Version.java: suffixes do not cause an
//! update notice for a HEAD/RC build with the same release numbers.
use openidea_plugin::Error;
use std::fmt;
pub struct Version {
    text: String,
    numbers: Vec<u64>,
}
impl Version {
    pub fn parse(text: &str) -> Result<Self, Error> {
        let text = text.trim().trim_start_matches('v');
        let core = text.split(['-', '+']).next().unwrap_or("");
        let numbers: Result<Vec<u64>, _> = core
            .split('.')
            .map(|part| {
                if part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()) {
                    return Err("Invalid DDEV version.".into());
                }
                part.parse::<u64>()
                    .map_err(|_| Error::from("Invalid DDEV version."))
            })
            .collect();
        Ok(Self {
            text: text.into(),
            numbers: numbers?,
        })
    }
    pub fn newer_than(&self, other: &Self) -> bool {
        (0..self.numbers.len().max(other.numbers.len()))
            .map(|i| self.numbers.get(i).copied().unwrap_or(0))
            .cmp(
                (0..self.numbers.len().max(other.numbers.len()))
                    .map(|i| other.numbers.get(i).copied().unwrap_or(0)),
            )
            .is_gt()
    }
}
impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.text)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn upstream_version_comparison_cases() {
        for (installed, latest, expected) in [
            ("1.0.0", "2.0.0", true),
            ("1.0.0", "1.1.0", true),
            ("1.0.0", "1.0.1", true),
            ("1.0.0", "1.0.0", false),
            ("v1.19.0-rc1", "1.18.9", false),
            ("1.0.0", "0.9.0", false),
            ("v1.24.4-36-ge74e3a95f", "v1.24.4", false),
            ("v1.24.4-36-ge74e3a95f", "v1.24.5", true),
            ("v1.25.0-rc1", "v1.25.0", false),
            ("1.2", "1.2.0", false),
            ("1.2", "1.2.0.1", true),
        ] {
            assert_eq!(
                Version::parse(latest)
                    .unwrap()
                    .newer_than(&Version::parse(installed).unwrap()),
                expected,
                "{installed} -> {latest}"
            );
        }
        for bad in [
            "",
            "v",
            "hello",
            "1..2",
            "1.2.x",
            "1.2.999999999999999999999",
        ] {
            assert!(Version::parse(bad).is_err());
        }
    }
}
