//! `.env` files: highlighting, the original inspections with their quick
//! fixes, completion, go to declaration and find usages of environment
//! variables in `${…}` and in code, keys of Dockerfiles and docker-compose
//! files. A port of JetBrains' ".env files" plugin (`ru.adelf.idea.dotenv`, MIT).

mod code;
mod docker;
mod hover;
mod semantic;
mod source_update;
mod index;
mod lint;
mod parse;

use index::{Declaration, Index, Usage, env_declarations, is_env_file, nested_usages, range};
use openidea_plugin::providers::{CodeAction, CompletionItem, CompletionKind, Document, Providers};
use openidea_plugin::syntax::{Highlight, SyntaxProvider, Token};
use openidea_plugin::{
    Context, Diagnostic, Error, Event, EventKind, Invocation, Location, Plugin, Position, Project, Range, Severity, TextEdit, host, text,
};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;
use std::time::{Duration, Instant};

/// What an activation or a batch of events may spend walking projects (of their 5 s).
const SCAN_BUDGET: Duration = Duration::from_secs(3);

#[derive(Default)]
pub struct DotEnv {
    /// Each open project's index, by root.
    projects: HashMap<String, Index>,
    updates: source_update::Updates,
}

/// The open project holding `path`, and its root.
fn project_of(path: &str) -> Option<(String, Project)> {
    host::projects()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|p| Some((p.root().ok()?, p)))
        .filter(|(root, _)| Path::new(path).starts_with(root))
        .max_by_key(|(root, _)| root.len())
}

/// The line holding byte `offset`: its start and text.
fn line_of(text: &str, offset: usize) -> (usize, &str) {
    let start = text[..offset].rfind('\n').map_or(0, |i| i + 1);
    let end = text[offset..].find('\n').map_or(text.len(), |i| offset + i);
    (start, &text[start..end])
}

/// Byte range of the environment variable key at `at`, if the caret is in one.
fn key_at(doc: &Document, at: Position) -> Option<std::ops::Range<usize>> {
    let offset = text::offset(&doc.text, at)?;
    let (start, line) = line_of(&doc.text, offset);
    let key = if doc.language == "dotenv" { code::key_in_nested(line, offset - start) } else { code::key_in_code(line, offset - start) }?;
    Some(start + key.start..start + key.end)
}

/// `$KEY` being typed in a double-quoted value, from the `$`.
fn dollar_at(doc: &Document, at: Position) -> Option<std::ops::Range<usize>> {
    let offset = text::offset(&doc.text, at)?;
    let tokens = parse::lex(&doc.text);
    let quoted = parse::properties(&doc.text, &tokens).into_iter().any(|p| {
        let Some(value) = p.value.filter(|_| p.quote == Some('"')) else {
            return false;
        };
        let closed = value.len() > 1 && doc.text[..value.end].ends_with('"');
        value.start < offset && offset <= value.end && (offset < value.end || !closed)
    });
    if !quoted {
        return None;
    }
    let (start, line) = line_of(&doc.text, offset);
    let dollar = code::dollar_at(line, offset - start)?;
    Some(start + dollar.start..start + dollar.end)
}

/// The key the caret is on where a `.env` file declares it.
fn declared_key_at(doc: &Document, at: Position) -> Option<String> {
    let offset = text::offset(&doc.text, at)?;
    let tokens = parse::lex(&doc.text);
    parse::properties(&doc.text, &tokens).into_iter().find(|p| p.key.start <= offset && offset <= p.key.end).map(|p| p.name(&doc.text))
}

impl DotEnv {
    /// `path`'s project index: just the root's env files until the project is walked.
    fn index(&mut self, path: &str) -> Option<&Index> {
        let (root, project) = project_of(path)?;
        Some(self.projects.entry(root.clone()).or_insert_with(|| Index::quick(&project, &root)))
    }

