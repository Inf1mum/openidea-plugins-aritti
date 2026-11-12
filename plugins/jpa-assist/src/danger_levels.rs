//! Three-level danger labels of the migration preview (G108): SAFE (green), CAUTION (yellow), DANGER (red), with a
//! per-change-type override in `schemaDangerLevels`.
//!
//! The level is what the preview and the Persistence Designer show. It never relaxes what is withheld: which
//! statements need an explicit opt-in still follows the intrinsic `Danger` of the plan.
use super::*;
use persistence_core::schema::{Change, ChangePlan, Danger, PlannedChange};

const MAX_ROWS: usize = 32;
const MAX_LISTED: usize = 40;

pub(crate) const CHANGE_TYPES: [&str; 14] = [
    "createTable",
    "dropTable",
    "addColumn",
    "dropColumn",
    "modifyDataType",
    "addNotNullConstraint",
    "dropNotNullConstraint",
    "addForeignKeyConstraint",
    "dropForeignKeyConstraint",
    "addUniqueConstraint",
    "dropUniqueConstraint",
    "createIndex",
    "dropIndex",
    "renameColumn",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Level {
    Safe,
    Caution,
    Danger,
}

impl Level {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Safe => "SAFE",
            Self::Caution => "CAUTION",
            Self::Danger => "DANGER",
        }
    }

    fn parse(text: &str) -> Option<Self> {
        [Self::Safe, Self::Caution, Self::Danger].into_iter().find(|level| level.label().eq_ignore_ascii_case(text))
    }
}

pub(crate) fn change_type(change: &Change) -> &'static str {
    match change {
        Change::CreateTable { .. } => "createTable",
        Change::DropTable { .. } => "dropTable",
        Change::AddColumn { .. } => "addColumn",
        Change::DropColumn { .. } => "dropColumn",
        Change::AlterColumnType { .. } => "modifyDataType",
        Change::AlterNullability { nullable: false, .. } => "addNotNullConstraint",
        Change::AlterNullability { .. } => "dropNotNullConstraint",
        Change::AddForeignKey { .. } => "addForeignKeyConstraint",
        Change::DropForeignKey { .. } => "dropForeignKeyConstraint",
        Change::AddUnique { .. } => "addUniqueConstraint",
        Change::DropUnique { .. } => "dropUniqueConstraint",
        Change::AddIndex { .. } => "createIndex",
        Change::DropIndex { .. } => "dropIndex",
        Change::RenameCandidate { .. } => "renameColumn",
    }
}

fn default_level(danger: Danger) -> Level {
    match danger {
        Danger::Safe => Level::Safe,
        Danger::DataLoss => Level::Danger,
        Danger::NeedsBackfill | Danger::Rewrite | Danger::Ambiguous => Level::Caution,
    }
}

pub(crate) fn parse_overrides(raw: Option<&str>) -> Result<Vec<(String, Level)>, String> {
    let Some(raw) = raw else {
        return Ok(Vec::new());
    };
    let rows: Vec<serde_json::Value> = serde_json::from_str(raw).map_err(|_| "schemaDangerLevels is not a list of rows.".to_owned())?;
    if rows.len() > MAX_ROWS {
        return Err(format!("schemaDangerLevels has more than {MAX_ROWS} rows."));
    }
    rows.iter()
        .map(|row| {
            let kind = row["changeType"].as_str().unwrap_or_default();
            if !CHANGE_TYPES.contains(&kind) {
                return Err(format!("schemaDangerLevels: unknown changeType {kind:?}."));
            }
            let level = row["level"].as_str().and_then(Level::parse).ok_or("schemaDangerLevels: level must be SAFE, CAUTION or DANGER.")?;
            Ok((kind.to_owned(), level))
        })
        .collect()
}

pub(crate) fn overrides() -> Result<Vec<(String, Level)>, Error> {
    parse_overrides(host::setting("schemaDangerLevels").as_deref()).map_err(Error::from)
}

