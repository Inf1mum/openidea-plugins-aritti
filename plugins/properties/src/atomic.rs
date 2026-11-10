//! Deferred, confirmed workspace edits. Bundle snapshots include unsaved text;
//! no shell process or direct file replacement participates in these actions.
use super::*;
use openidea_plugin::file_actions::{FileAction, FileActions, FileOperation};

const MAX_FILE: usize = 256 << 10;
const MAX_TOTAL: usize = 1 << 20;

pub(super) fn snapshot(doc: &Document) -> Result<(String, Vec<(String, String)>), Error> {
    let project = host::project_of(&doc.path).ok_or(Error::Stale)?;
    let root = project.root()?;
    let ownership = usages::SourceOwnership::current(&root).ok_or(Error::Stale)?;
    if !ownership.contains(&doc.path) || doc.text.len() > MAX_FILE { return Err(Error::Stale); }
    let (siblings, complete) = bundle_inventory(&project, &doc.path)?;
    // Preserve the raw sentinel before similarly named non-bundle files are
    // filtered out. A small filtered list is not proof of complete membership.
    if !complete {
        return Err("Bundle discovery reached its safety limit; no partial locale edit will be applied.".into());
    }
    let mut files = vec![(doc.path.clone(), doc.text.clone())];
    let mut total = doc.text.len();
    for path in siblings {
        if !ownership.contains(&path) { return Err(Error::Stale); }
        let stat = project.stat(&path)?;
        if stat.kind != openidea_plugin::FileKind::File || stat.size > MAX_FILE as u64 || stat.size > (MAX_TOTAL-total) as u64 {
            return Err("Bundle source exceeds its pre-read admission limit.".into());
        }
        let text = project.read_file(&path)?;
        if text.len() > MAX_FILE || text.len() > MAX_TOTAL-total { return Err("Bundle source exceeds its byte limit.".into()); }
        total += text.len();
        files.push((path, text));
    }
    if files.len() >= 200
        || files.iter().any(|(_, text)| text.len() > MAX_FILE)
        || files.iter().map(|(_, text)| text.len()).sum::<usize>() > MAX_TOTAL
    {
        return Err("Bundle action exceeds the 199-file, 256 KiB per file or 1 MiB total safety limit.".into());
    }
    if files.iter().any(|(_, text)| parse(text).iter().any(|p| p.exact_key(text).is_none())) {
        return Err("A bundle key cannot be represented losslessly; no partial locale operation will be prepared.".into());
    }
    Ok((root, files))
}

pub(super) fn unchanged(doc: &Document, root: &str, before: &[(String, String)]) -> Result<(), Error> {
    let (current_root, mut current) = snapshot(doc)?;
    let mut expected = before.to_vec();
    current.sort();
    expected.sort();
    if current_root != root || current != expected {
        return Err(Error::Stale);
    }
    // snapshot uses doc.text for the source, so also observe any newer live source
    // buffer/disk text while the confirmation was open.
    let project = host::project_of(&doc.path).ok_or(Error::Stale)?;
    if project.read_file(&doc.path)? != doc.text {
        return Err(Error::Stale);
    }
    Ok(())
}

fn confirm(doc: &Document, root: &str, before: &[(String, String)], title: &str, edits: &[(String, Vec<TextEdit>)]) -> Result<(), Error> {
    let mut preview = String::new();
    for (path, changes) in edits {
        preview.push_str(&format!("{}\n", serde_json::to_string(path).unwrap_or_default()));
        for edit in changes {
            preview.push_str(&format!(
                "{}:{}–{}:{} — replacement source:\n{}\n\n",
                edit.range.start.line + 1,
                edit.range.start.character + 1,
                edit.range.end.line + 1,
                edit.range.end.character + 1,
                ui_kit::prompts::readable_source(&edit.text)
            ));
        }
    }
    if preview.len() > ui_kit::prompts::MAX_CONFIRM_BYTES {
        return Err("The edit preview exceeds 16 KiB; edit fewer locales at once.".into());
    }
    if !ui_kit::prompts::confirm_exact(title, &preview)? {
        return Err(Error::Cancelled);
    }
    unchanged(doc, root, before)
}

