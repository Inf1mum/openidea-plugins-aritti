//! "Generate DDL for schema-validation errors…" (G121): a document that holds a Hibernate `SchemaManagementException`
//! stack trace (a `spring.jpa.hibernate.ddl-auto=validate` start-up failure) gets a reviewed DDL script that fills the
//! difference the trace names, taken from the entity model. Hibernate's validator reports `Schema-validation: missing
//! table [t]`, `missing column [c] in table [t]` and `wrong column type encountered in column [c] in table [t]; found
//! [..], but expecting [..]` (message formats read from hibernate-core 6.6, AbstractSchemaValidator). Missing
//! sequences are listed but not generated (sequences are not modelled). The script goes to a new file; nothing runs.
use super::*;
use super::migration_action::{absent, acknowledge, apply_default_lengths, choose_scope, scan_scoped, target_in_root};
use persistence_core::schema::{self, Change, ChangePlan, Fact, PlannedChange, SchemaSnapshot};
use persistence_core::sql::{self, Dialect, EmitOptions};

pub(super) const TITLE: &str = "Generate DDL for schema-validation errors…";
const TARGET: &str = "db/jpa-schema-repair.sql";
const MARKER: &str = "SchemaManagementException";
const PREFIX: &str = "Schema-validation: ";
const MAX_PROBLEMS: usize = 200;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Problem {
    MissingTable(String),
    MissingColumn { table: String, column: String },
    WrongType { table: String, column: String, found: String, expected: String },
    MissingSequence(String),
}

/// The text of the next `[...]` group of `rest`, and what follows it.
fn bracket(rest: &str) -> Option<(&str, &str)> {
    let start = rest.find('[')?;
    let end = rest[start..].find(']')? + start;
    Some((&rest[start + 1..end], &rest[end + 1..]))
}

fn bare(name: &str) -> String {
    name.rsplit('.').next().unwrap_or(name).trim_matches(['"', '`', '\'']).to_owned()
}

fn parse_line(rest: &str) -> Option<Problem> {
    if let Some(after) = rest.strip_prefix("missing table") {
        return Some(Problem::MissingTable(bare(bracket(after)?.0)));
    }
    if let Some(after) = rest.strip_prefix("missing column") {
        let (column, after) = bracket(after)?;
        let (table, _) = bracket(after.trim_start().strip_prefix("in table")?)?;
        return Some(Problem::MissingColumn { table: bare(table), column: bare(column) });
    }
    if let Some(after) = rest.strip_prefix("wrong column type encountered in column") {
        let (column, after) = bracket(after)?;
        let (table, after) = bracket(after.trim_start().strip_prefix("in table")?)?;
        let (found, after) = bracket(after.trim_start().strip_prefix(';')?.trim_start().strip_prefix("found")?)?;
        let (expected, _) = bracket(after.trim_start().strip_prefix(',')?.trim_start().strip_prefix("but expecting")?)?;
        return Some(Problem::WrongType { table: bare(table), column: bare(column), found: found.to_owned(), expected: expected.to_owned() });
    }
    rest.strip_prefix("missing sequence").and_then(bracket).map(|(name, _)| Problem::MissingSequence(bare(name)))
}

pub(super) fn parse(text: &str) -> Vec<Problem> {
    let mut found = Vec::new();
    for line in text.lines() {
        let mut rest = line;
        while let Some(at) = rest.find(PREFIX) {
            rest = &rest[at + PREFIX.len()..];
            if let Some(problem) = parse_line(rest) {
                if !found.contains(&problem) && found.len() < MAX_PROBLEMS {
                    found.push(problem);
                }
            }
        }
    }
    found
}

pub(super) fn eligible(doc: &Document) -> bool {
    !matches!(doc.language.as_str(), "java" | "kotlin") && doc.text.len() <= MAX_TEXT && doc.text.contains(MARKER) && !parse(&doc.text).is_empty()
}

