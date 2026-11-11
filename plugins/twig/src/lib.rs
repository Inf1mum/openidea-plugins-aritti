//! Twig for OpenIDEA: highlighting, structure view (blocks, macros) and
//! folding (tag pairs, comments) from a hand-written lexer, exported as the
//! plugin's `syntax-provider`. Detection comes from `languages/twig.toml`.

mod checks;
mod reviewed_symbols;
mod template_index;
mod signatures;
mod formatting;
mod comments;
mod delimiters;
mod injections;
mod indexing;
mod complete;
mod navigate;
mod macro_arguments;
mod hover;

use std::collections::BTreeMap;
use std::ops::Range;

use std::path::Path;

use navigate::{Extension, ExtensionKind};
use openidea_plugin::providers::{CompletionItem, CompletionKind, Document, Providers};
use openidea_plugin::syntax::{Fold, FoldKind, Highlight, Symbol, SymbolKind, SyntaxProvider, Token};
use openidea_plugin::text::{offset, position};
use openidea_plugin::{
    Context, Diagnostic, Error, Event, EventKind, Invocation, Location, LogLevel, Plugin, Project, Range as EditorRange, Severity, host,
};

const SOURCE: &str = "Twig";
const MAX_SYNTAX_BYTES: usize = 256 << 10;
const MAX_SYNTAX_TOKENS: usize = 16_384;
const MAX_STRUCTURE_TAGS: usize = 4096;
const MAX_STRUCTURE_ITEMS: usize = 2048;

const EXTENSION_GLOBS: &[&str] = &["src/**/*.php", "app/**/*.php", "lib/**/*.php"];
const EXTENSION_FILES: u32 = 1500;

#[derive(Default)]
pub struct Twig {
    reviewed_symbols:reviewed_symbols::State,
    /// Filters, functions and tests registered in the project's PHP, by absolute file path.
    extensions: BTreeMap<String, Vec<Extension>>,
}

impl Twig {
    fn scan_extensions(&mut self) {
        if !host::setting_bool("projectExtensions").unwrap_or(true) {return;}
        let deadline=std::time::Instant::now()+std::time::Duration::from_secs(3);
        let (mut count,mut bytes)=(0u32,0usize);
        for project in host::projects().unwrap_or_default() {
            for glob in EXTENSION_GLOBS {
                if count>=EXTENSION_FILES||bytes+(128<<10)>(12<<20)||std::time::Instant::now()>=deadline{return;}
                for path in project.find_files(glob,EXTENSION_FILES-count).unwrap_or_default(){
                    if count>=EXTENSION_FILES||bytes+(128<<10)>(12<<20)||std::time::Instant::now()>=deadline{return;}
                    count+=1;bytes+=self.scan_file(&project,&path);
                }
            }
        }
    }
    fn scan_file(&mut self,project:&Project,path:&str)->usize {
        let root=project.root().unwrap_or_default();
        let source=if host::setting_bool("projectExtensions").unwrap_or(true)&&same_owner(&root,path)&&extension_source(&root,path)
            &&project.stat(path).is_ok_and(|s|s.kind==openidea_plugin::FileKind::File&&s.size<=128<<10){
            project.read_file(path).ok().filter(|s|s.len()<=128<<10)
        }else{None};
        let bytes=source.as_ref().map_or(0,String::len);
        let found=source.map(|php|navigate::extensions(&php)).unwrap_or_default();
        if found.is_empty(){self.extensions.remove(path);}else{self.extensions.insert(path.to_owned(),found);}
        bytes
    }

    fn extra_names(&self, root: &str, kind: ExtensionKind) -> Vec<&str> {
        if root.is_empty()||!host::setting_bool("projectExtensions").unwrap_or(true){return Vec::new();}
        let roots:Vec<_>=host::projects().unwrap_or_default().into_iter().filter_map(|p|p.root().ok()).collect();
        let inside=|(path,_):&(&String,&Vec<Extension>)|extension_source(root,path)&&roots.iter().filter(|r|Path::new(path.as_str()).starts_with(r)).max_by_key(|r|r.len()).is_some_and(|r|r==root);
        let mut names: Vec<&str> = self
            .extensions
            .iter()
            .filter(inside)
            .flat_map(|(_, found)| found)
            .filter(|e| e.kind == kind)
            .map(|e| e.name.as_str())
            .collect();
        names.sort_unstable();
        names.dedup();
        names
    }
}

fn canonical(root:&str,path:&str)->bool {
    root.starts_with('/')&&root!="/"&&!root.ends_with('/')&&root.len()<=4096&&path.len()<=4096
        &&!root.chars().chain(path.chars()).any(char::is_control)&&!root.contains('\\')&&!path.contains('\\')
        &&!root.contains("//")&&!path.contains("//")&&!root.split('/').chain(path.split('/')).any(|p|matches!(p,"."|".."))
        &&path.starts_with(&format!("{root}/"))
}
fn extension_source(root:&str,path:&str)->bool {
    canonical(root,path)&&path.strip_prefix(&format!("{root}/")).is_some_and(|p|p.ends_with(".php")&&["src/","app/","lib/"].iter().any(|prefix|p.starts_with(prefix)))
}
fn same_owner(root:&str,path:&str)->bool {canonical(root,path)&&project_of(path).and_then(|p|p.root().ok()).as_deref()==Some(root)}

fn template_path(project: &Project, name: &str) -> Option<String> {
    if name.is_empty()
        || name.starts_with('/')
        || name.starts_with('@')
        || name.contains(['\\', ':', '*', '?', '[', ']'])
        || name.chars().any(char::is_control)
        || name.split('/').any(|p| p.is_empty() || matches!(p, "." | ".."))
    {
        return None;
    }
    let root = project.root().ok()?;
    let direct=format!("templates/{name}");let full=format!("{root}/{direct}");
    if !same_owner(&root,&full)||project.stat(&direct).ok()?.kind!=openidea_plugin::FileKind::File{return None;}
    Some(full)
}

fn macros_of(project: &Project, current: &Document, template: &str) -> Option<(String, String)> {
    if template == "_self" {
        return Some((current.path.clone(), current.text.clone()));
    }

    let path = template_path(project, template)?;
    // A literal import of this same template must use the supplied current
    // buffer, just like _self, rather than saved offsets/names.
    if path == current.path { return Some((path, current.text.clone())); }
    if project.stat(&path).ok()?.size > 128 << 10 { return None; }
    let text = project.read_file(&path).ok()?;
    if text.len() > 128 << 10 {
        return None;
    }
    Some((path, text))
}

fn resolved_macro(doc:&Document,call:usize,receiver:Option<&str>,name:&str)->Option<(navigate::Macro,String)> {
    let project=project_of(&doc.path);
    let (template,name)=if receiver==Some("_self") {Some(("_self".to_owned(),name.to_owned()))}
        else if !project.as_ref().is_some_and(is_symfony) {navigate::binding(&doc.text,call,receiver,name)} else {None}?;
    let (path,text)=if template=="_self" {(doc.path.clone(),doc.text.clone())}else{macros_of(project.as_ref()?,doc,&template)?};
    if path!=doc.path && !checks::check(&text).is_empty(){return None;}
    let mut found=navigate::macros(&text).into_iter().filter(|m|m.name==name);let result=found.next()?;if found.next().is_some(){return None;}
    Some((result,path))
}

fn in_twig_code(text: &str, offset: usize) -> bool {
    navigate::code_at(text, offset, false)
}

fn macro_items(project: &Project, doc: &Document, offset: usize) -> Vec<CompletionItem> {
    let (receiver, range) = match navigate::dotted_at(&doc.text, offset) {
        Some((receiver, _, range)) => (receiver, range),
        None => (None, offset..offset),
    };
    let replace = EditorRange::new(position(&doc.text, range.start), position(&doc.text, range.end));

    let template = match receiver {
        Some(alias) if alias == "_self" && in_twig_code(&doc.text, range.start) => Some("_self".into()),
        Some(alias) if in_twig_code(&doc.text, range.start) => {
            navigate::binding(&doc.text, range.start, Some(&alias), "").map(|(template, _)| template)
        }
        Some(_) => None,
        None if navigate::code_at(&doc.text, range.start, false) => from_import_template(&doc.text, range.start),
        None => None,
    };
    let Some(template) = template else {
        return Vec::new();
    };
    let Some((source_path, text)) = macros_of(project, doc, &template) else {
        return Vec::new();
    };

    // External broken source is not a trustworthy owner. The current buffer
    // may contain the unfinished call being completed, so retain local recovery.
    if source_path != doc.path && !checks::check(&text).is_empty() { return Vec::new(); }
    let macros = navigate::macros(&text);
    let mut counts = BTreeMap::new();
    for declaration in &macros { *counts.entry(declaration.name.as_str()).or_insert(0usize) += 1; }
    macros.iter()
        .filter(|m| counts.get(m.name.as_str()) == Some(&1))
        .map(|m| {
            let mut item = CompletionItem::new(&m.name, CompletionKind::Function);
            item.detail = Some(format!("macro({})", m.arguments));
            item.replace = Some(replace);
            item
        })
        .collect()
}

