//! Projection declaration conventions (Tools > JPA Buddy > Projection Declaration) and the actions that
//! keep an entity and its linked projections or DTOs in step.
//!
//! A projection or DTO is linked to its entity either by its type name (`projectionNameRegexp`, an
//! `(?<entity>...)` group) or by a Javadoc comment above it (`projectionCommentLinkRegexp`). Linked
//! types are found by a bounded scan of the project sources when an action runs, never in a provider.
//! Supported linked shapes: Java closed interface, Java record, Kotlin interface, Kotlin data class.
//! Plain DTO classes (fields plus accessors) are not parsed and are never edited.
use super::*;
use crate::model::{Edit, closing, import_edit, split_top_level};
use regex_lite::Regex;

pub(super) const SYNC_TITLE: &str = "Synchronize linked projections with this entity…";
pub(super) const RENAME_TITLE: &str = "Rename attribute and update linked projections…";
pub(super) const SYNC_DTO_TITLE: &str = "Synchronize linked DTOs with this entity…";
pub(super) const RENAME_DTO_TITLE: &str = "Rename attribute and update linked DTOs…";
const DEFAULT_NAME_REGEXP: &str = "(?<entity>.*)Projection";
const DEFAULT_DTO_NAME_REGEXP: &str = "(?<entity>.*)Dto";

/// The two declaration families that share the linking, sync and rename machinery; they differ only in settings and wording.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Kind {
    Projection,
    Dto,
}

impl Kind {
    fn noun(self) -> &'static str {
        match self {
            Self::Projection => "projections",
            Self::Dto => "DTOs",
        }
    }

    fn settings(self) -> [&'static str; 4] {
        match self {
            Self::Projection => ["projectionClassPostfix", "projectionNameRegexp", "projectionClassComment", "projectionCommentLinkRegexp"],
            Self::Dto => ["dtoClassPostfix", "dtoNameRegexp", "dtoClassComment", "dtoCommentLinkRegexp"],
        }
    }

    fn defaults(self) -> (&'static str, &'static str) {
        match self {
            Self::Projection => ("Projection", DEFAULT_NAME_REGEXP),
            Self::Dto => ("Dto", DEFAULT_DTO_NAME_REGEXP),
        }
    }
}
const SCAN_LIMIT: u32 = 1001;

pub(crate) struct Conventions {
    pub(super) postfix: String,
    name: Option<Regex>,
    comment: String,
    link: Option<Regex>,
}

fn pattern(setting: &str, default: &str, anchored: bool) -> Result<Option<Regex>, String> {
    let value = host::setting_string(setting).unwrap_or_else(|| default.to_owned());
    if value.is_empty() {
        return Ok(None);
    }
    let source = if anchored { format!("^(?:{value})$") } else { value.clone() };
    let regex = Regex::new(&source).map_err(|error| format!("{setting} is not a valid regular expression: {error}"))?;
    if !regex.capture_names().flatten().any(|name| name == "entity") {
        return Err(format!("{setting} needs an (?<entity>...) group."));
    }
    Ok(Some(regex))
}

impl Conventions {
    pub(super) fn load() -> Result<Self, String> {
        Self::load_for(Kind::Projection)
    }

