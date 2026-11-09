//! Spring Shell Tools for OpenIDEA: indexes the shell commands of a project
//! (`@ShellMethod` and `@Command` methods), checks availability methods and
//! providers, and completes and links the names they refer to. Text-based,
//! Java and Kotlin.

use openidea_plugin::diagnostics::DiagnosticsProvider;
use openidea_plugin::providers::{CompletionItem, CompletionKind, Document, LineMarker, Providers};
use openidea_plugin::text::{offset, position};
use openidea_plugin::{Diagnostic, Error, Invocation, Location, Plugin, Position, Range, Severity, register_plugin};
use spring_text::index::{Index, Indexes, PROVIDER_BUDGET};
use spring_text::java::{self, Annotation, Arg, JavaFile, Member, Str};
use spring_text::{SOURCE_GLOBS, parse_source, range};

mod actions;
mod command_rename;
mod hover;
mod options;
mod literals;
mod rules;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Command {
    /// The full command, words separated by one space.
    pub key: String,
    /// Qualified name of the declaring class.
    pub class: String,
    pub method: String,
    /// The key's literal, or the method name when the key is derived from it.
    pub anchor: Range,
    /// The method name.
    pub target: Range,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Shell {
    pub incomplete: bool,
    pub commands: Vec<Command>,
    pub groups: Vec<String>,
    /// Bean names, and whether the bean is an `AvailabilityProvider`.
    pub beans: Vec<(String, Range, bool)>,
}

/// Spring Shell's default command name: `addUser` → `add-user`.
pub fn kebab(name: &str) -> String {
    let mut out = String::new();
    let mut lower = false;
    for c in name.chars() {
        if lower && c.is_ascii_uppercase() {
            out.push('-');
        }
        lower = c.is_ascii_lowercase();
        out.push(c.to_ascii_lowercase());
    }
    out
}

fn words(strings: &[&Str]) -> String {
    options::normalized(&strings.iter().map(|s| s.value.as_str()).collect::<Vec<_>>().join(" "))
}

pub fn commands(f: &JavaFile, text: &str) -> Vec<Command> {
    let mut out = Vec::new();
    for (ti, t) in f.types.iter().enumerate() {
        let class = f.qualified(ti);
        let prefix = t
            .annotation("Command")
            .filter(|a| rules::owned(f, text, a, "org.springframework.shell.command.annotation.Command"))
            .map(|a| words(&a.strings(&["command"])));
        for m in t.methods() {
            let target = range(text, m.at, m.at + m.name.len());
            let mut push = |key: String, anchor: Range| {
                if !key.is_empty() && !key.contains('$') {
                    out.push(Command { key, class: class.clone(), method: m.name.clone(), anchor, target });
                }
            };
            if let Some(a) = m.annotation("ShellMethod").filter(|a| rules::supported(f, text, a)) {
                if a.arg("key").is_some_and(|arg| !hover::literal_values(arg) || arg.text.contains('\\')) {
                    continue;
                }
                let keys = a.strings(&["key"]);
                if keys.is_empty() {
                    if !m.name.is_ascii() {
                        continue;
                    }
                    push(kebab(&m.name), target);
                }
                for s in keys {
                    push(words(&[s]), range(text, s.at, s.end));
                }
            } else if let Some(a) =
                m.annotation("Command").filter(|a| rules::owned(f, text, a, "org.springframework.shell.command.annotation.Command"))
            {
                if a.arg("command").is_some() && rules::literals(a, "command").is_none()
                    || t.annotation("Command")
                        .filter(|a| rules::supported(f, text, a))
                        .is_some_and(|a| a.arg("command").is_some() && rules::literals(a, "command").is_none())
                {
                    continue;
                }
                let own = a.strings(&["command"]);
                let anchor = own.first().map_or(target, |s| range(text, s.at, s.end));
                let own = words(&own);
                let combined = options::normalized(&format!("{} {own}", prefix.as_deref().unwrap_or_default()));
                if combined.is_empty() && !m.name.is_ascii() {
                    continue;
                }
                push(if combined.is_empty() { kebab(&m.name) } else { combined }, anchor);
            }
        }
    }
    out
}

/// Every annotation of the file's types and members.
fn annotations(f: &JavaFile) -> impl Iterator<Item = &Annotation> {
    f.types.iter().flat_map(|t| t.annotations.iter().chain(t.members.iter().flat_map(|m| &m.annotations)))
}

fn groups(f: &JavaFile, text: &str) -> Vec<String> {
    let mut out: Vec<String> = annotations(f)
        .filter(|a| rules::supported(f, text, a))
        .flat_map(|a| {
            let name = match a.name.as_str() {
                "ShellCommandGroup" => "value",
                "ShellMethod" | "Command" => "group",
                _ => return Vec::new(),
            };
            rules::literals(a, name).unwrap_or_default()
        })
        .map(|s| s.value.clone())
        .filter(|g| !g.is_empty() && !g.contains("${"))
        .collect();
    out.sort();
    out.dedup();
    out
}

fn beans(f: &JavaFile, text: &str) -> Vec<(String, Range, bool)> {
    let provider_type = |ty: &java::TypeRef| rules::type_owned(f,text,ty,"org.springframework.shell.AvailabilityProvider");
    let providers: Vec<_> = f.types.iter().enumerate().filter(|(_,t)| t.implements.iter().any(&provider_type)).map(|(i,_)|i).collect();
    java::bean_names(f)
        .into_iter()
        .filter_map(|b| {
            let annotation = f
                .types
                .iter()
                .find(|t| t.at == b.at)
                .and_then(|t| {
                    t.annotations.iter().find(|a| {
                        let q = match a.name.as_str() {
                            "ShellComponent" => "org.springframework.shell.standard.ShellComponent",
                            "Component" => "org.springframework.stereotype.Component",
                            "Service" => "org.springframework.stereotype.Service",
                            "Repository" => "org.springframework.stereotype.Repository",
                            "Controller" => "org.springframework.stereotype.Controller",
                            "Configuration" => "org.springframework.context.annotation.Configuration",
                            _ => return false,
                        };
                        rules::owned(f, text, a, q)
                    })
                })
                .or_else(|| {
                    f.types
                        .iter()
                        .flat_map(|t| t.methods())
                        .find(|m| m.at == b.at)
                        .and_then(|m| m.annotation("Bean"))
                        .filter(|a| rules::owned(f, text, a, "org.springframework.context.annotation.Bean"))
                })?;
            if ["name", "value"]
                .iter()
                .any(|name| annotation.arg(name).is_some_and(|arg| !hover::literal_values(arg) || arg.text.contains(['$', '\\'])))
            {
                return None;
            }
            let ty = f.types.iter().flat_map(|t| t.methods()).find(|m| m.at == b.at).and_then(|m| m.ty.as_ref());
            let provider = if let Some(ty)=ty {
                provider_type(ty) || providers.iter().any(|i|rules::local_type_owned(f,text,ty,*i))
            } else {providers.iter().any(|i|f.types[*i].at==b.at)};
            Some((b.name, range(text, b.at, b.at + b.len), provider))
        })
        .collect()
}

fn source_model(path: &str, text: &str) -> Option<JavaFile> {
    // Java translates eligible Unicode escapes before token/comment scanning.
    // Without that preprocessing and offset mapping, source ownership is unknown.
    if path.ends_with(".java") && !literals::java_source_supported(text) {
        return None;
    }
    if text.len() > 128 << 10 {
        return None;
    }
    let mut stack = Vec::new();
    for token in java::tokens(text) {
        let s = token.s;
        if s.starts_with('"') || s.starts_with('\'') {
            let quote = if s.starts_with("\"\"\"") { 3 } else { 1 };
            if s.len() < 2 * quote || s.as_bytes().first() != s.as_bytes().last() {
                return None;
            }
            continue;
        }
        match s {
            "(" | "[" | "{" => stack.push(s),
            ")" | "]" | "}" => {
                if stack.pop()
                    != Some(match s {
                        ")" => "(",
                        "]" => "[",
                        _ => "{",
                    })
                {
                    return None;
                }
            }
            _ => {}
        }
    }
    if !stack.is_empty() {
        return None;
    }
    let file = parse_source(path, text);
    if file.imports.len() > 256
        || file.types.len() > 128
        || file.types.iter().any(|t| t.members.len() > 128)
        || file.types.iter().map(|t| t.members.len()).sum::<usize>() > 512
    {
        return None;
    }
    Some(file)
}
fn document_model(doc: &Document) -> Option<JavaFile> {
    if !matches!((doc.language.as_str(), doc.path.rsplit('.').next()), ("java", Some("java")) | ("kotlin", Some("kt"))) {
        return None;
    }
    source_model(&doc.path, &doc.text)
}
fn scan(path: &str, text: &str) -> Option<Shell> {
    let Some(f) = source_model(path, text) else {
        return Some(Shell { incomplete: true, ..Default::default() });
    };
    let shell = Shell { incomplete: false, commands: commands(&f, text), groups: groups(&f, text), beans: beans(&f, text) };
    (shell != Shell::default()).then_some(shell)
}

fn all_commands(index: &Index<Shell>) -> impl Iterator<Item = (&str, &Command)> {
    index.iter().flat_map(|(p, s)| s.commands.iter().map(move |c| (p, c)))
}

fn all_beans(index: &Index<Shell>) -> impl Iterator<Item = (&str, &(String, Range, bool))> {
    index.iter().flat_map(|(p, s)| s.beans.iter().map(move |b| (p, b)))
}

fn unique_provider_names(index:&Index<Shell>)->std::collections::BTreeSet<&str> {
    let mut names:std::collections::BTreeMap<&str,(std::collections::BTreeSet<(&str,u32,u32)>,bool)>=std::collections::BTreeMap::new();
    for (path,bean) in all_beans(index) {
        let entry=names.entry(&bean.0).or_insert_with(||(std::collections::BTreeSet::new(),true));
        entry.0.insert((path,bean.1.start.line,bean.1.start.character));
        entry.1 &= bean.2;
    }
    names.into_iter().filter(|(_,entry)|entry.0.len()==1&&entry.1).map(|(name,_)|name).collect()
}

fn simple(class: &str) -> &str {
    class.rsplit('.').next().unwrap_or(class)
}

fn problem(range: Range, severity: Severity, code: &str, message: String) -> Diagnostic {
    Diagnostic::new(range, severity, message).code(code)
}

/// Legacy indicator selectors address commands in their declaring class. An
/// unrelated class's equal command name is not an availability target.
fn named<'a>(index: &'a Index<Shell>, class: &str, key: &str) -> Vec<(&'a str, &'a Command)> {
    all_commands(index).filter(|(_, c)| c.class == class && c.key == key).collect()
}

