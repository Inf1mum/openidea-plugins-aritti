//! Reviewed missing-action generation for a uniquely owned plain Livewire class.
use crate::{Index, blade};
use openidea_plugin::{
    Error, Range, TextEdit, file_actions::FileAction, providers::Document, text,
};
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
struct Candidate {
    path: String,
    source: String,
    name: String,
    insert: usize,
}
#[derive(Serialize, Deserialize)]
struct Request {
    root: String,
    doc: String,
    version: u64,
    text: String,
    at: usize,
    candidate: Candidate,
}
pub(super) fn target<'a>(index: &'a Index, path: &str) -> Option<&'a crate::index::Entry> {
    let mapped: Vec<_> = index
        .packages
        .livewire_views
        .iter()
        .chain(index.packages.volt.iter())
        .filter(|(_, view)| view.path == path)
        .filter_map(|(name, _)| index.livewire.get(name))
        .collect();
    if !mapped.is_empty() && mapped.iter().all(|entry| entry.path == mapped[0].path) {
        Some(mapped[0])
    } else {
        let component = index.view_of(path)?.strip_prefix("livewire.")?;
        index.livewire.get(component)
    }
}
pub(crate) fn methods_for_view(
    index: &Index,
    project: &openidea_plugin::Project,
    path: &str,
    text: &str,
) -> Option<Vec<String>> {
    let entry = target(index, path)?;
    if !crate::owns_current_path(project, &entry.path)
        || project.stat(&entry.path).ok()?.size > 128 * 1024
    {
        return None;
    }
    let source = if path == entry.path {
        text.into()
    } else {
        project.read_file(&entry.path).ok()?
    };
    if let Some((methods, _)) = super::livewire_views::absence(&source) {
        return Some(methods);
    }
    blade::livewire_methods(&source)
}

