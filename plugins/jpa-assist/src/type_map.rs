//! The `sqlTypeMappings` setting: SQL types for Java types, JPA converters and Hibernate types that have no automatic
//! mapping (JetBrains database versioning help: "mappings can also be set for JPA converters and Hibernate types").
use super::*;

const MAX_ROWS: usize = 256;

fn simple(name: &str) -> &str {
    name.rsplit('.').next().unwrap_or(name)
}

fn same(entry: &str, key: &str) -> bool {
    let entry: String = entry.split_whitespace().collect();
    entry == key || (!entry.starts_with('@') && !key.starts_with('@') && simple(&entry) == simple(key))
}

/// The SQL type text of the first row whose `type` names `key`: a Java type, a converter or Hibernate type class
/// (simple or qualified), or the text of an annotation such as `@JdbcTypeCode(SqlTypes.JSON)`.
pub(super) fn find(rows: &str, key: &str) -> Result<Option<String>, String> {
    let rows: Vec<serde_json::Value> = serde_json::from_str(rows).map_err(|_| "sqlTypeMappings is not a list of rows.".to_owned())?;
    if rows.len() > MAX_ROWS {
        return Err(format!("sqlTypeMappings has more than {MAX_ROWS} rows."));
    }
    for row in &rows {
        let (Some(entry), Some(sql)) = (row["type"].as_str(), row["sqlType"].as_str()) else {
            return Err("sqlTypeMappings: each row needs a type and a sqlType.".into());
        };
        if !entry.trim().is_empty() && same(entry.trim(), key) {
            return Ok(Some(sql.trim().to_owned()).filter(|sql| !sql.is_empty()));
        }
    }
    Ok(None)
}

pub(super) fn mapped(key: &str) -> Result<Option<String>, Error> {
    match host::setting("sqlTypeMappings") {
        None => Ok(None),
        Some(rows) => find(&rows, key).map_err(Error::from),
    }
}

/// Rows of `liquibaseStringTypes`: the `dbms` and the type each DBMS gets for `${string.type}`.
pub(super) fn string_types() -> Result<Vec<(String, String)>, Error> {
    let Some(raw) = host::setting("liquibaseStringTypes") else {
        return Ok(Vec::new());
    };
    let rows: Vec<serde_json::Value> = serde_json::from_str(&raw).map_err(|_| Error::from("liquibaseStringTypes is not a list of rows."))?;
    if rows.len() > 32 {
        return Err("liquibaseStringTypes has more than 32 rows.".into());
    }
    rows.iter()
        .map(|row| match (row["dbms"].as_str(), row["type"].as_str()) {
            (Some(dbms), Some(ty)) => Ok((dbms.trim().to_owned(), ty.trim().to_owned())),
            _ => Err(Error::from("liquibaseStringTypes: each row needs a dbms and a type.")),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_match_simple_or_qualified_names_and_annotation_text() {
        let rows = r#"[{"type":"com.shop.Money","sqlType":"numeric(19,2)"},{"type":"@JdbcTypeCode(SqlTypes.JSON)","sqlType":"jsonb"},{"type":"MoneyConverter","sqlType":"bigint"}]"#;
        assert_eq!(find(rows, "Money").unwrap().as_deref(), Some("numeric(19,2)"));
        assert_eq!(find(rows, "shop.Money").unwrap().as_deref(), Some("numeric(19,2)"));
        assert_eq!(find(rows, "@JdbcTypeCode(SqlTypes.JSON)").unwrap().as_deref(), Some("jsonb"));
        assert_eq!(find(rows, "x.MoneyConverter").unwrap().as_deref(), Some("bigint"));
        assert_eq!(find(rows, "Other").unwrap(), None);
        assert!(find("{}", "Money").is_err() && find(r#"[{"type":"A"}]"#, "A").is_err());
    }
}
