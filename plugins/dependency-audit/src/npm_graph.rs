//! Read-only resolved physical edges from npm package-lock v2/v3. This does not
//! evaluate platform/peer constraints or run npm, and linked workspaces stay unknown.
use crate::*;
use serde_json::Value;

pub(crate) const VIEW: &str = "npm-dependency-graph";
pub(crate) struct Snapshot {
    root: String,
    path: String,
    text: String,
    items: Vec<TreeItem>,
    targets: HashMap<String, usize>,
    status: String,
    expansion: Expansion,
}

fn resolve(packages: &serde_json::Map<String, Value>, parent: &str, name: &str) -> Option<String> {
    if name.is_empty()
        || name.contains(['\\', ':'])
        || name
            .split('/')
            .any(|part| part.is_empty() || part == ".." || part == ".")
    {
        return None;
    }
    let mut base = parent;
    loop {
        if !base.ends_with("node_modules") {
            let key = if base.is_empty() {
                format!("node_modules/{name}")
            } else {
                format!("{base}/node_modules/{name}")
            };
            if packages.get(&key).is_some_and(|entry| {
                entry.get("link").and_then(Value::as_bool) != Some(true)
                    && entry.get("version").and_then(Value::as_str).is_some()
            }) {
                return Some(key);
            }
        }
        if base.is_empty() {
            return None;
        }
        base = base
            .rsplit_once('/')
            .map(|(parent, _)| parent)
            .unwrap_or("");
    }
}