fn availability_owner<'a>(f: &'a JavaFile, text: &str, annotation: &Annotation) -> Option<&'a java::TypeDecl> {
    f.types.iter().find(|t| {
        t.methods().any(|m| {
            m.annotation("ShellMethod").is_some_and(|a| rules::supported(f, text, a)) && m.annotations.iter().any(|a| a.at == annotation.at)
        })
    })
}

/// Problems and gutter links of a Java file.
fn analyze(doc: &Document, f: &JavaFile, index: &Index<Shell>) -> (Vec<Diagnostic>, Vec<LineMarker>) {
    let text = &doc.text;
    let at = |from: usize, to: usize| range(text, from, to);
    let name_of = |m: &Member| at(m.at, m.at + m.name.len());
    let line_of = |m: &Member| position(text, m.at).line;
    let mine = commands(f, text);
    let provider_names = unique_provider_names(index);
    let mut problems = Vec::new();
    let mut markers = Vec::new();

    for c in &mine {
        let other = all_commands(index)
            .find(|(p, o)| (*p == doc.path || index.fresh()) && o.key == c.key && !(*p == doc.path && o.anchor == c.anchor));
        if let Some((_, o)) = other {
            let message = format!("Command `{}` is also declared by {}.{}.", c.key, simple(&o.class), o.method);
            problems.push(problem(c.anchor, Severity::Error, "duplicate-command", message));
        }
    }

    for (ti, t) in f.types.iter().enumerate() {
        let class = f.qualified(ti);
        let legacy: Vec<&Member> =
            t.methods().filter(|m| m.annotation("ShellMethod").is_some_and(|a| rules::supported(f, text, a))).collect();
        for m in t.methods() {
            for s in m
                .annotation("CommandAvailability")
                .filter(|a| rules::supported(f, text, a))
                .and_then(|a| a.arg("provider"))
                .filter(|arg| hover::literal_values(arg))
                .into_iter()
                .flat_map(|a| &a.strings)
                .filter(|s| !s.value.contains('$'))
            {
                match all_beans(index).find(|(_, b)| b.0 == s.value && provider_names.contains(s.value.as_str())) {
                    Some((path, b)) => markers.push(LineMarker {
                        line: line_of(m),
                        icon: None,
                        tooltip: format!("Availability provider: {}", s.value),
                        target: Some(Location { path: path.into(), range: b.1 }),
                    }),
                    None => {}
                }
            }
        }
        for m in &legacy {
            let keys: Vec<&str> = mine.iter().filter(|c| c.class == class && c.method == m.name).map(|c| c.key.as_str()).collect();
            let conventional = format!("{}Availability", m.name);
            let available = if let Some(explicit) = m.annotation("ShellMethodAvailability").filter(|a| rules::supported(f, text, a)) {
                explicit.arg("value").filter(|arg| hover::literal_values(arg)).and_then(|arg| match arg.strings.as_slice() {
                    [name] => t.methods().find(|m| m.name == name.value && m.params.is_empty()),
                    _ => None,
                })
            } else {
                let candidate = |wildcard: bool| {
                    t.methods().find(|m| {
                        m.params.is_empty()
                            && !m.annotation("ShellMethod").is_some_and(|a| rules::supported(f, text, a))
                            && m.annotation("ShellMethodAvailability").filter(|a| rules::supported(f, text, a)).is_some_and(|a| {
                                if a.arg("value").is_none() {
                                    return wildcard;
                                }
                                let Some(arg) = a.arg("value").filter(|arg| hover::literal_values(arg)) else { return false };
                                arg.strings.iter().any(|s| if wildcard { s.value == "*" } else { keys.contains(&words(&[s]).as_str()) })
                            })
                    })
                };
                t.methods().find(|a| a.name == conventional && a.params.is_empty()).or_else(|| candidate(false)).or_else(|| candidate(true))
            };
            if let Some(a) = available {
                markers.push(LineMarker {
                    line: line_of(m),
                    icon: None,
                    tooltip: format!("Availability method: {}()", a.name),
                    target: Some(Location { path: doc.path.clone(), range: name_of(a) }),
                });
            }
        }
    }
    problems.extend(rules::check(doc, f));
    (problems, markers)
}