fn locale_target(doc: &Document, locale: &str) -> Result<String, Error> {
    let parts: Vec<_> = locale.split('_').collect();
    let valid = match parts.as_slice() {
        [language] => language.len() == 2 && language.bytes().all(|b| b.is_ascii_lowercase()),
        [language, country] => {
            language.len() == 2
                && language.bytes().all(|b| b.is_ascii_lowercase())
                && ((country.len() == 2 && country.bytes().all(|b| b.is_ascii_uppercase()))
                    || (country.len() == 3 && country.bytes().all(|b| b.is_ascii_digit())))
        }
        _ => false,
    };
    let base = split_locale(stem(&doc.path)).0;
    let new_stem = format!("{base}_{locale}");
    if !valid || split_locale(&new_stem) != (base, locale) {
        return Err("Use a known two-letter language, optionally _COUNTRY or _123 (for example fr or fr_CA).".into());
    }
    Ok(format!("{}/{}.properties", dir(&doc.path), new_stem))
}

fn vacant(project: &Project, target: &str) -> Result<(), Error> {
    if project.exists(target)? {
        return Err("The locale file already exists; nothing will be overwritten.".into());
    }
    match project.read_file(target) {
        Ok(_) => Err("The locale has an open buffer; nothing will be overwritten.".into()),
        Err(Error::NotFound(_)) => Ok(()),
        Err(error) => Err(error),
    }
}

impl FileActions for Properties {
    fn file_actions(&mut self, doc: &Document, selection: Range) -> Vec<FileAction> {
        if doc.language != "properties" || !doc.path.ends_with(".properties") || doc.text.len() > MAX_FILE {
            return Vec::new();
        }
        if parse(&doc.text).iter().any(|p| p.exact_key(&doc.text).is_none()) { return Vec::new(); }
        let Some(project) = host::project_of(&doc.path) else { return Vec::new() };
        let Ok(root) = project.root() else { return Vec::new() };
        let mut actions: Vec<_> = self
            .property_actions(doc, selection)
            .into_iter()
            .filter_map(|a| {
                a.data.map(|operation| FileAction {
                    title: a.title,
                    kind: a.kind,
                    data: Some(serde_json::json!([root, doc.path, doc.version, operation]).to_string()),
                    ..FileAction::default()
                })
            })
            .collect();
        let at = offset(&doc.text, selection.start).unwrap_or(doc.text.len());
        if let Some(prop) = parse(&doc.text).into_iter().find(|p| p.contains(at)) {
            let key = prop.exact_key(&doc.text).unwrap();
            if !key.is_empty() && key.len() <= 512 && !key.chars().any(char::is_control) {
                if scoped_rename::offered(doc) {
                    actions.push(FileAction {
                        title: "Change bundle key and local JDK literals…".into(),
                        kind: Some("refactor.rename".into()),
                        data: Some(serde_json::json!([root, doc.path, doc.version, format!("rename-local\n{key}")]).to_string()),
                        ..FileAction::default()
                    });
                }
                actions.push(FileAction {
                    title: "Change key in this bundle only…".into(),
                    kind: Some("refactor.rewrite".into()),
                    data: Some(serde_json::json!([root, doc.path, doc.version, format!("rename-bundle\n{key}")]).to_string()),
                    ..FileAction::default()
                });
            }
            actions.push(FileAction {
                title: "Set property value across selected locales…".into(),
                kind: Some("refactor.rewrite".into()),
                data: Some(serde_json::json!([root, doc.path, doc.version, format!("set\n{}", prop.unescaped_key(&doc.text))]).to_string()),
                ..FileAction::default()
            });
        }
        actions.push(FileAction {
            title: "Create locale file…".into(),
            kind: Some("refactor.rewrite".into()),
            data: Some(serde_json::json!([root, doc.path, doc.version, "create-locale"]).to_string()),
            ..FileAction::default()
        });
        actions
    }