    /// Walks the open projects not walked yet. Too slow for editor requests:
    /// only activation and events (5 s) do it.
    fn index_projects(&mut self) {
        let deadline = Instant::now() + SCAN_BUDGET;
        for project in host::projects().unwrap_or_default() {
            let Ok(root) = project.root() else { continue };
            if !self.projects.get(&root).is_some_and(|index| index.complete) {
                let index = Index::full(&project, &root, deadline);
                self.projects.insert(root, index);
            }
        }
    }

    /// Declarations visible from `path`, with `unsaved` (the editor's text of
    /// `path`) in place of the file on disk.
    fn known(&mut self, path: &str, unsaved: Option<&str>) -> Vec<Declaration> {
        let mut known: Vec<Declaration> = self
            .index(path)
            .map(|index| index.declarations.iter().filter(|d| d.location.path != path).cloned().collect())
            .unwrap_or_default();
        if let Some(text) = unsaved {
            known.extend(env_declarations(path, text));
        }
        known
    }

    /// Where `key` is used, with the `${…}` of `unsaved` in place of `path` on disk.
    fn usages(&mut self, path: &str, unsaved: &str, key: &str) -> Vec<Location> {
        let mut usages: Vec<Usage> =
            self.index(path).map(|index| index.usages.iter().filter(|u| u.location.path != path).cloned().collect()).unwrap_or_default();
        usages.extend(nested_usages(path, unsaved));
        usages.into_iter().filter(|u| u.key == key).map(|u| u.location).collect()
    }

    /// Runs the inspections on `text` and shows their problems in the editor.
    fn inspect(&mut self, path: &str, text: &str) -> Vec<lint::Problem> {
        if text.len() > index::MAX_SOURCE_BYTES {
            return Vec::new();
        }
        let tokens = parse::lex(text);
        let properties = parse::properties(text, &tokens);
        let cached = host::project_of(path).and_then(|project| project.root().ok()).and_then(|root| self.projects.get(&root));
        // Never call known()/index() on a live diagnostic request: their cold
        // path reads files. Unknown cache state must not become proven absence.
        const MAX_CACHED_KEYS: usize = 8192;
        let complete = cached.is_some_and(|index| index.complete && index.declarations.len() <= MAX_CACHED_KEYS);
        let mut known: HashSet<String> = properties.iter().map(|p| p.name(text)).collect();
        if let Some(index) = cached {
            known.extend(index.declarations.iter().take(MAX_CACHED_KEYS).filter(|d| d.location.path != path).map(|d| d.key.clone()));
        }
        lint::check(text, &tokens, &properties, |key| !complete || known.contains(key))
    }


}

impl Plugin for DotEnv {
    fn activate(&mut self, _ctx: &Context) -> Result<(), Error> {
        let _ = host::subscribe(EventKind::FileOpened);
        let _ = host::subscribe(EventKind::FileSaved);
        self.index_projects();
        Ok(())
    }

    fn run_command(&mut self, id: &str, _ctx: &Invocation, _arg: Option<&str>) -> Result<Option<String>, Error> {
        Err(Error::NotFound(format!("command {id}")))
    }

    fn handle_events(&mut self, events: &[Event], _ctx: &Invocation) {
        self.index_projects();
        for event in events {
            let (Event::FileOpened(path) | Event::FileSaved(path)) = event else {
                continue;
            };
            if !Index::indexes(path) {
                continue;
            }
            let Some((root, project)) = project_of(path) else {
                continue;
            };
            let read = project.read_file(path);
            if let Some(index) = self.projects.get_mut(&root) {
                match read {
                    Ok(text) => {
                        index.update(path, Some(&text));
                        index.add_compose_env_files(&project, &root, path, &text);
                    }
                    Err(Error::NotFound(_)) => {
                        index.update(path, None);
                        index.complete = false;
                    }
                    Err(_) => index.complete = false,
                }
            }
        }
    }
}

