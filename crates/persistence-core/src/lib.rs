//! Shared source-model and draft contracts. Unknown facts are never absence facts.
use serde::{Deserialize, Serialize};

pub mod ddl_parse;
pub mod generation;
pub mod introspect;
pub mod introspect_map;
pub mod liquibase;
pub mod naming;
pub mod schema;
pub mod sql;

pub const PROTOCOL_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Language {
    Java,
    Kotlin,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PersistenceRole {
    Entity,
    Embeddable,
    MappedSuperclass,
}

impl PersistenceRole {
    pub fn annotation(&self) -> &'static str {
        match self {
            Self::Entity => "Entity",
            Self::Embeddable => "Embeddable",
            Self::MappedSuperclass => "MappedSuperclass",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Resolution {
    SourceDeclared,
    CompilerResolved,
    ProviderResolved,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceIdentity {
    pub root: String,
    pub path: String,
    pub language: Language,
    pub version: u64,
}

impl SourceIdentity {
    /// Lexical confinement only; callers must independently verify current host ownership.
    pub fn is_confined(&self) -> bool {
        let clean = |s: &str| {
            s.starts_with('/')
                && !s.contains(['\\', '\0'])
                && !s.split('/').any(|p| matches!(p, "." | ".."))
        };
        clean(&self.root)
            && clean(&self.path)
            && self.root != self.path
            && (self.root == "/"
                || self
                    .path
                    .strip_prefix(self.root.trim_end_matches('/'))
                    .is_some_and(|p| p.starts_with('/')))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TypeIdentity {
    pub qualified_name: String,
    pub source_group: String,
    pub source: SourceIdentity,
    pub role: PersistenceRole,
    pub resolution: Resolution,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PersistenceSnapshot {
    pub protocol: u32,
    pub revision: u64,
    pub root: String,
    /// This is a positive inventory, not proof that all project types were indexed.
    pub types: Vec<TypeIdentity>,
    pub complete: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ColumnProperty {
    Name,
    Nullable,
    Unique,
    Insertable,
    Updatable,
    Length,
    Precision,
    Scale,
}

impl ColumnProperty {
    pub fn argument(self) -> &'static str {
        match self {
            Self::Name => "name",
            Self::Nullable => "nullable",
            Self::Unique => "unique",
            Self::Insertable => "insertable",
            Self::Updatable => "updatable",
            Self::Length => "length",
            Self::Precision => "precision",
            Self::Scale => "scale",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ColumnValue {
    Boolean(bool),
    Number(u32),
    Text(String),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ColumnChange {
    pub property: ColumnProperty,
    pub value: ColumnValue,
}

impl ColumnChange {
    pub fn literal(&self) -> Result<String, &'static str> {
        use ColumnProperty::*;
        use ColumnValue::*;
        match (&self.property, &self.value) {
            (Name, Text(s))
                if s.len() <= 128
                    && s.bytes().enumerate().all(|(i, b)| {
                        b.is_ascii_alphabetic()
                            || b == b'_'
                            || i > 0 && (b.is_ascii_digit() || b == b'$')
                    }) =>
            {
                Ok(format!("\"{s}\""))
            }
            (Nullable | Unique | Insertable | Updatable, Boolean(b)) => Ok(b.to_string()),
            (Length, Number(n)) if *n > 0 && *n <= i32::MAX as u32 => Ok(n.to_string()),
            (Precision | Scale, Number(n)) if *n <= i32::MAX as u32 => Ok(n.to_string()),
            _ => Err("Choose a valid literal column value."),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceEdit {
    pub start: usize,
    pub end: usize,
    pub replacement: String,
}

/// Applies byte-based edits without splitting UTF-8, overlapping or choosing an
/// order for ambiguous insertions. The host adapter converts ranges to UTF-16.
pub fn apply_edits(source: &str, edits: &[SourceEdit]) -> Result<String, &'static str> {
    let mut edits: Vec<_> = edits.iter().collect();
    edits.sort_by_key(|e| (e.start, e.end));
    let mut previous: Option<&SourceEdit> = None;
    for edit in &edits {
        if edit.start > edit.end
            || edit.end > source.len()
            || !source.is_char_boundary(edit.start)
            || !source.is_char_boundary(edit.end)
            || previous.is_some_and(|p| p.end > edit.start || p.start == edit.start)
        {
            return Err("Source edit ranges are invalid or overlap.");
        }
        previous = Some(edit);
    }
    let mut after = source.to_owned();
    for edit in edits.into_iter().rev() {
        after.replace_range(edit.start..edit.end, &edit.replacement);
    }
    Ok(after)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn confinement_does_not_confuse_siblings_and_rejects_dot_paths() {
        let mut id = SourceIdentity {
            root: "/project".into(),
            path: "/project/src/A.java".into(),
            language: Language::Java,
            version: 1,
        };
        assert!(id.is_confined());
        for path in [
            "/project-other/A.java",
            "/project/../other/A.java",
            "/project",
            "/project/a\\b",
        ] {
            id.path = path.into();
            assert!(!id.is_confined(), "{path}");
        }
    }
    #[test]
    fn byte_edits_preserve_unicode_and_reject_ambiguous_ranges() {
        let edit = SourceEdit {
            start: 4,
            end: 8,
            replacement: "title".into(),
        };
        assert_eq!(apply_edits("😀name", &[edit.clone()]).unwrap(), "😀title");
        assert!(
            apply_edits(
                "😀name",
                &[SourceEdit {
                    start: 1,
                    ..edit.clone()
                }]
            )
            .is_err()
        );
        assert!(apply_edits("😀name", &[edit.clone(), edit]).is_err());
    }
    #[test]
    fn column_literals_cannot_inject_source_and_use_java_integer_bounds() {
        assert_eq!(
            ColumnChange {
                property: ColumnProperty::Nullable,
                value: ColumnValue::Boolean(false)
            }
            .literal()
            .unwrap(),
            "false"
        );
        assert_eq!(
            ColumnChange {
                property: ColumnProperty::Name,
                value: ColumnValue::Text(String::new())
            }
            .literal()
            .unwrap(),
            "\"\""
        );
        for value in ["x\"), @Id", "0name", "x/y"] {
            assert!(
                ColumnChange {
                    property: ColumnProperty::Name,
                    value: ColumnValue::Text(value.into())
                }
                .literal()
                .is_err()
            );
        }
        assert!(
            ColumnChange {
                property: ColumnProperty::Length,
                value: ColumnValue::Number(u32::MAX)
            }
            .literal()
            .is_err()
        );
    }
}