fn candidate(index: &Index, doc: &Document, at: usize) -> Option<Candidate> {
    if index.partial
        || doc.language != "blade"
        || doc.text.len() > 128 * 1024
        || index
            .metadata
            .package_version("livewire/livewire")
            .is_none()
    {
        return None;
    }
    let call = blade::wire_calls(&doc.text)
        .into_iter()
        .find(|c| c.start <= at && at <= c.end)?;
    if call.name.starts_with('_') || call.name.len() > 64 {
        return None;
    }
    let entry = target(index, &doc.path)?;
    let project = crate::project_for(&doc.path)?;
    if !crate::owns_current_path(&project, &entry.path)
        || project.stat(&entry.path).ok()?.size > 128 * 1024
    {
        return None;
    }
    let source = if entry.path == doc.path {
        doc.text.clone()
    } else {
        project.read_file(&entry.path).ok()?
    };
    let (methods, insert) = if let Some(proof) = super::livewire_views::absence(&source) {
        proof
    } else {
        if entry.path.ends_with(".blade.php") {
            return None;
        }
        crate::eloquent_members::declaration(&source)?;
        let methods = blade::livewire_methods(&source)?;
        let code = crate::package_facts::code(&source)?;
        let class = code.find("class ")?;
        let open = code[class..].find('{')? + class;
        let insert = crate::php::closing(&source, open)?;
        if !code[insert + 1..].trim().is_empty() {
            return None;
        }
        (methods, insert)
    };
    if methods
        .iter()
        .any(|method| method.eq_ignore_ascii_case(&call.name))
    {
        return None;
    }
    Some(Candidate {
        path: entry.path.clone(),
        source,
        name: call.name,
        insert,
    })
}
pub(crate) fn actions(index: &Index, doc: &Document, selection: Range) -> Vec<FileAction> {
    let run = || {
        let at = text::offset(&doc.text, selection.start)?;
        let candidate = candidate(index, doc, at)?;
        let root = crate::project_for(&doc.path)?.root().ok()?;
        let data = Request {
            root,
            doc: doc.path.clone(),
            version: doc.version,
            text: doc.text.clone(),
            at,
            candidate: candidate.clone(),
        };
        Some(FileAction {
            title: crate::ui_text::format(
                "final.packageAction.action",
                "Create Livewire action {name}",
                &[("name", &candidate.name)],
            ),
            kind: Some("quickfix".into()),
            data: Some(format!(
                "livewire-action:{}",
                serde_json::to_string(&data).ok()?
            )),
            ..Default::default()
        })
    };
    run().into_iter().collect()
}
pub(crate) fn resolve(
    index: &Index,
    doc: &Document,
    action: FileAction,
) -> Result<FileAction, Error> {
    let raw = action
        .data
        .as_deref()
        .and_then(|v| v.strip_prefix("livewire-action:"))
        .filter(|v| v.len() < 512 * 1024)
        .ok_or(Error::Stale)?;
    let request: Request = serde_json::from_str(raw).map_err(|_| Error::Stale)?;
    let project = crate::project_for(&doc.path).ok_or(Error::Stale)?;
    if request.doc != doc.path
        || request.version != doc.version
        || request.text != doc.text
        || project.root()? != request.root
        || project.read_file(&doc.path)? != doc.text
    {
        return Err(Error::Stale);
    }
    if !crate::index::index_sources_current(&project,index){return Err(Error::Stale);}
    if candidate(index, doc, request.at).as_ref() != Some(&request.candidate) {
        return Err(Error::Stale);
    }
    let c = &request.candidate;
    let eol = if c.source.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let insertion = format!(
        "{eol}    public function {}(): void{eol}    {{{eol}    }}{eol}",
        c.name
    );
    let mut after = c.source.clone();
    after.insert_str(c.insert, &insertion);
    let preview = crate::ui_text::format(
        "final.packageAction.review",
        "Project: {root}\nCreate action {name} in {path}\n\nComplete resulting class source:\n{source}",
        &[
            ("root", &request.root),
            ("name", &c.name),
            ("path", &c.path),
            ("source", &ui_kit::prompts::readable_source(&after)),
        ],
    );
    if !ui_kit::prompts::confirm_exact(
        crate::ui_text::text(
            "final.packageAction.confirm",
            "Create missing Livewire action?",
        ),
        &preview,
    )? {
        return Err(Error::Cancelled);
    }
    if !crate::index::index_sources_current(&project,index){return Err(Error::Stale);}
    if project.read_file(&doc.path)? != doc.text
        || candidate(index, doc, request.at).as_ref() != Some(c)
    {
        return Err(Error::Stale);
    }
    let at = text::position(&c.source, c.insert);
    Ok(FileAction {
        title: crate::ui_text::format(
            "final.packageAction.action",
            "Create Livewire action {name}",
            &[("name", &c.name)],
        ),
        edits: vec![(
            c.path.clone(),
            vec![TextEdit {
                range: Range::new(at, at),
                text: insertion,
            }],
        )],
        open: Some(c.path.clone()),
        ..Default::default()
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;
    #[test]
    fn missing_action_is_reviewed_and_stale_or_foreign_sources_are_withheld() {
        let host = MockHost::install().with_capabilities(&["project.read", "ui.prompt"]);
        host.add_file("artisan", "<?php");
        host.add_file(
            "composer.json",
            r#"{"require":{"laravel/framework":"^13"},"autoload":{"psr-4":{"App\\":"app/"}}}"#,
        );
        host.add_file(
            "composer.lock",
            r#"{"packages":[{"name":"livewire/livewire","version":"4.4.8"}]}"#,
        );
        host.add_file("app/Livewire/Counter.php","<?php\nnamespace App\\Livewire;\nclass Counter extends \\Livewire\\Component\n{\n public function render() {}\n}\n");
        let source = "<button wire:click=\"save\">Save</button>";
        host.add_file("resources/views/livewire/counter.blade.php", source);
        let project = host.invocation().project().unwrap();
        let index = Index::build(&project);
        let doc = Document {
            path: "/project/resources/views/livewire/counter.blade.php".into(),
            language: "blade".into(),
            version: 1,
            text: source.into(),
        };
        let at = text::position(source, source.find("save").unwrap() + 2);
        let selected = Range::new(at, at);
        let action = actions(&index, &doc, selected).pop().unwrap();
        host.answer_next_prompt(Some(&["ok"]));
        let resolved = resolve(&index, &doc, action.clone()).unwrap();
        assert!(
            resolved.edits[0].1[0]
                .text
                .contains("public function save(): void")
        );
        assert!(host.process_runs().is_empty());
        let changed = Document {
            version: 2,
            ..doc.clone()
        };
        assert_eq!(resolve(&index, &changed, action), Err(Error::Stale));
        host.add_project("/project/app/Livewire");
        assert!(actions(&index, &doc, selected).is_empty());
    }
}
