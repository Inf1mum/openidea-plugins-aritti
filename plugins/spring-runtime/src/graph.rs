//! Same-snapshot Actuator bean IDs only; never a source, debugger or service graph.
use crate::*;
use openidea_plugin::views::{Field, FieldValue};
use spring_text::diagram::{self, Edge, Node};
pub const VIEW: &str = "runtime-graph";
const MAX_BEANS: usize = 4096;
const MAX_NODES: usize = 128;
const MAX_EDGES: usize = 512;
const MAX_DEPS: usize = 64;
#[derive(Clone)]
struct Record {
    position: usize,
    context: String,
    name: String,
    ty: String,
    scope: String,
    deps: Vec<String>,
    total: usize,
    verified: bool,
}
impl Record {
    fn of(position: usize, bean: &Bean) -> Self {
        Self {
            position,
            context: bean.context.clone(),
            name: bean.name.clone(),
            ty: bean.ty.clone(),
            scope: bean.scope.clone(),
            deps: bean.deps.iter().take(MAX_DEPS).map(|dep| presentation(dep, 512)).collect(),
            total: bean.deps.len(),
            verified: bean.dependencies_verified,
        }
    }
    fn matches(&self, bean: &Bean) -> bool {
        self.context == bean.context
            && self.name == bean.name
            && self.ty == bean.ty
            && self.scope == bean.scope
            && self.verified == bean.dependencies_verified
            && self.total == bean.deps.len()
            && self.deps == bean.deps.iter().take(MAX_DEPS).map(|dep| presentation(dep, 512)).collect::<Vec<_>>()
    }
}
#[derive(Default)]
pub(crate) struct State {
    epoch: u64,
    scope: Option<Scope>,
    at: u64,
    records: BTreeMap<String, Record>,
    selected: Option<String>,
    comparison: Option<(String, String)>,
}
impl State {
    pub fn clear(&mut self) {
        self.scope = None;
        self.records.clear();
        self.selected = None;
        self.comparison = None;
    }
}
struct Model {
    nodes: Vec<Node>,
    edges: Vec<Edge>,
    records: BTreeMap<String, Record>,
    unknown: usize,
    unverified: usize,
    omitted: usize,
    ambiguous: usize,
    invalid: usize,
    cyclic: usize,
    limited: bool,
}
fn valid(value: &str, max: usize) -> bool {
    !value.is_empty() && value.len() <= max && !value.chars().any(char::is_control)
}
fn eligible(bean: &Bean) -> bool {
    valid(&bean.context, 512)
        && valid(&bean.name, 512)
        && bean.ty.len() <= 4096
        && bean.scope.len() <= 256
        && !bean.ty.chars().any(char::is_control)
        && !bean.scope.chars().any(char::is_control)
}
/// Explicit selection work only. This is a dated source observation, never an
/// assertion that these sources built the application or cover its libraries.
fn source_comparison(project: &Project, scope: &Scope, ty: &str) -> String {
    let observed = now();
    let unknown = |reason: &str| format!("UNKNOWN: {reason}. No source/runtime match or mismatch inferred.");
    if !valid(ty, 4096) || !ty.contains('.') {
        return unknown("reported qualified Java type is unavailable");
    }
    let owned = |path: &str| -> Option<bool> {
        let valid_path = path.len() <= 4096 && !path.chars().any(char::is_control) && !path.contains('\\')
            && !path.split('/').any(|part| matches!(part, "." | ".."))
            && path.strip_prefix(scope.root.trim_end_matches('/')).is_some_and(|tail| tail.starts_with('/'));
        if !valid_path { return None; }
        host::project_of(path).and_then(|owner| owner.root().ok()).map(|root| root == scope.root)
    };
    let Ok(mut paths) = project.find_files("**/*.java", 2001) else { return unknown("Java discovery denied or failed"); };
    if paths.len() >= 2001 { return unknown("Java discovery exceeded the 2,000-file comparison budget"); }
    paths.sort(); paths.dedup();
    let deadline = Instant::now() + COMMAND_BUDGET;
    let mut bytes = 0usize;
    let mut hits = Vec::new();
    for path in paths {
        match owned(&path) {
            Some(true) => {},
            Some(false) => continue,
            None => return unknown("a discovered source has unavailable or invalid ownership"),
        }
        if Instant::now() >= deadline { return unknown("source comparison time budget exhausted"); }
        let Ok(text) = project.read_file(&path) else { return unknown("a discovered source could not be read"); };
        bytes = bytes.saturating_add(text.len());
        if text.len() > 512 << 10 || bytes > 8 << 20 { return unknown("source comparison byte budget exceeded"); }
        if owned(&path) != Some(true) { return unknown("source ownership changed during comparison"); }
        if let Some(types) = index_java(&path, &text) {
            if types.iter().any(|candidate| candidate.fqn == "\0incomplete") { return unknown("source type inventory is incomplete"); }
            hits.extend(types.iter().filter(|candidate| candidate.fqn == ty).take(2usize.saturating_sub(hits.len())).map(|_| path.clone()));
        }
    }
    if Instant::now() >= deadline || controls::verify(project, scope).is_err() {
        return unknown("comparison expired or project/settings changed");
    }
    let result: String = match hits.as_slice() {
        [] => "SOURCE INVENTORY DIFFERENCE: the exact reported type was not observed in discovered current-project Java sources.".into(),
        [path] => format!("Exact reported type observed in one discovered source: {}.", presentation(path, 1024)),
        _ => "AMBIGUOUS: the exact reported type occurs in more than one discovered source declaration.".into(),
    };
    format!("{result}\nComparison captured at Unix {observed}; source files may have changed since. Discovery excludes ignored/excluded files and other open project roots. Dependency/generated/Kotlin types may be outside this inventory. Type presence or absence does not prove source/runtime build identity, effective bean wiring, a stale application, or a running object.")
}
fn model(snapshot: &Snapshot, epoch: u64) -> Result<Model, Error> {
    if snapshot.beans.len() > MAX_BEANS {
        return Err("Beans snapshot exceeds the 4,096-record identity budget; graph withheld rather than guessing uniqueness.".into());
    }
    let mut groups: BTreeMap<(&str, &str), Vec<usize>> = BTreeMap::new();
    let mut invalid = 0;
    for (position, bean) in snapshot.beans.iter().enumerate() {
        if valid(&bean.context, 512) && valid(&bean.name, 512) {
            groups.entry((&bean.context, &bean.name)).or_default().push(position);
        }
        if !eligible(bean) {
            invalid += 1;
        }
    }
    let ambiguous = groups.values().filter(|positions| positions.len() != 1).count();
    let unique: BTreeMap<_, _> = groups
        .into_iter()
        .filter_map(|(identity, positions)| {
            (positions.len() == 1 && eligible(&snapshot.beans[positions[0]])).then_some((identity, positions[0]))
        })
        .collect();
    let chosen: BTreeMap<_, _> =
        unique.iter().take(MAX_NODES).enumerate().map(|(index, (identity, position))| (*identity, (index, *position))).collect();
    let mut result = Model {
        nodes: vec![],
        edges: vec![],
        records: BTreeMap::new(),
        unknown: 0,
        unverified: 0,
        omitted: 0,
        ambiguous,
        invalid,
        cyclic: 0,
        limited: unique.len() > MAX_NODES,
    };
    let mut ids = Vec::new();
    for ((context, name), (index, position)) in &chosen {
        let bean = &snapshot.beans[*position];
        let record = Record::of(*position, bean);
        let mut fingerprint = 0xcbf29ce484222325u64;
        for value in std::iter::once(snapshot.url.as_str())
            .chain([bean.context.as_str(), bean.name.as_str(), bean.ty.as_str(), bean.scope.as_str()])
            .chain(record.deps.iter().map(String::as_str))
        {
            for byte in value.len().to_le_bytes().into_iter().chain(value.bytes()) {
                fingerprint = (fingerprint ^ u64::from(byte)).wrapping_mul(0x100000001b3);
            }
        }
        for byte in record.total.to_le_bytes().into_iter().chain([u8::from(record.verified)]) {
            fingerprint = (fingerprint ^ u64::from(byte)).wrapping_mul(0x100000001b3);
        }
        let id = format!("runtime-graph:{epoch}:{}:{index}:{fingerprint:016x}", snapshot.fetched_at);
        ids.push(id.clone());
        result.nodes.push(Node::new(&id, &presentation(name, 256)).detail(&format!(
            "{} · {}",
            presentation(context, 80),
            presentation(if bean.ty.is_empty() { "type unreported" } else { simple(&bean.ty) }, 80)
        )));
        result.records.insert(id, Record::of(*position, bean));
    }
    let mut edges = BTreeSet::new();
    for ((context, _), (from, position)) in &chosen {
        let bean = &snapshot.beans[*position];
        result.limited |= bean.deps.len() > MAX_DEPS;
        if !bean.dependencies_verified {
            result.unverified += 1;
            continue;
        }
        let mut seen = BTreeSet::new();
        for dependency in bean.deps.iter().take(MAX_DEPS) {
            if !seen.insert(dependency.as_str()) {
                continue;
            }
            if !valid(dependency, 512) {
                result.unknown += 1;
                continue;
            }
            if let Some((to, _)) = chosen.get(&(*context, dependency.as_str())) {
                if edges.len() < MAX_EDGES {
                    edges.insert((*from, *to));
                } else {
                    result.limited = true;
                }
            } else if unique.contains_key(&(*context, dependency.as_str())) {
                result.omitted += 1;
            } else {
                result.unknown += 1;
            }
        }
    }
    // Bounded reachability, no recursive traversal or unbounded cycle enumeration.
    let mut reach = vec![vec![false; ids.len()]; ids.len()];
    for &(from, to) in &edges {
        reach[from][to] = true;
    }
    for via in 0..ids.len() {
        for from in 0..ids.len() {
            if reach[from][via] {
                for to in 0..ids.len() {
                    reach[from][to] |= reach[via][to];
                }
            }
        }
    }
    for (from, to) in edges {
        let cyclic = from == to || reach[to][from];
        result.cyclic += usize::from(cyclic);
        result.edges.push(Edge::new(&ids[from], &ids[to]).label("depends on").warning(cyclic));
    }
    Ok(result)
}
impl Runtime {
    fn graph_record(&self, key: &str, ctx: &Invocation) -> Result<&Record, Error> {
        let scope = Scope::current(&ctx.project().ok_or(Error::Stale)?)?;
        if self.scope.as_ref() != Some(&scope) || self.graph.scope.as_ref() != Some(&scope) {
            return Err(Error::Stale);
        }
        let snapshot = self
            .snapshot
            .as_ref()
            .filter(|snapshot| {
                snapshot.fetched_at == self.graph.at
                    && snapshot.url == scope.url
                    && snapshot.has("beans")
                    && snapshot.beans.len() <= MAX_BEANS
            })
            .ok_or(Error::Stale)?;
        let record = self.graph.records.get(key).ok_or(Error::Stale)?;
        if snapshot.beans.iter().filter(|bean| bean.context == record.context && bean.name == record.name).take(2).count() != 1
            || !snapshot.beans.get(record.position).is_some_and(|bean| record.matches(bean))
        {
            return Err(Error::Stale);
        }
        Ok(record)
    }
    pub(crate) fn graph_select(&mut self, key: &str, ctx: &Invocation) -> Result<(), Error> {
        let ty = self.graph_record(key, ctx)?.ty.clone();
        let project = ctx.project().ok_or(Error::Stale)?;
        let scope = Scope::current(&project)?;
        controls::verify(&project, &scope)?;
        let comparison = source_comparison(&project, &scope, &ty);
        controls::verify(&project, &scope)?;
        self.graph_record(key, ctx)?;
        self.graph.comparison = Some((key.into(), comparison));
        self.graph.selected = Some(key.into());
        host::refresh_view(VIEW);
        Ok(())
    }
    pub(crate) fn graph_back(&mut self) {
        self.graph.selected = None;
        self.graph.comparison = None;
        host::refresh_view(VIEW);
    }
    pub(crate) fn render_graph(&mut self, ctx: &Invocation) -> Result<View, Error> {
        let Some(project) = ctx.project() else {
            self.graph.clear();
            return Ok(View::message("Open a project to inspect its cached Actuator dependency graph."));
        };
        let scope = match Scope::current(&project).and_then(|scope| {
            controls::verify(&project, &scope)?;
            Ok(scope)
        }) {
            Ok(scope) => scope,
            Err(_) => {
                self.graph.clear();
                return Ok(View::message("Current project or Actuator scope is unavailable; cached graph withheld."));
            }
        };
        self.load_scope(scope.clone());
        let Some(snapshot) = &self.snapshot else {
            self.graph.clear();
            return Ok(View::message("No cached snapshot. Use explicit Refresh in Spring Runtime; this graph never fetches."));
        };
        if snapshot.url != scope.url {
            self.graph.clear();
            return Ok(View::message("Cached endpoint identity differs; graph withheld."));
        }
        if !snapshot.has("beans") {
            self.graph.clear();
            return Ok(View::message(
                "Beans endpoint unavailable in this snapshot; no dependency graph can be inferred from other endpoints.",
            ));
        }
        let provenance = format!(
            "Actuator beans snapshot · {} (Unix {}) · {} · project {} · auth mode {} · retained normalized type metadata; source/runtime correspondence UNKNOWN; graph rendering performs no source scan; no debugger object identity",
            age(snapshot.fetched_at),
            snapshot.fetched_at,
            presentation(&snapshot.url, 512),
            presentation(&scope.root, 512),
            scope.auth
        );
        let provenance =
            if self.error.is_some() { format!("{provenance} · last refresh failed; previous snapshot retained") } else { provenance };
        if let Some(key) = self.graph.selected.clone() {
            if let Ok(record) = self.graph_record(&key, ctx) {
                return Ok(View::new(Body::Form(vec![
                    Field::new("bean-id", "Exact reported bean ID", FieldValue::readonly_text(&record.name, false)),
                    Field::new("bean-context", "Exact reported context ID", FieldValue::readonly_text(&record.context, false)),
                    Field::new(
                        "bean-type",
                        "Retained normalized type (not object/classpath identity)",
                        FieldValue::readonly_text(&record.ty, true),
                    ),
                    Field::new("bean-scope", "Reported scope", FieldValue::readonly_text(&record.scope, false)),
                    Field::new(
                        "bean-dependencies",
                        "Reported dependency names (same-snapshot ownership only)",
                        FieldValue::readonly_text(
                            &format!(
                                "{} of {} names shown (raw JSON string-array typing {}):\n{}",
                                record.deps.len(),
                                record.total,
                                if record.verified {
                                    "verified at fetch"
                                } else {
                                    "UNVERIFIED legacy/malformed; outgoing graph edges withheld"
                                },
                                record.deps.join("\n")
                            ),
                            true,
                        ),
                    ),
                    Field::new("graph-provenance", "Provenance / source mismatch", FieldValue::readonly_text(&provenance, true)),
                    Field::new("graph-source-comparison", "Source comparison at explicit selection", FieldValue::readonly_text(
                        self.graph.comparison.as_ref().filter(|(owner, _)| owner == &key).map_or("UNKNOWN: no explicit source comparison retained.", |(_, result)| result.as_str()), true)),
                ]))
                .button(Button::new("graph-back", "Back to Graph"))
                .status(
                    "Read-only cached observation · source/project/build correspondence unverified · current runtime state may differ",
                ));
            }
            self.graph.selected = None;
            self.graph.comparison = None;
        }
        let Some(epoch) = self.graph.epoch.checked_add(1) else {
            return Err(Error::Stale);
        };
        self.graph.epoch = epoch;
        let result = match model(snapshot, epoch) {
            Ok(result) => result,
            Err(error) => {
                self.graph.clear();
                return Ok(View::message(&format!("{error} {provenance}")));
            }
        };
        self.graph.records = result.records;
        self.graph.scope = Some(scope);
        self.graph.at = snapshot.fetched_at;
        let status = format!(
            "{} · {} nodes / {} explicit edges · {} unresolved/unsupported-context references · {} known targets outside node budget · {} unverified dependency records (edges withheld) · {} duplicate identities suppressed · {} invalid records · {} cyclic shown edges (not runtime errors){}{}",
            provenance,
            result.nodes.len(),
            result.edges.len(),
            result.unknown,
            result.omitted,
            result.unverified,
            result.ambiguous,
            result.invalid,
            result.cyclic,
            if result.limited { " · PARTIAL presentation (128 nodes / 512 edges / 64 dependency names per bean)" } else { "" },
            if snapshot.trimmed { " · legacy partial snapshot" } else { "" }
        );
        Ok(View::new(Body::Html(diagram::html(&result.nodes, &result.edges, "No unique valid bean identities in this cached snapshot.")))
            .button(Button::new("graph-observations", "Inspect Snapshot List"))
            .status(&status))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;
    fn bean(context: &str, name: &str, deps: &[&str]) -> Bean {
        Bean {
            context: context.into(),
            name: name.into(),
            ty: "com.acme.Service".into(),
            scope: "singleton".into(),
            deps: deps.iter().map(|name| (*name).into()).collect(),
            dependencies_verified: true,
        }
    }
    fn snapshot(beans: Vec<Bean>) -> Snapshot {
        Snapshot { url: DEFAULT_URL.into(), fetched_at: 123, beans, ..Default::default() }
    }
    fn cached(host: &MockHost, beans: Vec<Bean>) -> Runtime {
        let scope = Scope::current(&host.invocation().project().unwrap()).unwrap();
        let mut snap = snapshot(beans);
        snap.url = scope.url.clone();
        Runtime { scope: Some(scope), snapshot: Some(snap), loaded: true, ..Default::default() }
    }
    #[test]
    fn relationships_require_unique_ids_in_the_same_snapshot_context() {
        let mut snap =
            snapshot(vec![bean("app", "a", &["b", "b", "parent", "db", "missing"]), bean("app", "b", &[]), bean("parent", "parent", &[])]);
        snap.health.push(("db".into(), "UP".into()));
        let graph = model(&snap, 1).unwrap();
        assert_eq!(graph.edges.len(), 1);
        assert_eq!(graph.unknown, 3);
        assert_eq!(graph.nodes.len(), 3);
        assert_eq!(graph.edges[0].label.as_deref(), Some("depends on"));
        let mut duplicate = bean("app", "b", &[]);
        duplicate.ty = "x".repeat(4097);
        snap.beans.push(duplicate);
        let graph = model(&snap, 2).unwrap();
        assert_eq!(graph.ambiguous, 1);
        assert!(graph.edges.is_empty());
        assert_eq!(graph.nodes.len(), 2);
    }
    #[test]
    fn cycles_and_self_edges_are_bounded_observations_not_runtime_errors() {
        let graph = model(&snapshot(vec![bean("app", "a", &["b", "a"]), bean("app", "b", &["a"])]), 1).unwrap();
        assert_eq!(graph.edges.len(), 3);
        assert_eq!(graph.cyclic, 3);
        assert!(graph.edges.iter().all(|edge| edge.warning));
    }
    #[test]
    fn malformed_dependency_json_and_legacy_coercions_never_create_edges() {
        let mut snap = snapshot(vec![]);
        snap.read("beans",&serde_json::json!({"contexts":{"app":{"beans":{"a":{"type":"x.A","dependencies":[true]},"true":{"type":"x.B","dependencies":[]}}}}}));
        assert!(!snap.beans.iter().find(|bean| bean.name == "a").unwrap().dependencies_verified);
        let graph = model(&snap, 1).unwrap();
        assert!(graph.edges.is_empty());
        assert_eq!(graph.unverified, 1);
        let legacy: Bean =
            serde_json::from_value(serde_json::json!({"context":"app","name":"legacy","ty":"x.L","scope":"singleton","deps":["true"]}))
                .unwrap();
        assert!(!legacy.dependencies_verified);
        snap.beans.push(legacy);
        assert!(model(&snap, 2).unwrap().edges.is_empty());
        snap.read("beans", &serde_json::json!({"contexts":{"fresh":{"beans":{"a":{"dependencies":["true"]},"true":{"dependencies":[]}}}}}));
        assert_eq!(model(&snap, 3).unwrap().edges.len(), 1);
    }
    #[test]
    fn node_edge_dependency_and_identity_budgets_are_explicit() {
        let names: Vec<_> = (0..160).map(|n| format!("bean{n:03}")).collect();
        let deps: Vec<_> = names.iter().take(80).map(String::as_str).collect();
        let graph = model(&snapshot(names.iter().map(|name| bean("app", name, &deps)).collect()), 1).unwrap();
        assert_eq!(graph.nodes.len(), MAX_NODES);
        assert!(graph.edges.len() <= MAX_EDGES);
        assert!(graph.limited);
        assert!(model(&snapshot((0..MAX_BEANS + 1).map(|n| bean("app", &format!("b{n}"), &[])).collect()), 2).is_err());
        let graph = model(
            &snapshot(vec![
                bean("app", "", &[]),
                bean(
                    "app",
                    "ok",
                    &["
invalid"],
                ),
            ]),
            3,
        )
        .unwrap();
        assert_eq!(graph.invalid, 1);
        assert_eq!(graph.unknown, 1);
    }
    #[test]
    fn cached_graph_render_and_explicit_source_selection_never_fetch() {
        let host = MockHost::install().with_capabilities(&["project.read", "storage", "net:localhost"]);
        host.on_fetch(|_| panic!("cached graph must never fetch"));
        host.add_file("src/Service.java", "class Service {}");
        let mut runtime = cached(&host, vec![bean("app", "a", &["b"]), bean("app", "b", &[])]);
        let shown = runtime.render(VIEW, None, &host.invocation()).unwrap();
        let Body::Html(html) = shown.body else { panic!("graph") };
        assert!(html.contains("<svg"));
        assert!(!html.contains("cdn."));
        assert!(!html.contains("https://"));
        let status = shown.status.unwrap();
        assert!(status.contains("Unix 123"));
        assert!(status.contains("correspondence UNKNOWN"));
        runtime.error = Some("failed refresh".into());
        let failed = runtime.render_graph(&host.invocation()).unwrap();
        assert!(failed.status.unwrap().contains("previous snapshot retained"));
        assert_eq!(runtime.snapshot.as_ref().unwrap().beans.len(), 2);
        let key = runtime.graph.records.keys().next().unwrap().clone();
        runtime.handle(VIEW, ViewEvent::Message(key), &host.invocation()).unwrap();
        let shown = runtime.render(VIEW, None, &host.invocation()).unwrap();
        let Body::Form(fields) = shown.body else { panic!("read-only observation") };
        assert!(fields.iter().all(|field| matches!(field.value, FieldValue::Other { ref kind, .. } if kind=="readonly-text")));
        assert!(host.opened_locations().is_empty());
    }
    #[test]
    fn scope_and_current_record_guards_reject_stale_or_forged_node_keys() {
        let host = MockHost::install();
        let mut runtime = cached(&host, vec![bean("app", "a", &[])]);
        runtime.render_graph(&host.invocation()).unwrap();
        let key = runtime.graph.records.keys().next().unwrap().clone();
        assert!(runtime.graph_select("forged", &host.invocation()).is_err());
        runtime.snapshot.as_mut().unwrap().beans[0].ty = "different.Type".into();
        assert!(runtime.graph_select(&key, &host.invocation()).is_err());
        runtime.snapshot.as_mut().unwrap().beans[0].ty = "com.acme.Service".into();
        host.set_setting("actuatorAuth", "\"basic\"");
        assert!(runtime.graph_select(&key, &host.invocation()).is_err());
        host.set_setting("actuatorAuth", "\"none\"");
        host.set_project_root(Some("/other"));
        assert!(runtime.graph_select(&key, &host.invocation()).is_err());
        let shown = runtime.render_graph(&host.invocation()).unwrap();
        assert!(matches!(shown.body, Body::Message(_)));
        assert!(runtime.graph.records.is_empty());
    }
    #[test]
    fn redraw_clear_and_missing_endpoint_do_not_reuse_previous_graph_members() {
        let host = MockHost::install();
        let mut runtime = cached(&host, vec![bean("app", "a", &[])]);
        runtime.render_graph(&host.invocation()).unwrap();
        let key = runtime.graph.records.keys().next().unwrap().clone();
        runtime.render_graph(&host.invocation()).unwrap();
        assert!(runtime.graph_select(&key, &host.invocation()).is_err());
        runtime.snapshot.as_mut().unwrap().missing.push("beans (404)".into());
        let shown = runtime.render_graph(&host.invocation()).unwrap();
        assert!(matches!(shown.body,Body::Message(message) if message.contains("unavailable")));
        assert!(runtime.graph.records.is_empty());
        runtime.clear(&host.invocation().project().unwrap()).unwrap();
        assert!(runtime.graph.records.is_empty());
        assert!(runtime.snapshot.is_none());
    }
    #[test]
    fn explicit_source_comparison_reports_exact_presence_difference_and_ambiguity_without_build_claims() {
        let host = MockHost::install();
        let project = host.invocation().project().unwrap();
        let scope = Scope::current(&project).unwrap();
        host.add_file("src/Other.java", "package elsewhere; class Service {}");
        let absent = source_comparison(&project, &scope, "com.acme.Service");
        assert!(absent.contains("SOURCE INVENTORY DIFFERENCE"));
        for caveat in ["captured at Unix", "ignored/excluded", "Dependency/generated/Kotlin", "does not prove source/runtime build identity"] {
            assert!(absent.contains(caveat), "{absent}");
        }
        host.add_file("src/Service.java", "package com.acme; class Service {}");
        let present = source_comparison(&project, &scope, "com.acme.Service");
        assert!(present.contains("observed in one discovered source: /project/src/Service.java"));
        host.add_file("test/Service.java", "package com.acme; class Service {}");
        assert!(source_comparison(&project, &scope, "com.acme.Service").contains("AMBIGUOUS"));
        assert_eq!(host.fetches(), 0);
        assert!(host.opened_locations().is_empty());
    }
    #[test]
    fn source_comparison_excludes_nested_project_candidates_and_refuses_partial_inventories() {
        let host = MockHost::install();
        let project = host.invocation().project().unwrap();
        let scope = Scope::current(&project).unwrap();
        host.add_project("/project/nested");
        host.add_file("nested/Service.java", "package com.acme; class Service {}");
        assert!(source_comparison(&project, &scope, "com.acme.Service").contains("SOURCE INVENTORY DIFFERENCE"));
        host.add_file("src/TooLarge.java", &" ".repeat((512 << 10) + 1));
        let partial = source_comparison(&project, &scope, "com.acme.Service");
        assert!(partial.starts_with("UNKNOWN:"));
        assert!(!partial.contains("SOURCE INVENTORY DIFFERENCE"));
        assert!(source_comparison(&project, &scope, "").starts_with("UNKNOWN:"));
    }
    #[test]
    fn capped_java_discovery_never_produces_a_source_absence_claim() {
        let host = MockHost::install();
        let project = host.invocation().project().unwrap();
        let scope = Scope::current(&project).unwrap();
        for n in 0..2001 { host.add_file(&format!("src/T{n}.java"), "class T {}"); }
        let comparison = source_comparison(&project, &scope, "com.acme.Service");
        assert!(comparison.contains("UNKNOWN: Java discovery exceeded"));
        assert!(!comparison.contains("SOURCE INVENTORY DIFFERENCE"));
    }
    #[test]
    fn failed_discovery_or_unverifiable_source_ownership_never_becomes_an_inventory_difference() {
        for denied in [false, true] {
            let host = MockHost::install();
            let project = host.invocation().project().unwrap();
            let scope = Scope::current(&project).unwrap();
            host.add_file("src/../Service.java", "package com.acme; class Service {}");
            let _host = if denied { host.with_capabilities(&["storage"]) } else { host };
            let comparison = source_comparison(&project, &scope, "com.acme.Service");
            assert!(comparison.starts_with("UNKNOWN:"), "{comparison}");
            assert!(!comparison.contains("SOURCE INVENTORY DIFFERENCE"));
        }
    }
    #[test]
    fn graph_source_comparison_is_a_dated_explicit_selection_observation_and_expires_with_selection() {
        let host = MockHost::install();
        host.on_fetch(|_| panic!("source comparison must never fetch"));
        let mut runtime = cached(&host, vec![bean("app", "a", &[])]);
        runtime.render_graph(&host.invocation()).unwrap();
        assert!(runtime.graph.comparison.is_none(), "render never compares source");
        let key = runtime.graph.records.keys().next().unwrap().clone();
        runtime.graph_select(&key, &host.invocation()).unwrap();
        let captured = runtime.graph.comparison.clone().unwrap();
        assert_eq!(captured.0, key);
        assert!(captured.1.contains("SOURCE INVENTORY DIFFERENCE"));
        host.add_file("src/Service.java", "package com.acme; class Service {}");
        let detail = runtime.render_graph(&host.invocation()).unwrap();
        assert!(format!("{:?}", detail.body).contains("SOURCE INVENTORY DIFFERENCE"), "render retains the dated observation, never silently rescans");
        runtime.graph_select(&key, &host.invocation()).unwrap();
        assert!(runtime.graph.comparison.as_ref().unwrap().1.contains("observed in one discovered source"));
        runtime.graph_back();
        assert!(runtime.graph.comparison.is_none());
        runtime.clear(&host.invocation().project().unwrap()).unwrap();
        assert!(runtime.graph.comparison.is_none());
        assert_eq!(host.fetches(), 0);
    }
    #[test]
    fn hostile_labels_are_escaped_and_never_become_html_or_service_links() {
        let snap = snapshot(vec![bean("app", "<script>alert(1)</script>", &[])]);
        let graph = model(&snap, 1).unwrap();
        let html = diagram::html(&graph.nodes, &graph.edges, "empty");
        assert!(html.contains("&lt;script&gt;"));
        assert!(!html.contains("<script>alert(1)"));
        assert!(graph.records.keys().all(|key| key.starts_with("runtime-graph:1:")));
    }
}
