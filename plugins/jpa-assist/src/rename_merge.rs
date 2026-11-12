//! Rename merging for the Flyway and Liquibase actions (G109): a dropped column plus an added column that the diff
//! pairs as a rename candidate can be merged into one rename statement; the redundant drop and add are then not emitted.
use super::*;
use persistence_core::schema::{Change, ChangePlan};
use persistence_core::sql::{EmitOptions, RenameDecision};

fn candidates(plan: &ChangePlan) -> Vec<RenameDecision> {
    plan.changes
        .iter()
        .filter_map(|planned| match &planned.change {
            Change::RenameCandidate { table, from, to } => Some(RenameDecision { table: table.clone(), from: from.clone(), to: to.clone() }),
            _ => None,
        })
        .collect()
}

fn conflicting(chosen: &[RenameDecision]) -> bool {
    chosen.iter().enumerate().any(|(i, a)| chosen[..i].iter().any(|b| b.table == a.table && (b.from == a.from || b.to == a.to)))
}

/// Asks per rename candidate, only when the plan holds some; "keep" leaves the drop and the add as they are.
pub(crate) fn ask_renames(plan: &ChangePlan, options: &mut EmitOptions) -> Result<(), Error> {
    let found = candidates(plan);
    if found.is_empty() {
        return Ok(());
    }
    let mut chosen = Vec::new();
    for candidate in found {
        let items = [
            PickItem::new("keep", "Keep the drop and the add").description("DROP COLUMN and ADD COLUMN stay as two statements"),
            PickItem::new("merge", "Merge into one rename").description("one rename statement; the redundant drop and add are removed"),
        ];
        let title = format!("{}.{} is dropped and {} is added: merge into a rename?", candidate.table, candidate.from, candidate.to);
        match host::pick(&title, &items)?.ok_or(Error::Cancelled)?.as_str() {
            "keep" => {}
            "merge" => chosen.push(candidate),
            _ => return Err(Error::Stale),
        }
    }
    if conflicting(&chosen) {
        return Err("Two merges would rename the same column or produce the same column; pick at most one per column.".into());
    }
    options.renames.extend(chosen);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decision(from: &str, to: &str) -> RenameDecision {
        RenameDecision { table: "t".into(), from: from.into(), to: to.into() }
    }

    #[test]
    fn two_merges_of_one_column_conflict() {
        assert!(conflicting(&[decision("a", "b"), decision("a", "c")]));
        assert!(conflicting(&[decision("a", "c"), decision("b", "c")]));
        assert!(!conflicting(&[decision("a", "b"), decision("c", "d")]));
    }
}
