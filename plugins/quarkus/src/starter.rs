//! Offline, explicitly versioned Maven/Qute scaffold; no registry or LS emulation.
use super::*;
use openidea_plugin::file_actions::{FileAction, FileActions, FileOperation};
use openidea_plugin::{FileKind, Project};
use std::time::{Duration, Instant};
const TTL: Duration = Duration::from_secs(300);
const MAX_SOURCE: usize = 128 << 10;
#[derive(Clone)]
struct Plan {
    root: String,
    target: String,
    doc: Document,
    files: BTreeMap<String, String>,
    at: Instant,
    token: String,
    issued: bool,
}
#[derive(Default)]
pub(super) struct State {
    plan: Option<Plan>,
    serial: u64,
}
fn bad(message: &str) -> Error {
    Error::Invalid(message.into())
}
fn normal(path: &str) -> bool {
    path.starts_with('/')
        && path != "/"
        && path.len() <= 4096
        && !path.contains(['\\', '\0'])
        && !path.contains("//")
        && !path.ends_with('/')
        && !path.chars().any(char::is_control)
        && !path.split('/').any(|p| matches!(p, "." | ".."))
}
fn identifier(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.as_bytes()[0].is_ascii_alphabetic()
        && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}
fn group(s: &str) -> bool {
    s.len() <= 128 && s.split('.').all(identifier)
}
fn artifact(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.as_bytes()[0].is_ascii_lowercase()
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"-_".contains(&b))
        && !matches!(
            s.to_ascii_uppercase().as_str(),
            "CON" | "PRN" | "AUX" | "NUL"
        )
        && !(s.len() == 4
            && (s.starts_with("com") || s.starts_with("lpt"))
            && matches!(s.as_bytes()[3], b'1'..=b'9'))
}
fn version(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.as_bytes()[0].is_ascii_digit()
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b".-_".contains(&b))
        && !s.contains("..")
        && !s.ends_with('.')
}
fn source(ctx: &Invocation) -> Result<(Project, Document), Error> {
    let project = ctx.project().ok_or(Error::Stale)?;
    let root = project.root()?;
    let editor = ctx
        .editor()
        .ok_or("Open a source file in the destination parent project first.")?;
    let d = editor.document()?;
    let doc = Document {
        path: editor.path(),
        language: editor.language(),
        text: d.text,
        version: d.version,
    };
    if !matches!(doc.language.as_str(), "java" | "xml") {
        return Err(bad(
            "Open a Java or XML source file in the destination parent project first.",
        ));
    }
    if !normal(&root)
        || !normal(&doc.path)
        || !doc.path.starts_with(&format!("{root}/"))
        || doc.text.len() > MAX_SOURCE
        || project.read_file(&doc.path)? != doc.text
    {
        return Err(Error::Stale);
    }
    Ok((project, doc))
}
fn owner(project: &Project, plan: &Plan, doc: &Document) -> Result<(), Error> {
    if plan.at.elapsed() >= TTL
        || project.root()? != plan.root
        || !normal(&plan.root)
        || !normal(&plan.target)
        || !plan.target.starts_with(&format!("{}/", plan.root))
        || doc != &plan.doc
        || project.read_file(&doc.path)? != doc.text
        || project_of(&doc.path).and_then(|p| p.root().ok()).as_deref() != Some(plan.root.as_str())
    {
        return Err(Error::Stale);
    }
    Ok(())
}
fn vacant(project: &Project, plan: &Plan) -> Result<(), Error> {
    if project.stat(&plan.root)?.kind != FileKind::Directory || project.exists(&plan.target)? {
        return Err(bad(
            "Starter destination must be a new child folder; no merge or overwrite.",
        ));
    }
    for name in plan.files.keys() {
        if project.exists(&format!("{}/{name}", plan.target))? {
            return Err(bad(
                "A starter file or unsaved target buffer already exists.",
            ));
        }
    }
    Ok(())
}
fn files(group: &str, artifact: &str, version: &str, platform: &str) -> BTreeMap<String, String> {
    let pom = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<project xmlns="http://maven.apache.org/POM/4.0.0" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xsi:schemaLocation="http://maven.apache.org/POM/4.0.0 https://maven.apache.org/xsd/maven-4.0.0.xsd">
  <modelVersion>4.0.0</modelVersion>
  <groupId>{group}</groupId>
  <artifactId>{artifact}</artifactId>
  <version>{version}</version>
  <properties>
    <project.build.sourceEncoding>UTF-8</project.build.sourceEncoding>
    <quarkus.platform.group-id>io.quarkus.platform</quarkus.platform.group-id>
    <quarkus.platform.artifact-id>quarkus-bom</quarkus.platform.artifact-id>
    <quarkus.platform.version>{platform}</quarkus.platform.version>
  </properties>
  <dependencyManagement>
    <dependencies>
      <dependency>
        <groupId>${{quarkus.platform.group-id}}</groupId>
        <artifactId>${{quarkus.platform.artifact-id}}</artifactId>
        <version>${{quarkus.platform.version}}</version>
        <type>pom</type>
        <scope>import</scope>
      </dependency>
    </dependencies>
  </dependencyManagement>
  <dependencies>
    <dependency><groupId>io.quarkus</groupId><artifactId>quarkus-qute</artifactId></dependency>
  </dependencies>
  <build>
    <plugins>
      <plugin>
        <groupId>${{quarkus.platform.group-id}}</groupId>
        <artifactId>quarkus-maven-plugin</artifactId>
        <version>${{quarkus.platform.version}}</version>
        <extensions>true</extensions>
        <executions><execution><goals><goal>build</goal><goal>generate-code</goal><goal>generate-code-tests</goal></goals></execution></executions>
      </plugin>
    </plugins>
  </build>
</project>
"#
    );
    BTreeMap::from([
        ("pom.xml".into(), pom),
        (
            "src/main/resources/templates/hello.txt".into(),
            "Hello from Qute!\n".into(),
        ),
        (
            "src/main/resources/application.properties".into(),
            "# Local Quarkus/Qute scaffold; configure your application here.\n".into(),
        ),
        (
            "README.md".into(),
            format!(
                "# {artifact}\n\nOffline Maven/Qute scaffold generated by Quarkus Tools.\n\nRequested platform: io.quarkus.platform:quarkus-bom:{platform}.\nThis user-supplied version was not resolved or checked against a registry.\nOnly quarkus-qute and a literal template are included: no REST endpoint, Java\nsource, wrapper, tests, extension catalog, downloaded codestarts or runtime proof.\nUse a compatible JDK and installed Maven; review pom.xml before building.\nFuture Java sources need an explicitly selected compiler plugin/release.\nQute expression/type resolution belongs to the configured language server.\n"
            ),
        ),
    ])
}
fn preview(plan: &Plan) -> String {
    let mut body = format!(
        "Destination: {}\nOwner project: {}\nSource: {} (version {})\n\nOFFLINE Maven/Qute scaffold. Platform version was supplied explicitly, not resolved/verified. No build, network, wrapper, endpoint or language-server configuration. Apply creates4 new UTF-8 files atomically; no overwrite. Then invoke Open Reviewed Starter separately.\n",
        plan.target, plan.root, plan.doc.path, plan.doc.version
    );
    for (path, text) in &plan.files {
        body.push_str(&format!(
            "\n--- {}/{path} ({} UTF-8 bytes) ---\n{text}",
            plan.target,
            text.len()
        ));
    }
    body
}
impl QuarkusTools {
    pub(super) fn prepare_starter(&mut self, ctx: &Invocation) -> Result<Option<String>, Error> {
        let (project, doc) = source(ctx)?;
        let root = project.root()?;
        self.starter.plan = None;
        let ask = |title, initial| host::input(title, initial)?.ok_or(Error::Cancelled);
        let g = ask(
            "Starter Maven groupId (literal Java-style dotted name)",
            Some("org.acme"),
        )?;
        let a = ask(
            "New starter child folder / Maven artifactId",
            Some("qute-starter"),
        )?;
        let v = ask("Starter project version (literal)", Some("1.0.0-SNAPSHOT"))?;
        let p = ask(
            "Exact Quarkus 3 platform version (required; not resolved or checked)",
            None,
        )?;
        if !group(&g) || !artifact(&a) || !version(&v) || !version(&p) || !p.starts_with("3.") {
            return Err(bad(
                "Use literal bounded Maven coordinates, a portable lowercase folder, and an explicit Quarkus3 version; no XML, interpolation, ranges or paths.",
            ));
        }
        let (project_now, current) = source(ctx)?;
        if project_now.root()? != root || current != doc {
            return Err(Error::Stale);
        }
        self.starter.serial = self.starter.serial.checked_add(1).ok_or(Error::Stale)?;
        let plan = Plan {
            target: format!("{root}/{a}"),
            root,
            files: files(&g, &a, &v, &p),
            doc,
            at: Instant::now(),
            token: format!("quarkus-starter:{}", self.starter.serial),
            issued: false,
        };
        owner(&project, &plan, &plan.doc)?;
        vacant(&project, &plan)?;
        if !ui_kit::prompts::confirm_exact(
            "Prepare offline Quarkus/Qute starter?",
            &preview(&plan),
        )? {
            return Err(Error::Cancelled);
        }
        let (project, current) = source(ctx)?;
        owner(&project, &plan, &current)?;
        vacant(&project, &plan)?;
        self.starter.plan = Some(plan);
        Ok(Some("Prepared only. Invoke Create Reviewed Quarkus/Qute Starter from this source file's intentions within5minutes; that FileAction creates the files. Then Open Reviewed Starter requests the normal project trust/open flow.".into()))
    }
    pub(super) fn starter_actions(&self, doc: &Document) -> Vec<FileAction> {
        let Some(plan) = self
            .starter
            .plan
            .as_ref()
            .filter(|p| !p.issued && p.at.elapsed() < TTL && &p.doc == doc)
        else {
            return Vec::new();
        };
        if project_of(&doc.path).and_then(|p| p.root().ok()).as_deref() != Some(plan.root.as_str())
        {
            return Vec::new();
        }
        vec![FileAction {
            title: "Create Reviewed Quarkus/Qute Starter…".into(),
            kind: Some("source".into()),
            data: Some(plan.token.clone()),
            ..Default::default()
        }]
    }
    pub(super) fn resolve_starter(
        &mut self,
        doc: &Document,
        action: FileAction,
    ) -> Result<FileAction, Error> {
        let plan = self
            .starter
            .plan
            .as_ref()
            .filter(|p| !p.issued && action.data.as_deref() == Some(p.token.as_str()))
            .cloned()
            .ok_or(Error::Stale)?;
        let project = project_of(&doc.path).ok_or(Error::Stale)?;
        owner(&project, &plan, doc)?;
        vacant(&project, &plan)?;
        if !ui_kit::prompts::confirm_exact("Create these exact starter files?", &preview(&plan))? {
            return Err(Error::Cancelled);
        }
        owner(&project, &plan, doc)?;
        vacant(&project, &plan)?;
        self.starter.plan.as_mut().ok_or(Error::Stale)?.issued = true;
        Ok(FileAction {
            title: "Create reviewed Quarkus/Qute starter".into(),
            kind: Some("source".into()),
            operations: plan
                .files
                .iter()
                .map(|(path, text)| FileOperation::Create {
                    path: format!("{}/{path}", plan.target),
                    text: text.clone(),
                    overwrite: false,
                })
                .collect(),
            open: Some(format!("{}/pom.xml", plan.target)),
            ..Default::default()
        })
    }
    pub(super) fn open_starter(&mut self, ctx: &Invocation) -> Result<Option<String>, Error> {
        let plan = self
            .starter
            .plan
            .as_ref()
            .filter(|p| p.issued)
            .cloned()
            .ok_or("Apply a reviewed starter FileAction first.")?;
        let project = ctx.project().ok_or(Error::Stale)?;
        verify_created(&project, &plan)?;
        if !ui_kit::prompts::confirm_exact(
            "Open reviewed starter folder?",
            &format!(
                "Folder: {}\nFour expected files exist and match project-visible contents. Unsaved buffers can mask disk bytes; extra files/build/runtime are not certified. Request normal trust/open flow in a new window?",
                plan.target
            ),
        )? {
            return Err(Error::Cancelled);
        }
        verify_created(&project, &plan)?;
        host::open_project(&plan.target, true)?;
        Ok(Some("Requested normal project trust/open flow; no build or automatic language-server settings.".into()))
    }
}
fn verify_created(project: &Project, plan: &Plan) -> Result<(), Error> {
    if project.root()? != plan.root
        || project.stat(&plan.target)?.kind != FileKind::Directory
        || project_of(&plan.target)
            .and_then(|p| p.root().ok())
            .as_deref()
            != Some(plan.root.as_str())
    {
        return Err(Error::Stale);
    }
    for (name, text) in &plan.files {
        let path = format!("{}/{name}", plan.target);
        let meta = project.stat(&path)?;
        if meta.kind != FileKind::File
            || meta.size != text.len() as u64
            || project.read_file(&path)? != *text
        {
            return Err(Error::Stale);
        }
    }
    Ok(())
}
impl FileActions for QuarkusTools {
    fn file_actions(&mut self, doc: &Document, range: Range) -> Vec<FileAction> {
        let mut actions = self.qute_file_actions(doc, range);
        actions.extend(self.starter_actions(doc));
        actions
    }
    fn resolve_file_action(
        &mut self,
        doc: &Document,
        action: FileAction,
    ) -> Result<FileAction, Error> {
        if action
            .data
            .as_deref()
            .is_some_and(|s| s.starts_with("quarkus-starter:"))
        {
            self.resolve_starter(doc, action)
        } else {
            self.resolve_qute_file_action(doc, action)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;
    fn setup() -> (MockHost, QuarkusTools, Document) {
        let h = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        let doc = Document {
            path: "/project/pom.xml".into(),
            language: "xml".into(),
            version: 1,
            text: "<project/>".into(),
        };
        h.add_file("pom.xml", &doc.text);
        h.open_editor(&doc.path, &doc.language, &doc.text, &[]);
        (h, QuarkusTools::default(), doc)
    }
    fn answers(h: &MockHost) {
        for s in ["org.acme", "qute-starter", "1.0.0-SNAPSHOT", "3.40.1"] {
            h.answer_next_prompt(Some(&[s]));
        }
        h.answer_next_prompt(Some(&[]));
    }
    fn prepare(h: &MockHost, q: &mut QuarkusTools) {
        answers(h);
        q.run_command("prepare-local-starter", &h.invocation(), None)
            .unwrap();
    }
    fn action(q: &mut QuarkusTools, doc: &Document) -> FileAction {
        q.file_actions(doc, Range::default())
            .into_iter()
            .find(|a| {
                a.data
                    .as_deref()
                    .is_some_and(|s| s.starts_with("quarkus-starter:"))
            })
            .unwrap()
    }
    fn issued(h: &MockHost, q: &mut QuarkusTools, doc: &Document) -> FileAction {
        prepare(h, q);
        let a = action(q, doc);
        h.answer_next_prompt(Some(&[]));
        q.resolve_file_action(doc, a).unwrap()
    }
    fn mock_apply(h: &MockHost, action: &FileAction) {
        for op in &action.operations {
            match op {
                FileOperation::Create {
                    path,
                    text,
                    overwrite,
                } => {
                    assert!(!overwrite);
                    h.add_file(path.strip_prefix("/project/").unwrap(), text);
                    h.add_dir("qute-starter");
                }
                _ => panic!("create-only"),
            }
        }
    }
    #[test]
    fn public_prepare_is_inert_resolve_returns_atomic_nonoverwriting_utf8_files_and_preserves_qute_action()
     {
        let (h, mut q, doc) = setup();
        prepare(&h, &mut q);
        assert!(
            !host::project_of(&doc.path)
                .unwrap()
                .exists("qute-starter/pom.xml")
                .unwrap()
        );
        assert!(h.opened_projects().is_empty());
        assert!(h.process_runs().is_empty());
        let a = action(&mut q, &doc);
        h.answer_next_prompt(Some(&[]));
        let resolved = q.resolve_file_action(&doc, a.clone()).unwrap();
        assert_eq!(resolved.operations.len(), 4);
        assert_eq!(
            resolved.open.as_deref(),
            Some("/project/qute-starter/pom.xml")
        );
        assert!(q.resolve_file_action(&doc, a).is_err());
        mock_apply(&h, &resolved);
        let text = host::project_of(&doc.path)
            .unwrap()
            .read_file("qute-starter/pom.xml")
            .unwrap();
        let generated = Document {
            path: "/project/qute-starter/pom.xml".into(),
            language: "xml".into(),
            version: 1,
            text,
        };
        assert!(
            q.file_actions(&generated, Range::default())
                .iter()
                .any(|a| a.title.contains("Create empty Qute template"))
        );
    }
    #[test]
    fn every_prompt_cancel_and_both_confirm_cancels_leave_no_operations_or_open() {
        for cut in 0..5 {
            let (h, mut q, doc) = setup();
            for s in ["org.acme", "qute-starter", "1.0.0-SNAPSHOT", "3.40.1"]
                .iter()
                .take(cut)
            {
                h.answer_next_prompt(Some(&[s]));
            }
            h.answer_next_prompt(None);
            assert!(q.prepare_starter(&h.invocation()).is_err());
            assert!(q.starter_actions(&doc).is_empty());
            assert!(h.opened_projects().is_empty());
        }
        let (h, mut q, doc) = setup();
        prepare(&h, &mut q);
        let a = action(&mut q, &doc);
        h.answer_next_prompt(None);
        assert!(q.resolve_file_action(&doc, a).is_err());
        assert!(!q.starter.plan.as_ref().unwrap().issued);
    }
    #[test]
    fn coordinates_xml_interpolation_ranges_paths_and_reserved_folder_names_are_rejected() {
        for (g, a, v, p) in [
            ("<bad>", "okay", "1", "3.40.1"),
            ("org.acme", "../escape", "1", "3.40.1"),
            ("org.acme", "con", "1", "3.40.1"),
            ("org.acme", "com1", "1", "3.40.1"),
            ("org.acme", "okay", "${v}", "3.40.1"),
            ("org.acme", "okay", "1", "[3,4)"),
            ("org.acme", "okay", "1", "2.16.0.Final"),
            ("org..acme", "okay", "1", "3.40.1"),
        ] {
            let (h, mut q, _) = setup();
            for s in [g, a, v, p] {
                h.answer_next_prompt(Some(&[s]));
            }
            assert!(
                q.prepare_starter(&h.invocation()).is_err(),
                "{g}/{a}/{v}/{p}"
            );
            assert!(q.starter.plan.is_none());
        }
    }
    #[test]
    fn existing_destination_unsaved_target_and_invalid_parent_refuse_before_issuance() {
        for mode in 0..3 {
            let (h, mut q, doc) = setup();
            prepare(&h, &mut q);
            let a = action(&mut q, &doc);
            match mode {
                0 => h.add_dir("qute-starter"),
                1 => h.open_editor("/project/qute-starter/pom.xml", "xml", "unsaved", &[]),
                _ => h.add_file("/project", "ordinary file"),
            }
            assert!(q.resolve_file_action(&doc, a).is_err());
            assert!(!q.starter.plan.as_ref().unwrap().issued);
        }
    }
    #[test]
    fn current_source_version_path_root_and_ttl_are_authoritative() {
        for mode in 0..5 {
            let (h, mut q, doc) = setup();
            prepare(&h, &mut q);
            let a = action(&mut q, &doc);
            let mut supplied = doc.clone();
            match mode {
                0 => h.type_in_editor("<!-- changed -->"),
                1 => supplied.version += 1,
                2 => supplied.path = "/other/pom.xml".into(),
                3 => {
                    h.add_project("/project");
                    h.add_project("/project/nested");
                    supplied.path = "/project/nested/pom.xml".into();
                }
                _ => q.starter.plan.as_mut().unwrap().at = Instant::now() - TTL,
            };
            assert!(q.resolve_file_action(&supplied, a).is_err());
            assert!(!q.starter.plan.as_ref().unwrap().issued);
        }
    }
    #[test]
    fn new_preparation_invalidates_old_token_and_providers_do_not_create_or_fetch() {
        let (h, mut q, doc) = setup();
        prepare(&h, &mut q);
        let old = action(&mut q, &doc);
        prepare(&h, &mut q);
        assert!(q.resolve_file_action(&doc, old).is_err());
        for _ in 0..3 {
            assert_eq!(q.starter_actions(&doc).len(), 1);
        }
        assert!(h.process_runs().is_empty());
        assert!(h.opened_projects().is_empty());
    }
    #[test]
    fn open_requires_applied_exact_files_cancel_and_foreign_owner_do_not_open() {
        let (h, mut q, doc) = setup();
        let issued = issued(&h, &mut q, &doc);
        assert!(q.open_starter(&h.invocation()).is_err());
        mock_apply(&h, &issued);
        h.answer_next_prompt(None);
        assert!(q.open_starter(&h.invocation()).is_err());
        assert!(h.opened_projects().is_empty());
        h.add_project("/project");
        h.set_project_root(Some("/other"));
        assert!(q.open_starter(&h.invocation()).is_err());
        h.set_project_root(Some("/project"));
        h.answer_next_prompt(Some(&[]));
        q.open_starter(&h.invocation()).unwrap();
        assert_eq!(
            h.opened_projects(),
            vec![("/project/qute-starter".into(), true)]
        );
    }
    #[test]
    fn changed_generated_file_or_unapplied_partial_files_do_not_certify_open() {
        for mode in 0..2 {
            let (h, mut q, doc) = setup();
            let issued = issued(&h, &mut q, &doc);
            if mode == 0 {
                mock_apply(&h, &issued);
                h.add_file("qute-starter/README.md", "changed");
            } else {
                h.add_file("qute-starter/pom.xml", "partial");
            }
            assert!(q.open_starter(&h.invocation()).is_err());
            assert!(h.opened_projects().is_empty());
        }
    }
    #[test]
    fn missing_permissions_editor_non_action_language_and_oversized_source_are_quiet() {
        let (h, mut q, _) = setup();
        h.open_editor("/project/readme.md", "markdown", "text", &[]);
        assert!(q.prepare_starter(&h.invocation()).is_err());
        let (h, mut q, _) = setup();
        h.open_editor("/project/pom.xml", "xml", &"x".repeat(MAX_SOURCE + 1), &[]);
        assert!(q.prepare_starter(&h.invocation()).is_err());
        let (h, mut q, _) = setup();
        let h = h.with_capabilities(&["editor.read"]);
        assert!(q.prepare_starter(&h.invocation()).is_err());
        assert!(h.opened_projects().is_empty());
    }
}
