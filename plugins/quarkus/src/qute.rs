//! Explicit source paths only; content generation and Qute type resolution remain LS-owned.
use super::*;
#[cfg(test)]
use openidea_plugin::file_actions::FileActions;
use openidea_plugin::file_actions::{FileAction, FileOperation};
use openidea_plugin::{FileKind, Project};
use spring_text::java as source;
use std::time::{Duration, Instant};
const MAX_TEXT: usize = 128 << 10;
#[derive(Clone)]
pub(super) struct Proof {
    path: String,
    text: String,
    at: Instant,
}
#[derive(Clone)]
struct Pending {
    root: String,
    module: String,
    doc: Document,
    template: Option<String>,
    field_at: Option<usize>,
}
#[derive(Default)]
pub(crate) struct State {
    pub(super) modules: BTreeMap<(String, String), Proof>,
    pending: BTreeMap<String, Pending>,
    inventories: BTreeMap<(String, String), Inventory>,
    removed_templates: BTreeSet<String>,
    navigation_events_lost: bool,
    next: u64,
}
struct Inventory {
    at: Instant,
    paths: BTreeSet<String>,
}
const NAV_TTL: Duration = Duration::from_secs(30);
const MAX_TEMPLATES: usize = 256;
fn absolute(path: &str) -> Option<String> {
    let path = path.replace('\\', "/");
    if path == "/"
        || path.contains("//")
        || path.len() == 3 && path.as_bytes().get(1) == Some(&b':')
    {
        return None;
    }
    let absolute = path.starts_with('/')
        || path.as_bytes().get(1) == Some(&b':')
            && path.as_bytes().get(2) == Some(&b'/')
            && path.as_bytes()[0].is_ascii_alphabetic();
    (absolute
        && path.len() <= 4096
        && !path.chars().any(char::is_control)
        && !path.split('/').any(|p| matches!(p, "." | "..")))
    .then(|| path.trim_end_matches('/').into())
}
fn inside(root: &str, path: &str) -> bool {
    path == root || path.starts_with(&format!("{root}/"))
}
fn module(root: &str, path: &str) -> Option<String> {
    let path = absolute(path)?;
    let module = if let Some(parent) = path.strip_suffix("/pom.xml") {
        parent
    } else if let Some((module, _)) = path.rsplit_once("/src/main/java/") {
        module
    } else if let Some((module, _)) = path.rsplit_once("/src/main/resources/templates/") {
        module
    } else if path == format!("{root}/.quarkus-cache") {
        root
    } else {
        return None;
    };
    inside(root, module).then(|| module.into())
}
fn template_path(path: &str) -> bool {
    if path.is_empty()
        || path.len() > 256
        || path.starts_with('/')
        || !path
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"/_-.".contains(&b))
        || ![".html", ".txt"]
            .iter()
            .any(|extension| path.ends_with(extension))
    {
        return false;
    }
    path.split('/').all(|part| {
        let stem = part.split('.').next().unwrap_or("").to_ascii_uppercase();
        !part.is_empty()
            && !part.starts_with('.')
            && !part.ends_with('.')
            && !matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
            && !(stem.len() == 4
                && (stem.starts_with("COM") || stem.starts_with("LPT"))
                && matches!(stem.as_bytes()[3], b'1'..=b'9'))
    })
}
fn balanced(text: &str) -> bool {
    let mut stack = Vec::new();
    for t in source::tokens(text) {
        if t.s.starts_with('"') && (t.s.len() < 2 || !t.s.ends_with('"')) {
            return false;
        }
        match t.s {
            "(" => stack.push(")"),
            "[" => stack.push("]"),
            "{" => stack.push("}"),
            ")" | "]" | "}" => {
                if stack.pop() != Some(t.s) {
                    return false;
                }
            }
            _ => {}
        }
    }
    stack.is_empty()
}
fn named(file: &source::JavaFile, name: &str, qualified: &str) -> bool {
    !file.types.iter().any(|t| t.name == name)
        && file
            .imports
            .iter()
            .any(|i| !i.is_static && i.path == qualified)
        && !file
            .imports
            .iter()
            .any(|i| !i.is_static && i.path.rsplit('.').next() == Some(name) && i.path != qualified)
}
fn annotation(file: &source::JavaFile, text: &str, a: &source::Annotation) -> bool {
    let written: String = source::tokens(&text[a.at + 1..a.name_at + a.name.len()])
        .iter()
        .map(|t| t.s)
        .collect();
    written == "io.quarkus.qute.Location"
        || written == "Location" && named(file, "Location", "io.quarkus.qute.Location")
}
fn location(doc: &Document, selection: Range) -> Option<(usize, String)> {
    if doc.language != "java"
        || !doc.path.ends_with(".java")
        || doc.text.len() > MAX_TEXT
        || !balanced(&doc.text)
    {
        return None;
    }
    let from = offset(&doc.text, selection.start)?;
    let to = offset(&doc.text, selection.end)?;
    let file = source::parse(&doc.text);
    let shadow = file.types.iter().any(|ty| {
        let header = source::tokens(&doc.text[ty.at + ty.name.len()..ty.end]);
        let header: Vec<_> = header.iter().take_while(|token| token.s != "{").collect();
        header.iter().any(|t| t.s == "<")
            && header
                .iter()
                .any(|t| matches!(t.s, "Template" | "Location"))
    });
    if shadow
        || file.types.iter().any(|t| t.name == "io")
        || file
            .imports
            .iter()
            .any(|i| i.path.rsplit('.').next() == Some("io"))
    {
        return None;
    }
    for field in file.types.iter().flat_map(|t| t.fields()) {
        if field.modifiers.iter().any(|m| m == "static") || from < field.start || to > field.end {
            continue;
        }
        let ty = field.ty.as_ref()?;
        if ty.array
            || !ty.args.is_empty()
            || !(ty.text.trim() == "io.quarkus.qute.Template"
                || ty.text.trim() == "Template"
                    && named(&file, "Template", "io.quarkus.qute.Template"))
        {
            continue;
        }
        let annotations: Vec<_> = field
            .annotations
            .iter()
            .filter(|a| a.name == "Location")
            .collect();
        let [a] = annotations.as_slice() else {
            continue;
        };
        if !annotation(&file, &doc.text, a) {
            continue;
        }
        let value = a.arg("value")?;
        let [literal] = value.strings.as_slice() else {
            continue;
        };
        if source::tokens(&value.text).len() != 1
            || doc.text.get(literal.at..literal.end) != Some(literal.value.as_str())
            || !template_path(&literal.value)
        {
            continue;
        }
        return Some((field.at, literal.value.clone()));
    }
    None
}
/// Only direct, literal Maven dependencies. Profiles, dependencyManagement,
/// property interpolation and mixed XML namespaces do not establish this proof.
pub(super) fn namespace_valid<R>(
    reader: &quick_xml::reader::NsReader<R>,
    element: &quick_xml::events::BytesStart<'_>,
) -> bool {
    use quick_xml::name::ResolveResult;
    const XML: &str = "http://www.w3.org/XML/1998/namespace";
    const XMLNS: &str = "http://www.w3.org/2000/xmlns/";
    if element.name().as_ref().starts_with("xmlns:")
        || matches!(
            reader.resolver().resolve_element(element.name()).0,
            ResolveResult::Unknown(_)
        )
    {
        return false;
    }
    for attribute in element.attributes() {
        let Ok(attribute) = attribute else {
            return false;
        };
        let key = attribute.key.as_ref();
        if key == "xmlns" || key.starts_with("xmlns:") {
            let Ok(value) = attribute.normalized_value(quick_xml::XmlVersion::Implicit1_0) else {
                return false;
            };
            if key == "xmlns:xmlns"
                || key.starts_with("xmlns:") && value.is_empty()
                || key == "xmlns:xml" && value != XML
                || key != "xmlns:xml" && matches!(value.as_ref(), XML | XMLNS)
            {
                return false;
            }
        } else if matches!(
            reader.resolver().resolve_attribute(attribute.key).0,
            ResolveResult::Unknown(_)
        ) {
            return false;
        }
    }
    true
}
fn qute_pom(text: &str) -> bool {
    literal_maven_dependency(text, |artifact| matches!(artifact, "quarkus-qute" | "quarkus-rest-qute"))
}
pub(crate) fn literal_maven_dependency(text: &str, supported: impl Fn(&str) -> bool) -> bool {
    use quick_xml::{events::Event, reader::NsReader};
    if text.len() > MAX_TEXT {
        return false;
    }
    let mut reader = NsReader::from_str(text);
    reader.config_mut().trim_text(true);
    let mut stack: Vec<String> = Vec::new();
    let (mut group, mut artifact, mut found, mut closed, mut nodes) =
        (String::new(), String::new(), false, false, 0);
    let (mut groups, mut artifacts, mut unsupported) = (0, 0, false);
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                if !namespace_valid(&reader, &e) {
                    return false;
                }
                nodes += 1;
                if nodes > 2048 || stack.len() >= 64 {
                    return false;
                }
                let name = e.name().as_ref().to_owned();
                if name.contains(':') {
                    return false;
                }
                if stack.is_empty() && (name != "project" || closed) {
                    return false;
                }
                for attr in e.attributes() {
                    let Ok(attr) = attr else {
                        return false;
                    };
                    if attr.key.as_ref() == "xmlns"
                        && (!stack.is_empty()
                            || attr.value.as_ref() != "http://maven.apache.org/POM/4.0.0")
                    {
                        return false;
                    }
                }
                if stack
                    .iter()
                    .map(String::as_str)
                    .eq(["project", "dependencies", "dependency"])
                    && matches!(
                        name.as_str(),
                        "scope" | "type" | "classifier" | "systemPath"
                    )
                {
                    unsupported = true;
                }
                if stack.iter().map(String::as_str).eq([
                    "project",
                    "dependencies",
                    "dependency",
                    "groupId",
                ]) || stack.iter().map(String::as_str).eq([
                    "project",
                    "dependencies",
                    "dependency",
                    "artifactId",
                ]) {
                    unsupported = true;
                }
                stack.push(name);
                if stack.iter().map(String::as_str).eq([
                    "project",
                    "dependencies",
                    "dependency",
                    "groupId",
                ]) {
                    groups += 1;
                }
                if stack.iter().map(String::as_str).eq([
                    "project",
                    "dependencies",
                    "dependency",
                    "artifactId",
                ]) {
                    artifacts += 1;
                }
                if stack
                    .iter()
                    .map(String::as_str)
                    .eq(["project", "dependencies", "dependency"])
                {
                    group.clear();
                    artifact.clear();
                    groups = 0;
                    artifacts = 0;
                    unsupported = false;
                }
            }
            Ok(Event::Text(text)) => {
                let value = text.as_ref();
                if stack.iter().map(String::as_str).eq([
                    "project",
                    "dependencies",
                    "dependency",
                    "groupId",
                ]) {
                    group.push_str(&value);
                }
                if stack.iter().map(String::as_str).eq([
                    "project",
                    "dependencies",
                    "dependency",
                    "artifactId",
                ]) {
                    artifact.push_str(&value);
                }
            }
            Ok(Event::End(_)) => {
                if stack
                    .iter()
                    .map(String::as_str)
                    .eq(["project", "dependencies", "dependency"])
                {
                    found |= !unsupported
                        && groups == 1
                        && artifacts == 1
                        && group.trim() == "io.quarkus"
                        && supported(artifact.trim());
                }
                if stack.pop().is_none() {
                    return false;
                }
                if stack.is_empty() {
                    closed = true;
                }
            }
            Ok(Event::Empty(e)) => {
                if !namespace_valid(&reader, &e) {
                    return false;
                }
                if stack
                    .iter()
                    .map(String::as_str)
                    .eq(["project", "dependencies", "dependency"])
                    && matches!(
                        e.name().as_ref(),
                        "groupId" | "artifactId" | "scope" | "type" | "classifier" | "systemPath"
                    )
                {
                    unsupported = true;
                }
                if stack.is_empty() || e.name().as_ref().contains(':') {
                    return false;
                }
            }
            Ok(Event::Eof) => return found && closed && stack.is_empty(),
            Ok(Event::DocType(_) | Event::GeneralRef(_) | Event::CData(_)) | Err(_) => {
                return false;
            }
            _ => {}
        }
    }
}
fn proof(project: &Project, module: &str) -> Result<Proof, Error> {
    let path = format!("{module}/pom.xml");
    let metadata = project.stat(&path)?;
    if metadata.kind != FileKind::File || metadata.size > MAX_TEXT as u64 {
        return Err(Error::Stale);
    }
    let text = project.read_file(&path)?;
    if !qute_pom(&text) {
        return Err(Error::Invalid(
            "A direct literal Maven Quarkus Qute dependency is required for this module.".into(),
        ));
    }
    Ok(Proof {
        path,
        text,
        at: Instant::now(),
    })
}
fn collision(project: &Project, root: &str, target: &str) -> Result<(), Error> {
    if !inside(root, target) || absolute(target).as_deref() != Some(target) {
        return Err(Error::Stale);
    }
    if project.exists(target)? {
        return Err(Error::Invalid(
            "The template target already exists or is open; choose a new path.".into(),
        ));
    }
    let mut parent = target.rsplit_once('/').map(|(p, _)| p);
    while let Some(path) = parent {
        if !inside(root, path) {
            return Err(Error::Stale);
        }
        if project.exists(path)? && project.stat(path)?.kind != FileKind::Directory {
            return Err(Error::Invalid(
                "Template parents must be ordinary project directories.".into(),
            ));
        }
        if path == root {
            break;
        }
        parent = path.rsplit_once('/').map(|(p, _)| p);
    }
    Ok(())
}
impl QuarkusTools {
    pub(crate) fn warm_qute(&mut self, path: &str) {
        let Some(project) = project_of(path) else {
            return;
        };
        let Some(root) = project.root().ok().and_then(|r| absolute(&r)) else {
            return;
        };
        let Some(module) = module(&root, path) else {
            return;
        };
        let key = (root, module.clone());
        if self.live.build_events_lost
            || self
                .live
                .removed_builds
                .contains(&format!("{module}/pom.xml"))
        {
            self.qute.modules.remove(&key);
            return;
        }
        match proof(&project, &module) {
            Ok(proof) => {
                if self.qute.modules.len() >= 16 {
                    self.qute.modules.clear();
                }
                self.qute.modules.insert(key.clone(), proof);
                if self.live.watching
                    && !self
                        .qute
                        .inventories
                        .get(&key)
                        .is_some_and(|i| i.at.elapsed() < NAV_TTL)
                {
                    self.qute.inventories.remove(&key);
                    if let Ok(paths) = project.find_files(
                        "**/src/main/resources/templates/**",
                        (MAX_TEMPLATES + 1) as u32,
                    ) {
                        if paths.len() <= MAX_TEMPLATES {
                            let prefix = format!("{module}/src/main/resources/templates/");
                            let known = paths
                                .into_iter()
                                .filter(|path| {
                                    absolute(path).as_deref() == Some(path.as_str())
                                        && path.strip_prefix(&prefix).is_some_and(template_path)
                                        && project_of(path).and_then(|p| p.root().ok()).as_deref()
                                            == Some(key.0.as_str())
                                        && !self.qute.removed_templates.iter().any(|p| {
                                            path == p || path.starts_with(&format!("{p}/"))
                                        })
                                        && project
                                            .stat(path)
                                            .is_ok_and(|s| s.kind == FileKind::File)
                                })
                                .collect();
                            if self.qute.inventories.len() >= 16 {
                                self.qute.inventories.clear();
                            }
                            self.qute.inventories.insert(
                                key,
                                Inventory {
                                    at: Instant::now(),
                                    paths: known,
                                },
                            );
                        }
                    }
                }
            }
            Err(_) => {
                self.qute.modules.remove(&key);
                self.qute.inventories.remove(&key);
            }
        }
    }
}
impl QuarkusTools {
    /// Cache invalidation must precede discovery deadline checks, including removal.
    pub(super) fn invalidate_qute_navigation(&mut self, events: &[openidea_plugin::Event]) {
        use openidea_plugin::Event;
        for event in events {
            if let Event::FilesChanged(b) = event {
                for path in b.created.iter().chain(&b.modified) {
                    self.qute.removed_templates.remove(path);
                }
                for path in b
                    .removed
                    .iter()
                    .filter(|p| p.contains("/src/main/resources/templates/"))
                {
                    if path.len() > 4096 || self.qute.removed_templates.len() >= 1024 {
                        self.qute.navigation_events_lost = true;
                    } else {
                        self.qute.removed_templates.insert(path.clone());
                    }
                }
            }
            let paths: Vec<&str> = match event {
                Event::FilesChanged(b) => b
                    .created
                    .iter()
                    .chain(&b.modified)
                    .chain(&b.removed)
                    .map(String::as_str)
                    .collect(),
                Event::FileOpened(p) | Event::FileSaved(p) => vec![p.as_str()],
                _ => vec![],
            };
            self.qute.inventories.retain(|(_, module), _| {
                !paths.iter().any(|p| {
                    p.starts_with(&format!("{module}/src/main/resources/templates/"))
                        || BUILD_FILES.iter().any(|b| *p == format!("{module}/{b}"))
                })
            });
        }
    }
    pub(super) fn qute_definition(
        &self,
        doc: &Document,
        at: Position,
    ) -> Vec<openidea_plugin::Location> {
        let Some(root) = self.cached_module(doc) else {
            return vec![];
        };
        if !self.live.watching || self.qute.navigation_events_lost {
            return vec![];
        }
        let Some(module) = module(&root, &doc.path) else {
            return vec![];
        };
        let key = (root, module.clone());
        if !self
            .qute
            .modules
            .get(&key)
            .is_some_and(|p| p.at.elapsed() < Duration::from_secs(120))
        {
            return vec![];
        }
        let Some(inventory) = self
            .qute
            .inventories
            .get(&key)
            .filter(|i| i.at.elapsed() < NAV_TTL)
        else {
            return vec![];
        };
        let Some((field_at, path)) = location(doc, Range::new(at, at)) else {
            return vec![];
        };
        // Go-to-definition belongs to the exact literal, not the whole field.
        let Some(off) = offset(&doc.text, at) else {
            return vec![];
        };
        let file = source::parse(&doc.text);
        let literal = file
            .types
            .iter()
            .flat_map(|t| t.fields())
            .filter(|f| f.at == field_at)
            .flat_map(|f| f.annotations.iter())
            .filter(|a| a.name == "Location")
            .filter_map(|a| a.arg("value"))
            .flat_map(|a| a.strings.iter())
            .any(|s| s.at <= off && off < s.end);
        let target = format!("{module}/src/main/resources/templates/{path}");
        if !literal || !inventory.paths.contains(&target) {
            return vec![];
        }
        vec![openidea_plugin::Location {
            path: target,
            range: Range::default(),
        }]
    }
    pub(super) fn qute_file_actions(
        &mut self,
        doc: &Document,
        selection: Range,
    ) -> Vec<FileAction> {
        if doc.text.len() > MAX_TEXT {
            return Vec::new();
        }
        let Some(root) = project_of(&doc.path)
            .and_then(|p| p.root().ok())
            .and_then(|r| absolute(&r))
        else {
            return Vec::new();
        };
        let Some(module) = module(&root, &doc.path) else {
            return Vec::new();
        };
        let source =
            if doc.language == "xml" && doc.path.ends_with("/pom.xml") && qute_pom(&doc.text) {
                None
            } else {
                let Some((at, path)) = location(doc, selection) else {
                    return Vec::new();
                };
                if !self
                    .qute
                    .modules
                    .get(&(root.clone(), module.clone()))
                    .is_some_and(|proof| proof.at.elapsed() < Duration::from_secs(120))
                {
                    return Vec::new();
                }
                Some((at, path))
            };
        let Some(next) = self.qute.next.checked_add(1) else {
            return Vec::new();
        };
        self.qute.next = next;
        if self.qute.pending.len() >= 4 {
            self.qute.pending.clear();
        }
        let token = format!("qute:{next}:{}", doc.version);
        let (field_at, template) = source
            .map(|(at, path)| (Some(at), Some(path)))
            .unwrap_or((None, None));
        let title = template
            .as_ref()
            .map(|path| format!("Create empty Qute template {path}…"))
            .unwrap_or_else(|| "Create empty Qute template in this Maven module…".into());
        self.qute.pending.insert(
            token.clone(),
            Pending {
                root,
                module,
                doc: doc.clone(),
                template,
                field_at,
            },
        );
        vec![FileAction {
            title,
            kind: Some("quickfix".into()),
            data: Some(token),
            ..FileAction::default()
        }]
    }
    pub(super) fn resolve_qute_file_action(
        &mut self,
        doc: &Document,
        action: FileAction,
    ) -> Result<FileAction, Error> {
        let token = action.data.as_deref().ok_or(Error::Stale)?;
        let pending = self.qute.pending.get(token).cloned().ok_or(Error::Stale)?;
        if doc.path != pending.doc.path
            || doc.language != pending.doc.language
            || doc.version != pending.doc.version
            || doc.text != pending.doc.text
        {
            return Err(Error::Stale);
        }
        let project = project_of(&doc.path).ok_or(Error::Stale)?;
        if project.root().ok().and_then(|r| absolute(&r)).as_deref() != Some(&pending.root)
            || project.read_file(&doc.path)? != doc.text
        {
            return Err(Error::Stale);
        }
        if self.live.build_events_lost
            || self
                .live
                .removed_builds
                .contains(&format!("{}/pom.xml", pending.module))
        {
            return Err(Error::Stale);
        }
        let build = proof(&project, &pending.module)?;
        if let Some(at) = pending.field_at {
            let pos = openidea_plugin::text::position(&doc.text, at);
            if location(doc, Range::new(pos, pos))
                .as_ref()
                .map(|(_, path)| path)
                != pending.template.as_ref()
            {
                return Err(Error::Stale);
            }
        }
        let template = match &pending.template {
            Some(path) => path.clone(),
            None => host::input(
                "Template path below src/main/resources/templates (.html or .txt)",
                None,
            )?
            .ok_or(Error::Cancelled)?,
        };
        if !template_path(&template) {
            return Err(Error::Invalid("Use a relative ASCII .html or .txt path without traversal, expressions or reserved names.".into()));
        }
        let target = format!("{}/src/main/resources/templates/{template}", pending.module);
        collision(&project, &pending.root, &target)?;
        let body = format!(
            "Create only: {target}\nModule: {}\nBuild proof: {} (direct literal Qute dependency)\nSource: {}\nSource version: {}\n\nExact new content: empty file (0 bytes).\n\nThe host creates and opens one non-overwriting file as an undoable workspace edit. Template parameters, bindings and runtime lookup are unresolved.",
            pending.module, build.path, doc.path, doc.version
        );
        if !ui_kit::prompts::confirm_exact("Create empty Qute template?", &body)? {
            return Err(Error::Cancelled);
        }
        if project_of(&doc.path)
            .and_then(|p| p.root().ok())
            .and_then(|r| absolute(&r))
            .as_deref()
            != Some(&pending.root)
            || project.read_file(&doc.path)? != doc.text
            || project.read_file(&build.path)? != build.text
        {
            return Err(Error::Stale);
        }
        collision(&project, &pending.root, &target)?;
        self.qute.pending.remove(token);
        Ok(FileAction {
            title: format!("Create Qute template {template}"),
            kind: Some("quickfix".into()),
            operations: vec![FileOperation::Create {
                path: target.clone(),
                text: String::new(),
                overwrite: false,
            }],
            open: Some(target),
            ..FileAction::default()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::{mock::MockHost, text::position};
    const POM: &str = "<project><dependencies><dependency><groupId>io.quarkus</groupId><artifactId>quarkus-qute</artifactId></dependency></dependencies></project>";
    const JAVA: &str = "import io.quarkus.qute.Template; import io.quarkus.qute.Location; class View { /* 😀 */ @Location(\"pages/home.html\") Template home; }";
    fn doc(text: &str) -> Document {
        Document {
            path: "/project/src/main/java/View.java".into(),
            language: "java".into(),
            version: 7,
            text: text.into(),
        }
    }
    fn cursor(doc: &Document) -> Range {
        let pos = position(&doc.text, doc.text.rfind("home;").unwrap());
        Range::new(pos, pos)
    }
    fn setup() -> (MockHost, QuarkusTools, Document) {
        let host = MockHost::install();
        host.add_file("pom.xml", POM);
        host.add_file("src/main/java/View.java", JAVA);
        let mut tools = QuarkusTools::default();
        tools.warm_qute("/project/src/main/java/View.java");
        (host, tools, doc(JAVA))
    }
    #[test]
    fn literal_field_uses_utf16_and_exact_known_namespaces() {
        let doc = doc(JAVA);
        assert_eq!(location(&doc, cursor(&doc)).unwrap().1, "pages/home.html");
        let doc = self::doc(
            &JAVA
                .replace("@Location", "@io.quarkus.qute.Location")
                .replace(" Template home", " io.quarkus.qute.Template home"),
        );
        assert!(location(&doc, cursor(&doc)).is_some());
    }
    #[test]
    fn uncertain_or_nonliteral_fields_are_quiet() {
        for text in [
            JAVA.replace("io.quarkus.qute.Template", "other.Template"),
            JAVA.replace("io.quarkus.qute.Location", "other.Location"),
            JAVA.replace("class View", "class View<Template>"),
            JAVA.replace("class View", "class View<Location>"),
            JAVA.replace(" Template home", " static Template home"),
            JAVA.replace("pages/home.html", "pages/home"),
            JAVA.replace("pages/home.html", "../home.html"),
            JAVA.replace("\"pages/home.html\"", "PREFIX + \"pages/home.html\""),
            JAVA.replace("pages/home.html", "pages/ho\\u006de.html"),
            JAVA.replace("class View", "class io {} class View"),
            JAVA.trim_end_matches('}').into(),
        ] {
            let doc = doc(&text);
            assert!(location(&doc, cursor(&doc)).is_none(), "{text}");
        }
    }
    #[test]
    fn build_evidence_excludes_profile_managed_scoped_dynamic_or_malformed_dependencies() {
        assert!(qute_pom(POM));
        assert!(qute_pom(&POM.replace(
            "<project>",
            "<project xmlns=\"http://maven.apache.org/POM/4.0.0\">"
        )));
        for text in [
            POM.replace("<dependencies>", "<dependencyManagement><dependencies>")
                .replace("</dependencies>", "</dependencies></dependencyManagement>"),
            POM.replace("<dependencies>", "<profiles><profile><dependencies>")
                .replace("</dependencies>", "</dependencies></profile></profiles>"),
            POM.replace("quarkus-qute", "${qute}"),
            POM.replace("</artifactId>", "</artifactId><scope>test</scope>"),
            POM.replace(
                "</artifactId>",
                "</artifactId><groupId>io.quarkus</groupId>",
            ),
            POM.replace("<groupId>", "<groupId xmlns=\"foreign\">"),
            POM.replace("</dependency>", "</wrong>"),
            format!("{POM}{POM}"),
        ] {
            assert!(!qute_pom(&text), "{text}");
        }
    }
    #[test]
    fn path_policy_rejects_escape_and_platform_ambiguities() {
        for path in [
            "/home.html",
            "../home.html",
            "a/../home.html",
            "C:/home.html",
            "a//b.txt",
            "a/.b.html",
            "CON.html",
            "com1.txt",
            "a\\b.html",
            "a.html.",
            "a.html\n",
            "a.${x}.html",
        ] {
            assert!(!template_path(path), "{path}");
        }
        assert!(template_path("nested/a-b_1.html"));
        assert!(absolute("/").is_none());
        assert!(absolute("////").is_none());
        assert!(absolute("C:/").is_none());
        assert_eq!(
            module("/project", "/project/module/src/main/java/View.java").as_deref(),
            Some("/project/module")
        );
        assert!(module("/project", "/project-other/src/main/java/View.java").is_none());
    }
    #[test]
    fn current_unsaved_source_is_supported_without_a_disk_saved_claim() {
        let (host, mut tools, doc) = setup();
        host.add_file(
            &doc.path,
            &JAVA.replace("pages/home.html", "pages/disk.html"),
        );
        host.open_editor(&doc.path, "java", &doc.text, &[]);
        let action = tools.file_actions(&doc, cursor(&doc)).remove(0);
        host.answer_next_prompt(Some(&["yes"]));
        let resolved = tools.resolve_file_action(&doc, action).unwrap();
        assert_eq!(
            resolved.open.as_deref(),
            Some("/project/src/main/resources/templates/pages/home.html")
        );
        assert!(host.process_runs().is_empty());
    }
    #[test]
    fn delivered_build_deletion_rejects_pending_generation_despite_retained_readable_text() {
        let (_host, mut tools, doc) = setup();
        let action = tools.file_actions(&doc, cursor(&doc)).remove(0);
        tools.live_events(&[openidea_plugin::Event::FilesChanged(
            openidea_plugin::FilesChanged {
                created: vec![],
                modified: vec![],
                removed: vec!["/project/pom.xml".into()],
            },
        )]);
        tools.warm_qute(&doc.path);
        assert!(tools.file_actions(&doc, cursor(&doc)).is_empty());
        assert!(matches!(
            tools.resolve_file_action(&doc, action),
            Err(Error::Stale)
        ));
    }
    #[test]
    fn resolve_prepares_only_empty_nonoverwriting_create_and_consumes_token() {
        let (host, mut tools, doc) = setup();
        let action = tools.file_actions(&doc, cursor(&doc)).remove(0);
        let replay = action.clone();
        host.answer_next_prompt(Some(&["yes"]));
        let resolved = tools.resolve_file_action(&doc, action).unwrap();
        assert_eq!(resolved.operations.len(), 1);
        assert!(
            matches!(&resolved.operations[0], FileOperation::Create {path,text,overwrite:false} if path=="/project/src/main/resources/templates/pages/home.html" && text.is_empty())
        );
        assert_eq!(
            resolved.open.as_deref(),
            Some("/project/src/main/resources/templates/pages/home.html")
        );
        assert!(
            !project_of(&doc.path)
                .unwrap()
                .exists(resolved.open.as_ref().unwrap())
                .unwrap()
        );
        assert!(tools.resolve_file_action(&doc, replay).is_err());
        assert!(host.process_runs().is_empty());
        assert_eq!(host.fetches(), 0);
    }
    #[test]
    fn stale_visible_text_version_root_and_build_fail_before_create() {
        for mode in 0..4 {
            let (host, mut tools, mut doc) = setup();
            let action = tools.file_actions(&doc, cursor(&doc)).remove(0);
            match mode {
                0 => doc.version += 1,
                1 => host.add_file("src/main/java/View.java", "changed"),
                2 => host.set_project_root(Some("/other")),
                _ => host.add_file("pom.xml", "<project/>"),
            }
            host.answer_next_prompt(Some(&["yes"]));
            assert!(tools.resolve_file_action(&doc, action).is_err());
        }
    }
    #[test]
    fn cancellation_and_existing_target_or_parent_never_create() {
        for mode in 0..4 {
            let (host, mut tools, doc) = setup();
            let action = tools.file_actions(&doc, cursor(&doc)).remove(0);
            match mode {
                0 => host.answer_next_prompt(None),
                1 => host.add_file("src/main/resources/templates/pages/home.html", "existing"),
                2 => host.open_editor(
                    "/project/src/main/resources/templates/pages/home.html",
                    "html",
                    "unsaved",
                    &[],
                ),
                _ => host.add_file("src/main/resources/templates/pages", "file, not directory"),
            }
            assert!(
                tools.resolve_file_action(&doc, action).is_err(),
                "mode {mode}"
            );
        }
    }
    #[test]
    fn cold_java_is_quiet_but_current_pom_can_prompt_known_module_path() {
        let host = MockHost::install();
        host.add_file("pom.xml", POM);
        let mut tools = QuarkusTools::default();
        let java = doc(JAVA);
        assert!(tools.file_actions(&java, cursor(&java)).is_empty());
        let pom = Document {
            path: "/project/pom.xml".into(),
            language: "xml".into(),
            version: 3,
            text: POM.into(),
        };
        let action = tools.file_actions(&pom, Range::default()).remove(0);
        host.answer_next_prompt(Some(&["hello.txt"]));
        host.answer_next_prompt(Some(&["yes"]));
        let resolved = tools.resolve_file_action(&pom, action).unwrap();
        assert_eq!(
            resolved.open.as_deref(),
            Some("/project/src/main/resources/templates/hello.txt")
        );
    }
    #[test]
    fn malformed_namespace_bindings_cannot_offer_or_resolve_qute_creation() {
        for binding in [
            "xmlns:xml=\"urn:foreign\"",
            "xmlns:xmlns=\"urn:foreign\"",
            "xmlns:a=\"http://www.w3.org/XML/1998/namespace\"",
            "bad:flag=\"x\"",
        ] {
            let host = MockHost::install();
            let malformed = POM.replacen("<project>", &format!("<project {binding}>"), 1);
            host.add_file("pom.xml", &malformed);
            let mut tools = QuarkusTools::default();
            let pom = Document {
                path: "/project/pom.xml".into(),
                language: "xml".into(),
                version: 1,
                text: malformed,
            };
            assert!(
                tools.file_actions(&pom, Range::default()).is_empty(),
                "{binding}"
            );
            let clean = Document {
                text: POM.into(),
                ..pom.clone()
            };
            let action = tools.file_actions(&clean, Range::default()).remove(0);
            assert!(
                tools.resolve_file_action(&clean, action).is_err(),
                "{binding}"
            );
        }
        let legal = POM.replacen(
            "<project>",
            "<project xmlns:xml=\"http://www.w3.org/XML/1998/namespace\" xml:lang=\"en\">",
            1,
        );
        assert!(qute_pom(&legal));
    }
    fn navigation_setup() -> (MockHost, QuarkusTools, Document) {
        let host = MockHost::install();
        host.add_file("pom.xml", POM);
        host.add_file("src/main/java/View.java", JAVA);
        host.add_file(
            "src/main/resources/templates/pages/home.html",
            "<p>owned</p>",
        );
        let mut tools = QuarkusTools::default();
        tools.activate_live();
        tools.warm_live(
            "/project/src/main/java/View.java",
            None,
            Instant::now() + Duration::from_secs(1),
        );
        (host, tools, doc(JAVA))
    }
    fn literal_at(doc: &Document) -> Position {
        position(&doc.text, doc.text.find("pages/home.html").unwrap() + 3)
    }
    #[test]
    fn public_definition_uses_exact_cached_qute_literal_and_keeps_no_provider_discovery() {
        let (host, mut tools, doc) = navigation_setup();
        let at = literal_at(&doc);
        let found = tools.definition(&doc, at);
        assert_eq!(found.len(), 1);
        assert_eq!(
            found[0].path,
            "/project/src/main/resources/templates/pages/home.html"
        );
        assert!(
            tools
                .definition(&doc, position(&doc.text, doc.text.rfind("home;").unwrap()))
                .is_empty()
        );
        // File created only after warm-up cannot enter this callback's inventory.
        host.add_file("src/main/resources/templates/pages/new.html", "new");
        let changed = Document {
            text: doc.text.replace("home.html", "new.html"),
            version: 8,
            ..doc.clone()
        };
        assert!(
            tools
                .definition(
                    &changed,
                    position(
                        &changed.text,
                        changed.text.find("pages/new.html").unwrap() + 3
                    )
                )
                .is_empty()
        );
        assert!(host.requests().is_empty() && host.process_runs().is_empty());
    }
    #[test]
    fn qute_link_missing_cold_expired_foreign_and_unsupported_sources_are_quiet() {
        let (host, mut tools, doc) = navigation_setup();
        assert!(
            QuarkusTools::default()
                .definition(&doc, literal_at(&doc))
                .is_empty()
        );
        for text in [
            JAVA.replace("io.quarkus.qute.Template", "other.Template"),
            JAVA.replace("pages/home.html", "../home.html"),
            JAVA.replace("pages/home.html", "pages/home"),
            JAVA.replace("\"pages/home.html\"", "TEMPLATE"),
            JAVA.replace(" Template home;", " static Template home;"),
        ] {
            let current = self::doc(&text);
            assert!(
                tools
                    .definition(
                        &current,
                        position(&text, text.find("@Location").unwrap() + 12)
                    )
                    .is_empty()
            );
        }
        for inventory in tools.qute.inventories.values_mut() {
            inventory.at = Instant::now() - NAV_TTL;
        }
        assert!(tools.definition(&doc, literal_at(&doc)).is_empty());
        host.set_project_root(Some("/other"));
        let other = Document {
            path: "/other/src/main/java/View.java".into(),
            ..doc.clone()
        };
        assert!(tools.definition(&other, literal_at(&other)).is_empty());
    }
    #[test]
    fn delivered_deletion_dominates_readable_old_inventory_and_creation_can_rewarm() {
        let (host, mut tools, doc) = navigation_setup();
        let path = "/project/src/main/resources/templates/pages/home.html";
        assert_eq!(tools.definition(&doc, literal_at(&doc)).len(), 1);
        tools.handle_events(
            &[openidea_plugin::Event::FilesChanged(
                openidea_plugin::FilesChanged {
                    created: vec![],
                    modified: vec![],
                    removed: vec![path.into()],
                },
            )],
            &host.invocation(),
        );
        // Deliberately retained MockHost source/disk must not resurrect deletion.
        tools.warm_qute(&doc.path);
        assert!(tools.definition(&doc, literal_at(&doc)).is_empty());
        tools.handle_events(
            &[openidea_plugin::Event::FilesChanged(
                openidea_plugin::FilesChanged {
                    created: vec![path.into()],
                    modified: vec![],
                    removed: vec![],
                },
            )],
            &host.invocation(),
        );
        assert_eq!(tools.definition(&doc, literal_at(&doc)).len(), 1);
    }
    #[test]
    fn qute_inventory_cap_and_denied_watch_never_enable_uncertain_targets() {
        let (host, mut tools, doc) = navigation_setup();
        for n in 0..MAX_TEMPLATES {
            host.add_file(&format!("src/main/resources/templates/many/{n}.html"), "x");
        }
        tools.qute.inventories.clear();
        tools.warm_qute(&doc.path);
        assert!(tools.definition(&doc, literal_at(&doc)).is_empty());
        tools.live.watching = false;
        assert!(tools.definition(&doc, literal_at(&doc)).is_empty());
        assert!(host.requests().is_empty() && host.process_runs().is_empty());
    }
    #[test]
    fn same_template_spelling_is_scoped_to_exact_module_and_nested_project_owner() {
        let host = MockHost::install();
        host.add_file("pom.xml", POM);
        host.add_file("src/main/java/View.java", JAVA);
        host.add_file("nested/pom.xml", POM);
        host.add_file("nested/src/main/java/View.java", JAVA);
        host.add_file(
            "nested/src/main/resources/templates/pages/home.html",
            "nested only",
        );
        let mut tools = QuarkusTools::default();
        tools.activate_live();
        tools.warm_live(
            "/project/src/main/java/View.java",
            None,
            Instant::now() + Duration::from_secs(1),
        );
        tools.warm_live(
            "/project/nested/src/main/java/View.java",
            None,
            Instant::now() + Duration::from_secs(1),
        );
        let root = self::doc(JAVA);
        let nested = Document {
            path: "/project/nested/src/main/java/View.java".into(),
            ..root.clone()
        };
        assert!(tools.definition(&root, literal_at(&root)).is_empty());
        assert_eq!(
            tools.definition(&nested, literal_at(&nested))[0].path,
            "/project/nested/src/main/resources/templates/pages/home.html"
        );
        host.add_project("/project/nested");
        assert!(tools.definition(&nested, literal_at(&nested)).is_empty());
    }
}