/// The annotation string literal at byte `at`, with its annotation and argument.
fn literal_at(f: &JavaFile, at: usize) -> Option<(&Annotation, &Arg, &Str)> {
    annotations(f).find_map(|a| a.args.iter().find_map(|arg| Some((a, arg, arg.strings.iter().find(|s| s.holds(at))?))))
}

/// The qualified name of the type declaring byte `at`.
fn class_at(f: &JavaFile, at: usize) -> String {
    f.types.iter().enumerate().rfind(|(_, t)| t.start <= at && at <= t.end).map(|(i, _)| f.qualified(i)).unwrap_or_default()
}

pub struct ShellTools {
    actions: actions::State,
    command_renames: command_rename::State,
    indexes: Indexes<Shell>,
}

impl Default for ShellTools {
    fn default() -> Self {
        Self { actions: Default::default(), command_renames: Default::default(), indexes: Indexes::new(SOURCE_GLOBS, scan) }
    }
}

impl DiagnosticsProvider for ShellTools {
    fn diagnostics(&mut self, doc: &Document) -> Vec<Diagnostic> {
        let Some(file) = document_model(doc) else {
            return Vec::new();
        };
        let mut local = Index::new(SOURCE_GLOBS, scan);
        local.update(&doc.path, &doc.text);
        let index = self.indexes.for_cached_document(&doc.path, &doc.text).unwrap_or(&mut local);
        let mut diagnostics = analyze(doc, &file, index).0;
        diagnostics.truncate(500);
        diagnostics
    }
}

impl Providers for ShellTools {
    fn line_markers(&mut self, doc: &Document) -> Vec<LineMarker> {
        let Some(f) = document_model(doc) else {
            return Vec::new();
        };
        let Some(index) = self.indexes.for_document(&doc.path, &doc.text, PROVIDER_BUDGET) else {
            return Vec::new();
        };
        let (_, markers) = analyze(doc, &f, index);
        markers
    }

    fn definition(&mut self, doc: &Document, at: Position) -> Vec<Location> {
        let Some(f) = document_model(doc) else {
            return Vec::new();
        };
        let Some(offset) = offset(&doc.text, at) else {
            return Vec::new();
        };
        let Some((a, arg, s)) = literal_at(&f, offset) else {
            return Vec::new();
        };
        if !rules::supported(&f, &doc.text, a) || !hover::literal_values(arg) || arg.text.contains('\\') || s.value.contains('$') {
            return Vec::new();
        }
        if a.name == "ShellMethodAvailability" && arg.name == "value" {
            if !hover::literal_values(arg) {
                return Vec::new();
            }
            if let Some(owner) = availability_owner(&f, &doc.text, a) {
                return owner
                    .methods()
                    .filter(|m| m.name == s.value && m.params.is_empty())
                    .map(|m| Location { path: doc.path.clone(), range: range(&doc.text, m.at, m.at + m.name.len()) })
                    .collect();
            }
        }
        let Some(index) = self.indexes.for_document(&doc.path, &doc.text, PROVIDER_BUDGET) else {
            return Vec::new();
        };
        match (a.name.as_str(), arg.name.as_str()) {
            ("ShellMethodAvailability", "value") => named(index, &class_at(&f, offset), &words(&[s]))
                .into_iter()
                .map(|(path, c)| Location { path: path.into(), range: c.target })
                .collect(),
            ("CommandAvailability", "provider") => {
                all_beans(index).filter(|(_, b)| b.0 == s.value).map(|(path, b)| Location { path: path.into(), range: b.1 }).collect()
            }
            _ => Vec::new(),
        }
    }

