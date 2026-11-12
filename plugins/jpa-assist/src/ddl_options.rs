//! Identifier and constraint options of generated DDL (JetBrains database versioning help): a maximum identifier
//! length, whether foreign keys get an index, and the primary key constraint name.
use super::*;
use persistence_core::schema::{Fact, Index, SchemaSnapshot};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct DdlOptions {
    /// Longest identifier in bytes (the help names Oracle before 12.1, 30 bytes); `None` is unlimited.
    pub(super) max_identifier: Option<usize>,
    /// `{table}` is replaced by the table name; `None` leaves the primary key unnamed.
    pub(super) primary_key_name: Option<String>,
}

impl DdlOptions {
    pub(super) fn from_settings() -> Result<Self, Error> {
        Self::parse(host::setting_string("jpaAssist.maxIdentifierLength").as_deref(), host::setting_string("jpaAssist.primaryKeyConstraintName").as_deref()).map_err(Error::from)
    }

    pub(super) fn parse(max: Option<&str>, pk: Option<&str>) -> Result<Self, String> {
        let max_identifier = match max.map(str::trim).filter(|m| !m.is_empty()) {
            None => None,
            Some(text) => match text.parse::<usize>() {
                Ok(0) => None,
                Ok(n) if n >= 8 => Some(n),
                _ => return Err("jpaAssist.maxIdentifierLength must be empty, 0 or a whole number of at least 8.".into()),
            },
        };
        let primary_key_name = pk.map(str::trim).filter(|p| !p.is_empty()).map(str::to_owned);
        if let Some(template) = &primary_key_name {
            let bare = template.replace("{table}", "t");
            if !bare.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_') || !bare.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                return Err("jpaAssist.primaryKeyConstraintName is letters, digits and '_' with an optional {table} placeholder.".into());
            }
        }
        Ok(Self { max_identifier, primary_key_name })
    }

    /// The name cut to the limit at a character boundary.
    pub(super) fn shorten(&self, name: &str) -> String {
        let Some(max) = self.max_identifier else {
            return name.to_owned();
        };
        let mut end = name.len().min(max);
        while !name.is_char_boundary(end) {
            end -= 1;
        }
        name[..end].to_owned()
    }

    pub(super) fn primary_key_constraint(&self, table: &str) -> Option<String> {
        self.primary_key_name.as_ref().map(|template| self.shorten(&template.replace("{table}", table)))
    }
}

pub(super) fn index_foreign_keys_enabled() -> bool {
    host::setting_bool("jpaAssist.indexForeignKeys").unwrap_or(false)
}

/// Adds an unnamed single-column index for every foreign key column that no primary key, unique constraint or
/// index already leads with. Tables whose facts are unknown are left alone.
pub(super) fn index_foreign_keys(snapshot: &mut SchemaSnapshot) {
    for table in &mut snapshot.tables {
        let (Fact::Known(keys), Fact::Known(uniques)) = (&table.foreign_keys, &table.unique_constraints) else {
            continue;
        };
        let primary = table.primary_key.known().cloned().unwrap_or_default();
        let covered = |columns: &[String], indexes: &[Index]| {
            primary.first() == columns.first() && !primary.is_empty()
                || uniques.iter().any(|u| u.columns.first() == columns.first())
                || indexes.iter().any(|i| i.columns.first() == columns.first())
        };
        let Fact::Known(indexes) = &mut table.indexes else {
            continue;
        };
        let wanted: Vec<Vec<String>> = keys.iter().map(|k| k.columns.clone()).collect();
        for columns in wanted {
            if !covered(&columns, indexes) {
                indexes.push(Index { name: None, columns, unique: false });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use persistence_core::schema::{ForeignKey, Table};

    #[test]
    fn options_parse_and_shorten_at_the_limit() {
        let options = DdlOptions::parse(Some("30"), Some("pk_{table}")).unwrap();
        assert_eq!(options.shorten("a_very_long_table_name_beyond_thirty_bytes"), "a_very_long_table_name_beyond_");
        assert_eq!(options.primary_key_constraint("customer").as_deref(), Some("pk_customer"));
        assert_eq!(DdlOptions::parse(Some(""), None).unwrap(), DdlOptions::default());
        assert_eq!(DdlOptions::default().shorten("kept_as_is"), "kept_as_is");
        for bad in [Some("3"), Some("x")] {
            assert!(DdlOptions::parse(bad, None).is_err());
        }
        assert!(DdlOptions::parse(None, Some("pk-{table}")).is_err());
    }

    #[test]
    fn foreign_key_columns_get_one_index_unless_already_led_by_one() {
        let fk = |column: &str| ForeignKey { name: None, columns: vec![column.into()], ref_table: "customer".into(), ref_columns: Fact::Known(vec!["id".into()]) };
        let mut orders = Table::new("orders");
        orders.foreign_keys = Fact::Known(vec![fk("customer_id"), fk("owner_id"), fk("id")]);
        orders.primary_key = Fact::Known(vec!["id".into()]);
        orders.indexes = Fact::Known(vec![Index { name: None, columns: vec!["owner_id".into(), "x".into()], unique: false }]);
        let mut snapshot = SchemaSnapshot { tables: vec![orders], ..Default::default() };
        index_foreign_keys(&mut snapshot);
        let indexes = snapshot.tables[0].indexes.known().unwrap();
        assert_eq!(indexes.len(), 2);
        assert!(indexes.iter().any(|i| i.columns == ["customer_id"]));
        let before = snapshot.clone();
        index_foreign_keys(&mut snapshot);
        assert_eq!(snapshot, before);
    }
}