    pub(crate) fn load_for(kind: Kind) -> Result<Self, String> {
        let [postfix_key, name_key, comment_key, link_key] = kind.settings();
        let (default_postfix, default_regexp) = kind.defaults();
        let postfix = host::setting_string(postfix_key).unwrap_or_else(|| default_postfix.into());
        if !postfix.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '_') {
            return Err(format!("{postfix_key} may contain only letters, digits and underscores."));
        }
        let comment = host::setting_string(comment_key).unwrap_or_default();
        if comment.contains("*/") || comment.len() > 1024 {
            return Err(format!("{comment_key} must be shorter than 1 KiB and must not contain `*/`."));
        }
        Ok(Self {
            postfix,
            name: pattern(name_key, default_regexp, true)?,
            comment,
            link: pattern(link_key, "", false)?,
        })
    }

    /// The simple entity name a type named `stem` is linked to, by name pattern first, then by the first Javadoc in `text`.
    pub(crate) fn entity_of(&self, stem: &str, text: &str) -> Option<String> {
        let by_name = self.name.as_ref().and_then(|regex| regex.captures(stem)).and_then(|caps| caps.name("entity").map(|m| m.as_str().to_owned()));
        if by_name.is_some() {
            return by_name;
        }
        let start = text.find("/**")?;
        let comment = &text[start..start + text[start..].find("*/")? + 2];
        let found = self.link.as_ref()?.captures_iter(comment).find_map(|caps| caps.name("entity").map(|m| m.as_str().trim().to_owned()))?;
        found.rsplit('.').next().map(str::to_owned)
    }

    pub(super) fn default_name(&self, entity: &str) -> String {
        format!("{entity}{}", self.postfix)
    }

    /// The configured class comment above the type declaration; `{entity}` is the entity's qualified name,
    /// `{entityName}` its simple name.
    pub(super) fn with_comment(&self, text: &str, entity_package: Option<&str>, entity: &str) -> String {
        if self.comment.trim().is_empty() {
            return text.to_owned();
        }
        let qualified = entity_package.map_or_else(|| entity.to_owned(), |package| format!("{package}.{entity}"));
        let filled = self.comment.replace("{entityName}", entity).replace("{entity}", &qualified);
        let lines: Vec<_> = filled.lines().collect();
        let block = match lines.as_slice() {
            [one] => format!("/** {} */\n", one.trim()),
            many => format!("/**\n{} */\n", many.iter().map(|line| format!(" * {}\n", line.trim_end())).collect::<String>()),
        };
        let mut offset = 0;
        for line in text.split_inclusive('\n') {
            let trimmed = line.trim();
            if !trimmed.is_empty() && !trimmed.starts_with("package ") && !trimmed.starts_with("import ") {
                return format!("{}{block}{}", &text[..offset], &text[offset..]);
            }
            offset += line.len();
        }
        text.to_owned()
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Shape {
    JavaInterface,
    Record,
    KotlinInterface,
    DataClass,
}

pub(super) struct Prop {
    pub(super) name: String,
    pub(super) ty: String,
    decl: (usize, usize),
    name_span: (usize, usize),
}

pub(super) struct Linked {
    pub(super) path: String,
    pub(super) text: String,
    pub(super) name: String,
    package: Option<String>,
    kotlin: bool,
    shape: Shape,
    pub(super) props: Vec<Prop>,
    list: (usize, usize),
    imports: Vec<String>,
}

pub(super) enum Change {
    Add { name: String, ty: String },
    Remove(String),
    Rename { old: String, new: String },
}

fn token_after(masked: &str, keyword: &str, name: &str) -> Option<usize> {
    words(masked, keyword).find_map(|at| {
        let rest = masked[at + keyword.len()..].trim_start();
        let after = rest.strip_prefix(name)?;
        after.chars().next().is_none_or(|ch| !ch.is_alphanumeric() && ch != '_' && ch != '$').then(|| at + keyword.len() + (masked[at + keyword.len()..].len() - rest.len()) + name.len())
    })
}

fn javadoc_above(text: &str, masked: &str, declaration: usize) -> Option<String> {
    let line_start = masked[..declaration].rfind('\n').map_or(0, |at| at + 1);
    let head = &text[..line_start.min(declaration)];
    let end = head.rfind("*/")?;
    let start = head[..end].rfind("/**")?;
    let between = &masked[end + 2..declaration];
    (!between.contains(['{', ';', '}'])).then(|| text[start..end + 2].to_owned())
}

fn bean_property(method: &str) -> Option<(String, &'static str)> {
    let (prefix, rest) = if let Some(rest) = method.strip_prefix("get") { ("get", rest) } else { ("is", method.strip_prefix("is")?) };
    let first = rest.chars().next().filter(char::is_ascii_uppercase)?;
    let second_upper = rest.chars().nth(1).is_some_and(|ch| ch.is_ascii_uppercase());
    let name = if second_upper { rest.to_owned() } else { format!("{}{}", first.to_ascii_lowercase(), &rest[1..]) };
    Some((name, prefix))
}

/// The linked shape of `text`, `Ok(None)` when the file is not a type named `stem`.
pub(super) fn parse(path: &str, text: &str, kotlin: bool) -> Result<Option<(Linked, Option<String>)>, String> {
    let stem = path.rsplit('/').next().and_then(|file| file.rsplit_once('.')).map(|(stem, _)| stem).unwrap_or_default();
    let masked = mask(text);
    if annotations(&masked).iter().any(|a| a.name == "Entity") {
        return Ok(None);
    }
    let declaration = if kotlin {
        token_after(&masked, "interface", stem).map(|at| (Shape::KotlinInterface, at)).or_else(|| {
            words(&masked, "data").next().and_then(|_| token_after(&masked, "class", stem)).map(|at| (Shape::DataClass, at))
        })
    } else {
        token_after(&masked, "interface", stem).map(|at| (Shape::JavaInterface, at)).or_else(|| token_after(&masked, "record", stem).map(|at| (Shape::Record, at)))
    };
    let Some((shape, after_name)) = declaration else {
        return Ok(None);
    };
    let keyword_at = masked[..after_name].rfind(stem).unwrap_or(after_name);
    let comment = javadoc_above(text, &masked, keyword_at);
    let package = masked.lines().find_map(|line| line.trim().strip_prefix("package ")).map(|p| p.trim().trim_end_matches(';').trim().to_owned());
    let imports = masked.lines().filter_map(|line| line.trim().strip_prefix("import ")).map(|i| i.trim().trim_end_matches(';').trim().to_owned()).collect();
    let unsupported = |why: &str| format!("{stem}: {why}");
    let bytes = masked.as_bytes();
    let mut props = Vec::new();
    let list;
    match shape {
        Shape::JavaInterface | Shape::KotlinInterface => {
            let open = masked[after_name..].find('{').map(|at| at + after_name).ok_or_else(|| unsupported("the interface has no body"))?;
            let close = closing(bytes, open).ok_or_else(|| unsupported("the interface body is not closed"))?;
            list = (open + 1, close);
            let member = if shape == Shape::JavaInterface {
                Regex::new(r"^([\w.<>\[\]?, ]+?)\s+((?:get|is)[A-Z]\w*)\s*\(\s*\)$").unwrap()
            } else {
                Regex::new(r"^val\s+(\w+)\s*:\s*([\w.<>?, ]+?)$").unwrap()
            };
            let mut from = open + 1;
            let mut cut = |slice_start: usize, slice_end: usize| -> Result<(), String> {
                let segment = &masked[slice_start..slice_end];
                let trimmed = segment.trim();
                if trimmed.is_empty() {
                    return Ok(());
                }
                let lead = slice_start + segment.len() - segment.trim_start().len();
                let caps = member.captures(trimmed).ok_or_else(|| unsupported(&format!("member `{trimmed}` is not a plain property accessor")))?;
                if shape == Shape::JavaInterface {
                    let method = caps.get(2).unwrap();
                    let (name, _) = bean_property(method.as_str()).ok_or_else(|| unsupported("accessor name is not a bean property"))?;
                    props.push(Prop { name, ty: caps[1].trim().to_owned(), decl: (lead, lead + trimmed.len() + 1), name_span: (lead + method.start(), lead + method.end()) });
                } else {
                    let name = caps.get(1).unwrap();
                    props.push(Prop { name: name.as_str().to_owned(), ty: caps[2].trim().trim_end_matches('?').to_owned(), decl: (lead, lead + trimmed.len()), name_span: (lead + name.start(), lead + name.end()) });
                }
                Ok(())
            };
            if shape == Shape::JavaInterface {
                for (at, ch) in masked[open + 1..close].char_indices().filter(|(_, ch)| *ch == ';') {
                    let _ = ch;
                    cut(from, open + 1 + at)?;
                    from = open + 1 + at + 1;
                }
                if !masked[from..close].trim().is_empty() {
                    return Err(unsupported("trailing member without `;`"));
                }
            } else {
                for line in masked[open + 1..close].split_inclusive('\n') {
                    cut(from, from + line.trim_end_matches('\n').len())?;
                    from += line.len();
                }
            }
        }
        Shape::Record | Shape::DataClass => {
            let open = masked[after_name..].find('(').map(|at| at + after_name).ok_or_else(|| unsupported("no parameter list"))?;
            let close = closing(bytes, open).ok_or_else(|| unsupported("the parameter list is not closed"))?;
            list = (open + 1, close);
            for (offset, part) in split_top_level(&masked[open + 1..close]) {
                let lead = open + 1 + offset + part.len() - part.trim_start().len();
                let part = part.trim();
                if part.is_empty() {
                    continue;
                }
                if shape == Shape::Record {
                    let (ty, name) = part.rsplit_once(char::is_whitespace).ok_or_else(|| unsupported("unreadable record component"))?;
                    props.push(Prop { name: name.to_owned(), ty: ty.trim().to_owned(), decl: (lead, lead + part.len()), name_span: (lead + part.len() - name.len(), lead + part.len()) });
                } else {
                    let caps = Regex::new(r"^(?:val|var)\s+(\w+)\s*:\s*(.+?)$").unwrap().captures(part).ok_or_else(|| unsupported("unreadable constructor property"))?;
                    let name = caps.get(1).unwrap();
                    props.push(Prop { name: name.as_str().to_owned(), ty: caps[2].trim().trim_end_matches('?').to_owned(), decl: (lead, lead + part.len()), name_span: (lead + name.start(), lead + name.end()) });
                }
            }
        }
    }
    let linked = Linked { path: path.to_owned(), text: text.to_owned(), name: stem.to_owned(), package, kotlin, shape, props, list, imports };
    Ok(Some((linked, comment)))
}

/// Linked projections of `entity`, plus `file: reason` for linked types that cannot be edited.
pub(super) fn linked_types(project: &Project, root: &str, conventions: &Conventions, entity: &Class, own_path: &str) -> Result<(Vec<Linked>, Vec<String>), Error> {
    let qualified = entity.package.as_deref().map_or_else(|| entity.name.clone(), |package| format!("{package}.{}", entity.name));
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut found = Vec::new();
    let mut skipped = Vec::new();
    let extension = if entity.kotlin { "kt" } else { "java" };
    let files = project.find_files(&format!("**/*.{extension}"), SCAN_LIMIT)?;
    if files.len() >= SCAN_LIMIT as usize {
        return Err("The project has too many sources to discover linked projections.".into());
    }
    let (mut mapped, mut scanned) = (Vec::new(), Vec::new());
    for file in files {
        if Instant::now() >= deadline {
            return Err("Linked projection discovery timed out.".into());
        }
        let file = if file.starts_with('/') { file } else { format!("{}/{file}", root.trim_end_matches('/')) };
        scanned.push(file.clone());
        if file == own_path || path(&file).as_deref() != Some(file.as_str()) || !inside(root, &file) {
            continue;
        }
        let stem = file.rsplit('/').next().and_then(|name| name.rsplit_once('.')).map(|(stem, _)| stem).unwrap_or_default();
        let by_name = conventions.name.as_ref().and_then(|regex| regex.captures(stem)).and_then(|caps| caps.name("entity").map(|m| m.as_str() == entity.name)).unwrap_or(false);
        let text = project.read_file(&file)?;
        if text.len() > MAX_TEXT {
            continue;
        }
        if !entity.kotlin {
            for name in mapstruct_links::dto_names(&text, &qualified) {
                if !mapped.contains(&name) {
                    mapped.push(name);
                }
            }
        }
        let by_comment_possible = conventions.link.is_some() && text.contains("/**");
        if !by_name && !by_comment_possible {
            continue;
        }
        let parsed = match parse(&file, &text, entity.kotlin) {
            Ok(Some(parsed)) => parsed,
            Ok(None) => {
                if by_name && !text.contains("@Entity") {
                    skipped.push(format!("{stem}: not an interface, record or data class"));
                }
                continue;
            }
            Err(reason) if by_name => {
                skipped.push(reason);
                continue;
            }
            Err(_) => continue,
        };
        let (linked, comment) = parsed;
        let by_comment = conventions.link.as_ref().zip(comment.as_deref()).is_some_and(|(regex, comment)| {
            regex.captures_iter(comment).filter_map(|caps| caps.name("entity").map(|m| m.as_str().trim().to_owned())).any(|value| value == qualified || value == entity.name)
        });
        if by_name || by_comment {
            found.push(linked);
        }
    }
    let known: Vec<String> = found.iter().map(|item| item.path.clone()).collect();
    let (by_mapper, unreadable) = mapstruct_links::extra_linked(project, &scanned, &mapped, own_path, &known)?;
    found.extend(by_mapper);
    skipped.extend(unreadable);
    found.sort_by(|a, b| a.path.cmp(&b.path));
    Ok((found, skipped))
}

fn simple_type(ty: &str) -> &str {
    ty.split('<').next().unwrap_or(ty).trim_end_matches('?').trim_end_matches("[]").rsplit('.').next().unwrap_or(ty)
}

impl Linked {
    pub(super) fn in_list(&self, at: usize) -> bool {
        at >= self.list.0 && at <= self.list.1
    }

    pub(super) fn is_interface(&self) -> bool {
        matches!(self.shape, Shape::JavaInterface | Shape::KotlinInterface)
    }

    pub(super) fn has_property(&self, name: &str) -> bool {
        self.props.iter().any(|prop| prop.name == name)
    }

    /// The declaration of a new property as it is written inside this type's member or parameter list.
    pub(super) fn member(&self, name: &str, ty: &str) -> String {
        match self.shape {
            Shape::Record => format!("{ty} {name}"),
            Shape::JavaInterface => format!("{ty} {}();", projection_action::getter(name, ty)),
            Shape::KotlinInterface | Shape::DataClass => format!("val {name}: {}?", ty.trim_end_matches('?')),
        }
    }

    pub(super) fn import_for(&self, entity: &Class, ty: &str) -> Option<String> {
        self.import_for_added(entity, ty)
    }

    fn import_for_added(&self, entity: &Class, ty: &str) -> Option<String> {
        let simple = simple_type(ty);
        let import = entity.imports.iter().find(|import| import.rsplit('.').next() == Some(simple) && !import.ends_with('*'))?;
        (self.package.as_deref() != import.rsplit_once('.').map(|(package, _)| package)).then(|| import.clone())
    }

    pub(super) fn edits(&self, entity: &Class, changes: &[Change]) -> Result<Vec<Edit>, String> {
        let text = &self.text;
        let mut edits = Vec::new();
        let mut imports = Vec::new();
        for change in changes {
            match change {
                Change::Add { name, ty } => {
                    if self.props.iter().any(|prop| &prop.name == name) {
                        return Err(format!("{} already has `{name}`.", self.name));
                    }
                    imports.extend(self.import_for_added(entity, ty));
                }
                Change::Remove(name) | Change::Rename { old: name, .. } => {
                    if !self.props.iter().any(|prop| &prop.name == name) {
                        return Err(format!("{} has no property `{name}`.", self.name));
                    }
                }
            }
            if let Change::Rename { new, .. } = change {
                if self.props.iter().any(|prop| &prop.name == new) {
                    return Err(format!("{} already has `{new}`; no property is renamed onto it.", self.name));
                }
            }
        }
        match self.shape {
            Shape::Record | Shape::DataClass => {
                let mut parts: Vec<(String, String)> = self.props.iter().map(|prop| (prop.name.clone(), text[prop.decl.0..prop.decl.1].to_owned())).collect();
                for change in changes {
                    match change {
                        Change::Remove(name) => parts.retain(|(n, _)| n != name),
                        Change::Rename { old, new } => {
                            let prop = self.props.iter().find(|prop| &prop.name == old).unwrap();
                            let declared = &text[prop.decl.0..prop.decl.1];
                            let local = (prop.name_span.0 - prop.decl.0, prop.name_span.1 - prop.decl.0);
                            let renamed = format!("{}{new}{}", &declared[..local.0], &declared[local.1..]);
                            if let Some(part) = parts.iter_mut().find(|(n, _)| n == old) {
                                *part = (new.clone(), renamed);
                            }
                        }
                        Change::Add { name, ty } => {
                            let declared = if self.shape == Shape::Record { format!("{ty} {name}") } else { format!("val {name}: {}?", ty.trim_end_matches('?')) };
                            parts.push((name.clone(), declared));
                        }
                    }
                }
                let inner = &text[self.list.0..self.list.1];
                let rebuilt = if parts.is_empty() {
                    String::new()
                } else if inner.contains('\n') {
                    let first = self.props.first().map_or(self.list.0, |prop| prop.decl.0);
                    let line_start = text[..first].rfind('\n').map_or(0, |at| at + 1);
                    let indent = if line_start >= self.list.0 { &text[line_start..first] } else { "        " };
                    let closing_indent = inner.rsplit_once('\n').map_or("", |(_, last)| last);
                    let crlf = if text.contains("\r\n") { "\r\n" } else { "\n" };
                    format!("{crlf}{indent}{}{crlf}{closing_indent}", parts.iter().map(|(_, part)| part.as_str()).collect::<Vec<_>>().join(&format!(",{crlf}{indent}")))
                } else {
                    parts.iter().map(|(_, part)| part.as_str()).collect::<Vec<_>>().join(", ")
                };
                edits.push(Edit { start: self.list.0, end: self.list.1, text: rebuilt });
            }
            Shape::JavaInterface | Shape::KotlinInterface => {
                let crlf = text.contains("\r\n");
                for change in changes {
                    match change {
                        Change::Remove(name) => {
                            let prop = self.props.iter().find(|prop| &prop.name == name).unwrap();
                            let line_start = text[..prop.decl.0].rfind('\n').map_or(0, |at| at + 1);
                            let line_end = text[prop.decl.1..].find('\n').map_or(text.len(), |at| prop.decl.1 + at + 1);
                            let alone = text[line_start..prop.decl.0].trim().is_empty() && text[prop.decl.1..line_end].trim().is_empty();
                            let (start, end) = if alone { (line_start, line_end) } else { prop.decl };
                            edits.push(Edit { start, end, text: String::new() });
                        }
                        Change::Rename { old, new } => {
                            let prop = self.props.iter().find(|prop| &prop.name == old).unwrap();
                            let replacement = if self.shape == Shape::JavaInterface {
                                let method = &text[prop.name_span.0..prop.name_span.1];
                                let prefix = if method.starts_with("is") { "is" } else { "get" };
                                format!("{prefix}{}", crate::model::capitalize(new))
                            } else {
                                new.clone()
                            };
                            edits.push(Edit { start: prop.name_span.0, end: prop.name_span.1, text: replacement });
                        }
                        Change::Add { name, ty } => {
                            let member = if self.shape == Shape::JavaInterface {
                                format!("    {ty} {}();", projection_action::getter(name, ty))
                            } else {
                                format!("    val {name}: {}?", ty.trim_end_matches('?'))
                            };
                            let close = self.list.1;
                            let line_start = text[..close].rfind('\n').map_or(0, |at| at + 1);
                            let own_line = text[line_start..close].trim().is_empty();
                            let (at, body) = if own_line { (line_start, format!("{member}\n")) } else { (close, format!("\n{member}\n")) };
                            edits.push(Edit { start: at, end: at, text: if crlf { body.replace('\n', "\r\n") } else { body } });
                        }
                    }
                }
            }
        }
        let refs: Vec<&str> = imports.iter().map(String::as_str).collect();
        edits.extend(import_edit(text, &refs, self.kotlin));
        Ok(edits)
    }
}

fn apply(edits: &[Edit], text: &str) -> Result<(Vec<TextEdit>, String), Error> {
    let converted: Vec<_> = edits.iter().map(|edit| edit.to_text_edit(text)).collect();
    let after = patched(text, &converted)?;
    Ok((converted, after))
}

fn entity_properties(class: &Class, interface_form: bool) -> Vec<(String, String)> {
    projection_action::offered(class, if interface_form { projection_action::Form::Interface } else { projection_action::Form::Record })
}

fn skipped_note(skipped: &[String]) -> String {
    if skipped.is_empty() { String::new() } else { format!("\nLinked but not edited: {}", skipped.join("; ")) }
}

fn no_linked_message(kind: Kind) -> String {
    let [_, name, _, link] = kind.settings();
    match kind {
        Kind::Projection => format!("No linked projection or DTO was found. A type is linked by a name matching {name} (an empty value disables it) or by a Javadoc comment matching {link}."),
        Kind::Dto => format!("No linked DTO was found. A type is linked by a name matching {name} (an empty value disables it) or by a Javadoc comment matching {link}."),
    }
}

fn verify_unchanged(project: &Project, linked: &[&Linked]) -> Result<(), Error> {
    for item in linked {
        if project.read_file(&item.path)? != item.text {
            return Err(Error::Stale);
        }
    }
    Ok(())
}

fn qualified_name(package: Option<&str>, name: &str) -> String {
    package.map_or_else(|| name.to_owned(), |package| format!("{package}.{name}"))
}

/// Per linked type, the `(entity attribute, property)` pairs that MapStruct mappers match by `@Mapping`; none when mappers cannot be read.
fn mapped_pairs(project: &Project, root: &str, entity: &Class, linked: &[Linked]) -> Vec<Vec<(String, String)>> {
    let sources = mapstruct_links::java_sources(project, root).and_then(|files| mapstruct_links::mappers(project, &files)).unwrap_or_default();
    let entity_name = qualified_name(entity.package.as_deref(), &entity.name);
    linked.iter().map(|item| mapstruct_links::correspondences(&sources, &entity_name, &qualified_name(item.package.as_deref(), &item.name))).collect()
}

pub(super) fn rename_plan_for(project: &Project, root: &str, entity: &Class, linked: &[Linked], old: &str, new: &str) -> mapstruct_links::Plan {
    let mut renames = vec![(qualified_name(entity.package.as_deref(), &entity.name), old.to_owned(), new.to_owned())];
    renames.extend(linked.iter().filter(|item| item.has_property(old)).map(|item| (qualified_name(item.package.as_deref(), &item.name), old.to_owned(), new.to_owned())));
    mapstruct_links::rename_plan(project, root, &renames)
}

pub(super) fn resolve_sync(pending: &Pending, doc: &Document, project: &Project) -> Result<FileAction, Error> {
    resolve_sync_of(pending, doc, project, Kind::Projection)
}

pub(super) fn resolve_sync_of(pending: &Pending, doc: &Document, project: &Project, kind: Kind) -> Result<FileAction, Error> {
    current(pending, doc)?;
    let class = dto_action::source(doc).ok_or(Error::Stale)?;
    let conventions = Conventions::load_for(kind)?;
    let (linked, skipped) = linked_types(project, &pending.root, &conventions, &class, &doc.path)?;
    if linked.is_empty() {
        return Err(format!("{}{}", no_linked_message(kind), skipped_note(&skipped)).into());
    }
    let (to_linked, noun) = (if kind == Kind::Dto { "toDtos" } else { "toProjections" }, kind.noun());
    let pairs = mapped_pairs(project, &pending.root, &class, &linked);
    let renamed_by_mapping = |index: usize, attribute: &str| pairs[index].iter().any(|(a, d)| a == attribute && linked[index].has_property(d));
    let owned_by_mapping = |index: usize, property: &str| pairs[index].iter().any(|(a, d)| d == property && class.attributes.iter().any(|attribute| &attribute.name == a));
    let directions = [
        PickItem::new(to_linked, &format!("Entity to {noun}: add or remove properties in linked types")),
        PickItem::new("toEntity", &format!("{} to entity: add attributes the entity lacks", crate::model::capitalize(noun))),
    ];
    let direction = host::pick("Synchronization direction", &directions)?.ok_or(Error::Cancelled)?;
    let mut items = Vec::new();
    let mut plan: Vec<(usize, Change)> = Vec::new();
    match direction.as_str() {
        key if key == to_linked => {
            for (index, item) in linked.iter().enumerate() {
                let interface = matches!(item.shape, Shape::JavaInterface | Shape::KotlinInterface);
                let wanted = entity_properties(&class, interface);
                for (name, ty) in &wanted {
                    if !item.props.iter().any(|prop| &prop.name == name) && !renamed_by_mapping(index, name) {
                        items.push(PickItem::new(&format!("add:{index}:{name}"), &format!("Add {name} to {}", item.name)).description(ty));
                        plan.push((index, Change::Add { name: name.clone(), ty: ty.clone() }));
                    }
                }
                for prop in &item.props {
                    if !class.attributes.iter().any(|attribute| attribute.name == prop.name) && !owned_by_mapping(index, &prop.name) {
                        items.push(PickItem::new(&format!("remove:{index}:{}", prop.name), &format!("Remove {} from {}", prop.name, item.name)).description("no longer an attribute of the entity"));
                        plan.push((index, Change::Remove(prop.name.clone())));
                    }
                }
            }
        }
        "toEntity" => {
            let mut seen: Vec<(String, String)> = Vec::new();
            for (index, item) in linked.iter().enumerate() {
                for prop in &item.props {
                    if class.attributes.iter().any(|attribute| attribute.name == prop.name) || owned_by_mapping(index, &prop.name) {
                        continue;
                    }
                    if let Some((_, other)) = seen.iter().find(|(name, _)| *name == prop.name) {
                        if *other != prop.ty {
                            return Err(format!("Linked types declare `{}` with different types ({other}, {}).", prop.name, prop.ty).into());
                        }
                        continue;
                    }
                    seen.push((prop.name.clone(), prop.ty.clone()));
                    items.push(PickItem::new(&format!("entity:{index}:{}", prop.name), &format!("Add {} to {}", prop.name, class.name)).description(&format!("{} from {}", prop.ty, item.name)));
                    plan.push((index, Change::Add { name: prop.name.clone(), ty: prop.ty.clone() }));
                }
            }
        }
        _ => return Err("Choose one of the offered directions.".into()),
    }
    if items.is_empty() {
        return Err(format!("The entity and its {} linked type(s) are already in sync.{}", linked.len(), skipped_note(&skipped)).into());
    }
    let chosen = host::pick_many("Changes to apply", &items)?.ok_or(Error::Cancelled)?;
    let mut picked = Vec::new();
    for key in &chosen {
        let position = items.iter().position(|item| item.key == *key).ok_or("Choose offered changes, each once.")?;
        if picked.contains(&position) {
            return Err("Choose offered changes, each once.".into());
        }
        picked.push(position);
    }
    if picked.is_empty() {
        return Err("Choose at least one change.".into());
    }
    current(pending, doc)?;
    let mut documents: Vec<(String, Vec<TextEdit>)> = Vec::new();
    let mut preview = String::new();
    if direction == "toEntity" {
        let mut fields = String::new();
        let mut imports = Vec::new();
        for &position in &picked {
            let (index, Change::Add { name, ty }) = &plan[position] else { continue };
            let item = &linked[*index];
            fields.push_str(&if class.kotlin { format!("    var {name}: {}? = null\n", ty.trim_end_matches('?')) } else { format!("    private {ty} {name};\n") });
            let simple = simple_type(ty);
            imports.extend(item.imports.iter().find(|import| import.rsplit('.').next() == Some(simple) && !import.ends_with('*')).cloned());
        }
        let close = class.body_end.ok_or("The entity has no class body to add attributes to.")?;
        let line_start = doc.text[..close].rfind('\n').map_or(0, |at| at + 1);
        let at = if doc.text[line_start..close].trim().is_empty() { line_start } else { close };
        let mut edits = vec![Edit::insert(at, &fields)];
        let refs: Vec<&str> = imports.iter().map(String::as_str).collect();
        edits.extend(import_edit(&doc.text, &refs, class.kotlin));
        let (converted, after) = apply(&edits, &doc.text)?;
        preview.push_str(&format!("Edited entity {}:\n{after}\n", doc.path));
        documents.push((doc.path.clone(), converted));
    } else {
        for (index, item) in linked.iter().enumerate() {
            let changes: Vec<_> = picked.iter().filter(|&&position| plan[position].0 == index).map(|&position| match &plan[position].1 {
                Change::Add { name, ty } => Change::Add { name: name.clone(), ty: ty.clone() },
                Change::Remove(name) => Change::Remove(name.clone()),
                Change::Rename { old, new } => Change::Rename { old: old.clone(), new: new.clone() },
            }).collect();
            if changes.is_empty() {
                continue;
            }
            let (converted, after) = apply(&item.edits(&class, &changes)?, &item.text)?;
            preview.push_str(&format!("Edited {}:\n{after}\n", item.path));
            documents.push((item.path.clone(), converted));
        }
    }
    let matched = pairs.iter().flatten().filter(|(a, d)| a != d).map(|(a, d)| format!("{a} = {d}")).collect::<Vec<_>>();
    let matched = if matched.is_empty() { String::new() } else { format!("\nMatched through MapStruct @Mapping, not added or removed: {}\n", matched.join(", ")) };
    if !ui_kit::prompts::confirm_exact(
        &format!("Synchronize entity and linked {noun}?"),
        &format!("Entity: {}\nSource version: {}\n{}{}{matched}\n\n{preview}Only the listed changes are applied; nothing is renamed or removed from the entity. Plain DTO classes are not parsed.", doc.path, doc.version, linked.iter().map(|l| format!("Linked: {}\n", l.path)).collect::<String>(), skipped_note(&skipped)),
    )? {
        return Err(Error::Cancelled);
    }
    let project = current(pending, doc)?;
    verify_unchanged(&project, &linked.iter().collect::<Vec<_>>())?;
    Ok(FileAction {
        title: format!("Synchronize {} with linked {noun}", class.name),
        kind: Some("refactor.rewrite".into()),
        edits: documents,
        ..FileAction::default()
    })
}

pub(super) fn resolve_rename(pending: &Pending, doc: &Document, project: &Project) -> Result<FileAction, Error> {
    resolve_rename_of(pending, doc, project, Kind::Projection)
}

pub(super) fn resolve_rename_of(pending: &Pending, doc: &Document, project: &Project, kind: Kind) -> Result<FileAction, Error> {
    let class = dto_action::source(doc).ok_or(Error::Stale)?;
    let conventions = Conventions::load_for(kind)?;
    let noun = kind.noun();
    let (linked, skipped) = linked_types(project, &pending.root, &conventions, &class, &doc.path)?;
    if linked.is_empty() {
        return Err(format!("{}{}", no_linked_message(kind), skipped_note(&skipped)).into());
    }
    let renamed = std::cell::RefCell::new(None::<(String, String)>);
    let mapper_plan = std::cell::RefCell::new(None::<mapstruct_links::Plan>);
    let mut action = attribute_refactor::resolve_with(
        pending,
        doc,
        |old| {
            let new = host::input(&format!("New attribute name (also renames the property in linked {noun})"), Some(old))?;
            if let Some(new) = &new {
                *renamed.borrow_mut() = Some((old.to_owned(), new.trim().to_owned()));
            }
            Ok(new.map(|new| new.trim().to_owned()))
        },
        |preview| {
            let Some((old, new)) = renamed.borrow().clone() else { return Ok(false) };
            let mut extra = String::new();
            for item in linked.iter().filter(|item| item.props.iter().any(|prop| prop.name == old)) {
                let edits = item.edits(&class, &[Change::Rename { old: old.clone(), new: new.clone() }]).map_err(Error::from)?;
                let (_, after) = apply(&edits, &item.text)?;
                extra.push_str(&format!("\nLinked type {}:\n{after}", item.path));
            }
            if extra.is_empty() {
                extra = format!("\nNo linked type declares `{old}`.{}", skipped_note(&skipped));
            }
            let plan = rename_plan_for(project, &pending.root, &class, &linked, &old, &new);
            extra.push_str(&plan.preview());
            *mapper_plan.borrow_mut() = Some(plan);
            ui_kit::prompts::confirm_exact(&format!("Rename attribute and update linked {noun}?"), &format!("{preview}\n{extra}"))
        },
    )?;
    let Some((old, new)) = renamed.into_inner() else { return Err(Error::Cancelled) };
    for item in linked.iter().filter(|item| item.props.iter().any(|prop| prop.name == old)) {
        let (converted, _) = apply(&item.edits(&class, &[Change::Rename { old: old.clone(), new: new.clone() }]).map_err(Error::from)?, &item.text)?;
        action.edits.push((item.path.clone(), converted));
    }
    let project = current(pending, doc)?;
    verify_unchanged(&project, &linked.iter().collect::<Vec<_>>())?;
    if let Some(plan) = mapper_plan.into_inner() {
        plan.verify(&project)?;
        plan.extend(&mut action)?;
    }
    action.title = format!("{} and update linked {noun}", action.title);
    Ok(action)
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;

    const ENTITY: &str = "package shop;\nimport jakarta.persistence.*;\nimport java.time.LocalDate;\n@Entity\npublic class Customer {\n    @Id Long id;\n    String title;\n    LocalDate joined;\n}\n";
    const ENTITY_PATH: &str = "/project/src/main/java/shop/Customer.java";

    #[test]
    fn named_groups_and_anchoring_follow_the_regexp() {
        let regex = pattern_for("(?<entity>.*)Dto").unwrap();
        assert_eq!(regex.captures("CustomerDto").unwrap().name("entity").unwrap().as_str(), "Customer");
        assert!(regex.captures("CustomerDtoExtra").is_none());
        assert!(Regex::new("^(?:(?<x>.*)Dto)$").unwrap().capture_names().flatten().all(|n| n != "entity"));
    }

    fn pattern_for(source: &str) -> Option<Regex> {
        Regex::new(&format!("^(?:{source})$")).ok()
    }

    fn parsed(path: &str, text: &str, kotlin: bool) -> Linked {
        parse(path, text, kotlin).unwrap().unwrap().0
    }

    #[test]
    fn interface_record_and_kotlin_shapes_are_parsed_with_properties() {
        let interface = parsed("/p/CustomerView.java", "package shop;\n\npublic interface CustomerView {\n    Long getId();\n    boolean isActive();\n    String getTitle();\n}\n", false);
        assert_eq!(interface.props.iter().map(|p| (p.name.as_str(), p.ty.as_str())).collect::<Vec<_>>(), [("id", "Long"), ("active", "boolean"), ("title", "String")]);
        let record = parsed("/p/CustomerDto.java", "package shop;\n\npublic record CustomerDto(\n        Long id,\n        String title\n) {\n}\n", false);
        assert_eq!(record.props.len(), 2);
        let data = parsed("/p/CustomerDto.kt", "package shop\n\ndata class CustomerDto(\n    val id: Long?,\n    val title: String?\n)\n", true);
        assert_eq!(data.props[1].ty, "String");
        let view = parsed("/p/CustomerView.kt", "package shop\n\ninterface CustomerView {\n    val id: Long?\n}\n", true);
        assert_eq!(view.props[0].name, "id");
        assert!(parse("/p/CustomerView.java", "package shop;\npublic interface CustomerView {\n    default String x() { return \"\"; }\n}\n", false).is_err());
    }

    fn entity_class() -> Class {
        let doc = Document { path: ENTITY_PATH.into(), language: "java".into(), text: ENTITY.into(), version: 1 };
        dto_action::source(&doc).unwrap()
    }

    fn after(linked: &Linked, changes: &[Change]) -> String {
        let edits = linked.edits(&entity_class(), changes).unwrap();
        apply(&edits, &linked.text).unwrap().1
    }

    #[test]
    fn edits_add_remove_and_rename_in_every_shape() {
        let class = entity_class();
        let interface = parsed("/p/CustomerView.java", "package shop;\n\npublic interface CustomerView {\n    Long getId();\n    String getName();\n}\n", false);
        let text = after(&interface, &[Change::Add { name: "joined".into(), ty: "LocalDate".into() }, Change::Remove("name".into()), Change::Rename { old: "id".into(), new: "key".into() }]);
        assert_eq!(text, "package shop;\n\nimport java.time.LocalDate;\n\npublic interface CustomerView {\n    Long getKey();\n    LocalDate getJoined();\n}\n");
        let record = parsed("/p/CustomerDto.java", "package shop;\n\npublic record CustomerDto(\n        Long id,\n        String name\n) {\n}\n", false);
        let text = after(&record, &[Change::Add { name: "title".into(), ty: "String".into() }, Change::Remove("name".into()), Change::Rename { old: "id".into(), new: "key".into() }]);
        assert!(text.contains("(\n        Long key,\n        String title\n) {"), "{text}");
        let single = parsed("/p/CustomerDto.java", "package shop;\npublic record CustomerDto(Long id) {}\n", false);
        assert!(after(&single, &[Change::Add { name: "title".into(), ty: "String".into() }]).contains("(Long id, String title)"));
        let data = parsed("/p/CustomerDto.kt", "package shop\n\ndata class CustomerDto(\n    val id: Long?\n)\n", true);
        let edits = data.edits(&class, &[Change::Add { name: "title".into(), ty: "String".into() }]).unwrap();
        assert!(apply(&edits, &data.text).unwrap().1.contains("val id: Long?,\n    val title: String?\n)"));
        assert!(interface.edits(&class, &[Change::Remove("missing".into())]).is_err());
        assert!(interface.edits(&class, &[Change::Add { name: "id".into(), ty: "Long".into() }]).is_err());
    }

    #[test]
    fn class_comment_is_inserted_above_the_declaration_with_the_qualified_entity() {
        let conventions = Conventions { postfix: "View".into(), name: None, comment: "A projection of {@link {entity}}".into(), link: None };
        let text = conventions.with_comment("package shop;\n\nimport a.B;\n\npublic interface V {\n}\n", Some("shop"), "Customer");
        assert_eq!(text, "package shop;\n\nimport a.B;\n\n/** A projection of {@link shop.Customer} */\npublic interface V {\n}\n");
        assert_eq!(conventions.default_name("Customer"), "CustomerView");
    }

    #[test]
    fn sync_to_projections_adds_and_removes_across_linked_types() {
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        host.add_file("src/main/java/shop/Customer.java", ENTITY);
        host.add_file("src/main/java/shop/CustomerProjection.java", "package shop;\n\npublic interface CustomerProjection {\n    Long getId();\n    String getGone();\n}\n");
        host.open_editor(ENTITY_PATH, "java", ENTITY, &[]);
        let doc = Document { path: ENTITY_PATH.into(), language: "java".into(), text: ENTITY.into(), version: 1 };
        let mut plugin = JpaAssist::default();
        let pending = plugin.file_actions(&doc, Range::default()).into_iter().find(|a| a.title == SYNC_TITLE).expect("offered");
        host.answer_next_prompt(Some(&["toProjections"]));
        host.answer_next_prompt(Some(&["add:0:title", "remove:0:gone"]));
        host.answer_next_prompt(Some(&[]));
        let action = plugin.resolve_file_action(&doc, pending).unwrap();
        let [(path, edits)] = action.edits.as_slice() else { panic!("one document") };
        assert!(path.ends_with("CustomerProjection.java"));
        let text = patched("package shop;\n\npublic interface CustomerProjection {\n    Long getId();\n    String getGone();\n}\n", edits).unwrap();
        assert_eq!(text, "package shop;\n\npublic interface CustomerProjection {\n    Long getId();\n    String getTitle();\n}\n");
    }

    fn plugin_action(host: &MockHost, title: &str, selection: Range) -> (JpaAssist, Document, FileAction) {
        host.open_editor(ENTITY_PATH, "java", ENTITY, &[]);
        let doc = Document { path: ENTITY_PATH.into(), language: "java".into(), text: ENTITY.into(), version: 1 };
        let mut plugin = JpaAssist::default();
        let pending = plugin.file_actions(&doc, selection).into_iter().find(|a| a.title == title).expect("offered");
        (plugin, doc, pending)
    }

    #[test]
    fn name_regexp_and_comment_link_regexp_decide_which_types_are_linked() {
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        host.set_setting("projectionNameRegexp", "\"(?<entity>.*)Dto\"");
        host.set_setting("projectionCommentLinkRegexp", "\"\\\\{@link (?<entity>[\\\\w.]+)\\\\}\"");
        host.add_file("src/main/java/shop/Customer.java", ENTITY);
        host.add_file("src/main/java/shop/CustomerDto.java", "package shop;\n\npublic record CustomerDto(Long id) {\n}\n");
        host.add_file("src/main/java/shop/CustomerProjection.java", "package shop;\n\npublic interface CustomerProjection {\n    Long getId();\n}\n");
        host.add_file("src/main/java/shop/Summary.java", "package shop;\n\n/** Summary of {@link shop.Customer}. */\npublic record Summary(Long id) {\n}\n");
        host.add_file("src/main/java/shop/OtherDto.java", "package shop;\n\npublic record OtherDto(Long id) {\n}\n");
        let (mut plugin, doc, pending) = plugin_action(&host, SYNC_TITLE, Range::default());
        host.answer_next_prompt(Some(&["toProjections"]));
        host.answer_next_prompt(Some(&["add:0:title", "add:1:title"]));
        host.answer_next_prompt(Some(&[]));
        let action = plugin.resolve_file_action(&doc, pending).unwrap();
        let paths: Vec<_> = action.edits.iter().map(|(path, _)| path.rsplit('/').next().unwrap().to_owned()).collect();
        assert_eq!(paths, ["CustomerDto.java", "Summary.java"]);
    }

    #[test]
    fn sync_to_entity_adds_missing_attributes_with_the_projection_type() {
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        host.add_file("src/main/java/shop/Customer.java", ENTITY);
        host.add_file("src/main/java/shop/CustomerProjection.java", "package shop;\n\nimport java.math.BigDecimal;\n\npublic interface CustomerProjection {\n    Long getId();\n    BigDecimal getBalance();\n}\n");
        let (mut plugin, doc, pending) = plugin_action(&host, SYNC_TITLE, Range::default());
        host.answer_next_prompt(Some(&["toEntity"]));
        host.answer_next_prompt(Some(&["entity:0:balance"]));
        host.answer_next_prompt(Some(&[]));
        let action = plugin.resolve_file_action(&doc, pending).unwrap();
        let [(path, edits)] = action.edits.as_slice() else { panic!("one document") };
        assert_eq!(path, ENTITY_PATH);
        let text = patched(ENTITY, edits).unwrap();
        assert!(text.contains("import java.math.BigDecimal;") && text.contains("    private BigDecimal balance;\n}"), "{text}");
    }

    #[test]
    fn rename_updates_the_entity_and_the_linked_property_together() {
        const RENAMED: &str = "package shop;\nimport jakarta.persistence.Entity;\nimport jakarta.persistence.Id;\nimport jakarta.persistence.Column;\nimport jakarta.persistence.NamedEntityGraph;\nimport jakarta.persistence.NamedAttributeNode;\nimport java.lang.Long;\nimport java.lang.String;\n@Entity\n@NamedEntityGraph(name=\"g\",attributeNodes={@NamedAttributeNode(\"title\")})\npublic class Customer {\n@Id private Long id;\n@Column(name=\"title\") private String title;\n}\n";
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        host.add_file("src/main/java/shop/Customer.java", RENAMED);
        let projection = "package shop;\n\npublic interface CustomerProjection {\n    String getTitle();\n}\n";
        host.add_file("src/main/java/shop/CustomerProjection.java", projection);
        host.open_editor(ENTITY_PATH, "java", RENAMED, &[]);
        let doc = Document { path: ENTITY_PATH.into(), language: "java".into(), text: RENAMED.into(), version: 1 };
        let at = RENAMED.find("String title;").unwrap() + "String ".len();
        let selection = Range::new(openidea_plugin::text::position(RENAMED, at), openidea_plugin::text::position(RENAMED, at + 5));
        let mut plugin = JpaAssist::default();
        let pending = plugin.file_actions(&doc, selection).into_iter().find(|a| a.title == RENAME_TITLE).expect("offered");
        host.answer_next_prompt(Some(&["name"]));
        host.answer_next_prompt(Some(&[]));
        let action = plugin.resolve_file_action(&doc, pending).unwrap();
        assert_eq!(action.edits.len(), 2);
        assert!(patched(RENAMED, &action.edits[0].1).unwrap().contains("private String name;"));
        assert!(patched(projection, &action.edits[1].1).unwrap().contains("String getName();"));
    }

    #[test]
    fn without_a_linked_type_the_sync_refuses_and_names_the_settings() {
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        host.add_file("src/main/java/shop/Customer.java", ENTITY);
        let (mut plugin, doc, pending) = plugin_action(&host, SYNC_TITLE, Range::default());
        let error = plugin.resolve_file_action(&doc, pending).unwrap_err();
        assert!(format!("{error:?}").contains("projectionNameRegexp"));
    }

    #[test]
    fn invalid_or_group_less_regexps_are_refused_when_loading() {
        let host = MockHost::install();
        host.set_setting("projectionNameRegexp", "\"(Dto\"");
        assert!(Conventions::load().is_err());
        host.set_setting("projectionNameRegexp", "\"(?<other>.*)Dto\"");
        assert!(Conventions::load().is_err());
        host.set_setting("projectionNameRegexp", "\"\"");
        assert!(Conventions::load().unwrap().name.is_none());
    }
}