    fn completion(&mut self, doc: &Document, at: Position, _trigger: Option<&str>) -> Vec<CompletionItem> {
        let Some(f) = document_model(doc) else {
            return Vec::new();
        };
        let Some(offset) = offset(&doc.text, at) else {
            return Vec::new();
        };
        let Some((a, arg, s)) = literal_at(&f, offset) else {
            return Vec::new();
        };
        if !rules::supported(&f, &doc.text, a) || !hover::literal_values(arg) || arg.text.contains('\\') || s.value.contains('$') {
            return Vec::new();
        }
        let replace = Some(range(&doc.text, s.at, s.end));
        let item = |label: &str, kind: CompletionKind, detail: &str, sort: &str| CompletionItem {
            detail: Some(detail.into()),
            replace,
            sort_text: Some(format!("{sort}{label}")),
            ..CompletionItem::new(label, kind)
        };
        if a.name == "ShellMethodAvailability" && arg.name == "value" {
            if let Some(owner) = availability_owner(&f, &doc.text, a) {
                return owner
                    .methods()
                    .filter(|m| m.params.is_empty() && m.ty.as_ref().is_some_and(|t| rules::availability_type(&f, &doc.text, t)))
                    .map(|m| item(&m.name, CompletionKind::Method, "Availability indicator method", "0"))
                    .collect();
            }
        }
        let Some(index) = self.indexes.for_document(&doc.path, &doc.text, PROVIDER_BUDGET) else {
            return Vec::new();
        };
        let mut out: Vec<CompletionItem> = match (a.name.as_str(), arg.name.as_str()) {
            ("ShellMethodAvailability", "value") => {
                let class = class_at(&f, offset);
                let mut out: Vec<_> = all_commands(index)
                    .filter(|(_, c)| c.class == class)
                    .map(|(_, c)| {
                        let detail = format!("{}.{}()", simple(&c.class), c.method);
                        item(&c.key, CompletionKind::Method, &detail, if c.class == class { "0" } else { "1" })
                    })
                    .collect();
                out.push(item("*", CompletionKind::Keyword, "every command of the class", "2"));
                out
            }
            ("ShellMethod" | "Command", "group") | ("ShellCommandGroup", "value") => {
                index.iter().flat_map(|(_, s)| &s.groups).map(|g| item(g, CompletionKind::Module, "command group", "0")).collect()
            }
            ("CommandAvailability", "provider") => unique_provider_names(index).into_iter()
                .map(|name| item(name, CompletionKind::Reference, "Declared AvailabilityProvider bean", "0"))
                .collect(),
            _ => Vec::new(),
        };
        out.sort_by(|a, b| a.label.cmp(&b.label));
        out.dedup_by(|a, b| a.label == b.label);
        out
    }
}

impl Plugin for ShellTools {
    fn run_command(&mut self, id: &str, ctx: &Invocation, _arg: Option<&str>) -> Result<Option<String>, Error> {
        if id == "command-occurrences" { return command_rename::occurrences(ctx); }
        Err(Error::NotFound(format!("command {id}")))
    }
}

register_plugin!(ShellTools, providers, diagnostics, hover, file_actions);

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;

    const LEGACY: &str = r#"package com.acme.cli;

import org.springframework.shell.standard.ShellComponent;
import org.springframework.shell.standard.ShellMethod; import org.springframework.shell.standard.ShellMethodAvailability; import org.springframework.shell.Availability;

@ShellComponent
public class UserCommands {
    @ShellMethod(value = "Add a user", group = "Users")
    public String addUser(@ShellOption String name) { return name; }

    @ShellMethod(key = {"remove", "rm"}, value = "Remove a user")
    public void remove(String name) {}

    @ShellMethod("List users")
    public void list() {}

    public Availability addUserAvailability() { return Availability.available(); }

    @ShellMethodAvailability({"remove", "nope"})
    public boolean canRemove() { return true; }

    @ShellMethodAvailability("*")
    public Availability always() { return Availability.available(); }
}
"#;

    const MODERN: &str = r#"package com.acme.cli;

import org.springframework.shell.command.annotation.Command;
import org.springframework.shell.command.annotation.CommandAvailability; import org.springframework.context.annotation.Bean; import org.springframework.context.annotation.Configuration; import org.springframework.shell.AvailabilityProvider;

@Command(command = "db", group = "Database")
class DbCommands {
    @Command(command = {"dump", "all"})
    @CommandAvailability(provider = "connected")
    void dumpAll() {}

    @Command
    @CommandAvailability(provider = "missing")
    void showTables() {}

    @Command(command = "list")
    void list() {}
}

@Configuration
class ShellConfig {
    @Bean
    AvailabilityProvider connected() { return () -> Availability.available(); }

    @Bean
    Clock clock() { return null; }
}
"#;

    fn doc(name: &str, text: &str) -> Document {
        Document { path: format!("/project/src/main/java/com/acme/cli/{name}"), language: "java".into(), version: 1, text: text.into() }
    }

    fn setup() -> MockHost {
        let host = MockHost::install();
        host.add_file("src/main/java/com/acme/cli/UserCommands.java", LEGACY);
        host.add_file("src/main/java/com/acme/cli/DbCommands.java", MODERN);
        host.add_file("src/main/java/com/acme/cli/Other.java", "import org.springframework.shell.standard.ShellComponent; import org.springframework.shell.standard.ShellMethod; @ShellComponent class Other { @ShellMethod(key = \"list\") void ls() {} }");
        host
    }

    fn pos(text: &str, needle: &str, delta: usize) -> Position {
        position(text, text.find(needle).unwrap() + delta)
    }

    const KOTLIN: &str = r#"package com.acme.cli

import org.springframework.shell.command.annotation.Command
import org.springframework.shell.command.annotation.CommandAvailability
import org.springframework.context.annotation.Bean
import org.springframework.shell.AvailabilityProvider
import org.springframework.shell.standard.ShellComponent
import org.springframework.shell.standard.ShellMethod
import org.springframework.shell.standard.ShellMethodAvailability

@ShellComponent
class KtCommands {
    @ShellMethod(key = ["greet", "hi"], value = "Say hello")
    suspend fun greet(name: String): String = "hi $name"

    @Command(command = ["kt", "dump"], group = "Kt")
    @CommandAvailability(provider = "ktMissing")
    fun dump() {}

    @ShellMethodAvailability(["greet", "nope"])
    fun canGreet(): Availability = Availability.available()
}