impl SyntaxProvider for DotEnv {
    fn highlights(&mut self, _language: &str, text: &str) -> Vec<Token> {
        parse::lex(text)
            .into_iter()
            .filter_map(|t| {
                let kind = match t.kind {
                    parse::Kind::Key | parse::Kind::NestedKey => Highlight::Keyword,
                    parse::Kind::Value | parse::Kind::Quote => Highlight::String,
                    parse::Kind::Comment | parse::Kind::Export => Highlight::Comment,
                    parse::Kind::Separator => Highlight::Operator,
                    parse::Kind::NestedStart | parse::Kind::NestedEnd => Highlight::Punctuation,
                    parse::Kind::Space | parse::Kind::Bad => return None,
                };
                Some(Token { range: t.range, kind })
            })
            .collect()
    }
}

impl Providers for DotEnv {
    fn completion(&mut self, doc: &Document, at: Position, _trigger: Option<&str>) -> Vec<CompletionItem> {
        let in_env_file = doc.language == "dotenv";
        if !in_env_file && !host::setting_bool("completion").unwrap_or(true) {
            return Vec::new();
        }
        // `"$DB` in a `.env` value completes to `${DB_HOST}`: the editor has no
        // popup on `$` (the original's typed handler), but one on the letters after it.
        let (key, dollar) = match key_at(doc, at) {
            Some(key) => (key, false),
            None if in_env_file => match dollar_at(doc, at) {
                Some(key) => (key, true),
                None => return Vec::new(),
            },
            None => return Vec::new(),
        };

        let replace = range(&doc.text, key);
        let show_values = host::setting_bool("showValues").unwrap_or(true);
        let mut known = self.known(&doc.path, in_env_file.then_some(doc.text.as_str()));
        known.sort_by_key(|d| !d.primary);
        let mut seen = HashSet::new();

        known
            .into_iter()
            .filter(|d| seen.insert(d.key.clone()))
            .map(|d| {
                let mut item = match dollar {
                    true => {
                        let mut item = CompletionItem::new(&format!("${{{}}}", d.key), CompletionKind::Variable);
                        item.filter_text = Some(format!("${}", d.key));
                        item
                    }
                    false => CompletionItem::new(&d.key, CompletionKind::Variable),
                };
                if show_values && !d.value.is_empty() {
                    item.detail = Some(format!("= {}", d.value));
                }
                item.replace = Some(replace.clone());
                item
            })
            .collect()
    }

    /// From a usage to the declarations; from a declaration in a `.env` file
    /// to the usages (the original's Ctrl+Click on a key).
    fn definition(&mut self, doc: &Document, at: Position) -> Vec<Location> {
        if doc.language == "dotenv" {
            if let Some(key) = declared_key_at(doc, at) {
                return self.usages(&doc.path, &doc.text, &key);
            }
        }
        let Some(key) = key_at(doc, at) else {
            return Vec::new();
        };

        let key = &doc.text[key];
        let unsaved = (doc.language == "dotenv").then_some(doc.text.as_str());
        let matching: Vec<Declaration> = self.known(&doc.path, unsaved).into_iter().filter(|d| d.key == key).collect();
        let primary = matching.iter().any(|d| d.primary);

        matching.into_iter().filter(|d| d.primary == primary).map(|d| d.location).collect()
    }

    /// Quick fixes use the same findings as versioned diagnostics.
    // ponytail: with no language server for .env, an empty answer comes back as `unavailable` and the
    // editor stops asking for 15 s; the host should treat a plugin's answer as available (upstream fix).
    fn code_actions(&mut self, doc: &Document, selection: Range) -> Vec<CodeAction> {
        let actions: Vec<CodeAction> = self
            .inspect(&doc.path, &doc.text)
            .into_iter()
            .filter_map(|problem| {
                let fix = problem.fix?;
                let at = range(&doc.text, problem.range);
                let touches = at.start.line <= selection.end.line && at.end.line >= selection.start.line;
                touches.then(|| CodeAction {
                    title: fix.title.into(),
                    kind: Some("quickfix".into()),
                    preferred: true,
                    edits: vec![(doc.path.clone(), vec![TextEdit { range: range(&doc.text, fix.range), text: fix.text }])],
                    data: None,
                })
            })
            .collect();

        actions
    }
}