/// The template of a `{% from 'x' import …` tag the caret is inside the name list of.
fn from_import_template(text: &str, offset: usize) -> Option<String> {
    let syntax=delimiters::Syntax::current();
    let tag_start = text[..offset].rfind(&syntax.values[0])?;
    if text[tag_start..offset].contains(&syntax.values[1]) {
        return None;
    }

    let body = text[tag_start + syntax.values[0].len()..offset].trim_start_matches(['-', '~']).trim_start();
    let rest = body.strip_prefix("from")?.trim_start();
    let quote = rest.chars().next().filter(|c| matches!(c, '\'' | '"'))?;
    let inner = &rest[1..];
    let end = inner.find(quote)?;
    let after = inner[end + 1..].trim_start();

    after.starts_with("import").then(|| inner[..end].to_owned())
}

impl Plugin for Twig {
    fn activate(&mut self, _ctx: &Context) -> Result<(), Error> {
        for kind in [EventKind::FileOpened, EventKind::FileSaved] {
            if let Err(error) = host::subscribe(kind) {
                host::log(LogLevel::Warn, &format!("Twig inspections need the {} event: {error}", kind.name()));
            }
        }

        if let Err(error)=host::subscribe_paths("files-changed", EXTENSION_GLOBS) {
            host::log(LogLevel::Warn,&format!("Twig extension cache notifications unavailable: {error}"));
        }
        self.scan_extensions();

        Ok(())
    }

    fn handle_events(&mut self, events: &[Event], _ctx: &Invocation) {
        let started=std::time::Instant::now();
        let mut count=0usize;
        if events.len()>256 {self.extensions.clear();return;}
        for event in events {
            if started.elapsed()>std::time::Duration::from_millis(100) {self.extensions.clear();return;}
            match event {
                Event::FileSaved(path) if path.ends_with(".php") => {
                    if let Some(project)=project_of(path) { self.scan_file(&project,path); }
                    else {self.extensions.remove(path);}
                }
                Event::FilesChanged(batch) => {
                    // No discovery or source reads in filesystem notifications.
                    // Saved-file notification or activation can establish new facts.
                    for path in batch.created.iter().chain(&batch.modified).chain(&batch.removed).take(257) {
                        count+=1;
                        if count>256 {self.extensions.clear();return;}
                        self.extensions.remove(path);
                    }
                }
                _ => {}
            }
        }
    }

    fn run_command(&mut self, id: &str, ctx: &Invocation, _arg: Option<&str>) -> Result<Option<String>, Error> {
        if id=="source-symbol-occurrences" {return if let Some(arg)=_arg{reviewed_symbols::occurrences(&mut self.reviewed_symbols,ctx,arg)}else{reviewed_symbols::occurrences_at_caret(&mut self.reviewed_symbols,ctx)};}
        if id=="toggle-comment" {return comments::toggle(ctx);}
        Err(Error::NotFound(format!("command {id}")))
    }
}

impl SyntaxProvider for Twig {
    fn highlights(&mut self, _language: &str, text: &str) -> Vec<Token> {
        let Some(_syntax)=delimiters::Syntax::scope() else {return Vec::new();};
        if text.len() > MAX_SYNTAX_BYTES {
            return Vec::new();
        }
        let mut tokens = lex(text).tokens;
        tokens.truncate(MAX_SYNTAX_TOKENS);
        tokens
    }

    fn outline(&mut self, _language: &str, text: &str) -> Vec<Symbol> {
        let Some(_syntax)=delimiters::Syntax::scope() else {return Vec::new();};
        structure(text).0
    }

    fn folding(&mut self, _language: &str, text: &str) -> Vec<Fold> {
        let Some(_syntax)=delimiters::Syntax::scope() else {return Vec::new();};
        structure(text).1
    }
}

fn macro_locations(path: &str, text: &str, name: &str) -> Vec<Location> {
    if text.len() > 128 << 10 || !checks::check(text).is_empty() {
        return Vec::new();
    }
    let found: Vec<_> = navigate::macros(text).into_iter().filter(|m| m.name == name).collect();
    if found.len() != 1 {
        return Vec::new();
    }
    let m = &found[0];
    vec![Location { path: path.into(), range: EditorRange::new(position(text, m.name_range.start), position(text, m.name_range.end)) }]
}

impl Providers for Twig {
    fn completion(&mut self, doc: &Document, at: openidea_plugin::Position, _trigger: Option<&str>) -> Vec<CompletionItem> {
        let Some(_syntax)=delimiters::Syntax::scope() else {return Vec::new();};
        if doc.language != "twig" || !doc.path.ends_with(".twig") || doc.text.len() > 128 << 10 {
            return Vec::new();
        }
        let Some(offset) = offset(&doc.text, at) else {
            return Vec::new();
        };
        let project = project_of(&doc.path);

        let symfony = project.as_ref().is_some_and(is_symfony);
        if let Some(slot) = macro_arguments::slot(&doc.text, offset).filter(|_|macro_arguments::supported(project.as_ref())) {
            let binding = if slot.receiver.as_deref() == Some("_self") {
                Some(("_self".to_owned(),slot.name.clone()))
            } else if !symfony {
                navigate::binding(&doc.text,slot.call,slot.receiver.as_deref(),&slot.name)
            } else {None};
            if let Some((template,name)) = binding {
                let source = if template=="_self" {Some(doc.text.clone())} else {
                    project.as_ref().and_then(|p|macros_of(p,doc,&template)).map(|(_,text)|text)
                };
                if let Some(source) = source {
                    let found:Vec<_>=navigate::macros(&source).into_iter().filter(|m|m.name==name).collect();
                    if found.len()==1 {return macro_arguments::items(&doc.text,&slot,&found[0].arguments);}
                }
            }
        }
        let macros = if symfony { Vec::new() } else { project.as_ref().map(|p| macro_items(p, doc, offset)).unwrap_or_default() };
        if !macros.is_empty() {
            return macros;
        }

        let names = !symfony;
        let root = project.as_ref().and_then(|p| p.root().ok()).unwrap_or_default();
        let extra = complete::Extra {
            filters: self.extra_names(&root, ExtensionKind::Filter),
            functions: self.extra_names(&root, ExtensionKind::Function),
            tests: self.extra_names(&root, ExtensionKind::Test),
        };

        complete::items_with(&doc.text, offset, names, &extra)
    }

    fn definition(&mut self, doc: &Document, at: openidea_plugin::Position) -> Vec<Location> {
        let Some(_syntax)=delimiters::Syntax::scope() else {return Vec::new();};
        if doc.language != "twig" || !doc.path.ends_with(".twig") || doc.text.len() > 128 << 10 {
            return Vec::new();
        }
        let Some(offset) = offset(&doc.text, at) else {
            return Vec::new();
        };
        if let Some(target)=signatures::definition(doc,at) {return vec![target];}
        let symbols=reviewed_symbols::definition(doc,at);if !symbols.is_empty(){return symbols;}
        if let Some((Some(receiver), name, _)) = navigate::dotted_at(&doc.text, offset) {
            if receiver == "_self" && navigate::code_at(&doc.text, offset, false) {
                return macro_locations(&doc.path, &doc.text, &name);
            }
        }
        let Some(project) = project_of(&doc.path) else {
            return Vec::new();
        };
        let range_in = |text: &str, range: Range<usize>| EditorRange::new(position(text, range.start), position(text, range.end));

        if let Some((name, _)) = navigate::template_at(&doc.text, offset) {
            if is_symfony(&project) {
                return Vec::new();
            }

            return template_path(&project, &name)
                .map(|path| vec![Location { path, range: EditorRange::new(position("", 0), position("", 0)) }])
                .unwrap_or_default();
        }

        let Some((receiver, word, word_range)) = navigate::dotted_at(&doc.text, offset) else {
            return Vec::new();
        };
        if !navigate::code_at(&doc.text, offset, false) {
            return Vec::new();
        }
        let template = if receiver.as_deref() == Some("_self") {
            Some(("_self".into(), word.clone()))
        } else {
            navigate::binding(&doc.text, offset, receiver.as_deref(), &word)
        };

        if let Some((template, name)) = template {
            if template != "_self" && is_symfony(&project) {
                return Vec::new();
            }
            let Some((path, text)) = macros_of(&project, doc, &template) else {
                return Vec::new();
            };
            return macro_locations(&path, &text, &name);
        }

        if receiver.is_some() || is_symfony(&project) || !navigate::code_at(&doc.text, offset, false) {
            return Vec::new();
        }

        if navigate::imports(&doc.text)
            .iter()
            .any(|import| matches!(import,navigate::Import::Names{names,..} if names.iter().any(|(_,used)|used==&word)))
        {
            return Vec::new();
        }
        let before = doc.text[..offset].trim_end_matches(|c: char| c.is_ascii_alphanumeric() || c == '_');
        let kind = if before.ends_with('|') {
            ExtensionKind::Filter
        } else if before.trim_end().ends_with(" is") || before.trim_end().ends_with(" not") {
            ExtensionKind::Test
        } else {
            ExtensionKind::Function
        };
        if kind == ExtensionKind::Function && !doc.text[word_range.end..].trim_start().starts_with('(') {
            return Vec::new();
        }
        let root = project.root().unwrap_or_default();
        if root.is_empty() {
            return Vec::new();
        }

        if !host::setting_bool("projectExtensions").unwrap_or(true){return Vec::new();}
        self.extensions.iter().filter(|(path,_)|Path::new(path.as_str()).starts_with(&root))
            .flat_map(|(path,found)|found.iter().map(move|e|(path,e)))
            .filter(|(_,e)|e.kind==kind&&e.name==word).take(16)
            .filter_map(|(path,e)|{
                if !same_owner(&root,path)||!extension_source(&root,path)||!project.stat(path).ok().is_some_and(|s|s.kind==openidea_plugin::FileKind::File&&s.size<=128<<10){return None;}
                let php=project.read_file(path).ok().filter(|s|s.len()<=128<<10)?;
                if !navigate::extensions(&php).iter().any(|fresh| fresh.name == e.name && fresh.kind == e.kind && fresh.range == e.range) {
                    return None;
                }
                Some(Location { path: path.clone(), range: range_in(&php, e.range.clone()) })
            })
            .collect()
    }
}