pub(crate) fn level_of(planned: &PlannedChange, overrides: &[(String, Level)]) -> Level {
    let kind = change_type(&planned.change);
    overrides.iter().rev().find(|(name, _)| name == kind).map_or_else(|| default_level(planned.danger), |(_, level)| *level)
}

fn subject(change: &Change) -> String {
    match change {
        Change::CreateTable { table } => table.name.clone(),
        Change::AddColumn { table, column } => format!("{table}.{}", column.name),
        Change::DropColumn { table, column }
        | Change::AlterColumnType { table, column, .. }
        | Change::AlterNullability { table, column, .. } => format!("{table}.{column}"),
        Change::RenameCandidate { table, from, to } => format!("{table}.{from} -> {to}"),
        Change::DropTable { table }
        | Change::AddForeignKey { table, .. }
        | Change::DropForeignKey { table, .. }
        | Change::AddUnique { table, .. }
        | Change::DropUnique { table, .. }
        | Change::AddIndex { table, .. }
        | Change::DropIndex { table, .. } => table.clone(),
    }
}

/// One line per change, prefixed by its level; the preview appends it to the plan summary.
pub(crate) fn listing(plan: &ChangePlan, overrides: &[(String, Level)]) -> String {
    let mut text = String::from("Levels (SAFE green, CAUTION yellow, DANGER red; per change type in schemaDangerLevels):");
    for planned in plan.changes.iter().take(MAX_LISTED) {
        let level = level_of(planned, overrides);
        text.push_str(&format!("\n[{}] {} {}", level.label(), change_type(&planned.change), subject(&planned.change)));
    }
    if plan.changes.len() > MAX_LISTED {
        text.push_str(&format!("\n... {} more", plan.changes.len() - MAX_LISTED));
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use persistence_core::schema::Change;

    fn planned(change: Change, danger: Danger) -> PlannedChange {
        PlannedChange { change, danger, reason: String::new() }
    }

    #[test]
    fn defaults_map_the_five_dangers_to_three_levels() {
        let drop = planned(Change::DropColumn { table: "t".into(), column: "c".into() }, Danger::DataLoss);
        let nn = planned(Change::AlterNullability { table: "t".into(), column: "c".into(), nullable: false, column_type: None }, Danger::NeedsBackfill);
        let create = planned(Change::DropTable { table: "t".into() }, Danger::Safe);
        assert_eq!(level_of(&drop, &[]), Level::Danger);
        assert_eq!(level_of(&nn, &[]), Level::Caution);
        assert_eq!(level_of(&create, &[]), Level::Safe);
    }

    #[test]
    fn override_applies_per_change_type_and_last_row_wins() {
        let rows = parse_overrides(Some(r#"[{"changeType":"dropColumn","level":"CAUTION"},{"changeType":"dropColumn","level":"safe"}]"#)).unwrap();
        let drop = planned(Change::DropColumn { table: "t".into(), column: "c".into() }, Danger::DataLoss);
        assert_eq!(level_of(&drop, &rows), Level::Safe);
        let table = planned(Change::DropTable { table: "t".into() }, Danger::DataLoss);
        assert_eq!(level_of(&table, &rows), Level::Danger);
    }

    #[test]
    fn invalid_rows_are_refused() {
        assert!(parse_overrides(Some(r#"[{"changeType":"nope","level":"SAFE"}]"#)).is_err());
        assert!(parse_overrides(Some(r#"[{"changeType":"dropColumn","level":"RED"}]"#)).is_err());
        assert!(parse_overrides(Some("{}")).is_err());
        assert!(parse_overrides(None).unwrap().is_empty());
    }

    #[test]
    fn listing_marks_each_change() {
        let plan = ChangePlan { changes: vec![planned(Change::DropColumn { table: "t".into(), column: "c".into() }, Danger::DataLoss)], notes: vec![] };
        assert!(listing(&plan, &[]).contains("[DANGER] dropColumn t.c"));
    }
}
