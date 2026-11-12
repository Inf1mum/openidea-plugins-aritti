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
/// A row with a `dbms` applies only to that database (`postgresql`, `mysql`) and wins over a row without one.
pub(super) fn find(rows: &str, key: &str, dbms: Option<&str>) -> Result<Option<String>, String> {
    let rows: Vec<serde_json::Value> = serde_json::from_str(rows).map_err(|_| "sqlTypeMappings is not a list of rows.".to_owned())?;
    if rows.len() > MAX_ROWS {
        return Err(format!("sqlTypeMappings has more than {MAX_ROWS} rows."));
    }
    let mut generic = None;
    for row in &rows {
        let (Some(entry), Some(sql)) = (row["type"].as_str(), row["sqlType"].as_str()) else {
            return Err("sqlTypeMappings: each row needs a type and a sqlType.".into());
        };
        if entry.trim().is_empty() || !same(entry.trim(), key) {
            continue;
        }
        let found = Some(sql.trim().to_owned()).filter(|sql| !sql.is_empty());
        match row["dbms"].as_str().map(str::trim).filter(|row_dbms| !row_dbms.is_empty()) {
            Some(row_dbms) if Some(row_dbms.to_ascii_lowercase().as_str()) == dbms => return Ok(found),
            Some(_) => {}
            None => generic = generic.or(Some(found)),
        }
    }
    Ok(generic.flatten())
}

pub(super) fn mapped(key: &str, dbms: Option<&str>) -> Result<Option<String>, Error> {
    match host::setting("sqlTypeMappings") {
        None => Ok(None),
        Some(rows) => find(&rows, key, dbms).map_err(Error::from),
    }
}

/// A row of `sqlJavaTypeMappings`: what a SQL type becomes when entities are generated from SQL.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct JavaTarget {
    pub kind: String,
    pub java: String,
    pub class: String,
}

/// The first row of `sqlJavaTypeMappings` for `sql_type` (compared without length or precision, case-insensitive).
/// A row for `dbms` wins over a row without a database.
pub(super) fn find_java(rows: &str, sql_type: &str, dbms: &str) -> Result<Option<JavaTarget>, String> {
    let rows: Vec<serde_json::Value> = serde_json::from_str(rows).map_err(|_| "sqlJavaTypeMappings is not a list of rows.".to_owned())?;
    if rows.len() > MAX_ROWS {
        return Err(format!("sqlJavaTypeMappings has more than {MAX_ROWS} rows."));
    }
    let wanted = sql_type.split('(').next().unwrap_or(sql_type).trim().to_ascii_lowercase();
    let mut generic = None;
    for row in &rows {
        let text = |key: &str| row[key].as_str().map(str::trim).unwrap_or_default();
        if text("sqlType").split('(').next().unwrap_or_default().trim().to_ascii_lowercase() != wanted || wanted.is_empty() {
            continue;
        }
        let kind = text("target");
        if !["javaType", "converter", "hibernateType"].contains(&kind) || text("java").is_empty() || (kind != "javaType" && text("class").is_empty()) {
            return Err(format!("sqlJavaTypeMappings: the row for {wanted} needs a target and a Java type (and a class for a converter or Hibernate type)."));
        }
        let found = JavaTarget { kind: kind.to_owned(), java: text("java").to_owned(), class: text("class").to_owned() };
        match text("dbms").to_ascii_lowercase().as_str() {
            "" => generic = generic.or(Some(found)),
            row_dbms if row_dbms == dbms => return Ok(Some(found)),
            _ => {}
        }
    }
    Ok(generic)
}

pub(super) fn java_mapped(sql_type: &str, dbms: &str) -> Result<Option<JavaTarget>, Error> {
    match host::setting("sqlJavaTypeMappings") {
        None => Ok(None),
        Some(rows) => find_java(&rows, sql_type, dbms).map_err(Error::from),
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
        assert_eq!(find(rows, "Money", None).unwrap().as_deref(), Some("numeric(19,2)"));
        assert_eq!(find(rows, "shop.Money", None).unwrap().as_deref(), Some("numeric(19,2)"));
        assert_eq!(find(rows, "@JdbcTypeCode(SqlTypes.JSON)", None).unwrap().as_deref(), Some("jsonb"));
        assert_eq!(find(rows, "x.MoneyConverter", None).unwrap().as_deref(), Some("bigint"));
        assert_eq!(find(rows, "Other", None).unwrap(), None);
        let per_dbms = r#"[{"type":"Money","sqlType":"numeric(19,2)"},{"type":"Money","dbms":"mysql","sqlType":"decimal(19,2)"},{"type":"Tag","dbms":"mysql","sqlType":"text"}]"#;
        assert_eq!(find(per_dbms, "Money", Some("mysql")).unwrap().as_deref(), Some("decimal(19,2)"));
        assert_eq!(find(per_dbms, "Money", Some("postgresql")).unwrap().as_deref(), Some("numeric(19,2)"));
        assert_eq!(find(per_dbms, "Money", None).unwrap().as_deref(), Some("numeric(19,2)"));
        assert_eq!(find(per_dbms, "Tag", Some("postgresql")).unwrap(), None);
        assert!(find("{}", "Money", None).is_err() && find(r#"[{"type":"A"}]"#, "A", None).is_err());
    }

    #[test]
    fn sql_java_rows_match_by_base_type_and_prefer_the_database() {
        let rows = r#"[{"sqlType":"NUMERIC","java":"BigDecimal","target":"javaType"},{"dbms":"mysql","sqlType":"numeric(10,2)","java":"Money","target":"converter","class":"shop.MoneyConverter"}]"#;
        let mysql = find_java(rows, "numeric", "mysql").unwrap().unwrap();
        assert_eq!((mysql.kind.as_str(), mysql.class.as_str()), ("converter", "shop.MoneyConverter"));
        assert_eq!(find_java(rows, "numeric", "postgresql").unwrap().unwrap().java, "BigDecimal");
        assert_eq!(find_java(rows, "text", "mysql").unwrap(), None);
        assert!(find_java(r#"[{"sqlType":"x","java":"A","target":"converter"}]"#, "x", "mysql").is_err());
        assert!(find_java("{}", "x", "mysql").is_err());
    }
}
