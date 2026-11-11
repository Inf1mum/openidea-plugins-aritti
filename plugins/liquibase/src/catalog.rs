//! Change types and attribute names of Liquibase changelogs, from the
//! Liquibase 4.29 XSD (Apache-2.0, see NOTICE). Attributes are listed for
//! common change types; expanded core attributes checked against the public
//! dbchangelog-latest.xsd on 2026-11-10. Names describe syntax, not edition availability.

pub const TOP_LEVEL: &[&str] = &["changeSet", "include", "includeAll", "property", "preConditions"];
pub const CHANGE_SET: &[&str] = &[
    "id", "author", "changes", "rollback", "comment", "preConditions", "validCheckSum", "context", "contextFilter",
    "labels", "dbms", "runOnChange", "runAlways", "failOnError", "runInTransaction", "logicalFilePath", "created", "ignore",
];
pub const INCLUDE: &[&str] = &["file", "relativeToChangelogFile", "errorIfMissing", "context", "labels", "ignore"];
pub const INCLUDE_ALL: &[&str] = &[
    "path", "relativeToChangelogFile", "errorIfMissingOrEmpty", "filter", "minDepth", "maxDepth", "endsWithFilter",
    "context", "labels", "ignore",
];
pub const COLUMN: &[&str] = &[
    "name", "type", "value", "valueNumeric", "valueBoolean", "valueDate", "valueComputed", "defaultValue",
    "defaultValueNumeric", "defaultValueBoolean", "defaultValueDate", "defaultValueComputed", "autoIncrement",
    "startWith", "incrementBy", "remarks", "constraints",
];
pub const CONSTRAINTS: &[&str] = &[
    "nullable", "primaryKey", "primaryKeyName", "unique", "uniqueConstraintName", "foreignKeyName", "references",
    "referencedTableName", "referencedColumnNames", "deleteCascade", "notNullConstraintName", "checkConstraint",
    "deferrable", "initiallyDeferred",
];
/// Keys whose YAML value is a sequence: completed as `key:` plus a `- ` item.
pub const LISTS: &[&str] = &["changes", "rollback", "columns", "preConditions"];