fn project_of(path: &str) -> Option<Project> {
    host::projects()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|p| Some((p.root().ok()?, p)))
        .filter(|(root, _)| Path::new(path).starts_with(root))
        .max_by_key(|(root, _)| root.len())
        .map(|(_, project)| project)
}

/// The test `aritti.symfony` uses to switch on. Inside such a project that
/// plugin owns tag, filter, function and test completion, so this one stays
/// silent there instead of listing everything twice.
fn is_symfony(project: &Project) -> bool {
    project.read_file("symfony.lock").is_ok()
        || project
            .read_file("composer.json")
            .is_ok_and(|composer| composer.contains("\"symfony/framework-bundle\"") || composer.contains("\"symfony/symfony\""))
}

impl openidea_plugin::diagnostics::DiagnosticsProvider for Twig {
    fn diagnostics(&mut self, doc: &openidea_plugin::providers::Document) -> Vec<Diagnostic> {
        let Some(_syntax)=delimiters::Syntax::scope() else {return vec![Diagnostic::new(EditorRange::new(openidea_plugin::Position::new(0,0),openidea_plugin::Position::new(0,0)),Severity::Error,"Invalid Twig delimiter settings: use 1–16 byte non-whitespace delimiters and distinct opening delimiters without shared prefixes.").code("invalid-delimiter-settings")];};
        if doc.language!="twig" || !doc.path.ends_with(".twig") || doc.text.len()>128<<10 || !host::setting_bool("diagnostics").unwrap_or(true) {return Vec::new();}
        let text=doc.text.as_str();
        let parsed=lex(text);if parsed.tags.len()>MAX_STRUCTURE_TAGS || parsed.tokens.len()>MAX_SYNTAX_TOKENS{return Vec::new();}

        let enabled = host::setting_bool("diagnostics").unwrap_or(true);
        let mut findings = checks::check(text);
        if host::setting_bool("expressionErrors").unwrap_or(false) {
            findings.extend(checks::brackets(text));
        }

        let diagnostics: Vec<Diagnostic> = findings
            .into_iter()
            .filter(|_| enabled)
            .take(500)
            .map(|f| Diagnostic {
                code: Some(f.code.into()),
                source: Some(SOURCE.into()),
                ..Diagnostic::new(EditorRange::new(position(text, f.range.start), position(text, f.range.end)), Severity::Error, f.message)
            })
            .collect();

        diagnostics
    }
}

// API 1.3 registers the optional exports on the same component.
openidea_plugin::register_plugin!(Twig, injections, providers, syntax, diagnostics, hover, indexing, language_features, file_actions);

const KEYWORDS: &[&str] =
    &["and", "or", "not", "in", "is", "matches", "starts", "ends", "with", "as", "only", "ignore", "missing", "if", "else", "for"];
const CONSTANTS: &[&str] = &["true", "false", "null", "none", "TRUE", "FALSE", "NULL", "NONE"];

/// A `{% name … %}` tag: its name, the identifier after it (a block's or
/// macro's name), and where it is.
#[derive(Debug, PartialEq, Eq)]
struct Tag {
    name: String,
    argument: Option<Range<usize>>,
    range: Range<usize>,
    /// Has an `{% endname %}`: not `{% block title 'Short' %}` or `{% set x = 1 %}`.
    paired: bool,
}

#[derive(Default)]
struct Lexed {
    tokens: Vec<Token>,
    tags: Vec<Tag>,
    comments: Vec<Range<usize>>,
    boundaries: Vec<(Range<usize>,bool)>,
    /// Openers (`{{`, `{%`, `{#`) whose closer is missing.
    unterminated: Vec<Range<usize>>,
}

impl Lexed {
    fn push(&mut self, range: Range<usize>, kind: Highlight) {
        if range.start < range.end {
            self.tokens.push(Token { range, kind });
        }
    }
}

struct Lexer<'a> {
    text: &'a str,
    bytes: &'a [u8],
    out: Lexed,
    syntax: delimiters::Syntax,
}

fn is_identifier_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_'
}