@Configuration
class KtConfig {
    @Bean
    fun ktConnected(): AvailabilityProvider = AvailabilityProvider { Availability.available() }
}
"#;

    #[test]
    fn diagnostics_skip_oversized_or_other_document_types() {
        let _host = setup();
        let mut tools = ShellTools::default();
        let large = "x".repeat(512 * 1024 + 1);
        assert!(tools.diagnostics(&doc("Large.java", &large)).is_empty());
        assert!(tools.diagnostics(&doc("Other.txt", LEGACY)).is_empty());
    }

    #[test]
    fn live_unsaved_checks_work_without_project_access_and_clear_after_fix() {
        for caps in [vec!["project.read"], vec![]] {
            let host = MockHost::install().with_capabilities(&caps);
            host.add_file(
                "src/main/java/com/acme/cli/UserCommands.java",
                &LEGACY.replace("public boolean canRemove()", "public Availability canRemove()"),
            );
            let path = doc("UserCommands.java", LEGACY).path;
            host.open_editor(&path, "java", LEGACY, &[]);
            let mut tools = ShellTools::default();
            host.refresh_diagnostics(&mut tools);
            let problems = host.provider_diagnostics();
            let problems = &problems[&path];
            assert!(problems.iter().any(|d| d.code.as_deref() == Some("availability-return-type")));
            assert!(problems.iter().any(|d| d.code.as_deref() == Some("unknown-command")));
            assert!(host.diagnostics().is_empty());
            let fixed = LEGACY.replace("public boolean canRemove()", "public Availability canRemove()");
            host.open_editor(&path, "java", &fixed, &[]);
            host.refresh_diagnostics(&mut tools);
            assert!(host.provider_diagnostics().values().flatten().all(|d| d.code.as_deref() != Some("availability-return-type")));
        }
    }

    #[test]
    fn kotlin_shell_command_is_indexed() {
        let kt = |text: &str| commands(&parse_source("K.kt", text), text).into_iter().map(|c| c.key).collect::<Vec<_>>();
        assert_eq!(kt(KOTLIN), ["greet", "hi", "kt dump"]);
        let beans = scan("K.kt", KOTLIN).unwrap().beans;
        assert!(beans.iter().any(|b| b.0 == "ktConnected" && b.2), "{beans:?}");
        let host = setup();
        host.add_file("src/main/kotlin/com/acme/cli/KtCommands.kt", KOTLIN);
        let mut tools = ShellTools::default();
        let d = Document {
            path: "/project/src/main/kotlin/com/acme/cli/KtCommands.kt".into(),
            language: "kotlin".into(),
            version: 1,
            text: KOTLIN.into(),
        };
        tools.line_markers(&d);
        assert!(host.diagnostics().is_empty());
        let problems = tools.diagnostics(&d);
        let messages: Vec<_> = problems.iter().map(|p| p.message.as_str()).collect();
        assert!(messages.iter().any(|m| m.contains("nope")), "{messages:?}");
        assert!(!messages.iter().any(|m| m.contains("ktMissing")), "{messages:?}");
        assert_eq!(messages.len(), 1, "{messages:?}");
    }

    #[test]
    fn kotlin_template_keys_are_not_commands() {
        let src = "import org.springframework.shell.standard.ShellMethod\n@ShellComponent\nclass K {\n    @ShellMethod(key = [\"$p-run\", \"ok\"])\n    fun a() {}\n}\n";
        let file = parse_source("K.kt", src);
        let keys: Vec<_> = commands(&file, src).into_iter().map(|c| c.key).collect();
        assert!(keys.is_empty(), "Mixed dynamic arrays are not evaluated.");
    }

    #[test]
    fn names_commands_of_both_models() {
        assert_eq!(kebab("addUser"), "add-user");
        assert_eq!(kebab("getURL"), "get-url");
        let keys = |text: &str| commands(&java::parse(text), text).into_iter().map(|c| c.key).collect::<Vec<_>>();
        assert_eq!(keys(LEGACY), ["add-user", "remove", "rm", "list"]);
        assert_eq!(keys(MODERN), ["db dump all", "db", "db list"]);
        assert!(keys("import picocli.CommandLine.Command;\n@Command(name = \"x\") class A { @Command void run() {} }").is_empty());
        let shell = scan("", MODERN).unwrap();
        assert_eq!(shell.groups, ["Database"]);
        assert_eq!(
            shell.beans.iter().map(|b| (b.0.as_str(), b.2)).collect::<Vec<_>>(),
            [("shellConfig", false), ("connected", true), ("clock", false)]
        );
    }

    #[test]
    fn checks_availability_and_duplicates() {
        let host = setup();
        let mut tools = ShellTools::default();
        let markers = tools.line_markers(&doc("UserCommands.java", LEGACY));
        let tips: Vec<_> = markers.iter().map(|m| (m.line, m.tooltip.as_str())).collect();
        assert_eq!(
            tips,
            [
                (8, "Availability method: addUserAvailability()"),
                (11, "Availability method: canRemove()"),
                (14, "Availability method: always()")
            ]
        );
        assert_eq!(markers[1].target.as_ref().unwrap().range.start, pos(LEGACY, "canRemove", 0));

        assert!(host.diagnostics().is_empty());
        let problems = tools.diagnostics(&doc("UserCommands.java", LEGACY));
        let codes: Vec<_> = problems.iter().map(|p| (p.code.as_deref().unwrap(), p.range.start.line)).collect();
        assert_eq!(codes, [("duplicate-command", 14), ("unknown-command", 18), ("availability-return-type", 19)]);
        assert_eq!(problems[1].range.start, pos(LEGACY, "nope", 0));
    }

    #[test]
    fn checks_availability_providers() {
        let host = setup();
        let mut tools = ShellTools::default();
        let markers = tools.line_markers(&doc("DbCommands.java", MODERN));
        assert_eq!(markers.len(), 1);
        assert_eq!(markers[0].line, 9);
        assert_eq!(markers[0].target.as_ref().unwrap().range.start, Position::new(22, 25));
        assert!(host.diagnostics().is_empty());
        let problems = tools.diagnostics(&doc("DbCommands.java", MODERN));
        let codes: Vec<_> = problems.iter().map(|p| (p.code.as_deref().unwrap(), p.severity)).collect();
        assert!(codes.is_empty(), "External/runtime beans are unknown: {codes:?}");
    }

    #[test]
    fn completes_names() {
        let _host = setup();
        let mut tools = ShellTools::default();
        let labels = |items: Vec<CompletionItem>| items.into_iter().map(|i| i.label).collect::<Vec<_>>();
        let d = doc("UserCommands.java", LEGACY);
        let at = pos(LEGACY, "\"nope\"", 1);
        assert_eq!(labels(tools.completion(&d, at, None)), ["*", "add-user", "list", "remove", "rm"]);
        assert_eq!(labels(tools.completion(&d, pos(LEGACY, "\"Users\"", 1), None)), ["Database", "Users"]);
        let d = doc("DbCommands.java", MODERN);
        assert_eq!(labels(tools.completion(&d, pos(MODERN, "\"missing\"", 1), None)), ["connected"]);
        assert!(tools.completion(&d, pos(MODERN, "\"list\"", 1), None).is_empty());
    }

    #[test]
    fn navigates_from_names() {
        let _host = setup();
        let mut tools = ShellTools::default();
        let found = tools.definition(&doc("UserCommands.java", LEGACY), pos(LEGACY, "\"remove\", \"nope\"", 2));
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].range.start, Position::new(11, 16));
        let found = tools.definition(&doc("DbCommands.java", MODERN), pos(MODERN, "\"connected\"", 3));
        assert_eq!(found[0].range.start, Position::new(22, 25));
    }
}

