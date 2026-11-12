//! Primary, secondary and ignored changes of the Liquibase changelog (G111). `schemaChangeLocations` assigns a change
//! type to the primary changelog (the default), to a secondary changelog in its own directory (changes that need
//! manual attention) or to Ignored. Ignored changes are listed in the preview and written nowhere; the refreshed
//! baseline records them, so later runs do not propose them again.
//!
//! A rename candidate keeps the drop and the add it pairs with in its own location, so a merged rename is never split.
use super::*;
use super::migration_action::{absent, absolute};
use persistence_core::liquibase::{self, LiquibaseOptions};
use persistence_core::schema::{Change, ChangePlan, PlannedChange};
use persistence_core::sql;

const MAX_ROWS: usize = 32;
const SECONDARY_SUFFIX: &str = "-secondary";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Location {
    Primary,
    Secondary,
    Ignored,
}

pub(crate) fn parse_routes(raw: Option<&str>) -> Result<Vec<(String, Location)>, String> {
    let Some(raw) = raw else {
        return Ok(Vec::new());
    };
    let rows: Vec<serde_json::Value> = serde_json::from_str(raw).map_err(|_| "schemaChangeLocations is not a list of rows.".to_owned())?;
    if rows.len() > MAX_ROWS {
        return Err(format!("schemaChangeLocations has more than {MAX_ROWS} rows."));
    }
    rows.iter()
        .map(|row| {
            let kind = row["changeType"].as_str().unwrap_or_default();
            if !crate::file_actions::danger_levels::CHANGE_TYPES.contains(&kind) {
                return Err(format!("schemaChangeLocations: unknown changeType {kind:?}."));
            }
            let location = match row["location"].as_str().unwrap_or_default() {
                "primary" => Location::Primary,
                "secondary" => Location::Secondary,
                "ignored" => Location::Ignored,
                _ => return Err("schemaChangeLocations: location must be primary, secondary or ignored.".to_owned()),
            };
            Ok((kind.to_owned(), location))
        })
        .collect()
}

fn location_of(change: &Change, routes: &[(String, Location)]) -> Location {
    let kind = crate::file_actions::danger_levels::change_type(change);
    routes.iter().rev().find(|(name, _)| name == kind).map_or(Location::Primary, |(_, location)| *location)
}

pub(crate) struct Routing {
    pub secondary: ChangePlan,
    pub ignored: Vec<PlannedChange>,
}

/// Leaves the primary changes in `plan` and returns the rest.
pub(crate) fn split_with(plan: &mut ChangePlan, routes: &[(String, Location)]) -> Routing {
    let candidates: Vec<(String, String, String, Location)> = plan
        .changes
        .iter()
        .filter_map(|p| match &p.change {
            Change::RenameCandidate { table, from, to } => Some((table.clone(), from.clone(), to.clone(), location_of(&p.change, routes))),
            _ => None,
        })
        .collect();
    let route = |change: &Change| -> Location {
        let paired = match change {
            Change::DropColumn { table, column } => candidates.iter().find(|(t, from, _, _)| t.eq_ignore_ascii_case(table) && from.eq_ignore_ascii_case(column)),
            Change::AddColumn { table, column } => candidates.iter().find(|(t, _, to, _)| t.eq_ignore_ascii_case(table) && to.eq_ignore_ascii_case(&column.name)),
            _ => None,
        };
        paired.map_or_else(|| location_of(change, routes), |(_, _, _, location)| *location)
    };
    let mut routing = Routing { secondary: ChangePlan::default(), ignored: Vec::new() };
    let mut kept = Vec::new();
    for planned in std::mem::take(&mut plan.changes) {
        match route(&planned.change) {
            Location::Primary => kept.push(planned),
            Location::Secondary => routing.secondary.changes.push(planned),
            Location::Ignored => routing.ignored.push(planned),
        }
    }
    plan.changes = kept;
    routing
}

pub(crate) fn split(plan: &mut ChangePlan) -> Result<Routing, Error> {
    let routes = parse_routes(host::setting("schemaChangeLocations").as_deref()).map_err(Error::from)?;
    Ok(split_with(plan, &routes))
}

pub(crate) struct SecondaryFile {
    pub path: String,
    pub text: String,
}

/// Options restricted to what one plan can honour: the emitter refuses decisions for changes it does not hold.
fn options_for(plan: &ChangePlan, base: &LiquibaseOptions) -> LiquibaseOptions {
    let mut options = base.clone();
    options.emit.renames.retain(|r| {
        plan.changes.iter().any(|c| matches!(&c.change, Change::RenameCandidate { table, from, to }
            if table.eq_ignore_ascii_case(&r.table) && from.eq_ignore_ascii_case(&r.from) && to.eq_ignore_ascii_case(&r.to)))
    });
    let targets = sql::backfill_targets(plan, &options.emit.renames);
    options.emit.backfills.retain(|b| targets.iter().any(|t| t.table.eq_ignore_ascii_case(&b.table) && t.column.eq_ignore_ascii_case(&b.column)));
    options
}