    fn resolve_file_action(&mut self, doc: &Document, action: FileAction) -> Result<FileAction, Error> {
        let data: serde_json::Value =
            serde_json::from_str(action.data.as_deref().ok_or("Missing action context.")?).map_err(|_| "Invalid action context.")?;
        let parts = data.as_array().filter(|p| p.len() == 4).ok_or("Invalid action context.")?;
        let root = parts[0].as_str().ok_or("Invalid project.")?;
        if parts[1].as_str() != Some(&doc.path) || parts[2].as_u64() != Some(doc.version) {
            return Err(Error::Stale);
        }
        let (current_root, before) = snapshot(doc)?;
        if root != current_root {
            return Err(Error::Stale);
        }
        let operation = parts[3].as_str().ok_or("Invalid operation.")?;
        if operation == "create-locale" {
            let locale = host::input("Locale (fr or fr_CA; creates an empty ASCII-content file)", None)?.ok_or(Error::Cancelled)?;
            let target = locale_target(doc, locale.trim())?;
            let project = host::project_of(&doc.path).ok_or(Error::Stale)?;
            vacant(&project, &target)?;
            let text = format!("# Locale: {}\n", locale.trim());
            if !ui_kit::prompts::confirm_exact(
                "Create locale file?",
                &format!(
                    "{}\n\n{text}\nNo translations are copied. The host applies its new-file encoding defaults; content is ASCII.",
                    serde_json::to_string(&target).unwrap_or_default()
                ),
            )? {
                return Err(Error::Cancelled);
            }
            unchanged(doc, root, &before)?;
            vacant(&project, &target)?;
            return Ok(FileAction {
                title: action.title,
                operations: vec![FileOperation::Create { path: target.clone(), text, overwrite: false }],
                open: Some(target),
                ..FileAction::default()
            });
        }
        let operation: Vec<_> = operation.splitn(3, '\n').collect();
        if let ["rename-local",key] = operation.as_slice() {
            return scoped_rename::resolve(doc,action,root,before,key);
        }
        let mut review_title = action.title.clone();
        let edits = match operation.as_slice() {
            ["rename-bundle", key] => {
                if key.is_empty() || key.len() > 512 || key.chars().any(char::is_control)
                    || parse(&doc.text).iter().filter(|p| p.exact_key(&doc.text).as_deref() == Some(*key)).count() != 1
                { return Err("Choose one unambiguous property key in the current file.".into()); }
                let new = host::input("New bundle key (code references are not changed)", Some(key))?.ok_or(Error::Cancelled)?;
                if new.is_empty() || new.len() > 512 || new.chars().any(char::is_control) || new == *key {
                    return Err("Use a different nonempty key of at most 512 bytes without control characters.".into());
                }
                let escaped = props::escape_key(&new);
                if props::unescape_exact(&escaped).as_deref() != Some(&new) { return Err(Error::Stale); }
                let mut edits = Vec::new();
                for (path,text) in &before {
                    let properties = parse(text);
                    if properties.iter().any(|p| p.exact_key(text).as_deref() == Some(&new)) {
                        return Err("The new key already exists in a locale; no files were changed.".into());
                    }
                    let matches: Vec<_> = properties.iter().filter(|p| p.exact_key(text).as_deref() == Some(*key)).collect();
                    if matches.len() > 1 { return Err("Resolve duplicate old keys before changing this bundle key.".into()); }
                    if let Some(property) = matches.first() {
                        edits.push((path.clone(), to_edits(text, vec![(property.key.clone(), escaped.clone())])));
                    }
                }
                review_title = format!("Change bundle key {} → {}? Only listed visible bundle files change; code references and ignored files are unchanged.", serde_json::to_string(key).unwrap_or_default(), serde_json::to_string(&new).unwrap_or_default());
                edits
            }
            ["translate", target, key] => {
                if *target == doc.path || !before.iter().any(|(path, _)| path == target) {
                    return Err(Error::Stale);
                }
                let text = &before.iter().find(|(path, _)| path == target).unwrap().1;
                if keys(text).contains(*key) {
                    return Err("This key already has a translation; refresh the action.".into());
                }
                Self::add_translation(doc, target, key)?
            }
            ["remove", key] => Self::remove_everywhere(doc, key)?,
            ["sort-bundle"] => Self::sort_bundle(doc)?,
            ["set", key] => {
                let items: Vec<_> = before.iter().map(|(path, _)| openidea_plugin::PickItem::new(path, file_name(path))).collect();
                let targets = host::pick_many("Locales to update", &items)?.ok_or(Error::Cancelled)?;
                if targets.is_empty() {
                    return Err(Error::Cancelled);
                }
                if targets.iter().collect::<HashSet<_>>().len() != targets.len()
                    || targets.iter().any(|p| !before.iter().any(|(path, _)| path == p))
                {
                    return Err(Error::Stale);
                }
                let value = host::input("New property value", None)?.ok_or(Error::Cancelled)?;
                if value.len() > 16 << 10 {
                    return Err("Value exceeds 16 KiB.".into());
                }
                let mut edits = Vec::new();
                for target in targets {
                    let text = &before.iter().find(|(path, _)| *path == target).unwrap().1;
                    let matching: Vec<_> = parse(text).into_iter().filter(|p| p.unescaped_key(text) == *key).collect();
                    if matching.len() > 1 {
                        return Err("Resolve duplicate keys before updating this locale.".into());
                    }
                    let changes = if let Some(prop) = matching.first() {
                        if prop.sep.is_some() {
                            vec![(prop.value.clone(), props::escape_value(&value))]
                        } else {
                            vec![(
                                prop.key.end..prop.value.end,
                                format!("{}{}", Settings::load().delimiter_text(), props::escape_value(&value)),
                            )]
                        }
                    } else {
                        vec![append_line(
                            text,
                            &format!("{}{}{}", props::escape_key(key), Settings::load().delimiter_text(), props::escape_value(&value)),
                        )]
                    };
                    edits.push((target, to_edits(text, changes)));
                }
                edits
            }
            _ => return Err("Unknown bundle action.".into()),
        };
        if edits.iter().any(|(path, _)| !before.iter().any(|(member, _)| member == path)) {
            return Err(Error::Stale);
        }
        confirm(doc, root, &before, &review_title, &edits)?;
        Ok(FileAction { title: action.title, kind: action.kind, edits, ..FileAction::default() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;
    fn setup() -> (MockHost, Properties, Document) {
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read", "editor.write"]);
        let doc = Document {
            path: "/project/messages.properties".into(),
            language: "properties".into(),
            version: 1,
            text: "hello=Hello\n".into(),
        };
        host.add_file(&doc.path, &doc.text);
        host.add_file("/project/messages_fr.properties", "hello=Bonjour\nkeep=original\n");
        (host, Properties::default(), doc)
    }
    fn action(plugin: &mut Properties, doc: &Document, title: &str) -> FileAction {
        plugin
            .file_actions(doc, Range::new(Position::new(0, 0), Position::new(0, 0)))
            .into_iter()
            .find(|a| a.title.starts_with(title))
            .unwrap()
    }
    #[test]
    fn scoped_bundle_key_change_preserves_values_comments_and_source_references() {
        let (host, mut plugin, mut doc) = setup();
        doc.text = "# note\nhel\\u006Co : Hello\n".into();
        host.add_file(&doc.path, &doc.text);
        host.add_file("/project/messages_de.properties", "other=Other\n");
        host.add_file("/project/src/main/java/A.java", "java.util.ResourceBundle.getBundle(\"messages\").getString(\"hello\")");
        let a=plugin.file_actions(&doc,Range::new(Position::new(1,1),Position::new(1,1))).into_iter().find(|a|a.title.starts_with("Change key")).unwrap();
        host.answer_next_prompt(Some(&["!new:key=😀 "]));
        host.answer_next_prompt(Some(&[]));
        let resolved=plugin.resolve_file_action(&doc,a).unwrap();
        assert_eq!(resolved.edits.len(),2);
        assert!(resolved.operations.is_empty());
        for (_,edits) in &resolved.edits {
            assert_eq!(edits.len(),1);
            assert_eq!(props::unescape_exact(&edits[0].text).as_deref(),Some("!new:key=😀 "));
        }
        let edit=&resolved.edits.iter().find(|(p,_)|p==&doc.path).unwrap().1[0];
        let mut after=doc.text.clone();
        after.replace_range(offset(&after,edit.range.start).unwrap()..offset(&after,edit.range.end).unwrap(),&edit.text);
        assert_eq!(after,"# note\n\\!new\\:key\\=😀\\  : Hello\n");
        assert_eq!(project_for(&doc.path).unwrap().read_file(&doc.path).unwrap(),doc.text,"resolution only prepares host edits");
        assert!(project_for(&doc.path).unwrap().read_file("/project/src/main/java/A.java").unwrap().contains("getString(\"hello\")"));
    }
    #[test]
    fn scoped_bundle_key_change_rejects_collisions_duplicates_and_invalid_names() {
        for (locale,new) in [("keep=x\nhello=Bonjour\n","keep"),("\\u0066resh=x\nhello=Bonjour\n","fresh"),("hello=a\nhel\\u006Co=b\n","fresh"),("hello=x\n",""),("hello=x\n","bad\nkey"),("hello=x\n","hello")] {
            let (host,mut plugin,doc)=setup();
            host.add_file("/project/messages_fr.properties",locale);
            let a=action(&mut plugin,&doc,"Change key");
            host.answer_next_prompt(Some(&[new]));
            host.answer_next_prompt(Some(&[]));
            assert!(plugin.resolve_file_action(&doc,a).is_err());
            assert_eq!(project_for(&doc.path).unwrap().read_file(&doc.path).unwrap(),doc.text);
        }
    }
    #[test]
    fn scoped_bundle_key_change_cancel_and_forged_context_do_not_prepare_edits() {
        for cancel_input in [true,false] {
            let (host,mut plugin,doc)=setup();
            let a=action(&mut plugin,&doc,"Change key");
            if !cancel_input { host.answer_next_prompt(Some(&["fresh"])); }
            host.answer_next_prompt(None);
            assert!(matches!(plugin.resolve_file_action(&doc,a),Err(Error::Cancelled)));
            assert_eq!(project_for(&doc.path).unwrap().read_file(&doc.path).unwrap(),doc.text);
        }
        let (host,mut plugin,doc)=setup();
        let mut a=action(&mut plugin,&doc,"Change key");
        a.data=Some(serde_json::json!(["/project",doc.path,doc.version,"rename-bundle\nmissing"]).to_string());
        assert!(plugin.resolve_file_action(&doc,a).is_err());
        assert!(host.opened_locations().is_empty());
    }
    #[test]
    fn atomic_snapshot_admits_source_metadata_and_bounded_current_owners() {
        let (host,_,doc)=setup();
        host.add_file("/project/messages_fr.properties",&"x".repeat(MAX_FILE+1));
        assert!(snapshot(&doc).err().unwrap().to_string().contains("pre-read admission"));
        host.add_file("/project/messages_fr.properties","hello=x\n");
        for n in 0..64 { host.add_project(&format!("/unrelated{n}")); }
        assert!(snapshot(&doc).is_err());
    }
    #[test]
    fn raw_discovery_cap_with_filtered_near_names_cannot_authorize_partial_bundle_mutation() {
        let (host, mut plugin, doc) = setup();
        for index in 0..201 {
            // These match messages*.properties but are different bundle bases,
            // sorting before the source and actual _fr locale in the inventory.
            host.add_file(&format!("/project/messages-other-{index:03}.properties"), "unrelated=1\n");
        }
        let project=host.invocation().project().unwrap();
        let (filtered, complete)=bundle_inventory(&project,&doc.path).unwrap();
        assert!(!complete);assert!(filtered.len()<199,"fixture must evade the old post-filter count guard");
        host.answer_next_prompt(Some(&[])); // Approve the old flow so cancellation cannot mask acceptance.
        let action=FileAction {title:"Remove from bundle".into(),data:Some(serde_json::json!(["/project",doc.path,doc.version,"remove\nhello"]).to_string()),..FileAction::default()};
        assert!(plugin.resolve_file_action(&doc,action).is_err(),"incomplete raw membership must never authorize partial all-locale edits");
        assert_eq!(project.read_file("/project/messages_fr.properties").unwrap(),"hello=Bonjour\nkeep=original\n");
        assert!(host.process_runs().is_empty());
    }
    #[test]
    fn bundle_mutations_require_lossless_key_identity_in_every_source() {
        let (host, mut plugin, mut doc) = setup();
        doc.text = "\\uD800=one\n\\uD801=two\n�=replacement\n".into();
        host.add_file(&doc.path, &doc.text);
        let a = FileAction {title:"Remove from bundle".into(), data:Some(serde_json::json!(["/project",doc.path,doc.version,"remove\n�"]).to_string()), ..FileAction::default()};
        host.answer_next_prompt(Some(&[]));
        assert!(plugin.resolve_file_action(&doc, a).is_err());
        assert_eq!(project_for(&doc.path).unwrap().read_file(&doc.path).unwrap(), doc.text);
        let normal = Document {text:"hello=Hello\n".into(), ..doc.clone()};
        host.add_file(&normal.path, &normal.text);
        host.add_file("/project/messages_fr.properties", "other\\uD800=value\n");
        assert!(snapshot(&normal).is_err());
        host.add_file("/project/messages_fr.properties", "\\uD83D\\uDE00=Paired\n");
        assert!(snapshot(&normal).is_ok());
    }
    #[test]
    fn atomic_selected_locale_changes_use_unsaved_text_without_running_tee() {
        let (host, mut plugin, doc) = setup();
        host.open_editor("/project/messages_fr.properties", "properties", "hello=Uncommitted\nkeep=unsaved\n", &[]);
        let a = action(&mut plugin, &doc, "Set property");
        host.answer_next_prompt(Some(&["/project/messages.properties", "/project/messages_fr.properties"]));
        host.answer_next_prompt(Some(&["Line\nDeux"]));
        host.answer_next_prompt(Some(&[]));
        let resolved = plugin.resolve_file_action(&doc, a).unwrap();
        assert_eq!(resolved.edits.len(), 2);
        assert_eq!(resolved.edits[1].1[0].text, "Line\\nDeux");
        assert_eq!(resolved.edits[1].1[0].range, Range::new(Position::new(0, 6), Position::new(0, 17)));
        assert_eq!(host.editor_text(), "hello=Uncommitted\nkeep=unsaved\n");
        assert!(host.process_runs().is_empty());
    }
    #[test]
    fn mutations_reject_source_versions_foreign_targets_and_duplicates() {
        let (host, mut plugin, mut doc) = setup();
        let a = action(&mut plugin, &doc, "Set property");
        doc.version += 1;
        assert!(matches!(plugin.resolve_file_action(&doc, a), Err(Error::Stale)));
        doc.version -= 1;
        let a = FileAction {
            title: "forged".into(),
            data: Some(serde_json::json!(["/project", doc.path, doc.version, "translate\n/project/other.properties\nhello"]).to_string()),
            ..FileAction::default()
        };
        host.add_file("/project/other.properties", "");
        assert!(matches!(plugin.resolve_file_action(&doc, a), Err(Error::Stale)));
        host.add_file("/project/messages_fr.properties", "hello=One\nhello=Two\n");
        let a = action(&mut plugin, &doc, "Set property");
        host.answer_next_prompt(Some(&["/project/messages_fr.properties"]));
        host.answer_next_prompt(Some(&["x"]));
        assert!(plugin.resolve_file_action(&doc, a).is_err());
        assert!(host.process_runs().is_empty());
    }
    #[test]
    fn cancelled_value_selection_or_confirmation_returns_no_edits() {
        for stage in 0..3 {
            let (host, mut plugin, doc) = setup();
            let a = action(&mut plugin, &doc, "Set property");
            if stage > 0 {
                host.answer_next_prompt(Some(&["/project/messages_fr.properties"]));
            }
            if stage > 1 {
                host.answer_next_prompt(Some(&["New"]));
            }
            host.answer_next_prompt(None);
            assert!(matches!(plugin.resolve_file_action(&doc, a), Err(Error::Cancelled)));
            assert_eq!(
                host.invocation().project().unwrap().read_file("/project/messages_fr.properties").unwrap(),
                "hello=Bonjour\nkeep=original\n"
            );
        }
    }
    #[test]
    fn snapshot_detects_same_length_edits_membership_and_project_changes() {
        let (host, _, doc) = setup();
        let (root, before) = snapshot(&doc).unwrap();
        host.add_file("/project/messages_fr.properties", "hello=BonjoUr\nkeep=original\n");
        assert!(matches!(unchanged(&doc, &root, &before), Err(Error::Stale)));
        host.add_file("/project/messages_fr.properties", "hello=Bonjour\nkeep=original\n");
        host.add_file("/project/messages_de.properties", "hello=Hallo\n");
        assert!(matches!(unchanged(&doc, &root, &before), Err(Error::Stale)));
        host.set_project_root(Some("/different"));
        assert!(matches!(unchanged(&doc, &root, &before), Err(Error::Stale)));
    }
    #[test]
    fn locale_creation_is_ascii_utf8_nonoverwriting_and_name_validated() {
        let (host, mut plugin, doc) = setup();
        for bad in ["../fr", "fr/CA", "xx", "fr_ca", "eng", "fr_CA_extra", "fr\nCA", ""] {
            assert!(locale_target(&doc, bad).is_err(), "{bad:?}");
        }
        assert!(locale_target(&doc, "es_419").unwrap().ends_with("messages_es_419.properties"));
        let a = action(&mut plugin, &doc, "Create locale");
        host.answer_next_prompt(Some(&["fr_CA"]));
        host.answer_next_prompt(Some(&[]));
        let resolved = plugin.resolve_file_action(&doc, a).unwrap();
        let FileOperation::Create { path, text, overwrite } = &resolved.operations[0] else { panic!() };
        assert_eq!(path, "/project/messages_fr_CA.properties");
        assert_eq!(text, "# Locale: fr_CA\n");
        assert!(!overwrite);
        assert!(text.is_ascii());
        assert!(!host.invocation().project().unwrap().exists(path).unwrap());
        host.open_editor(path, "properties", "unsaved", &[]);
        assert!(vacant(&host.invocation().project().unwrap(), path).is_err());
    }
    #[test]
    fn locale_creation_cancels_and_refuses_existing_locale() {
        for confirm in [false, true] {
            let (host, mut plugin, doc) = setup();
            let a = action(&mut plugin, &doc, "Create locale");
            if confirm {
                host.answer_next_prompt(Some(&["de"]));
            }
            host.answer_next_prompt(None);
            assert!(matches!(plugin.resolve_file_action(&doc, a), Err(Error::Cancelled)));
        }
        let (host, mut plugin, doc) = setup();
        let a = action(&mut plugin, &doc, "Create locale");
        host.answer_next_prompt(Some(&["fr"]));
        assert!(plugin.resolve_file_action(&doc, a).is_err());
        assert!(host.process_runs().is_empty());
    }
}