#[cfg(test)]
mod annotation_owner_safety_regressions {
 use super::*;
 use openidea_plugin::{hover::HoverProvider,mock::MockHost};
 #[test]
 fn aliased_kotlin_annotation_import_does_not_publish_command_hover() {
  let _host=MockHost::install().with_capabilities(&[]);
  let text="import org.springframework.shell.standard.ShellMethod as Other\nclass C { @ShellMethod(key=\"run\",value=\"Unowned description\") fun run(){} }";
  let doc=Document{path:"/project/C.kt".into(),language:"kotlin".into(),version:1,text:text.into()};
  assert!(ShellTools::default().hover(&doc,position(text,text.find("run(){}").unwrap())).is_none());
 }
 #[test]
 fn conflicting_static_alias_and_package_root_owners_are_quiet() {
  let _host=MockHost::install().with_capabilities(&[]);
  for text in [
   "import org.springframework.shell.standard.ShellMethod; import static foreign.Owner.ShellMethod; class C { @ShellMethod(key=\"run\",value=\"Unowned\") void run(){} }",
   "import foreign.org; class C { @org.springframework.shell.standard.ShellMethod(key=\"run\",value=\"Unowned\") void run(){} }",
   "class C<org> { @org.springframework.shell.standard.ShellMethod(key=\"run\",value=\"Unowned\") void run(){} }",
  ] {
   let doc=Document{path:"/project/C.java".into(),language:"java".into(),version:1,text:text.into()};
   assert!(ShellTools::default().hover(&doc,position(text,text.find("run(){}").unwrap())).is_none(),"{text}");
  }
  for text in [
   "import org.springframework.shell.standard.ShellMethod\ntypealias ShellMethod=foreign.Other\nclass C { @ShellMethod(key=\"run\",value=\"Unowned\") fun run(){} }",
   "import org.springframework.shell.standard.ShellMethod\nimport foreign.Other as ShellMethod\nclass C { @ShellMethod(key=\"run\",value=\"Unowned\") fun run(){} }",
  ] {
   let doc=Document{path:"/project/C.kt".into(),language:"kotlin".into(),version:1,text:text.into()};
   assert!(ShellTools::default().hover(&doc,position(text,text.find("run(){}").unwrap())).is_none(),"{text}");
  }
 }
 #[test]
 fn canonical_fqn_annotation_survives_unrelated_short_shadow_and_alias() {
  let _host=MockHost::install().with_capabilities(&[]);
  let text="import org.springframework.shell.standard.ShellMethod as Other\ntypealias ShellMethod=foreign.Other\nclass C { @org.springframework.shell.standard.ShellMethod(key=\"run\",value=\"Owned\") fun run(){} }";
  let doc=Document{path:"/project/C.kt".into(),language:"kotlin".into(),version:1,text:text.into()};
  assert!(ShellTools::default().hover(&doc,position(text,text.find("run(){}").unwrap())).is_some());
 }
 #[test]
 fn generic_shadowed_annotation_name_does_not_publish_command_hover() {
  let _host=MockHost::install().with_capabilities(&[]);
  let text="import org.springframework.shell.standard.ShellMethod; class C<ShellMethod> { @ShellMethod(key=\"run\",value=\"Unowned description\") void run(){} }";
  let doc=Document{path:"/project/C.java".into(),language:"java".into(),version:1,text:text.into()};
  assert!(ShellTools::default().hover(&doc,position(text,text.find("run(){}").unwrap())).is_none());
 }
}

#[cfg(test)]
mod availability_type_owner_regressions {
 use super::*;
 use openidea_plugin::mock::MockHost;
 #[test]
 fn generic_return_type_does_not_become_availability_indicator_candidate() {
  let _host=MockHost::install().with_capabilities(&[]);
  let text="import org.springframework.shell.standard.ShellMethod; import org.springframework.shell.standard.ShellMethodAvailability; import org.springframework.shell.Availability; class C<Availability> { @ShellMethod(key=\"run\",value=\"Run\") @ShellMethodAvailability(\"ga\") void run(){} Availability gate(){return null;} }";
  let doc=Document{path:"/project/C.java".into(),language:"java".into(),version:1,text:text.into()};
  let at=position(text,text.find("\"ga\"").unwrap()+1);
  assert!(ShellTools::default().completion(&doc,at,None).is_empty());
 }
 #[test]
 fn explicit_canonical_availability_return_type_is_a_candidate_despite_short_shadow() {
  let _host=MockHost::install().with_capabilities(&[]);
  let text="import org.springframework.shell.standard.ShellMethod; import org.springframework.shell.standard.ShellMethodAvailability; class C<Availability> { @ShellMethod(key=\"run\",value=\"Run\") @ShellMethodAvailability(\"ga\") void run(){} org.springframework.shell.Availability gate(){return null;} }";
  let doc=Document{path:"/project/C.java".into(),language:"java".into(),version:1,text:text.into()};
  let at=position(text,text.find("\"ga\"").unwrap()+1);
  assert_eq!(ShellTools::default().completion(&doc,at,None).iter().map(|i|i.label.as_str()).collect::<Vec<_>>(),["gate"]);
 }
 #[test]
 fn aliased_return_type_import_does_not_become_availability_indicator_candidate() {
  let _host=MockHost::install().with_capabilities(&[]);
  let text="import org.springframework.shell.standard.ShellMethod\nimport org.springframework.shell.standard.ShellMethodAvailability\nimport org.springframework.shell.Availability as Other\nclass C { @ShellMethod(key=\"run\",value=\"Run\") @ShellMethodAvailability(\"ga\") fun run(){} fun gate(): Availability {return null;} }";
  let doc=Document{path:"/project/C.kt".into(),language:"kotlin".into(),version:1,text:text.into()};
  let at=position(text,text.find("\"ga\"").unwrap()+1);
  assert!(ShellTools::default().completion(&doc,at,None).is_empty());
 }
}