pub const CHANGE_TYPES: &[(&str, &[&str])] = &[
    ("addAutoIncrement", &["catalogName", "schemaName", "tableName", "columnName", "columnDataType", "startWith", "incrementBy", "defaultOnNull", "generationType"]),
    ("addColumn", &["catalogName", "schemaName", "tableName", "columns"]),
    ("addDefaultValue", &["catalogName", "schemaName", "tableName", "columnName", "columnDataType", "defaultValue", "defaultValueNumeric", "defaultValueBoolean", "defaultValueDate", "defaultValueComputed"]),
    ("addForeignKeyConstraint", &["baseTableName", "baseColumnNames", "constraintName", "referencedTableName", "referencedColumnNames", "onDelete", "onUpdate", "deleteCascade", "deferrable", "initiallyDeferred", "validate"]),
    ("addLookupTable", &["existingTableCatalogName", "existingTableSchemaName", "existingTableName", "existingColumnName", "newTableCatalogName", "newTableSchemaName", "newTableName", "newColumnName", "newColumnDataType", "constraintName"]),
    ("addNotNullConstraint", &["catalogName", "schemaName", "tableName", "columnName", "columnDataType", "defaultNullValue", "constraintName", "validate"]),
    ("addPrimaryKey", &["catalogName", "schemaName", "tableName", "columnNames", "constraintName", "tablespace", "validate"]),
    ("addUniqueConstraint", &["catalogName", "schemaName", "tableName", "columnNames", "constraintName", "tablespace", "deferrable", "initiallyDeferred", "validate"]),
    ("alterSequence", &["catalogName", "schemaName", "sequenceName", "startValue", "incrementBy", "maxValue", "minValue", "ordered", "cacheSize", "dataType", "cycle"]),
    ("createIndex", &["catalogName", "schemaName", "tableName", "indexName", "unique", "tablespace", "columns"]),
    ("createProcedure", &["catalogName", "schemaName", "procedureName", "path", "encoding", "relativeToChangelogFile", "dbms", "replaceIfExists"]),
    ("createSequence", &["catalogName", "schemaName", "sequenceName", "startValue", "incrementBy", "minValue", "maxValue", "cycle", "cacheSize", "dataType"]),
    ("createTable", &["catalogName", "schemaName", "tableName", "tablespace", "remarks", "ifNotExists", "columns"]),
    ("createView", &["catalogName", "schemaName", "viewName", "selectQuery", "replaceIfExists", "fullDefinition", "remarks"]),
    ("customChange", &["class"]),
    ("delete", &["catalogName", "schemaName", "tableName", "where"]),
    ("dropAllForeignKeyConstraints", &[]),
    ("dropColumn", &["catalogName", "schemaName", "tableName", "columnName", "columns"]),
    ("dropDefaultValue", &["catalogName", "schemaName", "tableName", "columnName", "columnDataType"]),
    ("dropForeignKeyConstraint", &["baseTableName", "constraintName"]),
    ("dropIndex", &["catalogName", "schemaName", "tableName", "indexName"]),
    ("dropNotNullConstraint", &["catalogName", "schemaName", "tableName", "columnName", "columnDataType"]),
    ("dropPrimaryKey", &["catalogName", "schemaName", "tableName", "constraintName", "dropIndex"]),
    ("dropProcedure", &["catalogName", "schemaName", "procedureName"]),
    ("dropSequence", &["catalogName", "schemaName", "sequenceName"]),
    ("dropTable", &["catalogName", "schemaName", "tableName", "cascadeConstraints"]),
    ("dropUniqueConstraint", &["catalogName", "schemaName", "tableName", "constraintName", "uniqueColumns"]),
    ("dropView", &["catalogName", "schemaName", "viewName", "ifExists"]),
    ("empty", &[]),
    ("executeCommand", &["executable", "os", "timeout"]),
    ("insert", &["catalogName", "schemaName", "tableName", "columns"]),
    ("loadData", &["catalogName", "schemaName", "tableName", "file", "relativeToChangelogFile", "encoding", "separator", "quotchar", "columns"]),
    ("loadUpdateData", &["catalogName", "schemaName", "tableName", "file", "relativeToChangelogFile", "encoding", "separator", "quotchar", "commentLineStartsWith", "usePreparedStatements", "primaryKey", "onlyUpdate", "columns"]),
    ("mergeColumns", &["catalogName", "schemaName", "tableName", "column1Name", "column2Name", "joinString", "finalColumnName", "finalColumnType"]),
    ("modifyDataType", &["catalogName", "schemaName", "tableName", "columnName", "newDataType"]),
    ("output", &["target"]),
    ("renameColumn", &["catalogName", "schemaName", "tableName", "oldColumnName", "newColumnName", "columnDataType", "remarks"]),
    ("renameSequence", &["catalogName", "schemaName", "oldSequenceName", "newSequenceName"]),
    ("renameTable", &["catalogName", "schemaName", "oldTableName", "newTableName"]),
    ("renameView", &[]),
    ("setColumnRemarks", &["catalogName", "schemaName", "tableName", "columnName", "columnDataType", "remarks", "columnParentType"]),
    ("setTableRemarks", &["catalogName", "schemaName", "tableName", "remarks"]),
    ("sql", &["sql", "dbms", "splitStatements", "stripComments", "endDelimiter", "comment"]),
    ("sqlFile", &["path", "relativeToChangelogFile", "dbms", "encoding", "splitStatements", "stripComments", "endDelimiter"]),
    ("stop", &[]),
    ("tagDatabase", &["tag"]),
    ("update", &["catalogName", "schemaName", "tableName", "columns", "where"]),
];

// ponytail: extension change types (MongoDB, Hibernate…) are flagged as unknown; add their names when reported
/// Liquibase Pro change types: known (never flagged), not offered.
pub const PRO_CHANGE_TYPES: &[&str] = &[
    "addCheckConstraint", "createFunction", "createPackage", "createPackageBody", "createSynonym", "createTrigger",
    "disableCheckConstraint", "disableTrigger", "dropCheckConstraint", "dropFunction", "dropPackage",
    "dropPackageBody", "dropSynonym", "dropTrigger", "enableCheckConstraint", "enableTrigger", "markUnused",
    "renameTrigger",
];

pub fn change_type(name: &str) -> Option<&'static [&'static str]> {
    CHANGE_TYPES.iter().find(|(known, _)| *known == name).map(|(_, attributes)| *attributes)
}

pub fn is_change_type(name: &str) -> bool {
    change_type(name).is_some() || PRO_CHANGE_TYPES.contains(&name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn knows_every_change_type_once() {
        let mut names: Vec<&str> = CHANGE_TYPES.iter().map(|(n, _)| *n).chain(PRO_CHANGE_TYPES.iter().copied()).collect();
        assert_eq!(CHANGE_TYPES.len(), 47);
        assert_eq!(PRO_CHANGE_TYPES.len(), 18);
        names.sort();
        names.dedup();
        assert_eq!(names.len(), 65);
        assert!(change_type("createTable").unwrap().contains(&"tableName"));
        assert!(change_type("createTable").unwrap().contains(&"columns"));
        assert_eq!(change_type("createTrigger"), None);
        assert!(is_change_type("createTrigger"));
        assert!(!is_change_type("createTabel"));
    }
}