/// Primary options for the primary plan only (renames and backfills of secondary changes are removed).
pub(crate) fn primary_options(plan: &ChangePlan, base: &LiquibaseOptions) -> LiquibaseOptions {
    options_for(plan, base)
}

fn secondary_directory(root: &str, primary_directory: &str) -> Result<String, Error> {
    let directory = match host::setting_string("liquibaseSecondaryDirectory").map(|v| v.trim().to_owned()).filter(|v| !v.is_empty()) {
        Some(relative) => absolute(root, relative.trim_matches('/')),
        None => format!("{}{SECONDARY_SUFFIX}", primary_directory.trim_end_matches('/')),
    };
    if path(&directory).as_deref() != Some(directory.as_str()) || !inside(root, &directory) {
        return Err("liquibaseSecondaryDirectory must be a plain directory inside the project.".into());
    }
    if directory.trim_end_matches('/') == primary_directory.trim_end_matches('/') {
        return Err("The secondary changelog directory must differ from the primary one.".into());
    }
    Ok(directory)
}

pub(crate) fn secondary_file(
    project: &Project,
    root: &str,
    routing: &Routing,
    base: &LiquibaseOptions,
    primary_directory: &str,
    name: &str,
) -> Result<Option<SecondaryFile>, Error> {
    if routing.secondary.changes.is_empty() {
        return Ok(None);
    }
    let options = options_for(&routing.secondary, base);
    let emitted = liquibase::emit_xml(&routing.secondary, &options).map_err(|e| Error::from(format!("The secondary changelog cannot be generated: {e:?}")))?;
    if emitted.emitted() == 0 {
        return Err("Every secondary change is withheld or unsupported; route those change types elsewhere or include their danger levels.".into());
    }
    let path = format!("{}/{name}", secondary_directory(root, primary_directory)?);
    absent(project, &path)?;
    Ok(Some(SecondaryFile { path, text: emitted.render() }))
}

pub(crate) fn preview(routing: &Routing, secondary: Option<&SecondaryFile>) -> String {
    let mut text = String::new();
    if let Some(file) = secondary {
        text.push_str(&format!(
            "Secondary changelog (changes needing manual attention, {} changes; not linked to the master, include it where it should run): {}\n{}\n",
            routing.secondary.changes.len(),
            file.path,
            file.text
        ));
    }
    if !routing.ignored.is_empty() {
        text.push_str("Ignored (written nowhere; the refreshed baseline records them, so later runs do not propose them again):\n");
        for planned in &routing.ignored {
            text.push_str(&format!("  {} - {}\n", crate::file_actions::danger_levels::change_type(&planned.change), planned.reason));
        }
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use persistence_core::schema::Danger;

    fn planned(change: Change, danger: Danger) -> PlannedChange {
        PlannedChange { change, danger, reason: "r".into() }
    }

    fn plan() -> ChangePlan {
        ChangePlan {
            changes: vec![
                planned(Change::DropColumn { table: "t".into(), column: "old".into() }, Danger::DataLoss),
                planned(Change::DropTable { table: "u".into() }, Danger::DataLoss),
                planned(Change::DropColumn { table: "t".into(), column: "x".into() }, Danger::DataLoss),
                planned(Change::RenameCandidate { table: "t".into(), from: "old".into(), to: "new".into() }, Danger::Ambiguous),
            ],
            notes: vec![],
        }
    }

    #[test]
    fn change_types_are_routed_and_default_is_primary() {
        let routes = parse_routes(Some(r#"[{"changeType":"dropTable","location":"ignored"},{"changeType":"dropColumn","location":"secondary"}]"#)).unwrap();
        let mut p = plan();
        let routing = split_with(&mut p, &routes);
        assert_eq!(p.changes.len(), 2, "the paired drop stays with its rename candidate");
        assert_eq!(routing.secondary.changes.len(), 1);
        assert_eq!(routing.ignored.len(), 1);
        let mut all = plan();
        assert!(split_with(&mut all, &[]).secondary.changes.is_empty());
        assert_eq!(all.changes.len(), 4);
    }

    #[test]
    fn a_rename_keeps_its_drop_and_add_together() {
        let routes = parse_routes(Some(r#"[{"changeType":"renameColumn","location":"secondary"}]"#)).unwrap();
        let mut p = plan();
        let routing = split_with(&mut p, &routes);
        assert_eq!(routing.secondary.changes.len(), 2);
        assert_eq!(p.changes.len(), 2);
    }

    #[test]
    fn invalid_routes_are_refused() {
        assert!(parse_routes(Some(r#"[{"changeType":"nope","location":"primary"}]"#)).is_err());
        assert!(parse_routes(Some(r#"[{"changeType":"dropTable","location":"elsewhere"}]"#)).is_err());
    }
}