#[cfg(test)]
mod description_literal_safety_regressions {
 use super::*;
 use openidea_plugin::{hover::HoverProvider,mock::MockHost};
 #[test]
 fn escaped_description_values_are_not_guessed_from_partial_lexer_decoding() {
  let _host=MockHost::install().with_capabilities(&[]);
  for value in [r#""\141bc""#,r#""\u0061bc""#, "\"\"\"\n  First\n   Second\n  \"\"\""] {
   let text=format!(r#"import org.springframework.shell.standard.ShellMethod; class C {{ @ShellMethod(key="run",value={value}) void run(){{}} }}"#);
   let doc=Document{path:"/project/C.java".into(),language:"java".into(),version:1,text:text.clone()};
   assert!(ShellTools::default().hover(&doc,position(&text,text.find("run(){}").unwrap())).is_none(),"{text}");
  }
 }
 #[test]
 fn owned_kotlin_interpolated_description_is_not_a_known_declared_description() {
  let _host=MockHost::install().with_capabilities(&[]);
  for text in [
   "import org.springframework.shell.standard.ShellMethod\nclass C { @ShellMethod(key=\"run\",value=\"Hello $name\") fun run(){} }",
   "import org.springframework.shell.command.annotation.Command\n@Command(description=\"Hello ${name}\") class C { @Command(command=\"run\") fun run(){} }",
  ] {
   let doc=Document{path:"/project/C.kt".into(),language:"kotlin".into(),version:1,text:text.into()};
   assert!(ShellTools::default().hover(&doc,position(text,text.find("run(){}").unwrap())).is_none(),"{text}");
  }
 }
}

#[cfg(test)]
mod java_unicode_source_safety_regression {
 use super::*;
 use openidea_plugin::{hover::HoverProvider,mock::MockHost};
 #[test]
 fn unicode_preprocessed_comment_does_not_publish_nonexistent_command_description() {
  let _host=MockHost::install().with_capabilities(&[]);
  let text=r#"import org.springframework.shell.standard.ShellMethod;
class C {
 \u002f\u002a
 @ShellMethod(key="run",value="Not a declaration") void run(){}
 \u002a\u002f
}"#;
  let doc=Document{path:"/project/C.java".into(),language:"java".into(),version:1,text:text.into()};
  let at=position(text,text.find("run(){}").unwrap());
  let mut tools=ShellTools::default();
  assert!(tools.hover(&doc,at).is_none());
  assert!(tools.diagnostics(&doc).is_empty());
  assert!(tools.line_markers(&doc).is_empty());
  assert!(tools.definition(&doc,at).is_empty());
  assert!(tools.completion(&doc,at,None).is_empty());
 }
}

#[cfg(test)]
mod provider_type_owner_regressions {
 use super::*;
 use openidea_plugin::mock::MockHost;
 fn candidates(text:&str,kotlin:bool)->Vec<CompletionItem> {
  let host=MockHost::install();let path=if kotlin{"C.kt"}else{"C.java"};host.add_file(path,text);
  let doc=Document{path:format!("/project/{path}"),language:if kotlin{"kotlin"}else{"java"}.into(),version:1,text:text.into()};
  let mut tools=ShellTools::default();tools.activate(&host.context("aritti.spring-shell")).unwrap();
  tools.completion(&doc,position(text,text.find("\"pa\"").unwrap()+1),None)
 }
 #[test]
 fn generic_availability_provider_return_type_is_not_a_known_provider_bean() {
  let text="import org.springframework.context.annotation.Bean; import org.springframework.shell.AvailabilityProvider; import org.springframework.shell.command.annotation.Command; import org.springframework.shell.command.annotation.CommandAvailability; class C<AvailabilityProvider> { @Bean AvailabilityProvider candidate(){return null;} @Command(command=\"run\") @CommandAvailability(provider=\"pa\") void run(){} }";
  assert!(candidates(text,false).is_empty());
 }
 #[test]
 fn canonical_provider_return_type_survives_short_generic_shadow() {
  let text="import org.springframework.context.annotation.Bean; import org.springframework.shell.command.annotation.Command; import org.springframework.shell.command.annotation.CommandAvailability; class C<AvailabilityProvider> { @Bean org.springframework.shell.AvailabilityProvider candidate(){return null;} @Command(command=\"run\") @CommandAvailability(provider=\"pa\") void run(){} }";
  assert_eq!(candidates(text,false).iter().map(|i|i.label.as_str()).collect::<Vec<_>>(),["candidate"]);
 }
 #[test]
 fn aliased_availability_provider_import_is_not_a_known_provider_bean() {
  let text="import org.springframework.context.annotation.Bean\nimport org.springframework.shell.AvailabilityProvider as Other\nimport org.springframework.shell.command.annotation.Command\nimport org.springframework.shell.command.annotation.CommandAvailability\nclass C { @Bean fun candidate(): AvailabilityProvider {return null;} @Command(command=\"run\") @CommandAvailability(provider=\"pa\") fun run(){} }";
  assert!(candidates(text,true).is_empty());
 }
 #[test]
 fn foreign_or_generic_subclass_return_cannot_borrow_local_provider_interface() {
  for ty in ["foreign.P","P"] {
   let generic=if ty=="P"{"<P>"}else{""};
   let text=format!("import org.springframework.context.annotation.Bean; import org.springframework.shell.command.annotation.Command; import org.springframework.shell.command.annotation.CommandAvailability; abstract class P implements org.springframework.shell.AvailabilityProvider {{}} class C{generic} {{ @Bean {ty} candidate(){{return null;}} @Command(command=\"run\") @CommandAvailability(provider=\"pa\") void run(){{}} }}");
   assert!(candidates(&text,false).is_empty(),"{ty}");
  }
 }

