//! Conventional missing fully-qualified FormRequest scaffold, no controller promotion/inference.
use crate::{Main, view_creation};
use openidea_plugin::{
    Error, FileKind, Project, Range,
    file_actions::{FileAction, FileOperation},
    host,
    providers::Document,
    text::offset,
};
use std::collections::BTreeMap;
#[cfg(test)]
const PREFIX: &str = "\\App\\Http\\Requests\\";
#[derive(Default)]
pub(crate) struct State {
    next: u64,
    pending: BTreeMap<String, Pending>,
}
#[derive(Clone)]
struct Pending {
    doc: Document,
    root: String,
    candidate: Candidate,
    at: usize,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Candidate { name: String, prefix: String, begin: usize, end: usize }
#[derive(Clone, Debug, PartialEq, Eq)]
struct Mapping { source: String, prefix: String, stamp: String }
fn class_name(s: &str) -> bool {
    s.len() <= 96
        && s.len() > 7
        && s.ends_with("Request")
        && !s.eq_ignore_ascii_case("FormRequest")
        && s.bytes().enumerate().all(|(i, b)| {
            if i == 0 {
                b.is_ascii_uppercase()
            } else {
                b.is_ascii_alphanumeric() || b == b'_'
            }
        })
}
fn code(text: &str) -> Option<String> {
    if text.len() > 128 << 10
        || !text.trim_start().starts_with("<?php")
        || text.contains("<<<")
        || text.contains('`')
    {
        return None;
    }
    let b = text.as_bytes();
    let mut out = b.to_vec();
    let mut at = 0;
    let mut stack = Vec::new();
    while at < b.len() {
        let start = at;
        if b[at..].starts_with(b"/*") {
            let end = b[at + 2..].windows(2).position(|p| p == b"*/")?;
            at += end + 4;
        } else if b[at..].starts_with(b"//") || b[at] == b'#' {
            at += b[at..]
                .iter()
                .position(|c| *c == b'\n')
                .unwrap_or(b.len() - at);
        } else if matches!(b[at], b'\'' | b'"') {
            let quote = b[at];
            at += 1;
            let mut closed = false;
            while at < b.len() {
                if b[at] == quote {
                    at += 1;
                    closed = true;
                    break;
                }
                at += if b[at] == b'\\' { 2 } else { 1 };
            }
            if !closed {
                return None;
            }
        } else {
            if b[at..].starts_with(b"?>") {
                return None;
            }
            match b[at] {
                b'(' => stack.push(b')'),
                b'[' => stack.push(b']'),
                b'{' => stack.push(b'}'),
                b')' | b']' | b'}' if stack.pop() != Some(b[at]) => return None,
                _ => {}
            }
            if stack.len() > 128 {
                return None;
            }
            at += 1;
            continue;
        }
        for x in &mut out[start..at] {
            if !matches!(*x, b'\r' | b'\n') {
                *x = b' ';
            }
        }
    }
    stack
        .is_empty()
        .then(|| String::from_utf8(out).ok())
        .flatten()
}
fn namespace(prefix: &str) -> bool {
    let reserved = ["abstract","and","array","as","break","callable","case","catch","class","clone","const","continue","declare","default","die","do","echo","else","elseif","empty","enddeclare","endfor","endforeach","endif","endswitch","endwhile","enum","eval","exit","extends","false","final","finally","fn","for","foreach","function","global","goto","if","implements","include","include_once","instanceof","insteadof","interface","isset","list","match","namespace","new","null","or","parent","print","private","protected","public","readonly","require","require_once","return","self","static","switch","throw","trait","true","try","unset","use","var","while","xor","yield","int","float","bool","string","void","iterable","object","mixed","never"];
    let Some(plain) = prefix.strip_suffix('\\') else { return false };
    prefix.len() <= 256 && plain.split('\\').count() <= 8 && plain.split('\\').all(|part| {
        !part.is_empty() && part.len() <= 48 && !reserved.contains(&part.to_ascii_lowercase().as_str())
            && part.bytes().enumerate().all(|(i,b)| b.is_ascii_alphabetic() || b == b'_' || i > 0 && b.is_ascii_digit())
    })
}
fn parameter_list(masked: &str, begin: usize) -> bool {
    let mut stack = Vec::new();
    for (at, byte) in masked.as_bytes()[..begin].iter().enumerate() {
        match byte { b'('|b'['|b'{' => stack.push((*byte,at)), b')'|b']'|b'}' => {stack.pop();}, _ => {} }
    }
    let Some((b'(',open)) = stack.last() else { return false };
    let header = masked[..*open].trim_end();
    let length = header.bytes().rev().take_while(|b| b.is_ascii_alphanumeric() || *b == b'_').count();
    if length == 0 { return false; }
    let prefix = header[..header.len()-length].trim_end();
    let prefix = prefix.strip_suffix('&').unwrap_or(prefix).trim_end();
    prefix.strip_suffix("function").is_some_and(|before| before.bytes().next_back().is_none_or(|b| !b.is_ascii_alphanumeric() && b != b'_'))
}
// Intentionally separate from the broad PHP index: one semicolon namespace,
// simple unaliased class imports, and one plain class header only.
fn imported_request(masked: &str, short: &str) -> Option<String> {
    if !class_name(short) { return None; }
    let words: Vec<_> = masked.split(|c: char| !c.is_ascii_alphanumeric() && c != '_').filter(|s| !s.is_empty()).collect();
    if words.iter().filter(|w| w.eq_ignore_ascii_case("namespace")).count() != 1
        || words.iter().filter(|w| w.eq_ignore_ascii_case("class")).count() != 1 { return None; }
    let header = masked.strip_prefix("<?php")?.split_once('{')?.0;
    let mut statements = header.split(';');
    let ns = statements.next()?.trim().strip_prefix("namespace ")?.trim();
    if !namespace(&format!("{ns}\\")) { return None; }
    let mut imported = None;
    let mut aliases = std::collections::BTreeSet::new();
    let rest: Vec<_> = statements.collect();
    let (class, imports) = rest.split_last()?;
    let class_words: Vec<_> = class.split_whitespace().collect();
    if class_words.len() != 2 || class_words[0] != "class"
        || !class_words[1].bytes().enumerate().all(|(i,b)| b.is_ascii_alphabetic() || b == b'_' || i > 0 && b.is_ascii_digit())
        || class_words[1].eq_ignore_ascii_case(short) { return None; }
    for statement in imports {
        let full = statement.trim().strip_prefix("use ")?.trim();
        let full = full.strip_prefix('\\').unwrap_or(full);
        if !namespace(&format!("{full}\\")) { return None; }
        let alias = full.rsplit('\\').next()?;
        if !aliases.insert(alias.to_ascii_lowercase()) { return None; }
        if alias == short { imported = Some(format!("\\{full}")); }
    }
    imported
}
fn candidate(doc: &Document, at: usize) -> Option<Candidate> {
    if doc.language != "php" || !doc.path.ends_with(".php") || doc.path.ends_with(".blade.php") { return None; }
    let masked = code(&doc.text)?;
    if at > masked.len() || !masked.is_char_boundary(at) { return None; }
    let token = |b: u8| b.is_ascii_alphanumeric() || matches!(b,b'_'|b'\\');
    let begin = masked[..at].bytes().rposition(|b| !token(b)).map_or(0,|n|n+1);
    let end = at + masked[at..].bytes().take_while(|b| token(*b)).count();
    let full = masked.get(begin..end)?;
    if full.len() > 384 || !masked[..begin].trim_end().ends_with(['(',',']) || !parameter_list(&masked,begin) { return None; }
    let resolved;
    let full = if full.starts_with('\\') { full } else {
        resolved = imported_request(&masked, full)?;
        &resolved
    };
    let (prefix,name) = full.strip_prefix('\\')?.rsplit_once("\\Http\\Requests\\")?;
    let prefix = format!("{prefix}\\");
    if !namespace(&prefix) || !class_name(name) { return None; }
    let after = &masked[end..];
    if !after.starts_with(char::is_whitespace) { return None; }
    let var = after.trim_start().strip_prefix('$')?;
    let n = var.bytes().take_while(|b| b.is_ascii_alphanumeric() || *b == b'_').count();
    if n == 0 || !var.as_bytes()[0].is_ascii_alphabetic() && var.as_bytes()[0] != b'_' || !var[n..].trim_start().starts_with([',',')','=']) { return None; }
    Some(Candidate {name:name.into(),prefix,begin,end})
}
fn source_root(value: &str) -> Option<String> {
    let value = value.strip_suffix('/').unwrap_or(value);
    (value.len() <= 256 && value.split('/').count() <= 8 && value.split('/').all(|part| {
        !part.is_empty() && !part.starts_with('.') && !["vendor","node_modules","storage"].contains(&part.to_ascii_lowercase().as_str())
            && part.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b,b'_'|b'-'|b'.'))
    })).then(||value.into())
}
fn composer(project: &Project, root: &str, candidate: &Candidate) -> Result<Mapping, Error> {
    let path = format!("{root}/composer.json");
    if host::project_of(&path).and_then(|p| p.root().ok()).as_deref() != Some(root) { return Err(Error::Stale); }
    let stat = project.stat(&path)?;
    if stat.kind != FileKind::File || stat.size > 64 << 10 { return Err(Error::Stale); }
    let text = project.read_file(&path)?;
    let value = crate::strict_json::parse(&text,64 << 10)?;
    let target_namespace = format!("{}Http\\Requests\\",candidate.prefix).to_ascii_lowercase();
    let mut selected = None;
    for key in ["autoload","autoload-dev"] {
        let Some(section) = value.get(key) else { continue };
        let object = section.as_object().ok_or(Error::Stale)?;
        for source in ["files","classmap"] {
            if object.get(source).is_some_and(|v| !v.as_array().is_some_and(|v|v.is_empty())) {
                return Err(Error::Invalid("Opaque Composer files/classmap loading is outside this source namespace subset.".into()));
            }
        }
        if object.contains_key("psr-0") { return Err(Error::Stale); }
        if let Some(psr) = object.get("psr-4") {
            for (prefix,path) in psr.as_object().ok_or(Error::Stale)? {
                let lower = prefix.to_ascii_lowercase();
                if prefix.is_empty() || target_namespace.starts_with(&lower) || lower.starts_with(&target_namespace) {
                    if key != "autoload" || prefix != &candidate.prefix || !namespace(prefix) || selected.is_some() {
                        return Err(Error::Invalid("Overlapping/default/development/case-variant namespace mappings need a separate autoloader workflow.".into()));
                    }
                    selected = path.as_str().and_then(source_root);
                    if selected.is_none() { return Err(Error::Invalid("The selected PSR-4 source root must be one normalized literal project-relative directory.".into())); }
                }
            }
        }
    }
    Ok(Mapping {source:selected.ok_or("Composer must declare one exact literal production PSR-4 prefix for this Http\\Requests namespace.")?,prefix:candidate.prefix.clone(),stamp:text})
}
fn validate(project: &Project, root: &str, doc: &Document, candidate: &Candidate) -> Result<(String,Mapping),Error> {
    if project.read_file(&doc.path)? != doc.text { return Err(Error::Stale); }
    let mapping = composer(project,root,candidate)?;
    let target = format!("{root}/{}/Http/Requests/{}.php",mapping.source,candidate.name);
    view_creation::missing(project,&target)?;
    let mut parent = root.to_owned();
    let source_parts: Vec<&str> = mapping.source.split('/').collect();
    for (i,part) in source_parts.iter().copied().chain(["Http","Requests"]).enumerate() {
        parent.push('/');parent.push_str(part);
        if host::project_of(&parent).and_then(|p|p.root().ok()).as_deref()!=Some(root) { return Err(Error::Stale); }
        match project.stat(&parent) {
            Ok(s) if s.kind == FileKind::Directory => {},
            Err(Error::NotFound(_)) if i >= source_parts.len() => {},
            Ok(_) => return Err(Error::Stale),
            Err(e) => return Err(e),
        }
    }
    // A root-wide bounded scan also catches case-equivalent directory spellings.
    let paths = project.find_files(&format!("{}/**/*.php",mapping.source),4097)?;
    if paths.len() >= 4097 { return Err(Error::Invalid("Source-root collision scan is incomplete.".into())); }
    if paths.iter().any(|path| !crate::hover::canonical(path) || !crate::under(path,root)) { return Err(Error::Stale); }
    if paths.iter().any(|path| path.eq_ignore_ascii_case(&target)) { return Err(Error::Invalid("A case-equivalent request target already exists.".into())); }
    Ok((target,mapping))
}
pub(crate) fn promotion_namespace(prefix:&str)->bool {namespace(prefix)}
pub(crate) fn validate_promotion(project:&Project,root:&str,doc:&Document,name:&str,prefix:&str)->Result<(String,String,String),Error> {
    if !class_name(name) || !namespace(prefix) {return Err(Error::Invalid("Use a supported ASCII NameRequest and explicit mapped namespace.".into()));}
    let (target,mapping)=validate(project,root,doc,&Candidate{name:name.into(),prefix:prefix.into(),begin:0,end:0})?;
    Ok((target,mapping.stamp,mapping.source))
}
fn content(name: &str, prefix: &str, eol: &str) -> String {
    format!("<?php\n\nnamespace {prefix}Http\\Requests;\n\nuse Illuminate\\Foundation\\Http\\FormRequest;\n\nclass {name} extends FormRequest\n{{\n    public function authorize(): bool\n    {{\n        return false;\n    }}\n\n    public function rules(): array\n    {{\n        return [];\n    }}\n}}\n").replace('\n',eol)
}
fn unchanged(main: &Main, p: &Pending, doc: &Document) -> Result<(), Error> {
    if doc.path != p.doc.path
        || doc.language != p.doc.language
        || doc.version != p.doc.version
        || doc.text != p.doc.text
        || candidate(doc, p.at).as_ref() != Some(&p.candidate)
    {
        return Err(Error::Stale);
    }
    let (project, root) = view_creation::owner(doc, Some(&p.root))?;
    if main.indexes.get(&root).is_none_or(|i| i.partial)
        || project.read_file(&doc.path)? != doc.text
    {
        return Err(Error::Stale);
    }
    Ok(())
}
pub(crate) fn actions(main: &mut Main, doc: &Document, selection: Range) -> Vec<FileAction> {
    let Ok((_, root)) = view_creation::owner(doc, None) else {
        return vec![];
    };
    if main.indexes.get(&root).is_none_or(|i| i.partial) {
        return vec![];
    }
    let Some(at) = offset(&doc.text, selection.start) else {
        return vec![];
    };
    let Some(candidate) = candidate(doc, at) else {
        return vec![];
    };
    let Some(end) = offset(&doc.text, selection.end) else {
        return vec![];
    };
    if end < at || end < candidate.begin || end > candidate.end {
        return vec![];
    }
    if main.request_creations.pending.len() >= 8 {
        main.request_creations.pending.clear();
    }
    let Some(next) = main.request_creations.next.checked_add(1) else {
        return vec![];
    };
    main.request_creations.next = next;
    let key = format!("request-create:{next}:{}", doc.version);
    main.request_creations.pending.insert(
        key.clone(),
        Pending {
            doc: doc.clone(),
            root,
            candidate: candidate.clone(),
            at: candidate.begin,
        },
    );
    vec![FileAction {
        title: format!("Create reviewed mapped FormRequest {}…",candidate.name),
        kind: Some("quickfix".into()),
        data: Some(key),
        ..Default::default()
    }]
}
pub(crate) fn resolve(
    main: &mut Main,
    doc: &Document,
    action: FileAction,
) -> Result<FileAction, Error> {
    let key = action.data.as_deref().ok_or(Error::Stale)?;
    let p = main
        .request_creations
        .pending
        .get(key)
        .cloned()
        .ok_or(Error::Stale)?;
    unchanged(main, &p, doc)?;
    let (project, _) = view_creation::owner(doc, Some(&p.root))?;
    let (target, stamp) = validate(&project, &p.root, doc, &p.candidate)?;
    let text = content(
        &p.candidate.name,
        &p.candidate.prefix,
        if doc.text.contains("\r\n") {
            "\r\n"
        } else {
            "\n"
        },
    );
    let preview = format!(
        "Project: {}\nSource: {}\nDocument version: {}\nResolved source type: \\{}Http\\Requests\\{}\n\nVerified production Composer mapping: {} → {}/\nCreate (no overwrite): {}\n\nExact new contents:\n{}\n\nSource imports, variables and controller code are unchanged. Authorization deliberately returns false and validation rules are empty; fill both manually before using this request. This is a conventional missing-type scaffold, not promotion/extraction of validation or authorization policy. Installed framework/PHP/autoloader state and custom generation settings are not resolved. No Artisan/PHP/process is executed.\n\nApply one undoable file creation?",
        p.root, doc.path, doc.version, p.candidate.prefix, p.candidate.name, stamp.prefix, stamp.source, target, text
    );
    if !ui_kit::prompts::confirm_exact("Create conventional FormRequest?", &preview)? {
        return Err(Error::Cancelled);
    }
    unchanged(main, &p, doc)?;
    let (current, current_stamp) = validate(&project, &p.root, doc, &p.candidate)?;
    if target != current || stamp != current_stamp {
        return Err(Error::Stale);
    }
    main.request_creations.pending.remove(key);
    Ok(FileAction {
        title: format!("Create FormRequest {}", p.candidate.name),
        operations: vec![FileOperation::Create {
            path: target.clone(),
            text,
            overwrite: false,
        }],
        open: Some(target),
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::{file_actions::FileActions, mock::MockHost, text::position};
    const COMPOSER: &str = r#"{"autoload":{"psr-4":{"App\\":"app/","Database\\Factories\\":"database/factories/"}},"autoload-dev":{"psr-4":{"Tests\\":"tests/"}}}"#;
    fn setup() -> (MockHost, Main, Document, Range) {
        let h = MockHost::install();
        h.add_dir("app");
        h.add_dir("app/Http");
        h.add_dir("app/Http/Requests");
        h.add_file("composer.json", COMPOSER);
        let text = "<?php\r\nnamespace App\\Http\\Controllers;\r\nclass C { /* 😀 */ public function store(\\App\\Http\\Requests\\StorePostRequest $request): array { return []; } }\r\n";
        h.add_file("app/Http/Controllers/C.php", text);
        h.open_editor("/project/app/Http/Controllers/C.php", "php", text, &[]);
        let d = Document {
            path: "/project/app/Http/Controllers/C.php".into(),
            language: "php".into(),
            version: 5,
            text: text.into(),
        };
        let at = position(text, text.find("StorePostRequest").unwrap());
        let mut main = Main::default();
        main.indexes
            .insert("/project".into(), crate::index::Index::default());
        (h, main, d, Range::new(at, at))
    }
    #[test]
    fn exact_conventional_namespace_and_import_scaffold_is_one_nonoverwriting_create_without_source_edits()
     {
        let (h, mut main, d, at) = setup();
        let a = main.file_actions(&d, at).remove(0);
        let replay = a.clone();
        assert!(a.edits.is_empty() && a.operations.is_empty());
        h.answer_next_prompt(Some(&["yes"]));
        let fixed = main.resolve_file_action(&d, a).unwrap();
        assert!(fixed.edits.is_empty());
        let [
            FileOperation::Create {
                path,
                text,
                overwrite,
            },
        ] = fixed.operations.as_slice()
        else {
            panic!("one create")
        };
        assert_eq!(path, "/project/app/Http/Requests/StorePostRequest.php");
        assert!(!overwrite);
        assert!(h.invocation().project().unwrap().read_file(path).is_err());
        for expected in [
            "namespace App\\Http\\Requests;",
            "use Illuminate\\Foundation\\Http\\FormRequest;",
            "class StorePostRequest extends FormRequest",
            "return false;",
            "return [];",
        ] {
            assert!(text.contains(expected));
        }
        assert!(text.contains("\r\n"));
        assert_eq!(
            h.invocation()
                .project()
                .unwrap()
                .read_file(&d.path)
                .unwrap(),
            d.text
        );
        assert!(main.resolve_file_action(&d, replay).is_err());
        assert!(h.process_runs().is_empty());
        assert_eq!(h.fetches(), 0);
    }
    #[test]
    fn ambiguous_wrong_or_opaque_composer_namespace_metadata_is_refused() {
        for composer in [
            r#"{"autoload":{"psr-4":{"App\\":"other/"}}}"#,
            r#"{"autoload":{"psr-4":{"App\\":"app/","App\\Http\\Requests\\":"custom/"}}}"#,
            r#"{"autoload":{"psr-4":{"App\\":"app/","App\\":"other/"}}}"#,
            r#"{"autoload":{"psr-4":{"App\\":"app/"},"files":["helpers.php"]}}"#,
            r#"{"autoload":{"psr-4":{"App\\":"app/"}},"autoload-dev":{"psr-4":{"App\\":"test/"}}}"#,
        ] {
            let (h, mut main, d, at) = setup();
            let a = main.file_actions(&d, at).remove(0);
            h.add_file("composer.json", composer);
            h.answer_next_prompt(Some(&["yes"]));
            assert!(main.resolve_file_action(&d, a).is_err(), "{composer}");
            assert!(h.process_runs().is_empty());
        }
    }
    #[test]
    fn cancellation_stale_source_foreign_target_and_case_equivalent_collisions_never_create() {
        for mode in 0..6 {
            let (h, mut main, mut d, at) = setup();
            let a = main.file_actions(&d, at).remove(0);
            match mode {
                0 => h.answer_next_prompt(None),
                1 => d.version += 1,
                2 => h.add_file(&d.path, "changed"),
                3 => h.add_project("/project/app/Http/Requests"),
                4 => h.add_file("app/Http/Requests/storepostrequest.php", "KEEP"),
                _ => h.open_editor(
                    "/project/app/Http/Requests/StorePostRequest.php",
                    "php",
                    "OPEN KEEP",
                    &[],
                ),
            }
            assert!(main.resolve_file_action(&d, a).is_err(), "{mode}");
            assert!(h.process_runs().is_empty());
        }
    }
    #[test]
    fn unsupported_alias_string_comment_heredoc_nullable_and_malformed_type_contexts_are_quiet() {
        let (_h, mut main, d, _) = setup();
        for text in [
            d.text.replace(PREFIX, "Alias\\"),
            d.text
                .replace("public function store(", "public function store(?"),
            d.text.replace(
                "StorePostRequest $request",
                "StorePostRequest|Other $request",
            ),
            d.text
                .replace("public function store(", "public function store(/* ")
                .replace(" $request", " */ $request"),
            d.text.replace("StorePostRequest", "FormRequest"),
            format!("{} /* unclosed", d.text),
            d.text
                .replace("public function store(", "public function store(<<<TYPE\n"),
        ] {
            let doc = Document { text, ..d.clone() };
            let at = doc.text.find("Request").unwrap();
            assert!(candidate(&doc, at).is_none(), "{}", doc.text);
        }
        main.indexes.get_mut("/project").unwrap().partial = true;
        assert!(
            main.file_actions(
                &d,
                Range::new(
                    position(&d.text, d.text.find("StorePostRequest").unwrap()),
                    position(&d.text, d.text.find("StorePostRequest").unwrap())
                )
            )
            .is_empty()
        );
    }
    #[test]
    fn current_unsaved_php_snapshot_is_admitted_without_requiring_disk_save() {
        let (h, mut main, d, at) = setup();
        h.add_file(&d.path, "disk differs");
        let a = main.file_actions(&d, at).remove(0);
        h.answer_next_prompt(Some(&["yes"]));
        assert_eq!(main.resolve_file_action(&d, a).unwrap().operations.len(), 1);
        assert!(h.process_runs().is_empty());
    }
}

#[cfg(test)]
mod mapped_tests {
    use super::*;
    use openidea_plugin::{file_actions::FileActions, mock::MockHost, text::position};
    const MAPPING: &str = r#"{"autoload":{"psr-4":{"App\\":"app/","Acme\\Web\\":"src/Web/"}},"autoload-dev":{"psr-4":{"Tests\\":"tests/"}}}"#;
    fn setup() -> (MockHost, Main, Document, Range) {
        let host = MockHost::install().with_capabilities(&["project.read","editor.read"]);
        for dir in ["app","app/Http","app/Http/Controllers","src","src/Web"] {host.add_dir(dir);}
        host.add_file("composer.json",MAPPING);
        let source = "<?php\r\nnamespace App\\Http\\Controllers;\r\nclass PostController { /* 😀 */ public function store(\\Acme\\Web\\Http\\Requests\\StorePostRequest $request): array { return []; } }\r\n";
        let path = "/project/app/Http/Controllers/PostController.php";
        host.add_file(path,source);host.open_editor(path,"php",source,&[]);
        let doc = Document {path:path.into(),language:"php".into(),version:8,text:source.into()};
        let at = position(source,source.find("StorePostRequest").unwrap()+2);
        let mut main = Main::default();main.indexes.insert("/project".into(),crate::index::Index::default());
        (host,main,doc,Range::new(at,at))
    }
    fn action(main:&mut Main,doc:&Document,range:Range)->FileAction {
        main.file_actions(doc,range).into_iter().find(|a|a.data.as_deref().is_some_and(|d|d.starts_with("request-create:"))).expect("absolute mapped request scaffold")
    }
    #[test]
    fn custom_namespace_and_compound_source_root_create_exact_reviewed_namespace_without_source_edits() {
        let (host,mut main,doc,range)=setup();
        let lazy=action(&mut main,&doc,range);
        assert!(lazy.edits.is_empty()&&lazy.operations.is_empty());
        host.answer_next_prompt(Some(&["ok"]));
        let ready=main.resolve_file_action(&doc,lazy).unwrap();
        let [FileOperation::Create{path,text,overwrite}]=ready.operations.as_slice() else{panic!("one atomic create")};
        assert_eq!(path,"/project/src/Web/Http/Requests/StorePostRequest.php");
        assert!(!overwrite&&ready.edits.is_empty());
        assert!(text.contains("namespace Acme\\Web\\Http\\Requests;\r\n"));
        assert_eq!(text.matches("use Illuminate\\Foundation\\Http\\FormRequest;").count(),1);
        assert!(text.contains("class StorePostRequest extends FormRequest")&&text.contains("return false;")&&text.contains("return [];"));
        assert_eq!(host.invocation().project().unwrap().read_file(&doc.path).unwrap(),doc.text);
        assert!(host.invocation().project().unwrap().read_file(path).is_err());
        assert!(host.process_runs().is_empty()&&host.terminals().is_empty()&&host.requests().is_empty());
    }
    #[test]
    fn app_namespace_can_use_an_explicit_existing_custom_source_directory() {
        let (host,mut main,mut doc,_)=setup();
        doc.text=doc.text.replace("\\Acme\\Web\\Http\\Requests\\","\\App\\Http\\Requests\\");
        host.open_editor(&doc.path,"php",&doc.text,&[]);host.add_dir("domain");
        host.add_file("composer.json",r#"{"autoload":{"psr-4":{"App\\":"domain/"}}}"#);
        let at=position(&doc.text,doc.text.find("StorePostRequest").unwrap());
        let lazy=action(&mut main,&doc,Range::new(at,at));host.answer_next_prompt(Some(&["ok"]));
        let ready=main.resolve_file_action(&doc,lazy).unwrap();
        let [FileOperation::Create{path,text,overwrite}]=ready.operations.as_slice() else{panic!("create")};
        assert_eq!(path,"/project/domain/Http/Requests/StorePostRequest.php");
        assert!(text.contains("namespace App\\Http\\Requests;")&&!*overwrite);
    }
    #[test]
    fn ambiguous_fallback_development_case_and_opaque_composer_mappings_are_refused() {
        for composer in [
            r#"{"autoload":{"psr-4":{"Acme\\":"src/","Acme\\Web\\":"src/Web/"}}}"#,
            r#"{"autoload":{"psr-4":{"Acme\\Web\\":"src/Web/","Acme\\Web\\Http\\Requests\\":"other/"}}}"#,
            r#"{"autoload":{"psr-4":{"Acme\\Web\\":"src/Web/","acme\\web\\":"other/"}}}"#,
            r#"{"autoload":{"psr-4":{"Acme\\Web\\":"src/Web/","":"fallback/"}}}"#,
            r#"{"autoload":{"psr-4":{"Acme\\Web\\":"src/Web/"}},"autoload-dev":{"psr-4":{"Acme\\Web\\":"tests/"}}}"#,
            r#"{"autoload":{"psr-4":{"Acme\\Web\\":["src/Web/"]}}}"#,
            r#"{"autoload":{"psr-4":{"Acme\\Web\\":"src/Web/"},"files":["helpers.php"]}}"#,
            r#"{"autoload":{"psr-4":{"Acme\\Web\\":"src/Web/"},"classmap":["legacy/"]}}"#,
            r#"{"autoload":{"psr-4":{"Acme\\Web\\":"src/Web/"},"psr-0":{}}}"#,
            r#"{"autoload":{"psr-4":{"Acme\\Web\\":"src/Web/","Acme\\Web\\":"other/"}}}"#,
        ] {
            let (host,mut main,doc,range)=setup();let lazy=action(&mut main,&doc,range);
            host.add_file("composer.json",composer);host.answer_next_prompt(Some(&["ok"]));
            assert!(main.resolve_file_action(&doc,lazy).is_err(),"{composer}");assert!(host.process_runs().is_empty());
        }
    }
    #[test]
    fn non_normalized_hidden_ignored_or_missing_source_roots_are_refused() {
        for root in ["../outside/","/absolute/","./src/Web/","src//Web/","src\\Web\\","vendor/pkg/","node_modules/pkg/",".private/","storage/code/","missing/root/"] {
            let (host,mut main,doc,range)=setup();let lazy=action(&mut main,&doc,range);
            let composer=serde_json::json!({"autoload":{"psr-4":{"Acme\\Web\\":root}}}).to_string();
            host.add_file("composer.json",&composer);host.answer_next_prompt(Some(&["ok"]));
            assert!(main.resolve_file_action(&doc,lazy).is_err(),"{root}");
        }
        let (host,mut main,doc,range)=setup();let lazy=action(&mut main,&doc,range);
        host.add_file("src/Web","not a directory");host.answer_next_prompt(Some(&["ok"]));
        assert!(main.resolve_file_action(&doc,lazy).is_err());
    }
    #[test]
    fn named_parameter_context_is_required_and_expression_scope_is_not_broadened() {
        let (_host,_main,doc,_)=setup();
        for source in [
            "<?php foo(\\Acme\\Web\\Http\\Requests\\StorePostRequest $request);",
            "<?php $f(\\Acme\\Web\\Http\\Requests\\StorePostRequest $request);",
            "<?php fn(\\Acme\\Web\\Http\\Requests\\StorePostRequest $request) => [];",
            "<?php function store($x = call(\\Acme\\Web\\Http\\Requests\\StorePostRequest $request)) {}",
            "<?php function store($x = [1, \\Acme\\Web\\Http\\Requests\\StorePostRequest $request]) {}",
            "<?php function store(?\\Acme\\Web\\Http\\Requests\\StorePostRequest $request) {}",
            "<?php function store(\\Acme\\Web\\Http\\Requests\\StorePostRequest|Other $request) {}",
            "<?php function store(\\Acme\\Web\\Http\\Requests\\StorePostRequest &$request) {}",
            "<?php function store(\\Acme\\Match\\Http\\Requests\\StorePostRequest $request) {}",
        ] {
            let d=Document{text:source.into(),..doc.clone()};
            assert!(candidate(&d,source.find("StorePostRequest").unwrap()).is_none(),"{source}");
        }
    }
    #[test]
    fn foreign_nested_parent_case_equivalent_target_dirty_target_and_stale_source_never_create() {
        for mode in 0..5 {
            let (host,mut main,mut doc,range)=setup();let lazy=action(&mut main,&doc,range);
            match mode {
                0=>host.add_project("/project/src/Web"),
                1=>host.add_file("src/Web/http/requests/storepostrequest.php","KEEP"),
                2=>host.open_editor("/project/src/Web/Http/Requests/StorePostRequest.php","php","OPEN KEEP",&[]),
                3=>doc.version+=1,
                _=>host.open_editor(&doc.path,"php","<?php changed",&[]),
            }
            host.answer_next_prompt(Some(&["ok"]));assert!(main.resolve_file_action(&doc,lazy).is_err(),"{mode}");
            assert!(host.process_runs().is_empty());
        }
    }
    #[test]
    fn complete_bounded_source_collision_inventory_is_required() {
        let (host,mut main,doc,range)=setup();let lazy=action(&mut main,&doc,range);
        for n in 0..4097 {host.add_file(&format!("src/Web/Existing/Source{n}.php"),"<?php");}
        host.answer_next_prompt(Some(&["ok"]));
        assert!(main.resolve_file_action(&doc,lazy).unwrap_err().to_string().contains("incomplete"));
    }
    #[test]
    fn imported_request_parameter_creates_exact_mapped_class_without_editing_controller() {
        let (h,mut main,mut doc,_) = setup();
        doc.text = "<?php\nnamespace App\\Http\\Controllers;\nuse App\\Http\\Requests\\StorePostRequest;\nclass C { public function store(StorePostRequest $request): array { return []; } }\n".into();
        h.add_file(&doc.path,&doc.text);
        h.open_editor(&doc.path,"php",&doc.text,&[]);
        let at=position(&doc.text,doc.text.rfind("StorePostRequest").unwrap());
        let actions=main.file_actions(&doc,Range::new(at,at));
        assert_eq!(actions.len(),1,"imported parameter must expose the reviewed scaffold action");
        h.answer_next_prompt(Some(&["yes"]));
        let result=main.resolve_file_action(&doc,actions[0].clone()).unwrap();
        assert!(result.edits.is_empty());
        let [FileOperation::Create{path,text,overwrite}]=result.operations.as_slice() else {panic!("one create")};
        assert_eq!(path,"/project/app/Http/Requests/StorePostRequest.php");
        assert!(!overwrite && text.contains("namespace App\\Http\\Requests;") && text.contains("use Illuminate\\Foundation\\Http\\FormRequest;"));
        assert_eq!(h.invocation().project().unwrap().read_file(&doc.path).unwrap(),doc.text);
    }

    #[test]
    fn imported_parameter_refuses_ambiguous_or_semantic_header_resolution() {
        let base="<?php\nnamespace App\\Http\\Controllers;\nuse App\\Http\\Requests\\StorePostRequest;\nclass C { public function store(StorePostRequest $request) {} }";
        for source in [
            base.replace("use App", "use function App"),
            base.replace("StorePostRequest;", "StorePostRequest as Alias;"),
            base.replace("class C", "class StorePostRequest"),
            base.replace("class C", "class C extends ParentClass"),
            base.replace("class C", "use Other\\StorePostRequest;\nclass C"),
            base.replace("namespace App\\Http\\Controllers;", "namespace App\\Http\\Controllers {" ) + "}",
            base.replace("class C", "namespace Other;\nclass C"),
            base.replace("StorePostRequest $request", "Http\\Requests\\StorePostRequest $request"),
        ] {
            let doc=Document{path:"/project/C.php".into(),language:"php".into(),version:1,text:source};
            let at=doc.text.rfind("StorePostRequest $request").unwrap();
            assert!(candidate(&doc,at).is_none(),"{}",doc.text);
        }
        let doc=Document{path:"/project/C.php".into(),language:"php".into(),version:1,text:base.into()};
        assert!(candidate(&doc,doc.text.rfind("StorePostRequest $request").unwrap()).is_some());
    }

}
