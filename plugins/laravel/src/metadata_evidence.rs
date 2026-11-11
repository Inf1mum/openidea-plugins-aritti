//! Evidence for reads already performed while constructing metadata snapshots.
use super::*;
pub(crate) type SourceEvidence = BTreeMap<String, Option<(usize, String)>>;
pub(crate) fn merge<K: Ord, V: PartialEq>(
    map: &mut BTreeMap<K, V>,
    incomplete: &mut bool,
    entries: impl IntoIterator<Item = (K, V)>,
) {
    for (key, value) in entries {
        if map.get(&key).is_some_and(|old| old != &value) {
            *incomplete = true;
        }
        map.insert(key, value);
    }
}
pub(crate) fn record(evidence: &mut SourceEvidence, path: &str, text: &str, incomplete: &mut bool) {
    merge(
        evidence,
        incomplete,
        [(
            path.into(),
            Some((text.len(), crate::artisan_metadata::hash(text))),
        )],
    );
}
pub(crate) fn missing(evidence: &mut SourceEvidence, path: &str, incomplete: &mut bool) {
    merge(evidence, incomplete, [(path.into(), None)]);
}
pub(crate) fn read(
    project: &Project,
    root: &str,
    path: &str,
    limit: usize,
    evidence: &mut SourceEvidence,
    incomplete: &mut bool,
) -> Option<String> {
    if host::project_of(&format!("{root}/{path}"))
        .and_then(|p| p.root().ok())
        .as_deref()
        != Some(root)
    {
        *incomplete = true;
        return None;
    }
    let absolute = format!("{root}/{path}");
    let stat = match project.stat(&absolute) {
        Ok(stat) => stat,
        Err(openidea_plugin::Error::NotFound(_)) => {
            missing(evidence, path, incomplete);
            return None;
        }
        Err(_) => {
            *incomplete = true;
            return None;
        }
    };
    if stat.kind != openidea_plugin::FileKind::File || stat.size > limit as u64 {
        *incomplete = true;
        return None;
    }
    let text = match project.read_file(&absolute) {
        Ok(text) if text.len() <= limit => text,
        _ => {
            *incomplete = true;
            return None;
        }
    };
    record(evidence, path, &text, incomplete);
    Some(text)
}
#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;
    #[test]
    fn accepted_and_proven_missing_sources_have_distinct_evidence() {
        let h = MockHost::install();
        h.add_file("ide.json", "{}");
        let p = h.invocation().project().unwrap();
        let mut facts = SourceEvidence::new();
        let mut incomplete = false;
        assert_eq!(
            read(&p, "/project", "ide.json", 32, &mut facts, &mut incomplete).as_deref(),
            Some("{}")
        );
        assert_eq!(
            facts["ide.json"],
            Some((2, crate::artisan_metadata::hash("{}")))
        );
        assert!(
            read(
                &p,
                "/project",
                "missing.json",
                32,
                &mut facts,
                &mut incomplete
            )
            .is_none()
        );
        assert_eq!(facts["missing.json"], None);
        assert!(!incomplete);
    }
    #[test]
    fn oversize_and_foreign_sources_never_become_missing_proof() {
        let h = MockHost::install();
        h.add_file("ide.json", "{}");
        h.add_file("nested/ide.json", "{}");
        h.add_project("/project/nested");
        let p = h.invocation().project().unwrap();
        let mut facts = SourceEvidence::new();
        let mut incomplete = false;
        assert!(read(&p, "/project", "ide.json", 1, &mut facts, &mut incomplete).is_none());
        assert!(incomplete);
        assert!(!facts.contains_key("ide.json"));
        incomplete = false;
        assert!(
            read(
                &p,
                "/project",
                "nested/ide.json",
                32,
                &mut facts,
                &mut incomplete
            )
            .is_none()
        );
        assert!(incomplete);
        assert!(!facts.contains_key("nested/ide.json"));
    }
    #[test]
    fn repeated_reads_and_queries_cannot_certify_a_mixed_snapshot() {
        let mut sources = SourceEvidence::new();
        let mut bad = false;
        record(&mut sources, "ide.json", "{}", &mut bad);
        record(&mut sources, "ide.json", "{}", &mut bad);
        assert!(!bad);
        missing(&mut sources, "ide.json", &mut bad);
        assert!(bad);
        let mut queries = BTreeMap::new();
        bad = false;
        merge(
            &mut queries,
            &mut bad,
            [(
                ("**/ide.json".to_string(), 129u32),
                Some(vec!["/project/ide.json".to_string()]),
            )],
        );
        merge(
            &mut queries,
            &mut bad,
            [(("**/ide.json".to_string(), 129u32), Some(Vec::new()))],
        );
        assert!(bad);
    }
}