 #[test]
 fn unambiguous_local_provider_return_type_remains_a_candidate() {
  for ty in ["P","com.acme.P"] {
   let text=format!("package com.acme; import org.springframework.context.annotation.Bean; import org.springframework.shell.command.annotation.Command; import org.springframework.shell.command.annotation.CommandAvailability; abstract class P implements org.springframework.shell.AvailabilityProvider {{}} class C {{ @Bean {ty} candidate(){{return null;}} @Command(command=\"run\") @CommandAvailability(provider=\"pa\") void run(){{}} }}");
   assert_eq!(candidates(&text,false).iter().map(|i|i.label.as_str()).collect::<Vec<_>>(),["candidate"],"{ty}");
  }
 }

 #[test]
 fn ambiguous_provider_bean_names_are_not_presented_as_known_candidates() {
  for other in ["String","org.springframework.shell.AvailabilityProvider"] {
   let text=format!("import org.springframework.context.annotation.Bean; import org.springframework.shell.command.annotation.Command; import org.springframework.shell.command.annotation.CommandAvailability; class C {{ @Bean(\"shared\") org.springframework.shell.AvailabilityProvider first(){{return null;}} @Bean(\"shared\") {other} second(){{return null;}} @Command(command=\"run\") @CommandAvailability(provider=\"pa\") void run(){{}} }}");
   assert!(candidates(&text,false).is_empty(),"{other}");
  }
 }

 #[test]
 fn repeated_alias_of_one_physical_provider_is_not_a_second_declaration() {
  let text="import org.springframework.context.annotation.Bean; import org.springframework.shell.command.annotation.Command; import org.springframework.shell.command.annotation.CommandAvailability; class C { @Bean(name={\"shared\",\"shared\"}) org.springframework.shell.AvailabilityProvider first(){return null;} @Command(command=\"run\") @CommandAvailability(provider=\"pa\") void run(){} }";
  assert_eq!(candidates(text,false).iter().map(|i|i.label.as_str()).collect::<Vec<_>>(),["shared"]);
 }

 #[test]
 fn ambiguous_or_unproved_provider_names_do_not_pick_an_arbitrary_gutter_target() {
  for other in ["String","org.springframework.shell.AvailabilityProvider"] {
   let text=format!("import org.springframework.context.annotation.Bean; import org.springframework.shell.command.annotation.Command; import org.springframework.shell.command.annotation.CommandAvailability; class C {{ @Bean(\"shared\") org.springframework.shell.AvailabilityProvider first(){{return null;}} @Bean(\"shared\") {other} second(){{return null;}} @Command(command=\"run\") @CommandAvailability(provider=\"shared\") void run(){{}} }}");
   let host=MockHost::install();host.add_file("C.java",&text);let doc=Document{path:"/project/C.java".into(),language:"java".into(),version:1,text};
   let mut tools=ShellTools::default();tools.activate(&host.context("aritti.spring-shell")).unwrap();
   assert!(tools.line_markers(&doc).is_empty(),"{other}");
   if other=="String" {
    let mut unproved=doc.clone();unproved.version=2;
    unproved.text=unproved.text.replace("@Bean(\"shared\") org.springframework.shell.AvailabilityProvider first(){return null;}","");
    assert!(tools.line_markers(&unproved).is_empty());
   }
  }
 }

}

#[cfg(test)]
mod option_numeric_owner_regressions {
 use super::*;
 use openidea_plugin::mock::MockHost;
 #[test]
 fn aliased_foreign_numeric_type_names_do_not_trigger_builtin_format_diagnostics() {
  let _host=MockHost::install().with_capabilities(&[]);
  for (imports,ty) in [
   ("import java.lang.Integer as Other\nimport foreign.Custom as Integer\n","Integer"),
   ("import foreign.Custom as kotlin\n","kotlin.Int"),
  ] {
   let text=format!("import org.springframework.shell.command.annotation.Command\\nimport org.springframework.shell.command.annotation.Option\\n{imports}@Command class C {{ @Command(command=\\\"run\\\") fun run(@Option(defaultValue=\\\"bad\\\") n:{ty}){{}} }}").replace("\\n","\n").replace("\\\"","\"");
   let doc=Document{path:"/project/C.kt".into(),language:"kotlin".into(),version:1,text};
   assert!(!ShellTools::default().diagnostics(&doc).iter().any(|d|d.code.as_deref()==Some("option-default-format")),"{ty}");
  }
 }
}

#[cfg(test)]
mod availability_wrong_type_shadow_regressions {
 use super::*;
 use openidea_plugin::mock::MockHost;
 #[test]
 fn availability_bounded_generic_named_string_is_not_java_lang_string_return() {
  let _host=MockHost::install().with_capabilities(&[]);
  for (class_generic,method_generic) in [("<String extends org.springframework.shell.Availability>",""),("","<String extends org.springframework.shell.Availability> ")] {
   let text=format!("import org.springframework.shell.standard.ShellMethod; import org.springframework.shell.standard.ShellMethodAvailability; @org.springframework.shell.standard.ShellComponent class C{class_generic} {{ @ShellMethod(key=\"run\",value=\"Run\") void run(){{}} @ShellMethodAvailability(\"run\") {method_generic}String gate(){{return null;}} }}");
   let doc=Document{path:"/project/C.java".into(),language:"java".into(),version:1,text};
   assert!(!ShellTools::default().diagnostics(&doc).iter().any(|d|d.code.as_deref()==Some("availability-return-type")),"{class_generic}/{method_generic}");
   let exact=Document{text:doc.text.replace("String gate()","java.lang.String gate()"),..doc};
   assert!(ShellTools::default().diagnostics(&exact).iter().any(|d|d.code.as_deref()==Some("availability-return-type")));
  }
 }
}