// API 1.3 registers the optional exports on the same component.
openidea_plugin::register_plugin!(DotEnv, providers, syntax, diagnostics, hover, file_actions, semantic_tokens);

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;

    fn doc(path: &str, language: &str, text: &str) -> Document {
        Document { path: path.into(), language: language.into(), version: 1, text: text.into() }
    }

    fn project() -> MockHost {
        let host = MockHost::install();
        host.set_project_root(Some("/p"));
        host.add_file(".env", "DB_HOST=localhost\nAPI_URL=\"http://x\"\n");
        host.add_file(".env.example", "DB_HOST=example\nONLY_EXAMPLE=1\n");
        host.add_file("app.py", "host = os.environ['DB_HOST']\n");
        host
    }

    fn walked() -> DotEnv {
        let mut plugin = DotEnv::default();
        plugin.index_projects();
        plugin
    }

    #[test]
    fn completes_keys_in_code_with_the_primary_values() {
        let _host = project();
        let php = doc("/p/index.php", "php", "<?php getenv('DB');");
        let items = DotEnv::default().completion(&php, Position::new(0, 16), None);
        let labels: Vec<_> = items.iter().map(|i| (i.label.as_str(), i.detail.as_deref())).collect();
        assert_eq!(labels, [("DB_HOST", Some("= localhost")), ("API_URL", Some("= http://x")), ("ONLY_EXAMPLE", Some("= 1"))]);
        assert_eq!(items[0].replace, Some(Range::new(Position::new(0, 14), Position::new(0, 16))));
        assert!(DotEnv::default().completion(&php, Position::new(0, 3), None).is_empty());
    }

    #[test]
    fn completes_a_dollar_in_a_quoted_value() {
        let _host = project();
        let env = doc("/p/.env", "dotenv", "URL=\"http://$DB\"\nRAW=$DB\n");
        let items = DotEnv::default().completion(&env, Position::new(0, 15), None);
        let db = items.iter().find(|i| i.label == "${DB_HOST}").unwrap();
        assert_eq!(db.filter_text.as_deref(), Some("$DB_HOST"));
        assert_eq!(db.replace, Some(Range::new(Position::new(0, 12), Position::new(0, 15))));
        assert!(DotEnv::default().completion(&env, Position::new(1, 7), None).is_empty(), "not outside quotes");
    }

    #[test]
    fn goes_to_the_dotenv_declaration_first() {
        let _host = project();
        let py = doc("/p/app.py", "python", "os.environ['DB_HOST']\nos.getenv('ONLY_EXAMPLE')");
        let targets = DotEnv::default().definition(&py, Position::new(0, 14));
        assert_eq!(targets, [Location { path: "/p/.env".into(), range: Range::new(Position::new(0, 0), Position::new(0, 7)) }]);
        let targets = DotEnv::default().definition(&py, Position::new(1, 12));
        assert_eq!(targets[0].path, "/p/.env.example");
    }

    #[test]
    fn goes_from_a_declaration_to_its_usages() {
        let _host = project();
        let env = doc("/p/.env", "dotenv", "DB_HOST=localhost\nURL=\"${DB_HOST}\"\n");
        let targets: Vec<_> = walked().definition(&env, Position::new(0, 3)).into_iter().map(|l| (l.path, l.range.start)).collect();
        assert_eq!(targets, [("/p/app.py".into(), Position::new(0, 19)), ("/p/.env".into(), Position::new(1, 7))]);
    }

    #[test]
    fn does_not_offer_unversioned_project_wide_rename_from_a_changed_line() {
        let _host=project();let env=doc("/p/.env","dotenv","DATABASE_HOST=localhost\nAPI_URL=\"${DB_HOST}\"\n");
        let actions=walked().code_actions(&env,Range::new(Position::new(0,5),Position::new(0,5)));
        assert!(actions.iter().all(|a| !a.title.starts_with("Rename")));
    }

    #[test]
    fn saved_files_update_the_index() {
        let host = project();
        let mut plugin = walked();
        host.add_file("worker.py", "os.getenv('API_URL')\n");
        plugin.handle_events(&[Event::FileSaved("/p/worker.py".into())], &host.invocation());
        let env = doc("/p/.env", "dotenv", "API_URL=x\n");
        assert_eq!(plugin.definition(&env, Position::new(0, 1))[0].path, "/p/worker.py");
    }

    #[test]
    fn publishes_problems_and_offers_their_fixes() {
        let _host = project();
        let env = doc("/p/.env", "dotenv", "A = 1\nB=\"${DB_HOST}${NOPE}\"\n");
        let actions = DotEnv::default().code_actions(&env, Range::new(Position::new(0, 0), Position::new(0, 0)));
        let titles: Vec<_> = actions.iter().map(|a| a.title.as_str()).collect();
        assert_eq!(titles, ["Remove spaces around '='"]);
        let problems = openidea_plugin::diagnostics::DiagnosticsProvider::diagnostics(&mut walked(), &env);
        let codes: Vec<_> = problems.iter().map(|d| d.code.as_deref().unwrap()).collect();
        assert_eq!(codes, ["DotEnvSpaceAroundSeparatorInspection", "UndefinedNestedVariableInspection"]);
    }

    #[test]
    fn diagnostics_are_cache_only_and_absence_requires_a_complete_owned_cache() {
        use openidea_plugin::diagnostics::DiagnosticsProvider;
        let host = project();
        let env = doc("/p/.env", "dotenv", "A = 1\nB=\"${DB_HOST}${NOPE}\"\n");
        let mut cold = DotEnv::default();
        let problems = cold.diagnostics(&env);
        assert_eq!(problems.len(), 1, "local syntax still checked, absence unknown");
        assert_eq!(problems[0].code.as_deref(), Some("DotEnvSpaceAroundSeparatorInspection"));
        assert!(cold.projects.is_empty(), "diagnostics must not lazily probe root files");
        let mut warm = walked();
        assert!(warm.diagnostics(&env).iter().any(|p| p.code.as_deref() == Some("UndefinedNestedVariableInspection")));
        warm.projects.get_mut("/p").unwrap().complete = false;
        assert!(warm.diagnostics(&env).iter().all(|p| p.code.as_deref() != Some("UndefinedNestedVariableInspection")));
        let _denied = host.with_capabilities(&[]);
        assert_eq!(cold.diagnostics(&env).len(), 1);
        assert!(cold.projects.is_empty());
    }

    #[test]
    fn diagnostics_overlay_unsaved_declarations_and_bound_documents() {
        use openidea_plugin::diagnostics::DiagnosticsProvider;
        let _host = project();
        let mut plugin = walked();
        let env = doc("/p/.env", "dotenv", "LOCAL=1\nB=\"${LOCAL}\"\n");
        assert!(plugin.diagnostics(&env).is_empty());
        let other = doc("/other/.env", "dotenv", "B=\"${NOPE}\"\n");
        assert!(plugin.diagnostics(&other).is_empty(), "foreign project cannot prove absence");
        let huge = doc("/p/.env", "dotenv", &"#".repeat(index::MAX_SOURCE_BYTES + 1));
        assert!(plugin.diagnostics(&huge).is_empty());
    }

    #[test]
    fn highlights_keys_values_and_comments() {
        let kinds: Vec<_> = DotEnv::default().highlights("dotenv", "A=1 # c").into_iter().map(|t| t.kind).collect();
        assert_eq!(kinds, [Highlight::Keyword, Highlight::Operator, Highlight::String, Highlight::Comment]);
    }
}

impl openidea_plugin::diagnostics::DiagnosticsProvider for DotEnv {
    fn diagnostics(&mut self, doc: &Document) -> Vec<Diagnostic> {
        if !is_env_file(&doc.path) || doc.text.len() > index::MAX_SOURCE_BYTES {
            return Vec::new();
        }
        self.inspect(&doc.path, &doc.text)
            .into_iter()
            .map(|p| {
                let severity = match p.level {
                    lint::Level::Error => Severity::Error,
                    lint::Level::Warning => Severity::Warning,
                };
                Diagnostic::new(range(&doc.text, p.range), severity, p.message).code(p.code)
            })
            .collect()
    }
}