pub(super) fn actions(generation: &mut Generation, doc: &Document, selection: Range) -> Option<Vec<FileAction>> {
    if !eligible(doc) {
        return None;
    }
    let root = host::project_of(&doc.path).and_then(|project| project.root().ok()).and_then(|root| path(&root))?;
    if path(&doc.path).as_deref() != Some(doc.path.as_str()) || !inside(&root, &doc.path) {
        return Some(Vec::new());
    }
    if generation.pending.len() > 8 {
        generation.pending.clear();
    }
    generation.next = generation.next.checked_add(1)?;
    let data = format!("jpa:{}:{}:{}", Operation::SchemaRepair.key(), generation.next, doc.version);
    generation.pending.insert(data.clone(), Pending { root, doc: doc.clone(), operation: Operation::SchemaRepair, selection });
    Some(vec![FileAction { title: TITLE.into(), kind: Some("refactor.rewrite".into()), data: Some(data), ..FileAction::default() }])
}

fn table_of<'a>(snapshot: &'a SchemaSnapshot, name: &str) -> Option<&'a schema::Table> {
    snapshot.tables.iter().find(|t| t.name.eq_ignore_ascii_case(name))
}

/// The plan filling the reported differences, and what could not be planned.
fn plan_for(model: &SchemaSnapshot, problems: &[Problem]) -> (ChangePlan, Vec<String>) {
    let mut unplanned = Vec::new();
    let mut wanted = SchemaSnapshot { complete: true, ..SchemaSnapshot::default() };
    let mut present = SchemaSnapshot { complete: true, ..SchemaSnapshot::default() };
    let mut types = Vec::new();
    let keep = |name: &str, wanted: &mut SchemaSnapshot, present: &mut SchemaSnapshot| -> Option<()> {
        if table_of(wanted, name).is_some() {
            return Some(());
        }
        let table = table_of(model, name)?.clone();
        wanted.tables.push(table.clone());
        present.tables.push(table);
        Some(())
    };
    for problem in problems {
        match problem {
            Problem::MissingTable(name) => {
                if keep(name, &mut wanted, &mut present).is_none() {
                    unplanned.push(format!("table {name} is not in the entity model"));
                    continue;
                }
                present.tables.retain(|t| !t.name.eq_ignore_ascii_case(name));
            }
            Problem::MissingColumn { table, column } | Problem::WrongType { table, column, .. } => {
                if keep(table, &mut wanted, &mut present).is_none() {
                    unplanned.push(format!("table {table} is not in the entity model"));
                    continue;
                }
                let Some(Fact::Known(columns)) = table_of(&wanted, table).map(|t| &t.columns) else {
                    unplanned.push(format!("the columns of table {table} are not fully known"));
                    continue;
                };
                let Some(model_column) = columns.iter().find(|c| c.name.eq_ignore_ascii_case(column)).cloned() else {
                    unplanned.push(format!("column {table}.{column} is not in the entity model"));
                    continue;
                };
                match problem {
                    Problem::MissingColumn { .. } => {
                        if let Some(Fact::Known(list)) = present.tables.iter_mut().find(|t| t.name.eq_ignore_ascii_case(table)).map(|t| &mut t.columns) {
                            list.retain(|c| !c.name.eq_ignore_ascii_case(column));
                        }
                    }
                    Problem::WrongType { found, expected, .. } => types.push((table.clone(), model_column, format!("Hibernate found [{found}] but expects [{expected}]"))),
                    _ => {}
                }
            }
            Problem::MissingSequence(name) => unplanned.push(format!("sequence {name} (sequences are not modelled)")),
        }
    }
    let mut plan = schema::diff(&present, &wanted).unwrap_or_default();
    for (table, column, reason) in types {
        let name = table_of(model, &table).map_or(table, |t| t.name.clone());
        plan.changes.push(PlannedChange { change: Change::AlterColumnType { table: name, column: column.name.clone(), from: column.clone(), to: column }, danger: schema::Danger::Rewrite, reason });
    }
    (plan, unplanned)
}

fn dialect_choice(project: &Project) -> Result<Dialect, Error> {
    let mut dialects = [PickItem::new("postgresql", "PostgreSQL"), PickItem::new("mysql", "MySQL")];
    crate::datasource_facts::prefer(project, &mut dialects);
    match host::pick("SQL dialect (no database connection)", &dialects)?.ok_or(Error::Cancelled)?.as_str() {
        "postgresql" => Ok(Dialect::PostgreSql),
        "mysql" => Ok(Dialect::MySql),
        _ => Err(Error::Stale),
    }
}