fn is_identifier(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

fn lex(text: &str) -> Lexed {
    let mut lexer = Lexer { text, bytes: text.as_bytes(), out: Lexed::default(),syntax:delimiters::Syntax::current() };
    lexer.html(0);
    lexer.out
}

impl Lexer<'_> {
    fn at(&self, i: usize, s: &str) -> bool {
        self.bytes[i..].starts_with(self.syntax.expand(s).as_bytes())
    }

    fn find(&self, from: usize, s: &str) -> Option<usize> {
        self.text[from..].find(self.syntax.expand(s)).map(|i| from + i)
    }

    fn html(&mut self, mut i: usize) {
        while i < self.bytes.len() {
            if self.at(i, "{#") || self.at(i, "{{") || self.at(i, "{%") {
                i = self.twig(i);
                continue;
            }

            if self.at(i, "<!--") {
                let end = self.find(i + 4, "-->").map_or(self.bytes.len(), |e| e + 3);
                self.out.push(i..end, Highlight::Comment);
                i = end;
                continue;
            }

            let opens_element =
                self.bytes[i] == b'<' && self.bytes.get(i + 1).is_some_and(|b| is_identifier_start(*b) || *b == b'/' || *b == b'!');
            if opens_element {
                i = self.element(i);
                continue;
            }

            i += 1;
        }
    }

    /// `<name attr="value" …>`, with Twig anywhere in it.
    fn element(&mut self, start: usize) -> usize {
        let mut i = start + 1;
        while i < self.bytes.len() && (is_identifier(self.bytes[i]) || matches!(self.bytes[i], b'/' | b'!' | b'-' | b':')) {
            i += 1;
        }
        self.out.push(start..i, Highlight::Tag);

        while i < self.bytes.len() {
            let b = self.bytes[i];

            if self.at(i, "{#") || self.at(i, "{{") || self.at(i, "{%") {
                i = self.twig(i);
            } else if b == b'>' {
                self.out.push(i..i + 1, Highlight::Tag);
                return i + 1;
            } else if self.at(i, "/>") {
                self.out.push(i..i + 2, Highlight::Tag);
                return i + 2;
            } else if b == b'"' || b == b'\'' {
                i = self.attribute_value(i);
            } else if b.is_ascii_whitespace() || b == b'=' {
                i += 1;
            } else if b == b'<' {
                return i;
            } else {
                let name_start = i;
                while i < self.bytes.len()
                    && !["{{","{%","{#"].iter().any(|s|self.at(i,s))
                    && !matches!(self.bytes[i], b' ' | b'\t' | b'\n' | b'\r' | b'=' | b'>' | b'/' | b'"' | b'\'' | b'{' | b'<')
                {
                    i += 1;
                }
                i = i.max(name_start + 1);
                self.out.push(name_start..i, Highlight::Attribute);
            }
        }

        i
    }

    fn attribute_value(&mut self, start: usize) -> usize {
        let quote = self.bytes[start];
        let mut segment = start;
        let mut i = start + 1;

        while i < self.bytes.len() {
            if self.bytes[i] == quote {
                self.out.push(segment..i + 1, Highlight::String);
                return i + 1;
            }

            if self.at(i, "{{") || self.at(i, "{%") || self.at(i, "{#") {
                self.out.push(segment..i, Highlight::String);
                i = self.twig(i);
                segment = i;
                continue;
            }

            i += 1;
        }

        self.out.push(segment..i, Highlight::String);
        i
    }

    /// A Twig comment, print or tag starting at `start`; returns its end.
    fn twig(&mut self, start: usize) -> usize {
        if self.at(start, "{#") {
            let closer = self.find(start + self.syntax.values[4].len(), "#}");
            if closer.is_none() {
                self.out.unterminated.push(start..start + self.syntax.values[4].len());
            }
            let end = closer.map_or(self.bytes.len(), |e| e + self.syntax.values[5].len());
            self.out.push(start..end, Highlight::Comment);
            self.out.comments.push(start..end);
            return end;
        }

        let is_tag = self.at(start, "{%");
        let close = if is_tag { "%}" } else { "}}" };
        let open_len=self.syntax.expand(if is_tag {"{%"} else {"{{"}).len();
        let mut i = start + open_len;
        if self.bytes.get(i).is_some_and(|b| *b == b'-' || *b == b'~') {
            i += 1;
        }
        self.out.push(start..i, Highlight::Punctuation);
        self.out.boundaries.push((start..i,true));

        let mut tag_name: Option<String> = None;
        let mut argument = None;
        let mut token_count = 0;
        let mut assigns = false;
        let mut previous = 0u8;
        let mut closed = false;
        let mut groups:Vec<(u8,bool)>=Vec::new();
        let mut groups_known=true;

        while i < self.bytes.len() {
            let b = self.bytes[i];

            let trimmed_close = (b == b'-' || b == b'~') && self.at(i + 1, close);
            if groups.is_empty() && (self.at(i, close) || trimmed_close) {
                let end = i + self.syntax.expand(close).len() + usize::from(trimmed_close);
                self.out.push(i..end, Highlight::Punctuation);
                self.out.boundaries.push((i..end,false));
                i = end;
                closed = true;
                break;
            }

            if b.is_ascii_whitespace() {
                i += 1;
                continue;
            }

            let token_start = i;
            token_count += 1;
            if b == b'"' || b == b'\'' {
                i += 1;
                while i < self.bytes.len() && self.bytes[i] != b {
                    i += if self.bytes[i] == b'\\' { 2 } else { 1 };
                }
                let closed_string = self.bytes.get(i) == Some(&b);
                i = (i + 1).min(self.bytes.len());
                self.out.push(token_start..i, Highlight::String);
                if closed_string
                    && tag_name.as_deref() == Some("embed")
                    && argument.is_none()
                    && self.bytes.get(i.saturating_sub(1)) == Some(&b)
                    && i > token_start + 1
                {
                    argument = Some(token_start + 1..i - 1);
                }
            } else if b.is_ascii_digit() {
                i = self.number_end(i);
                self.out.push(token_start..i, Highlight::Number);
            } else if is_identifier_start(b) || b >= 0x80 {
                while i < self.bytes.len() && (is_identifier(self.bytes[i]) || self.bytes[i] >= 0x80) {
                    i += 1;
                }
                let word = &self.text[token_start..i];
                let next = self.text[i..].trim_start().bytes().next();
                let is_tag_name = is_tag && tag_name.is_none();

                let declaration_name = is_tag && !is_tag_name && argument.is_none();
                if declaration_name {
                    argument = Some(token_start..i);
                }

                let last=self.out.tokens.last().map(|t|(&self.text[t.range.clone()],t.kind==Highlight::Keyword));
                let prior=self.out.tokens.iter().rev().nth(1).map(|t|(&self.text[t.range.clone()],t.kind==Highlight::Keyword));
                let test_position=last==Some(("is",true)) || last==Some(("not",true))&&prior==Some(("is",true));
                let test_name=test_position&&["constant","defined","divisible","empty","even","iterable","none","null","odd","same"].contains(&word);
                let test_by=word=="by"&&self.out.tokens.last().is_some_and(|t|t.kind==Highlight::Function&&&self.text[t.range.clone()]=="divisible");
                let tail=self.text[i..].trim_start();
                let named_argument=groups_known&&groups.last().is_some_and(|(open,call)|*open==b'('&&*call)
                    &&last.is_some_and(|(raw,_)|matches!(raw,"("|","))
                    &&(tail.starts_with(':')&&!tail.starts_with("::")||tail.starts_with('=')&&!tail.starts_with("==")&&!tail.starts_with("=>"));
                let kind = if is_tag_name {
                    tag_name = Some(word.to_owned());
                    Highlight::Keyword
                } else if declaration_name && matches!(tag_name.as_deref(),Some("macro"|"apply")) {
                    Highlight::Function
                } else if declaration_name && matches!(tag_name.as_deref(), Some("block" | "endblock")) {
                    Highlight::Property
                } else if previous == b'|' {
                    Highlight::Function
                } else if previous == b'.' {
                    if next == Some(b'(') { Highlight::FunctionCall } else { Highlight::Property }
                } else if test_name {
                    Highlight::Function
                } else if test_by {
                    Highlight::Keyword
                } else if named_argument {
                    Highlight::Property
                } else if KEYWORDS.contains(&word) {
                    Highlight::Keyword
                } else if CONSTANTS.contains(&word) {
                    Highlight::Constant
                } else if next == Some(b'(') {
                    Highlight::FunctionCall
                } else if word == "_self" || word == "_context" || word == "loop" {
                    Highlight::SelfVariable
                } else {
                    Highlight::Variable
                };

                self.out.push(token_start..i, kind);
            } else {
                i += if b == b'.' && self.bytes.get(i + 1) == Some(&b'.') { 2 } else { 1 };
                assigns |= b == b'=' && !matches!(self.bytes.get(i), Some(b'=')) && !matches!(previous, b'=' | b'!' | b'<' | b'>');
                match b {
                    b'('|b'['|b'{'=>{
                        let compound_test=self.out.tokens.last().is_some_and(|t|t.kind==Highlight::Keyword&&matches!(&self.text[t.range.clone()],"as"|"by"))
                            &&self.out.tokens.iter().rev().nth(1).is_some_and(|t|t.kind==Highlight::Function&&matches!(&self.text[t.range.clone()],"same"|"divisible"));
                        let callable=b==b'(' && !(is_tag&&tag_name.as_deref()==Some("macro"))
                            &&(compound_test||self.out.tokens.last().is_some_and(|t|matches!(t.kind,Highlight::Function|Highlight::FunctionCall)));
                        if groups.len()<128 {groups.push((b,callable));}else{groups_known=false;}
                    },
                    b')'|b']'|b'}'=>{let expected=match b{b')'=>b'(',b']'=>b'[',_=>b'{'};if groups.pop().is_none_or(|(open,_)|open!=expected){groups_known=false;}},
                    _=>{}
                }
                let kind = if b"()[]{},".contains(&b) { Highlight::Punctuation } else { Highlight::Operator };
                self.out.push(token_start..i, kind);
            }

            previous = if b == b'.' && i - token_start == 2 { 0 } else { b };
        }

        if !closed {
            self.out.unterminated.push(start..start + open_len);
        }

        if let Some(name) = tag_name {
            let raw = name == "verbatim" || name == "raw";
            let paired = match name.as_str() {
                "block" => token_count <= 2,
                "set" => !assigns,
                _ => true,
            };
            self.out.tags.push(Tag { name: name.clone(), argument, range: start..i, paired });

            if raw {
                return self.find_tag(i, &format!("end{name}")).unwrap_or(self.bytes.len());
            }
        }

        i
    }

    /// Twig 3 numeric grammar: grouped digits, optional decimal/exponent.
    /// A range's two dots are operators, never part of the preceding number.
    fn number_end(&self, from: usize) -> usize {
        let digits = |mut at: usize| {
            while at < self.bytes.len() {
                if self.bytes[at].is_ascii_digit() {
                    at += 1;
                } else if self.bytes[at] == b'_' && self.bytes.get(at + 1).is_some_and(u8::is_ascii_digit) {
                    at += 2;
                } else {
                    break;
                }
            }
            at
        };
        let mut end = digits(from);
        if self.bytes.get(end) == Some(&b'.') && self.bytes.get(end + 1).is_some_and(u8::is_ascii_digit) {
            end = digits(end + 1);
        }
        if matches!(self.bytes.get(end), Some(b'e' | b'E')) {
            let mut exponent = end + 1;
            if matches!(self.bytes.get(exponent), Some(b'+' | b'-')) {
                exponent += 1;
            }
            if self.bytes.get(exponent).is_some_and(u8::is_ascii_digit) {
                end = digits(exponent);
            }
        }
        end
    }

    /// Start of the next `{% name` tag after `from`.
    fn find_tag(&self, from: usize, name: &str) -> Option<usize> {
        let mut at = from;
        while let Some(open) = self.find(at, "{%") {
            let inside = self.text[open + self.syntax.values[0].len()..].trim_start_matches(['-', '~']).trim_start();
            if let Some(rest) = inside.strip_prefix(name) {
                let rest = rest.trim_start();
                if rest.trim_start_matches(['-','~']).starts_with(&self.syntax.values[1]) {
                    return Some(open);
                }
            }
            at = open + self.syntax.values[0].len();
        }
        None
    }
}

