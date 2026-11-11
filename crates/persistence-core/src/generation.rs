//! Typed, source-generation choices. These are not provider/database facts.
use crate::PersistenceRole;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Namespace {
    Jakarta,
    Javax,
}
impl Namespace {
    pub fn package(self) -> &'static str {
        match self {
            Self::Jakarta => "jakarta.persistence",
            Self::Javax => "javax.persistence",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum IdentifierType {
    Long,
    Integer,
    Short,
    String,
    Uuid,
    BigInteger,
}
impl IdentifierType {
    pub fn source_type(self, kotlin: bool) -> &'static str {
        match (self, kotlin) {
            (Self::Long, false) => "java.lang.Long",
            (Self::Long, true) => "kotlin.Long",
            (Self::Integer, false) => "java.lang.Integer",
            (Self::Integer, true) => "kotlin.Int",
            (Self::Short, false) => "java.lang.Short",
            (Self::Short, true) => "kotlin.Short",
            (Self::String, false) => "java.lang.String",
            (Self::String, true) => "kotlin.String",
            (Self::Uuid, _) => "java.util.UUID",
            (Self::BigInteger, _) => "java.math.BigInteger",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GenerationStrategy {
    Assigned,
    Identity,
    Sequence,
    Table,
    Auto,
    Uuid,
}
impl GenerationStrategy {
    pub fn constant(self) -> Option<&'static str> {
        Some(match self {
            Self::Assigned => return None,
            Self::Identity => "IDENTITY",
            Self::Sequence => "SEQUENCE",
            Self::Table => "TABLE",
            Self::Auto => "AUTO",
            Self::Uuid => "UUID",
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SequenceGenerator {
    pub generator: String,
    pub sequence_name: String,
    pub initial_value: i32,
    pub allocation_size: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TableGenerator {
    pub generator: String,
    pub table_name: String,
    pub pk_column_name: String,
    pub value_column_name: String,
    pub pk_column_value: String,
    pub initial_value: i32,
    pub allocation_size: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IdentifierDraft {
    pub name: String,
    pub column: String,
    pub id_type: IdentifierType,
    pub strategy: GenerationStrategy,
    pub sequence: Option<SequenceGenerator>,
    pub table: Option<TableGenerator>,
}

pub fn identifier(value: &str) -> bool {
    literal_name(value) && !"_ abstract assert boolean break byte case catch char class const continue default do double else enum extends final finally float for if implements import instanceof int interface long native new package private protected public return short static strictfp super switch synchronized this throw throws transient try void volatile while true false null record sealed permits var yield fun val object when is in as typealias typeof constructor init companion suspend reified inline out by where".split_ascii_whitespace().any(|word| word == value)
}
fn literal_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.bytes().enumerate().all(|(i, b)| {
            b.is_ascii_alphabetic() || b == b'_' || (i > 0 && (b.is_ascii_digit() || b == b'$'))
        })
}

impl IdentifierDraft {
    pub fn validate(&self, namespace: Namespace) -> Result<(), &'static str> {
        if !identifier(&self.name)
            || ["java", "kotlin", "javax", "jakarta", "Class"].contains(&self.name.as_str())
            || (!self.column.is_empty() && !literal_name(&self.column))
        {
            return Err(
                "Choose a source identifier and a literal column name without keywords, quotes or path separators.",
            );
        }
        let numeric = matches!(self.id_type, IdentifierType::Long | IdentifierType::Integer);
        match self.strategy {
            GenerationStrategy::Assigned => {}
            GenerationStrategy::Identity
            | GenerationStrategy::Sequence
            | GenerationStrategy::Table
                if !numeric =>
            {
                return Err(
                    "Identity, sequence and table generation require Long or Integer in this portable source subset.",
                );
            }
            GenerationStrategy::Uuid
                if namespace != Namespace::Jakarta
                    || !matches!(self.id_type, IdentifierType::Uuid | IdentifierType::String) =>
            {
                return Err(
                    "UUID generation requires Jakarta Persistence and a UUID or String identifier.",
                );
            }
            GenerationStrategy::Auto
                if !numeric
                    && !(namespace == Namespace::Jakarta
                        && matches!(
                            self.id_type,
                            IdentifierType::Uuid | IdentifierType::String
                        )) =>
            {
                return Err("AUTO requires Long/Integer, or a Jakarta UUID/String identifier.");
            }
            _ => {}
        }
        if self.sequence.is_some() != (self.strategy == GenerationStrategy::Sequence)
            || self.table.is_some() != (self.strategy == GenerationStrategy::Table)
        {
            return Err("Supply only the settings for the selected generator strategy.");
        }
        let allocation = |n: u32| n > 0 && n <= i32::MAX as u32;
        if let Some(s) = &self.sequence {
            if !literal_name(&s.generator)
                || !literal_name(&s.sequence_name)
                || !allocation(s.allocation_size)
            {
                return Err(
                    "Sequence names must be literal identifiers and allocation size a positive Java integer.",
                );
            }
        }
        if let Some(t) = &self.table {
            if [
                &t.generator,
                &t.table_name,
                &t.pk_column_name,
                &t.value_column_name,
                &t.pk_column_value,
            ]
            .iter()
            .any(|s| !literal_name(s))
                || !allocation(t.allocation_size)
                || t.pk_column_name == t.value_column_name
            {
                return Err(
                    "Table generator names must be literal identifiers, key/value columns distinct, and allocation size positive.",
                );
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum InheritanceStrategy {
    SingleTable,
    Joined,
    TablePerClass,
}
impl InheritanceStrategy {
    pub fn constant(self) -> &'static str {
        match self {
            Self::SingleTable => "SINGLE_TABLE",
            Self::Joined => "JOINED",
            Self::TablePerClass => "TABLE_PER_CLASS",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DiscriminatorType {
    String,
    Char,
    Integer,
}
impl DiscriminatorType {
    pub fn constant(self) -> &'static str {
        match self {
            Self::String => "STRING",
            Self::Char => "CHAR",
            Self::Integer => "INTEGER",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DiscriminatorColumn {
    pub name: String,
    pub discriminator_type: DiscriminatorType,
    /// Only meaningful for `String`; 0 keeps the provider default.
    pub length: u32,
}

/// Either the root of a hierarchy or one subclass of it. The subclass cannot know the root's
/// strategy lexically, so it declares only what is valid under every strategy.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", tag = "kind")]
pub enum InheritanceDraft {
    #[serde(rename_all = "camelCase")]
    Root {
        strategy: InheritanceStrategy,
        discriminator: Option<DiscriminatorColumn>,
        discriminator_value: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    Subclass {
        superclass: String,
        discriminator_value: Option<String>,
        primary_key_join_column: Option<String>,
    },
}
impl InheritanceDraft {
    fn validate(&self) -> Result<(), &'static str> {
        match self {
            Self::Root { strategy, discriminator, discriminator_value } => {
                if let Some(column) = discriminator {
                    if *strategy == InheritanceStrategy::TablePerClass {
                        return Err("Table-per-class hierarchies have no discriminator column.");
                    }
                    if !literal_name(&column.name) {
                        return Err("The discriminator column must be a literal name.");
                    }
                    if column.length > 255
                        || (column.length > 0 && column.discriminator_type != DiscriminatorType::String)
                    {
                        return Err("Discriminator length applies only to string discriminators (at most 255).");
                    }
                }
                if let Some(value) = discriminator_value {
                    if *strategy == InheritanceStrategy::TablePerClass {
                        return Err("Table-per-class hierarchies have no discriminator value.");
                    }
                    let kind = discriminator.as_ref().map_or(DiscriminatorType::String, |c| c.discriminator_type);
                    discriminator_value_valid(value, kind)?;
                }
                Ok(())
            }
            Self::Subclass { superclass, discriminator_value, primary_key_join_column } => {
                if superclass.is_empty() || superclass.len() > 256 || !superclass.split('.').all(identifier) {
                    return Err("Choose a literal superclass name.");
                }
                if let Some(value) = discriminator_value {
                    discriminator_value_valid(value, DiscriminatorType::String)?;
                }
                if primary_key_join_column.as_deref().is_some_and(|c| !literal_name(c)) {
                    return Err("The primary-key join column must be a literal name.");
                }
                Ok(())
            }
        }
    }
}

fn discriminator_value_valid(value: &str, kind: DiscriminatorType) -> Result<(), &'static str> {
    let literal = !value.is_empty()
        && value.len() <= 64
        && value.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b' '));
    let fits = match kind {
        DiscriminatorType::String => true,
        DiscriminatorType::Char => value.chars().count() == 1,
        DiscriminatorType::Integer => value.parse::<i32>().is_ok(),
    };
    if literal && fits {
        return Ok(());
    }
    Err("Use a short alphanumeric discriminator value matching the discriminator type.")
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ManagedClassDraft {
    pub name: String,
    pub role: PersistenceRole,
    pub namespace: Namespace,
    pub table_name: String,
    pub identifier: Option<IdentifierDraft>,
    pub versioned: bool,
    #[serde(default)]
    pub inheritance: Option<InheritanceDraft>,
}
impl ManagedClassDraft {
    pub fn validate(&self) -> Result<(), &'static str> {
        if !identifier(&self.name)
            || ["java", "kotlin", "javax", "jakarta"].contains(&self.name.as_str())
        {
            return Err("Choose a valid persistence class name.");
        }
        if let Some(inheritance) = &self.inheritance {
            if self.role != PersistenceRole::Entity {
                return Err("Only an entity declares inheritance mapping.");
            }
            inheritance.validate()?;
        }
        if let Some(InheritanceDraft::Subclass { superclass, .. }) = &self.inheritance {
            if self.identifier.is_some() || self.versioned {
                return Err("A subclass inherits its identifier and version from the hierarchy root.");
            }
            if superclass == &self.name {
                return Err("A class cannot extend itself.");
            }
            if !self.table_name.is_empty() && !literal_name(&self.table_name) {
                return Err("The subclass table must be a literal name.");
            }
        } else if self.role == PersistenceRole::Entity {
            if !literal_name(&self.table_name) || self.identifier.is_none() {
                return Err("An entity needs a literal table name and identifier configuration.");
            }
        } else if !self.table_name.is_empty() {
            return Err("Only entity creation declares a table.");
        }
        if self.role == PersistenceRole::Embeddable && (self.identifier.is_some() || self.versioned)
        {
            return Err(
                "An embeddable cannot declare this entity identifier/version configuration.",
            );
        }
        if let Some(id) = &self.identifier {
            id.validate(self.namespace)?;
            if self.versioned && matches!(id.name.as_str(), "version" | "Version") {
                return Err("The identifier and version field names must differ.");
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn id() -> IdentifierDraft {
        IdentifierDraft {
            name: "key".into(),
            column: "record_id".into(),
            id_type: IdentifierType::Long,
            strategy: GenerationStrategy::Assigned,
            sequence: None,
            table: None,
        }
    }
    #[test]
    fn strategy_type_and_namespace_matrix_is_portable_and_explicit() {
        for kind in [
            IdentifierType::Long,
            IdentifierType::Integer,
            IdentifierType::Short,
            IdentifierType::String,
            IdentifierType::Uuid,
            IdentifierType::BigInteger,
        ] {
            let mut value = id();
            value.id_type = kind;
            assert!(value.validate(Namespace::Javax).is_ok());
            value.strategy = GenerationStrategy::Identity;
            assert_eq!(
                value.validate(Namespace::Jakarta).is_ok(),
                matches!(kind, IdentifierType::Long | IdentifierType::Integer)
            );
            value.strategy = GenerationStrategy::Uuid;
            assert_eq!(
                value.validate(Namespace::Jakarta).is_ok(),
                matches!(kind, IdentifierType::Uuid | IdentifierType::String)
            );
            assert!(value.validate(Namespace::Javax).is_err());
        }
    }
    #[test]
    fn generator_payloads_are_strategy_bound_and_cannot_inject_source() {
        let mut value = id();
        value.strategy = GenerationStrategy::Sequence;
        assert!(value.validate(Namespace::Jakarta).is_err());
        value.sequence = Some(SequenceGenerator {
            generator: "ids".into(),
            sequence_name: "record_seq".into(),
            initial_value: -5,
            allocation_size: 10,
        });
        assert!(value.validate(Namespace::Jakarta).is_ok());
        value.sequence.as_mut().unwrap().sequence_name = "seq\"), @Id".into();
        assert!(value.validate(Namespace::Jakarta).is_err());
        value.sequence.as_mut().unwrap().sequence_name = "record_seq".into();
        value.sequence.as_mut().unwrap().allocation_size = u32::MAX;
        assert!(value.validate(Namespace::Jakarta).is_err());
        value.strategy = GenerationStrategy::Assigned;
        assert!(value.validate(Namespace::Jakarta).is_err());
    }
    #[test]
    fn persistence_roles_and_generated_field_collisions_are_validated() {
        let mut draft = ManagedClassDraft {
            name: "Record".into(),
            role: PersistenceRole::Entity,
            namespace: Namespace::Jakarta,
            table_name: "records".into(),
            identifier: Some(id()),
            versioned: true,
            inheritance: None,
        };
        assert!(draft.validate().is_ok());
        draft.identifier.as_mut().unwrap().name = "version".into();
        assert!(draft.validate().is_err());
        draft.identifier.as_mut().unwrap().name = "Version".into();
        assert!(draft.validate().is_err());
        draft.identifier.as_mut().unwrap().name = "Class".into();
        draft.versioned = false;
        assert!(draft.validate().is_err());
        draft.role = PersistenceRole::Embeddable;
        draft.table_name.clear();
        assert!(draft.validate().is_err());
        draft.identifier = None;
        draft.versioned = false;
        assert!(draft.validate().is_ok());
        draft.role = PersistenceRole::MappedSuperclass;
        draft.identifier = Some(id());
        assert!(draft.validate().is_ok());
    }

    fn entity() -> ManagedClassDraft {
        ManagedClassDraft {
            name: "Vehicle".into(),
            role: PersistenceRole::Entity,
            namespace: Namespace::Jakarta,
            table_name: "vehicle".into(),
            identifier: Some(id()),
            versioned: false,
            inheritance: None,
        }
    }
    fn root(strategy: InheritanceStrategy, kind: DiscriminatorType, length: u32) -> InheritanceDraft {
        InheritanceDraft::Root {
            strategy,
            discriminator: Some(DiscriminatorColumn { name: "dtype".into(), discriminator_type: kind, length }),
            discriminator_value: None,
        }
    }
    #[test]
    fn inheritance_roots_validate_strategy_discriminator_and_value() {
        let mut draft = entity();
        draft.inheritance = Some(root(InheritanceStrategy::SingleTable, DiscriminatorType::String, 31));
        assert!(draft.validate().is_ok());
        draft.inheritance = Some(root(InheritanceStrategy::TablePerClass, DiscriminatorType::String, 0));
        assert!(draft.validate().is_err());
        draft.inheritance = Some(root(InheritanceStrategy::Joined, DiscriminatorType::Integer, 5));
        assert!(draft.validate().is_err());
        draft.inheritance = Some(InheritanceDraft::Root {
            strategy: InheritanceStrategy::Joined,
            discriminator: Some(DiscriminatorColumn {
                name: "kind\") @X".into(),
                discriminator_type: DiscriminatorType::String,
                length: 0,
            }),
            discriminator_value: None,
        });
        assert!(draft.validate().is_err());
        for (kind, value, ok) in [
            (DiscriminatorType::Integer, "7", true),
            (DiscriminatorType::Integer, "seven", false),
            (DiscriminatorType::Char, "C", true),
            (DiscriminatorType::Char, "CC", false),
            (DiscriminatorType::String, "CAR\")", false),
        ] {
            draft.inheritance = Some(InheritanceDraft::Root {
                strategy: InheritanceStrategy::SingleTable,
                discriminator: Some(DiscriminatorColumn { name: "dtype".into(), discriminator_type: kind, length: 0 }),
                discriminator_value: Some(value.into()),
            });
            assert_eq!(draft.validate().is_ok(), ok, "{value}");
        }
        draft.role = PersistenceRole::MappedSuperclass;
        draft.table_name.clear();
        assert!(draft.validate().is_err());
    }
    #[test]
    fn inheritance_subclasses_inherit_identity_and_may_omit_the_table() {
        let mut draft = entity();
        draft.name = "Car".into();
        draft.table_name.clear();
        draft.identifier = None;
        draft.inheritance = Some(InheritanceDraft::Subclass {
            superclass: "Vehicle".into(),
            discriminator_value: Some("CAR".into()),
            primary_key_join_column: Some("vehicle_id".into()),
        });
        assert!(draft.validate().is_ok());
        draft.identifier = Some(id());
        assert!(draft.validate().is_err());
        draft.identifier = None;
        draft.versioned = true;
        assert!(draft.validate().is_err());
        draft.versioned = false;
        draft.inheritance = Some(InheritanceDraft::Subclass {
            superclass: "Car".into(),
            discriminator_value: None,
            primary_key_join_column: None,
        });
        assert!(draft.validate().is_err());
        draft.inheritance = Some(InheritanceDraft::Subclass {
            superclass: "a..B".into(),
            discriminator_value: None,
            primary_key_join_column: None,
        });
        assert!(draft.validate().is_err());
    }

    #[test]
    fn inheritance_drafts_deserialize_from_the_designer_payload() {
        let root: InheritanceDraft = serde_json::from_str(
            r#"{"kind":"root","strategy":"single-table","discriminator":{"name":"dtype","discriminatorType":"string","length":31},"discriminatorValue":null}"#,
        )
        .unwrap();
        assert!(matches!(root, InheritanceDraft::Root { strategy: InheritanceStrategy::SingleTable, .. }));
        let sub: InheritanceDraft = serde_json::from_str(
            r#"{"kind":"subclass","superclass":"Vehicle","discriminatorValue":"CAR","primaryKeyJoinColumn":null}"#,
        )
        .unwrap();
        assert!(matches!(sub, InheritanceDraft::Subclass { .. }));
        assert!(serde_json::from_str::<InheritanceDraft>(r#"{"kind":"other"}"#).is_err());
    }
}