fn build(project: &Project, root: &str, scope: &crate::schema_snapshot::Scope, problems: &[Problem], dialect: Dialect) -> Result<(String, usize, Vec<String>), Error> {
    let model = scan_scoped(project, root, scope)?;
    let (mut plan, unplanned) = plan_for(&model.snapshot, problems);
    apply_default_lengths(&mut plan);
    let emitted = sql::emit(&plan, dialect, &EmitOptions::default()).map_err(|e| Error::from(format!("SQL cannot be generated: {e:?}")))?;
    if emitted.emitted() == 0 {
        return Err("The trace names nothing the entity model can fill: no statement was produced.".into());
    }
    Ok((emitted.render(), emitted.emitted(), unplanned))
}

pub(super) fn resolve(pending: &Pending, doc: &Document, project: &Project) -> Result<FileAction, Error> {
    current(pending, doc)?;
    let root = pending.root.as_str();
    let problems = parse(&doc.text);
    if problems.is_empty() {
        return Err(Error::Stale);
    }
    let scope = choose_scope(project, root)?;
    let dialect = dialect_choice(project)?;
    let target = target_in_root(project, root, TARGET)?;
    absent(project, &target)?;
    let (text, count, unplanned) = build(project, root, &scope, &problems, dialect)?;
    if !unplanned.is_empty() {
        acknowledge(&unplanned.iter().map(|u| format!("not generated: {u}")).collect::<Vec<_>>())?;
    }
    let preview = format!(
        "Target: {target}\nDialect: {dialect:?}\nSchema-validation problems in the trace: {}\nStatements: {count}\n\nA ddl-auto=validate start-up failure means Hibernate could not map the entities to the existing tables; this script fills the reported difference from the entity model. Exact SQL file:\n{text}\nThe script is never executed; review it and move it into a migration.",
        problems.len()
    );
    if !ui_kit::prompts::confirm_exact("Create schema repair DDL?", &preview)? {
        return Err(Error::Cancelled);
    }
    current(pending, doc)?;
    absent(project, &target)?;
    if build(project, root, &scope, &problems, dialect)?.0 != text {
        return Err(Error::Stale);
    }
    Ok(FileAction {
        title: "Create schema repair DDL".into(),
        kind: Some("refactor.rewrite".into()),
        operations: vec![FileOperation::Create { path: target.clone(), text, overwrite: false }],
        open: Some(target),
        ..FileAction::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const TRACE: &str = "Caused by: org.hibernate.tool.schema.spi.SchemaManagementException: Schema-validation: missing table [public.invoice]\nCaused by: ...SchemaManagementException: Schema-validation: missing column [email] in table [client]\nSchema-validation: wrong column type encountered in column [name] in table [client]; found [int4 (Types#INTEGER)], but expecting [varchar(255) (Types#VARCHAR)]\nSchema-validation: missing sequence [client_seq]\nSchema-validation: missing column [email] in table [client]\n";

    #[test]
    fn hibernate_validation_messages_are_parsed_once_each() {
        assert_eq!(
            parse(TRACE),
            vec![
                Problem::MissingTable("invoice".into()),
                Problem::MissingColumn { table: "client".into(), column: "email".into() },
                Problem::WrongType { table: "client".into(), column: "name".into(), found: "int4 (Types#INTEGER)".into(), expected: "varchar(255) (Types#VARCHAR)".into() },
                Problem::MissingSequence("client_seq".into()),
            ]
        );
    }

    #[test]
    fn unrelated_text_yields_nothing() {
        assert!(parse("Schema-validation: something else [x]\nmissing table [t]").is_empty());
    }

    #[test]
    fn plan_fills_missing_table_and_column_from_the_model() {
        let doc = Document {
            path: "/p/src/main/java/shop/Customer.java".into(),
            language: "java".into(),
            version: 0,
            text: "package shop;\nimport jakarta.persistence.*;\n@Entity\npublic class Customer {\n    @Id Long id;\n    @Column(length = 100) String email;\n}\n".into(),
        };
        let model = crate::schema_snapshot::snapshot_from_entities(&[doc]);
        let name = model.tables[0].name.clone();
        let (plan, unplanned) = plan_for(&model, &[Problem::MissingTable(name.clone()), Problem::MissingTable("ghost".into())]);
        assert!(matches!(plan.changes[0].change, Change::CreateTable { .. }));
        assert_eq!(unplanned.len(), 1);
        let (plan, _) = plan_for(&model, &[Problem::MissingColumn { table: name, column: "email".into() }]);
        assert!(matches!(&plan.changes[0].change, Change::AddColumn { column, .. } if column.name == "email"));
    }
}