/// Structure-view symbols (blocks, macros) and folds (tag pairs, comments).
fn structure(text: &str) -> (Vec<Symbol>, Vec<Fold>) {
    if text.len() > MAX_SYNTAX_BYTES {
        return (Vec::new(), Vec::new());
    }
    let lexed = lex(text);
    if lexed.tags.len() > MAX_STRUCTURE_TAGS {
        return (Vec::new(), Vec::new());
    }
    let line_starts: Vec<usize> = std::iter::once(0).chain(text.match_indices('\n').map(|(at, _)| at + 1)).collect();
    let line_at = |at: usize| (line_starts.partition_point(|start| *start <= at) - 1) as u32;
    let mut symbols: Vec<Symbol> = Vec::new();
    let mut folds = Vec::new();
    // (tag name, tag start, symbol index)
    let mut open: Vec<(&str, usize, Option<usize>)> = Vec::new();

    for tag in &lexed.tags {
        if let Some(name) = tag.name.strip_prefix("end") {
            let Some(at) = open.iter().rposition(|(n, _, _)| *n == name) else {
                continue;
            };
            let (_, start, symbol) = open[at];
            open.truncate(at);

            let (start_line, end_line) = (line_at(start), line_at(tag.range.start));
            if end_line > start_line {
                folds.push(Fold { start_line, end_line, kind: FoldKind::Region });
            }
            if let Some(symbol) = symbol {
                symbols[symbol].range.end = tag.range.end;
            }
            continue;
        }

        let kind = match tag.name.as_str() {
            "block" => Some(SymbolKind::Field),
            "macro" => Some(SymbolKind::Function),
            "embed" => Some(SymbolKind::Module),
            _ => None,
        };
        let symbol = kind.zip(tag.argument.clone()).map(|(kind, name_range)| {
            let parent = open.iter().rev().find_map(|(_, _, s)| *s);
            symbols.push(Symbol {
                name: text[name_range.clone()].to_owned(),
                detail: None,
                kind,
                range: tag.range.clone(),
                name_range,
                parent,
            });
            symbols.len() - 1
        });
        if tag.paired
            && [
                "apply",
                "autoescape",
                "block",
                "cache",
                "embed",
                "filter",
                "for",
                "if",
                "macro",
                "raw",
                "sandbox",
                "set",
                "spaceless",
                "verbatim",
                "with",
            ]
            .contains(&tag.name.as_str())
        {
            open.push((tag.name.as_str(), tag.range.start, symbol));
        }
    }

    for comment in &lexed.comments {
        let (start_line, end_line) = (line_at(comment.start), line_at(comment.end));
        if end_line > start_line {
            folds.push(Fold { start_line, end_line, kind: FoldKind::Comment });
        }
    }

    // Prefix truncation keeps every retained symbol's parent index valid.
    symbols.truncate(MAX_STRUCTURE_ITEMS);
    folds.truncate(MAX_STRUCTURE_ITEMS);
    (symbols, folds)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(text: &str) -> Vec<(&str, &'static str)> {
        lex(text).tokens.iter().map(|t| (&text[t.range.clone()], t.kind.name())).collect()
    }

    #[test]
    fn highlights_twig_in_html() {
        let text = r#"<a href="{{ path('home') }}" class="x">{# note #}{{ user.name|upper }}</a>"#;
        assert_eq!(
            kinds(text),
            [
                ("<a", "tag"),
                ("href", "attribute"),
                ("\"", "string"),
                ("{{", "punctuation"),
                ("path", "function-call"),
                ("(", "punctuation"),
                ("'home'", "string"),
                (")", "punctuation"),
                ("}}", "punctuation"),
                ("\"", "string"),
                ("class", "attribute"),
                ("\"x\"", "string"),
                (">", "tag"),
                ("{# note #}", "comment"),
                ("{{", "punctuation"),
                ("user", "variable"),
                (".", "operator"),
                ("name", "property"),
                ("|", "operator"),
                ("upper", "function"),
                ("}}", "punctuation"),
                ("</a", "tag"),
                (">", "tag"),
            ]
        );
    }

    #[test]
    fn highlights_tags() {
        assert_eq!(
            kinds("{%- if items is not empty and true -%}"),
            [
                ("{%-", "punctuation"),
                ("if", "keyword"),
                ("items", "variable"),
                ("is", "keyword"),
                ("not", "keyword"),
                ("empty", "function"),
                ("and", "keyword"),
                ("true", "constant"),
                ("-%}", "punctuation"),
            ]
        );
    }

    #[test]
    fn skips_verbatim_contents() {
        let text = "{% verbatim %}{{ not twig }}{% endverbatim %}";
        let names: Vec<_> = kinds(text).into_iter().map(|(t, _)| t).collect();
        assert_eq!(names, ["{%", "verbatim", "%}", "{%", "endverbatim", "%}"]);
    }

    #[test]
    fn classifies_test_operators_apply_filters_and_named_call_arguments_by_local_shape(){
        let tokens=kinds("{{ value is defined }}{{ value is not empty }}{{ n is divisible by(3) }}{{ a is same as(b) }}{{ value is null }}{{ empty }}{{ null }}{% apply upper %}x{% endapply %}{{ range(low:1, high=3) }}");
        for token in [("defined","function"),("empty","function"),("divisible","function"),("by","keyword"),("same","function"),("null","function"),("upper","function"),("low","property"),("high","property"),("empty","variable"),("null","constant")]{assert!(tokens.contains(&token),"{token:?}: {tokens:?}");}
    }
    #[test]
    fn named_argument_roles_do_not_leak_into_hash_keys_grouped_comparisons_or_macro_parameters(){
        let source="{{ f(key: 1, data: {inside: 2}, test: (value == 3)) }}{% macro badge(label='x') %}{{ label }}{% endmacro %}{{ a is same as(expected: b) }}";
        let tokens=kinds(source);
        for token in [("key","property"),("data","property"),("test","property"),("inside","variable"),("value","variable"),("label","variable"),("expected","property")]{assert!(tokens.contains(&token),"{token:?}: {tokens:?}");}
        assert!(!tokens.contains(&("inside","property"))&&!tokens.contains(&("label","property"))&&!tokens.contains(&("value","property")));
    }
    #[test]
    fn added_roles_keep_every_unicode_prefix_raw_fold_and_outline_range_valid(){
        let source="😀\r\n{% block 名称 %}{{ value is not empty }}{{ f(arg: 2, other=(3)) }}\r\n{% endblock %}{% verbatim %}{{ f(hidden: 1) }}{% endverbatim %}";
        for end in source.char_indices().map(|(i,_)|i).chain(std::iter::once(source.len())){
            let text=&source[..end];for t in lex(text).tokens{assert!(t.range.end<=end&&text.is_char_boundary(t.range.start)&&text.is_char_boundary(t.range.end));}
        }
        let (outline,folds)=structure(source);assert!(outline.iter().any(|s|s.name=="名称"));assert!(!folds.is_empty());
        let tokens=kinds(source);assert!(!tokens.contains(&("hidden","property")));
    }
    #[test]
    fn classifies_numbers_members_and_named_declarations_by_verified_source_shape() {
        let tokens = kinds(
            "{{ 1..3 1.25 1_000.25e-2 2E+3 1__2 7e+ user.method() user.field }}{% macro input(name) %}{% endmacro %}{% block title %}{% endblock title %}",
        );
        for token in [
            ("1", "number"),
            ("..", "operator"),
            ("3", "number"),
            ("1.25", "number"),
            ("1_000.25e-2", "number"),
            ("2E+3", "number"),
            ("method", "function-call"),
            ("field", "property"),
            ("input", "function"),
            ("title", "property"),
        ] {
            assert!(tokens.contains(&token), "{token:?}");
        }
        assert!(!tokens.iter().any(|(value, _)| *value == "1..3" || *value == "1__2" || *value == "7e+"));
    }

    #[test]
    fn unicode_identifiers_and_every_valid_prefix_keep_exact_token_and_symbol_boundaries() {
        let source =
            "😀<p>{{ naïve.名()|upper }} {{ 😀name }}</p>\r\n{% block 名称 %}\n{% endblock %}\n{% macro café(x) %}{{ x }}{% endmacro %}";
        for end in source.char_indices().map(|(at, _)| at).chain(std::iter::once(source.len())) {
            let text = &source[..end];
            let tokens = lex(text).tokens;
            for token in &tokens {
                assert!(token.range.start < token.range.end && token.range.end <= text.len());
                assert!(text.is_char_boundary(token.range.start) && text.is_char_boundary(token.range.end));
                assert_eq!(openidea_plugin::text::offset(text, position(text, token.range.start)), Some(token.range.start));
                assert_eq!(openidea_plugin::text::offset(text, position(text, token.range.end)), Some(token.range.end));
            }
            assert!(tokens.windows(2).all(|pair| pair[0].range.end <= pair[1].range.start));
            let (symbols, _) = structure(text);
            for (index, symbol) in symbols.iter().enumerate() {
                assert_eq!(&text[symbol.name_range.clone()], symbol.name);
                assert!(
                    symbol.range.start <= symbol.name_range.start
                        && symbol.name_range.end <= symbol.range.end
                        && symbol.range.end <= text.len()
                );
                assert!(symbol.parent.is_none_or(|parent| parent < index));
            }
        }
        let tokens = kinds(source);
        assert!(tokens.contains(&("名", "function-call")));
        assert!(tokens.contains(&("😀name", "variable")));
    }

    #[test]
    fn raw_closers_must_be_exact_and_unclosed_raw_contents_remain_suppressed() {
        for opener in ["raw", "verbatim"] {
            let source = format!(
                "{{% {opener} %}}\n{{% end{opener}Extra %}}{{{{ bad }}}}\n{{% end{opener}π %}}{{{{ also_bad }}}}\n{{% end{opener} argument %}}\n{{% end{opener} %}}\n{{{{ good }}}}"
            );
            let tokens = kinds(&source);
            assert!(!tokens.iter().any(|(name, _)| matches!(*name, "bad" | "also_bad" | "argument")));
            assert!(tokens.contains(&("good", "variable")));
            let unclosed = format!("{{% {opener} %}} {{{{ hidden }}}} {{% if hidden %}}");
            assert!(!kinds(&unclosed).iter().any(|(name, _)| *name == "hidden"));
        }
    }

    #[test]
    fn original_embed_outline_and_known_block_folds_exclude_branches_and_unknown_tags() {
        let source = "{% embed 'panel.twig' %}\r\n{% block title %}\r\nA\r\n{% endblock %}\r\n{% endembed %}\r\n{% if yes %}\nA\n{% else %}\nB\n{% endif %}\n{% mystery %}\nX\n{% endmystery %}";
        let (symbols, folds) = structure(source);
        assert_eq!(
            symbols.iter().map(|symbol| (symbol.name.as_str(), symbol.kind, symbol.parent)).collect::<Vec<_>>(),
            [("panel.twig", SymbolKind::Module, None), ("title", SymbolKind::Field, Some(0))]
        );
        assert_eq!(folds.iter().map(|fold| (fold.start_line, fold.end_line)).collect::<Vec<_>>(), [(1, 3), (0, 4), (5, 9)]);
        assert_eq!(&source[symbols[0].name_range.clone()], "panel.twig");
        assert!(structure("{% embed 'unterminated\\'").0.is_empty());
    }

    #[test]
    fn syntax_export_limits_are_explicit_and_do_not_leak_partial_structure_parents() {
        let _host=openidea_plugin::mock::MockHost::install();
        let mut plugin = Twig::default();
        let huge = "x".repeat(MAX_SYNTAX_BYTES + 1);
        assert!(plugin.highlights("twig", &huge).is_empty());
        assert!(plugin.folding("twig", &huge).is_empty());
        assert!(plugin.outline("twig", &huge).is_empty());
        let dense = "{{ x + y }}".repeat(4000);
        assert_eq!(plugin.highlights("twig", &dense).len(), MAX_SYNTAX_TOKENS);
        let tags = "{% block a %}{% endblock %}".repeat(MAX_STRUCTURE_TAGS / 2 + 1);
        assert!(structure(&tags).0.is_empty());
        assert!(structure(&tags).1.is_empty());
        let source = "{# {% block fake %}\n{% endblock %} #}<!-- {% macro fake() %} -->";
        assert!(structure(source).0.is_empty());
    }

    #[test]
    fn outlines_and_folds_blocks() {
        let text = "{% extends 'base.html.twig' %}\n{% block body %}\n  {% set x = 1 %}\n  {% block inner %}\n  {% endblock %}\n  {% for i in 1..3 %}\n  {% endfor %}\n  {% block title 'Short' %}\n{% endblock %}\n{#\n long\n#}\n{% macro field(name) %}\n{% endmacro %}\n";
        let (symbols, folds) = structure(text);

        let outline: Vec<_> = symbols.iter().map(|s| (s.name.as_str(), s.kind, s.parent)).collect();
        assert_eq!(
            outline,
            [
                ("body", SymbolKind::Field, None),
                ("inner", SymbolKind::Field, Some(0)),
                ("title", SymbolKind::Field, Some(0)),
                ("field", SymbolKind::Function, None),
            ]
        );
        assert!(text[symbols[0].range.clone()].starts_with("{% block body %}"));
        assert!(text[symbols[0].range.clone()].ends_with("{% endblock %}"));

        let lines: Vec<_> = folds.iter().map(|f| (f.start_line, f.end_line, f.kind)).collect();
        assert_eq!(
            lines,
            [
                (3, 4, FoldKind::Region),
                (5, 6, FoldKind::Region),
                (1, 8, FoldKind::Region),
                (12, 13, FoldKind::Region),
                (9, 11, FoldKind::Comment),
            ]
        );
    }

    fn project(files: &[(&str, &str)]) -> openidea_plugin::mock::MockHost {
        let host = openidea_plugin::mock::MockHost::install();
        host.set_project_root(Some("/p"));
        for (path, text) in files {
            host.add_file(path, text);
        }
        host
    }

    fn open(host: &openidea_plugin::mock::MockHost, path: &str) -> Vec<(Option<String>, Option<String>)> {
        Twig::default().handle_events(&[Event::FileOpened(path.into())], &host.invocation());
        let project = host::project_of(path).unwrap();
        let text = project.read_file(path).unwrap();
        host.open_editor(path, "twig", &text, &[]);
        host.refresh_diagnostics(&mut Twig::default());
        host.provider_diagnostics().get(path).into_iter().flatten().map(|d| (d.code.clone(), d.source.clone())).collect()
    }

    #[test]
    fn publishes_findings_for_a_twig_file() {
        let host = project(&[("/p/a.html.twig", "{% if a %}\n{{ x ")]);
        let found = open(&host, "/p/a.html.twig");
        assert!(found.contains(&(Some("unclosed-tag".into()), Some("Twig".into()))));
        assert!(found.iter().any(|(code, _)| code.as_deref() == Some("unterminated-delimiter")));
    }

    #[test]
    fn live_diagnostics_are_bounded_language_scoped_and_replace_fixed_current_text(){
        use openidea_plugin::diagnostics::DiagnosticsProvider;
        let h=openidea_plugin::mock::MockHost::install();let mut twig=Twig::default();
        let mut doc=Document{path:"/project/page.twig".into(),language:"twig".into(),version:4,text:"{% endif %}".repeat(600)};
        let found=twig.diagnostics(&doc);assert_eq!(found.len(),500);
        doc.version+=1;doc.text="😀\r\n{% if ready %}ok{% endif %}".into();assert!(twig.diagnostics(&doc).is_empty());
        doc.text="{% if ready %}".into();doc.language="html".into();assert!(twig.diagnostics(&doc).is_empty());
        doc.language="twig".into();doc.text.push_str(&" ".repeat(128<<10));assert!(twig.diagnostics(&doc).is_empty());
        assert!(h.process_runs().is_empty());assert_eq!(h.fetches(),0);
    }
    #[test]
    fn diagnostics_can_be_switched_off() {
        let host = project(&[("/p/a.html.twig", "{% if a %}")]);
        host.set_setting("diagnostics", "false");
        assert!(open(&host, "/p/a.html.twig").is_empty());
    }

    #[test]
    fn completes_names_outside_symfony_but_not_inside() {
        let doc = |path: &str| Document { path: path.into(), language: "twig".into(), version: 1, text: "{% i".into() };
        let at = openidea_plugin::Position::new(0, 4);

        let _plain = project(&[("composer.json", "{}")]);
        assert!(Twig::default().completion(&doc("/p/a.twig"), at, None).iter().any(|i| i.label == "endif"));

        let _symfony = project(&[("composer.json", r#"{"require": {"symfony/framework-bundle": "^7"}}"#)]);
        assert!(Twig::default().completion(&doc("/p/a.twig"), at, None).is_empty());
    }
}

#[cfg(test)]
mod project_tests {
    use super::*;
    use openidea_plugin::mock::MockHost;

    const FORMS: &str = "{% macro input(name, value = '') %}<input>{% endmacro %}\n{% macro button(label) %}<b>{% endmacro %}\n";

    fn host() -> MockHost {
        let host = MockHost::install();
        host.add_file("composer.json", r#"{"require": {"twig/twig": "^3"}}"#);
        host.add_file("templates/forms.html.twig", FORMS);
        host.add_file("src/Ext.php", "<?php\nclass Ext { function f() { return [new TwigFilter('money', fn() => 1), new TwigFunction('asset_url', fn() => 1)]; } }\n");
        host
    }

    fn doc(text: &str) -> Document {
        Document { path: "/project/templates/page.html.twig".into(), language: "twig".into(), version: 1, text: text.into() }
    }

    fn at(marked: &str) -> (Document, openidea_plugin::Position) {
        let offset = marked.find('^').unwrap();
        let text = marked.replacen('^', "", 1);
        let position = position(&text, offset);
        (doc(&text), position)
    }

    fn labels(items: Vec<CompletionItem>) -> Vec<String> {
        items.into_iter().map(|i| i.label).collect()
    }

    #[test]
    fn nested_project_targets_and_disabled_or_projectless_extension_cache_never_supply_outer_owners(){
        let h=host();let mut twig=Twig::default();twig.activate(&h.context("aritti.twig")).unwrap();
        let(mut template,at_template)=at("{% include 'forms.ht^ml.twig' %}");template.path="/project/page.twig".into();assert_eq!(twig.definition(&template,at_template).len(),1);
        h.add_project("/project/templates");assert!(twig.definition(&template,at_template).is_empty());
        let(mut imported,at_import)=at("{% import 'forms.html.twig' as forms %}{{ forms.in^put() }}");imported.path="/project/page.twig".into();assert!(twig.definition(&imported,at_import).is_empty());
        let(mut d,p)=at("{{ value|mon^ey }}");d.path="/project/page.twig".into();assert_eq!(twig.definition(&d,p).len(),1);
        h.set_setting("projectExtensions","false");assert!(twig.definition(&d,p).is_empty());
        let(mut d,p)=at("{{ value|mo^ }}");d.path="/project/page.twig".into();assert!(!labels(twig.completion(&d,p,None)).contains(&"money".into()));
        h.set_setting("projectExtensions","true");h.set_project_root(None);assert!(!labels(twig.completion(&d,p,None)).contains(&"money".into()));
    }
    #[test]
    fn independently_opened_php_project_and_oversized_fresh_target_cannot_supply_outer_references(){
        let h=host();let mut twig=Twig::default();twig.activate(&h.context("aritti.twig")).unwrap();let(d,p)=at("{{ value|mon^ey }}");
        h.add_project("/project/src");assert!(twig.definition(&d,p).is_empty());let(d,p)=at("{{ value|mo^ }}");assert!(!labels(twig.completion(&d,p,None)).contains(&"money".into()));
        let h=host();let mut twig=Twig::default();twig.activate(&h.context("aritti.twig")).unwrap();let(d,p)=at("{{ value|mon^ey }}");let php=h.invocation().project().unwrap().read_file("src/Ext.php").unwrap();h.add_file("src/Ext.php",&format!("{php}{}"," ".repeat(128<<10)));assert!(twig.definition(&d,p).is_empty());
    }
    #[test]
    fn saved_vendor_php_does_not_expand_the_documented_source_ownership(){
        let h=host();h.add_file("vendor/Ext.php","<?php new TwigFilter('vendor_only', fn()=>1);");let mut twig=Twig::default();twig.activate(&h.context("aritti.twig")).unwrap();
        twig.handle_events(&[Event::FileSaved("/project/vendor/Ext.php".into())],&h.invocation());
        let(d,p)=at("{{ value|vendor_on^ly }}");assert!(twig.definition(&d,p).is_empty());let(d,p)=at("{{ value|vendor_^ }}");assert!(!labels(twig.completion(&d,p,None)).contains(&"vendor_only".into()));
    }
    #[test]
    fn completes_macros_of_an_imported_template() {
        let host = host();
        let mut twig = Twig::default();
        twig.activate(&host.context("aritti.twig")).unwrap();

        let (d, p) = at("{% import 'forms.html.twig' as forms %}{{ forms.^ }}");
        assert_eq!(labels(twig.completion(&d, p, None)), ["input", "button"]);
        let (d, p) = at("{% import 'forms.html.twig' as forms %}{{ forms.bu^ }}");
        assert_eq!(labels(twig.completion(&d, p, None)), ["input", "button"]);
        let (d, p) = at("{% from 'forms.html.twig' import ^ %}");
        assert_eq!(labels(twig.completion(&d, p, None)), ["input", "button"]);
        let (d, p) = at("{% macro local(a) %}{% endmacro %}{% import _self as me %}{{ me.^ }}");
        assert_eq!(labels(twig.completion(&d, p, None)), ["local"]);
        let (d, p) = at("{{ other.^ }}");
        assert!(labels(twig.completion(&d, p, None)).is_empty());
    }

    #[test]
    fn changed_php_notification_removes_cached_completion_until_saved_source_rewarms() {
        let host=host();let mut twig=Twig::default();twig.activate(&host.context("aritti.twig")).unwrap();
        let(d,p)=at("{{ value|mo^ }}");
        assert!(labels(twig.completion(&d,p,None)).contains(&"money".into()));
        twig.handle_events(&[Event::FilesChanged(openidea_plugin::FilesChanged {created:vec![],modified:vec!["/project/src/Ext.php".into()],removed:vec![]})],&host.invocation());
        assert!(!labels(twig.completion(&d,p,None)).contains(&"money".into()));
        twig.handle_events(&[Event::FileSaved("/project/src/Ext.php".into())],&host.invocation());
        assert!(labels(twig.completion(&d,p,None)).contains(&"money".into()));
        twig.handle_events(&[Event::FilesChanged(openidea_plugin::FilesChanged {created:vec![],modified:vec![],removed:vec!["/project/src/Ext.php".into()]})],&host.invocation());
        let(d,p)=at("{{ value|mon^ey }}");assert!(twig.definition(&d,p).is_empty());
    }

    #[test]
    fn dynamic_template_expressions_do_not_borrow_a_literal_prefix_target() {
        let _host=host();
        let mut twig=Twig::default();
        for source in ["{% include 'forms.ht^ml.twig' ~ suffix %}","{% extends 'forms.ht^ml.twig'|lower %}","{{ include('forms.ht^ml.twig' ~ suffix) }}","{{ source('forms.ht^ml.twig' ?? fallback) }}"] {
            let(d,p)=at(source);
            assert!(twig.definition(&d,p).is_empty(),"{source}");
        }
        for source in ["{% include 'forms.ht^ml.twig' with {'a':1} only %}","{{ include('forms.ht^ml.twig', {'a':1}) }}"] {
            let(d,p)=at(source);assert_eq!(twig.definition(&d,p).len(),1,"{source}");
        }
    }

    #[test]
    fn later_imports_do_not_supply_earlier_macro_owners() {
        let _host=host();
        let mut twig=Twig::default();
        for source in ["{{ forms.in^put() }}{% import 'forms.html.twig' as forms %}", "{{ fi^eld() }}{% from 'forms.html.twig' import input as field %}"] {
            let(d,p)=at(source);
            assert!(twig.definition(&d,p).is_empty(),"{source}");
        }
        let(d,p)=at("{{ forms.^ }}{% import 'forms.html.twig' as forms %}");
        assert!(twig.completion(&d,p,None).is_empty());
    }

    #[test]
    fn macro_completion_withholds_duplicate_or_malformed_target_ownership() {
        let host=host();
        let mut twig=Twig::default();
        host.add_file("templates/ambiguous.twig","{% macro repeated(a) %}{% endmacro %}{% macro repeated(b) %}{% endmacro %}{% macro unique() %}{% endmacro %}");
        let(d,p)=at("{% import 'ambiguous.twig' as m %}{{ m.^ }}");
        assert_eq!(labels(twig.completion(&d,p,None)),["unique"]);
        let(d,p)=at("{% import 'ambiguous.twig' as m %}{{ m.re^peated() }}");
        assert!(twig.definition(&d,p).is_empty());
        host.add_file("templates/broken.twig","{% macro broken(a) %}");
        let(d,p)=at("{% import 'broken.twig' as m %}{{ m.^ }}");
        assert!(twig.completion(&d,p,None).is_empty());
    }

    #[test]
    fn literal_self_template_import_uses_unsaved_current_macro_source_and_utf16_range() {
        let _host=host();
        let mut twig=Twig::default();
        let (mut d,p)=at("😀{% macro fresh(a) %}{% endmacro %}{% import 'forms.html.twig' as forms %}{{ forms.fr^esh() }}");
        d.path="/project/templates/forms.html.twig".into();
        let found=twig.definition(&d,p);
        assert_eq!(found.len(),1);
        assert_eq!(found[0].path,d.path);
        assert_eq!(found[0].range.start.character,11);
        let (mut d,p)=at("{% macro fresh(a) %}{% endmacro %}{% import 'forms.html.twig' as forms %}{{ forms.^ }}");
        d.path="/project/templates/forms.html.twig".into();
        assert_eq!(labels(twig.completion(&d,p,None)),["fresh"]);
    }

    #[test]
    fn goes_to_macros_and_templates() {
        let host = host();
        let mut twig = Twig::default();
        twig.activate(&host.context("aritti.twig")).unwrap();

        let (d, p) = at("{% import 'forms.html.twig' as forms %}{{ forms.but^ton('x') }}");
        let found = twig.definition(&d, p);
        assert_eq!(found.len(), 1);
        assert_eq!((found[0].path.as_str(), found[0].range.start.line), ("/project/templates/forms.html.twig", 1));

        let (d, p) = at("{% from 'forms.html.twig' import input as field %}{{ fie^ld('a') }}");
        assert_eq!(twig.definition(&d, p)[0].range.start.line, 0);

        let (d, p) = at("{% extends 'forms.ht^ml.twig' %}");
        assert_eq!(twig.definition(&d, p)[0].path, "/project/templates/forms.html.twig");
    }

    #[test]
    fn project_extension_names_complete_and_navigate_outside_symfony() {
        let host = host();
        let mut twig = Twig::default();
        twig.activate(&host.context("aritti.twig")).unwrap();

        let (d, p) = at("{{ price|mo^ }}");
        assert!(labels(twig.completion(&d, p, None)).contains(&"money".to_owned()));
        let (d, p) = at("{{ asse^ }}");
        assert!(labels(twig.completion(&d, p, None)).contains(&"asset_url".to_owned()));
        let (d, p) = at("{{ price|mon^ey }}");
        let found = twig.definition(&d, p);
        assert_eq!((found[0].path.as_str(), found[0].range.start.line), ("/project/src/Ext.php", 1));

        host.add_file("symfony.lock", "{}");
        let (d, p) = at("{{ price|mo^ }}");
        assert!(twig.completion(&d, p, None).is_empty());
        let (d, p) = at("{{ price|mon^ey }}");
        assert!(twig.definition(&d, p).is_empty());
    }
    #[test]
    fn scoped_imports_shadow_only_the_current_block_and_embed_isolates_outer_bindings() {
        let host = host();
        host.add_file("templates/other.twig", "{% macro input(x) %}{% endmacro %}");
        let mut twig = Twig::default();
        let (d, p) = at(
            "{% import 'forms.html.twig' as f %}{% block one %}{% import 'other.twig' as f %}{{ f.in^put() }}{% endblock %}{% block two %}{{ f.input() }}{% endblock %}",
        );
        assert!(twig.definition(&d, p)[0].path.ends_with("other.twig"));
        let (d, p) = at(
            "{% import 'forms.html.twig' as f %}{% block one %}{% import 'other.twig' as f %}{{ f.input() }}{% endblock %}{% block two %}{{ f.in^put() }}{% endblock %}",
        );
        assert!(twig.definition(&d, p)[0].path.ends_with("forms.html.twig"));
        for source in [
            "{% import 'forms.html.twig' as f %}{% embed 'base.twig' %}{{ f.in^put() }}{% endembed %}",
            "{% import 'forms.html.twig' as f %}{% macro local() %}{{ f.in^put() }}{% endmacro %}",
            "{% if dynamic %}{% import 'forms.html.twig' as f %}{% endif %}{{ f.in^put() }}",
            "{% import 'forms.html.twig' as f %}{% import 'other.twig' as f %}{{ f.in^put() }}",
        ] {
            let (d, p) = at(source);
            assert!(twig.definition(&d, p).is_empty(), "{source}");
        }
        let (d, p) = at("{% embed 'base.twig' %}{% import 'other.twig' as f %}{{ f.in^put() }}{% endembed %}");
        assert!(twig.definition(&d, p)[0].path.ends_with("other.twig"));
    }
    #[test]
    fn commented_raw_html_and_quoted_imports_or_calls_do_not_create_owners() {
        let host = host();
        let mut twig = Twig::default();
        twig.activate(&host.context("aritti.twig")).unwrap();
        for source in [
            "{# {% import 'forms.html.twig' as f %} #}{{ f.in^put() }}",
            "{% verbatim %}{% import 'forms.html.twig' as f %}{% endverbatim %}{{ f.in^put() }}",
            "{% import 'forms.html.twig' as f %}<p>f.in^put()</p>",
            "{% import 'forms.html.twig' as f %}{{ 'f.in^put()' }}",
            "{# {% extends 'forms.ht^ml.twig' %} #}",
            "<p>include('forms.ht^ml.twig')</p>",
            "{{ \"include('forms.ht^ml.twig')\" }}",
            "{{ object.include('forms.ht^ml.twig') }}",
            "{{ asset_u^rl }}",
        ] {
            let (d, p) = at(source);
            assert!(twig.definition(&d, p).is_empty(), "{source}");
        }
        host.add_file("templates/fake.twig", "{# {% macro input() %}{% endmacro %} #}");
        let (d, p) = at("{% import 'fake.twig' as f %}{{ f.in^put() }}");
        assert!(twig.definition(&d, p).is_empty());
    }
    #[test]
    fn template_paths_have_conventional_ownership_without_namespace_or_suffix_guesses() {
        let host = host();
        host.add_file("elsewhere/unique.twig", "x");
        let mut twig = Twig::default();
        for name in ["unique.twig", "@bundle/forms.html.twig", "../templates/forms.html.twig", "forms*.twig", "forms.html.twig/../x"] {
            let (d, p) = at(&format!("{{% include '{name}^' %}}"));
            assert!(twig.definition(&d, p).is_empty(), "{name}");
        }
        let (d, p) = at("{% include 'forms.ht^ml.twig' %}");
        assert_eq!(twig.definition(&d, p).len(), 1);
        host.add_file("symfony.lock", "{}");
        assert!(twig.definition(&d, p).is_empty());
        let (d, p) = at("{% import 'forms.html.twig' as f %}{{ f.in^put() }}");
        assert!(twig.definition(&d, p).is_empty());
        let (d, p) = at("{% import 'forms.html.twig' as f %}{{ f.^ }}");
        assert!(twig.completion(&d, p, None).is_empty());
    }
    #[test]
    fn self_macros_use_unsaved_unicode_source_without_project_or_stale_ranges() {
        let host = MockHost::install().with_capabilities(&[]);
        host.set_project_root(None);
        let mut twig = Twig::default();
        let (d, p) = at("😀 {% macro café(x) %}{{ x }}{% endmacro %}{{ _self.ca^fé('x') }}");
        let found = twig.definition(&d, p);
        assert_eq!(found.len(), 1);
        let r = found[0].range;
        assert_eq!(&d.text[offset(&d.text, r.start).unwrap()..offset(&d.text, r.end).unwrap()], "café");
        let fixed = Document { text: d.text.replace("macro café", "macro other"), version: 2, ..d.clone() };
        assert!(twig.definition(&fixed, position(&fixed.text, fixed.text.find("_self.café").unwrap() + 8)).is_empty());
        let (d, p) = at("{% macro x() %}{% endmacro %}{% macro x() %}{% endmacro %}{{ _self.^x() }}");
        assert!(twig.definition(&d, p).is_empty());
        assert_eq!(host.fetches(), 0);
    }
    #[test]
    fn refreshed_targets_duplicate_bindings_and_provider_bounds_clear_old_navigation() {
        let host = host();
        let mut twig = Twig::default();
        let (d, p) = at("{% from 'forms.html.twig' import input as field %}{{ fie^ld() }}");
        assert_eq!(twig.definition(&d, p).len(), 1);
        host.open_editor("/project/templates/forms.html.twig", "twig", "{% macro changed() %}{% endmacro %}", &[]);
        assert!(twig.definition(&d, p).is_empty());
        let (d, p) = at("{% from 'forms.html.twig' import input as field,button as field %}{{ fie^ld() }}");
        assert!(twig.definition(&d, p).is_empty());
        let (d, p) = at("{% macro one() %}{% endmacro %}{{ _self.o^ne() }}");
        let wrong = Document { language: "html".into(), ..d.clone() };
        assert!(twig.definition(&wrong, p).is_empty());
        let huge = Document { text: format!("{}{}", " ".repeat(128 << 10), d.text), ..d };
        assert!(twig.definition(&huge, position(&huge.text, huge.text.len() - 6)).is_empty());
        let (d, p) = at(&format!("{}{{% macro one() %}}{{% endmacro %}}{{{{ _self.o^ne() }}}}", "{% do 1 %}".repeat(4096)));
        assert!(twig.definition(&d, p).is_empty());
    }
}