fn model(text: &str) -> Result<(Vec<TreeItem>, HashMap<String, usize>, String), Error> {
    if text.len() > 512 << 10 {
        return Err("Lockfile graph input exceeds512KiB.".into());
    }
    let root: Value = serde_json::from_str(text)
        .map_err(|_| Error::Invalid("Invalid package lock JSON.".into()))?;
    if !matches!(
        root.get("lockfileVersion").and_then(Value::as_u64),
        Some(2 | 3)
    ) {
        return Err("Graph inspection currently supports package-lock v2/v3.".into());
    }
    let packages = root
        .get("packages")
        .and_then(Value::as_object)
        .filter(|packages| packages.len() <= 2000)
        .ok_or("A bounded packages map is required (at most2000 entries).")?;
    let root_members =
        manifest::object_members(text, 0).ok_or("Ambiguous lockfile root ownership.")?;
    let package_at = root_members
        .iter()
        .find(|(name, _, _)| name == "packages")
        .map(|(_, _, at)| *at)
        .ok_or("Missing packages source.")?;
    let source_packages = manifest::object_members(text, package_at)
        .ok_or("Duplicate or unknown package source ownership.")?;
    let mut positions = HashMap::new();
    for (name, span, at) in source_packages {
        let fields =
            manifest::object_members(text, at).ok_or("Ambiguous package record ownership.")?;
        for (field, _, at) in fields {
            if [
                "dependencies",
                "optionalDependencies",
                "peerDependencies",
                "devDependencies",
            ]
            .contains(&field.as_str())
                && manifest::object_members(text, at).is_none()
            {
                return Err("Ambiguous dependency edge ownership.".into());
            }
        }
        positions.insert(name, span.0);
    }
    let mut items = Vec::new();
    let mut targets = HashMap::new();
    let mut unknown = 0usize;
    let mut edges = 0usize;
    for (index, (path, entry)) in packages.iter().enumerate() {
        let node = format!("npm-node:{index}");
        let label = if path.is_empty() {
            "Project root"
        } else {
            path
        };
        items.push(
            TreeItem::new(&node, label)
                .expandable(path.is_empty())
                .icon("package")
                .detail(
                    entry
                        .get("version")
                        .and_then(Value::as_str)
                        .unwrap_or("No resolved version"),
                ),
        );
        if let Some(at) = positions.get(path.as_str()) {
            targets.insert(node.clone(), *at);
        }
        let mut dependencies = std::collections::BTreeMap::new();
        for kind in [
            "dependencies",
            "optionalDependencies",
            "peerDependencies",
            "devDependencies",
        ] {
            if kind == "devDependencies" && !path.is_empty() {
                continue;
            }
            for (name, requirement) in entry
                .get(kind)
                .and_then(Value::as_object)
                .into_iter()
                .flatten()
            {
                dependencies.insert(
                    (name, kind),
                    requirement.as_str().unwrap_or("Unknown requirement"),
                );
            }
        }
        for ((name, kind), requirement) in dependencies {
            edges += 1;
            if edges > 8000 {
                return Err("Lockfile graph exceeds8000 edges.".into());
            }
            let key = format!("npm-edge:{index}:{edges}");
            let target = resolve(packages, path, name);
            let detail = match &target {
                Some(target) => format!(
                    "{kind} {requirement} → {target} ({})",
                    packages[target]
                        .get("version")
                        .and_then(Value::as_str)
                        .unwrap_or("unknown")
                ),
                None => {
                    unknown += 1;
                    format!("{kind} {requirement} → unresolved or linked workspace")
                }
            };
            items.push(
                TreeItem::new(&key, name)
                    .parent(&node)
                    .icon(if target.is_some() {
                        "package"
                    } else {
                        "warning"
                    })
                    .detail(&detail),
            );
            if let Some(at) = target.as_deref().and_then(|target| positions.get(target)) {
                targets.insert(key, *at);
            }
        }
    }
    Ok((
        items,
        targets,
        format!(
            "{} package records · {edges} edges · {unknown} unresolved/linked. Physical lockfile lookup only; optional/peer/platform activation is not evaluated.",
            packages.len()
        ),
    ))
}
impl Snapshot {
    pub(crate) fn capture(ctx: &Invocation) -> Result<Self, Error> {
        let editor = ctx.editor().ok_or("Open package-lock.json first.")?;
        let path = editor.path();
        if !path.ends_with("/package-lock.json") {
            return Err("Open package-lock.json first.".into());
        }
        let project = ctx.project().ok_or(Error::Stale)?;
        let root = project.root()?;
        if !owned_source_path(&root, &path) {
            return Err(Error::Stale);
        }
        let text = editor.document()?.text;
        let (items, targets, status) = model(&text)?;
        Ok(Self {
            root,
            path,
            text,
            items,
            targets,
            status,
            expansion: Expansion::default(),
        })
    }
    fn current(&self, ctx: &Invocation) -> bool {
        ctx.project().is_some_and(|p| {
            p.root().ok().as_deref() == Some(self.root.as_str())
                && owned_source_path(&self.root, &self.path)
                && p.read_file(&self.path).is_ok_and(|text| text == self.text)
        })
    }
    pub(crate) fn render(&self, ctx: &Invocation) -> View {
        if !self.current(ctx) {
            View::message(
                "Dependency graph source changed or belongs to another project. Inspect the lockfile again.",
            )
        } else {
            {
                let mut items = self.items.clone();
                self.expansion.apply(&mut items);
                View::new(Body::Tree(items)).status(&self.status)
            }
        }
    }
    pub(crate) fn handle(&mut self, event: ViewEvent, ctx: &Invocation) -> Result<(), Error> {
        if !self.current(ctx) {
            return Err(Error::Stale);
        }
        match &event {
            ViewEvent::Expand(key) | ViewEvent::Collapse(key)
                if self
                    .items
                    .iter()
                    .any(|item| item.parent.as_deref() == Some(key)) =>
            {
                self.expansion.handle(&event);
                host::refresh_view(VIEW);
                Ok(())
            }
            ViewEvent::Activate(key) => self.activate(key, ctx),
            _ => Ok(()),
        }
    }
    pub(crate) fn activate(&self, key: &str, ctx: &Invocation) -> Result<(), Error> {
        if !self.current(ctx) {
            return Err(Error::Stale);
        }
        let Some(at) = self.targets.get(key) else {
            return Ok(());
        };
        let p = openidea_plugin::text::position(&self.text, *at);
        host::open_location(&self.path, p.line, p.character)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nested_lookup_resolves_nearest_physical_version_and_leaves_links_unknown() {
        let text = r#"{"lockfileVersion":3,"packages":{"":{"dependencies":{"a":"*","linked":"*"}},"node_modules/a":{"version":"1","dependencies":{"b":"*"}},"node_modules/b":{"version":"2"},"node_modules/a/node_modules/b":{"version":"1"},"node_modules/linked":{"link":true,"resolved":"packages/linked"}}}"#;
        let (items, targets, status) = model(text).unwrap();
        assert!(items.iter().any(|item| {
            item.detail
                .as_deref()
                .is_some_and(|detail| detail.contains("→ node_modules/a/node_modules/b (1)"))
        }));
        assert!(status.contains("1 unresolved/linked"));
        assert!(!targets.is_empty());
        assert!(model(r#"{"lockfileVersion":1}"#).is_err());
    }
    #[test]
    fn navigation_uses_package_owner_not_equal_nested_metadata_key() {
        let text = r#"{"lockfileVersion":3,"packages":{"":{"dependencies":{"b":"*"},"metadata":{"node_modules/b":{}}},"node_modules/b":{"version":"1"}}}"#;
        let (_, targets, _) = model(text).unwrap();
        let expected = text.rfind("node_modules/b").unwrap();
        assert!(
            targets
                .values()
                .all(|at| *at == expected || *at == text.find("\"\":{").unwrap() + 1)
        );
        assert!(model(r#"{"lockfileVersion":3,"packages":{"node_modules/b":{"version":"1"},"node_modules/b":{"version":"2"}}}"#).is_err());
    }
    #[test]
    fn native_expansion_survives_rerender_and_leaf_nodes_do_not_show_spinners() {
        let h = openidea_plugin::mock::MockHost::install();
        let text = r#"{"lockfileVersion":3,"packages":{"":{"dependencies":{"a":"*"}},"node_modules/a":{"version":"1","dependencies":{"b":"*"}},"node_modules/b":{"version":"2"}}}"#;
        h.open_editor("/project/package-lock.json", "json", text, &[(0, 0, 0, 0)]);
        let mut s = Snapshot::capture(&h.invocation()).unwrap();
        let view = s.render(&h.invocation());
        let Body::Tree(items) = view.body else {
            panic!()
        };
        let node = items
            .iter()
            .find(|item| item.label == "node_modules/a")
            .unwrap();
        assert!(node.expandable && !node.expanded);
        let key = node.key.clone();
        s.handle(ViewEvent::Expand(key.clone()), &h.invocation())
            .unwrap();
        let Body::Tree(items) = s.render(&h.invocation()).body else {
            panic!()
        };
        assert!(items.iter().find(|item| item.key == key).unwrap().expanded);
        assert!(
            items
                .iter()
                .filter(|item| item.parent.is_some())
                .all(|item| !item.expandable)
        );
    }
    #[test]
    fn snapshot_uses_current_buffer_and_withholds_stale_navigation() {
        let h = openidea_plugin::mock::MockHost::install();
        let text = r#"{"lockfileVersion":3,"packages":{"":{"dependencies":{"a":"*"}},"node_modules/a":{"version":"1"}}}"#;
        h.open_editor("/project/package-lock.json", "json", text, &[(0, 0, 0, 0)]);
        let s = Snapshot::capture(&h.invocation()).unwrap();
        let key = s.targets.keys().next().unwrap();
        s.activate(key, &h.invocation()).unwrap();
        h.type_in_editor("{}");
        assert!(matches!(
            s.activate(key, &h.invocation()),
            Err(Error::Stale)
        ));
        assert!(h.requests().is_empty() && h.process_runs().is_empty());
    }
}
