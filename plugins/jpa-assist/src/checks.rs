//! Problems in entities and repositories, each with an optional quick fix.

use openidea_plugin::text::position;
use openidea_plugin::{Diagnostic, Range, Severity};

use crate::build::{Build, Plugin};
use crate::model::{
    Annotation, Class, Edit, Relation, annotations, closing, declaration, declaration_keyword, declared_type, element_type, has_word,
    import_edit, is_ident, kotlin_supertype, mask, members, plain_literal, split_top_level, value_range, without_annotations, words,
};
use crate::queries::{Props, Repository, boxed, problem};

#[derive(Debug)]
pub struct Finding {
    pub start: usize,
    pub end: usize,
    pub severity: Severity,
    pub code: &'static str,
    pub message: String,
    pub fix: Option<(&'static str, Vec<Edit>)>,
}

impl Finding {
    pub fn new(start: usize, end: usize, severity: Severity, code: &'static str, message: impl Into<String>) -> Self {
        Self { start, end, severity, code, message: message.into(), fix: None }
    }

    pub(super) fn fix(mut self, title: &'static str, edits: Vec<Edit>) -> Self {
        self.fix = Some((title, edits));
        self
    }

    pub fn diagnostic(&self, text: &str) -> Diagnostic {
        let range = Range::new(position(text, self.start), position(text, self.end));
        Diagnostic::new(range, self.severity, self.message.clone()).code(self.code)
    }
}

fn fetch_type(text: &str) -> &'static str {
    if text.contains("javax.persistence") {
        return "javax.persistence.FetchType";
    }

    "jakarta.persistence.FetchType"
}

/// Lazy fetching for an association: explicit `EAGER` on any of them, the eager default of a to-one.
fn fetch_finding(text: &str, masked: &str, a: &Annotation, to_one: bool, kotlin: bool) -> Option<Finding> {
    let args = a.args.map(|(start, end)| (start, &masked[start..end]));
    let lazy = "FetchType.LAZY";
    let mut edits: Vec<Edit> = import_edit(text, &[fetch_type(text)], kotlin).into_iter().collect();

    if let Some(eager) = args.and_then(|(start, inner)| words(inner, "EAGER").next().map(|i| start + i)) {
        let from = if masked[..eager].ends_with("FetchType.") { eager - "FetchType.".len() } else { eager };
        edits.push(Edit { start: from, end: eager + "EAGER".len(), text: lazy.into() });

        let finding = Finding::new(
            a.start,
            a.end(),
            Severity::Warning,
            "jpa-eager-fetch",
            "Eager fetching loads this association with every query on the entity. Use LAZY and fetch it explicitly where you need it. Hibernate User Guide 29.6.1: EAGER fetching is almost always a bad choice; a forgotten JOIN FETCH is initialized by a secondary statement, which can cause N+1 queries.",
        );

        return Some(finding.fix("Fetch lazily", edits));
    }

    if !to_one {
        return None;
    }

    if args.is_some_and(|(_, inner)| has_word(inner, "LAZY") || has_word(inner, "mappedBy")) {
        return None;
    }

    edits.push(match a.args {
        None => Edit::insert(a.name_end, &format!("(fetch = {lazy})")),
        Some((start, end)) if masked[start..end].trim().is_empty() => Edit::insert(start, &format!("fetch = {lazy}")),
        Some((start, _)) => Edit::insert(start, &format!("fetch = {lazy}, ")),
    });

    let finding = Finding::new(
        a.start,
        a.end(),
        Severity::Info,
        "jpa-eager-to-one",
        "To-one associations are fetched eagerly by default (Jakarta Persistence). Set fetch = FetchType.LAZY. Hibernate User Guide 29.6.1: EAGER fetching is to be avoided because it can lead to N+1 queries.",
    );

    Some(finding.fix("Fetch lazily", edits))
}

// Live owned source path: only a direct fetch member and a proved matching enum.
fn fetch_finding_owned(masked: &str, a: &Annotation, to_one: bool, class: &Class) -> Option<Finding> {
    if !plain_type_owner(masked,class.kotlin,class) {return None;}
    let written = &masked[a.start+1..a.name_end];
    let full = if written.contains('.') { written } else { class.imports.iter().find(|i| i.rsplit('.').next()==Some(a.name))?.as_str() };
    let ns = ["jakarta.persistence", "javax.persistence"].into_iter().find(|ns| full==format!("{ns}.{}",a.name))?;
    let package_prefix=ns.split('.').next().unwrap();
    if words(masked,package_prefix).any(|at| !masked[at+package_prefix.len()..].starts_with(".persistence.")) {return None;}
    let mut fetch=None;
    let mut keys=std::collections::HashSet::new();
    if let Some((start,end))=a.args {
        let inner=&masked[start..end];
        if !inner.trim().is_empty() {
            for (offset,part) in split_top_level(inner) {
                let (key,value)=part.split_once('=')?; let key=key.trim();
                if !["targetEntity","targetClass","cascade","fetch","optional","mappedBy","orphanRemoval"].contains(&key) || !keys.insert(key) {return None;}
                if key=="fetch" { let value=value.trim(); let at=start+offset+part.find('=').unwrap()+1+part[part.find('=').unwrap()+1..].len()-part[part.find('=').unwrap()+1..].trim_start().len(); fetch=Some((value,at)); }
            }
        }
    }
    if let Some((value,at))=fetch {
        let expected=format!("{ns}.FetchType.EAGER");
        let short=value=="FetchType.EAGER" || value=="FetchType.LAZY";
        if short {
            let imports:Vec<_>=class.imports.iter().filter(|i|i.rsplit('.').next()==Some("FetchType")).collect();
            if imports.len()!=1 || imports[0].as_str()!=format!("{ns}.FetchType") {return None;}
            if words(masked,"FetchType").any(|at| {
                let after=&masked[at+"FetchType".len()..];
                let enum_value=[".EAGER",".LAZY"].iter().any(|suffix|after.strip_prefix(suffix).is_some_and(|tail|tail.as_bytes().first().is_none_or(|b|!is_ident(*b))));
                let import=masked[..at].trim_end().ends_with(&format!("import {ns}.")) && after.trim_start().starts_with(';');
                !enum_value && !import
            }) {return None;}
        } else if value!=expected && value!=format!("{ns}.FetchType.LAZY") {return None;}
        // Types or type parameters can hide the enum simple name or package prefix.
        let prefix=if short {"FetchType"} else {ns.split('.').next().unwrap()};
        for keyword in ["class","interface","enum","record","object","typealias"] {
            for pos in words(masked,keyword) {
                let rest=masked[pos+keyword.len()..].trim_start();
                let name:String=rest.chars().take_while(|c|c.is_ascii_alphanumeric() || matches!(c,'_'|'$')).collect();
                if name==prefix || rest.split('{').next().is_some_and(|h|h.contains('<') && words(h,prefix).next().is_some()) {return None;}
            }
        }
        if class.imports.iter().any(|i|i.rsplit('.').next()==Some(prefix) && !short) {return None;}
        if value.ends_with(".LAZY") {return None;}
        return Some(Finding::new(a.start,a.end(),Severity::Warning,"jpa-eager-fetch","This source annotation explicitly requests eager fetching. Consider lazy fetching with explicit loading where needed. Hibernate User Guide 29.6.1: EAGER fetching is almost always a bad choice; a forgotten JOIN FETCH is initialized by a secondary statement, which can cause N+1 queries.")
            .fix("Fetch lazily",vec![Edit{start:at,end:at+value.len(),text:format!("{ns}.FetchType.LAZY")}]))
    }
    if !to_one || keys.contains("mappedBy") {return None;}
    let value=format!("fetch = {ns}.FetchType.LAZY");
    let edit=match a.args {None=>Edit::insert(a.name_end,&format!("({value})")),Some((start,end)) if masked[start..end].trim().is_empty()=>Edit::insert(start,&value),Some((start,_))=>Edit::insert(start,&format!("{value}, "))};
    Some(Finding::new(a.start,a.end(),Severity::Info,"jpa-eager-to-one","This source annotation omits fetch; JPA defaults to eager for to-one associations. Consider an explicit lazy request. Hibernate User Guide 29.6.1: EAGER fetching is to be avoided because it can lead to N+1 queries.").fix("Fetch lazily",vec![edit]))
}

fn lombok_data_fix(text: &str, a: &Annotation, kotlin: bool) -> Vec<Edit> {
    let line_start = text[..a.start].rfind('\n').map_or(0, |p| p + 1);
    let indent = &text[line_start..a.start];
    let separator = if indent.trim().is_empty() { format!("\n{indent}") } else { " ".into() };

    let mut edits: Vec<Edit> = import_edit(text, &["lombok.Getter", "lombok.Setter"], kotlin).into_iter().collect();
    edits.push(Edit { start: a.start, end: a.end(), text: format!("@Getter{separator}@Setter") });

    edits
}

fn lombok_data_fix_owned(text: &str, masked: &str, a: &Annotation) -> Option<Vec<Edit>> {
    // Qualify replacements so foreign Getter/Setter imports cannot capture them.
    for keyword in ["class","interface","enum","record","object","typealias"] {
        for at in words(masked,keyword) {
            let header=masked[at+keyword.len()..].trim_start().split('{').next()?;
            let name=header.split(|c:char| !c.is_ascii_alphanumeric() && c!='_' && c!='$').next()?;
            if name=="lombok" || (header.contains('<') && words(header,"lombok").next().is_some()) {return None;}
        }
    }
    let line_start=text[..a.start].rfind('\n').map_or(0,|p|p+1);let indent=&text[line_start..a.start];
    let separator=if indent.trim().is_empty(){format!("\n{indent}")}else{" ".into()};
    Some(vec![Edit{start:a.start,end:a.end(),text:format!("@lombok.Getter{separator}@lombok.Setter")}])
}

pub(super) fn remove_annotation(text: &str, a: &Annotation) -> Edit {
    let line_start = text[..a.start].rfind('\n').map_or(0, |p| p + 1);
    let rest = &text[a.end()..];
    let line_end = rest.find('\n').map_or(text.len(), |p| a.end() + p + 1);
    let alone = text[line_start..a.start].trim().is_empty() && text[a.end()..line_end].trim().is_empty();
    match alone {
        true => Edit { start: line_start, end: line_end, text: String::new() },
        false => Edit { start: a.start, end: a.end(), text: String::new() },
    }
}

pub(super) fn owned_class_start(text: &str, masked: &str, class: &Class) -> Option<usize> {
    if class.body_end.is_none() || class.name.is_empty() || class.name.len() > 128 { return None; }
    let (at, keyword) = declaration_keyword(masked, 0, &["class", "interface", "enum", "record", "object"])?;
    if keyword != "class" { return None; }
    let after = &masked[at + 5..];
    let name_at = at + 5 + after.len() - after.trim_start().len();
    if openidea_plugin::text::offset(text, class.pos) != Some(name_at) || !crate::attribute_info::balanced(masked)
        || !crate::attribute_info::balanced(&masked[..at]) { return None; }
    let all=annotations(masked);if all.len()>256 {return None;}
    let roles:Vec<_>=all.iter().filter(|a|a.start<at && ["Entity","MappedSuperclass","Embeddable"].contains(&a.name)
        && !all.iter().any(|outer|outer.args.is_some_and(|(s,e)|a.start>=s && a.start<e))).collect();
    let [role]=roles.as_slice() else {return None;};
    let expected=match class.kind {crate::model::Kind::Entity=>"Entity",crate::model::Kind::MappedSuperclass=>"MappedSuperclass",crate::model::Kind::Embeddable=>"Embeddable"};
    if role.name!=expected {return None;}
    let mut start=declaration_start(masked,role.start);
    if class.kotlin {
        let mut offset=0;
        for line in masked[..role.start].split_inclusive('\n') {
            offset+=line.len();let trimmed=line.trim_start();
            if line.ends_with('\n') && (trimmed.starts_with("package ") || trimmed.starts_with("import ")) {start=start.max(offset);}
        }
    }
    let prefix=without_annotations(masked,start,at);
    if prefix.split_whitespace().any(|word| !["public","private","protected","static","final","abstract","sealed","non-sealed","strictfp","annotation","data","inner","open","expect","actual","internal","value"].contains(&word)) {return None;}
    Some(at)
}

/// Problems of the entity declared in `text`.
#[cfg_attr(not(test), allow(dead_code))] // Legacy flat annotation fixture; live notes require declaration ownership.
pub fn inspect_entity(text: &str, kotlin: bool) -> Vec<Finding> {
    entity_findings(text, kotlin, None)
}

pub fn inspect_entity_owned(text: &str, kotlin: bool, class: &Class) -> Vec<Finding> {
    entity_findings(text, kotlin, Some(class))
}

fn entity_findings(text: &str, kotlin: bool, owned_class: Option<&Class>) -> Vec<Finding> {
    let masked = mask(text);
    let annotations = annotations(&masked);
    let class_at_owned = if let Some(class) = owned_class {
        if class.kind != crate::model::Kind::Entity { return vec![]; }
        let Some(at) = owned_class_start(text, &masked, class) else { return vec![]; };
        Some(at)
    } else { None };
    let annotations = if let Some(at) = class_at_owned {
        let members = class_members(&masked, kotlin);
        if members.len() > 1024 { return vec![]; }
        declaration_annotations(&masked, annotations, &members, at)
    } else { annotations };
    let Some(entity) = annotations.iter().find(|a| a.name == "Entity") else { return Vec::new() };
    let mut findings = Vec::new();

    if let Some(class_at) = words(&masked[entity.end()..], "class").next().map(|i| i + entity.end()) {
        let modifiers = &masked[entity.end()..class_at];

        if let Some(at) = words(modifiers, "final").next().filter(|_| !kotlin).map(|i| i + entity.end()) {
            let end = at + "final".len();
            let removed = end + masked[end..].bytes().take_while(u8::is_ascii_whitespace).count();
            findings.push(
                Finding::new(
                    at,
                    end,
                    Severity::Warning,
                    "jpa-final-entity",
                    "A final entity can't be proxied, so its lazy associations are loaded eagerly.",
                )
                .fix("Remove final", vec![Edit { start: at, end: removed, text: String::new() }]),
            );
        }

        let own_equality = ["equals", "hashCode", "toString"].iter().all(|n| overrides(&masked[class_at..], n));

        let only_identifiers = identifier_only_constructor(&masked, class_at);

        if let Some(at) = words(modifiers, "data").next().filter(|_| kotlin && !own_equality && !only_identifiers).map(|i| i + entity.end()) {
            findings.push(Finding::new(
                at,
                at + "data".len(),
                Severity::Warning,
                "jpa-data-class",
                "A data class entity gets equals, hashCode and toString over every property, lazy associations included.",
            ));
        }
    }

    for a in &annotations {
        let finding = match a.name {
            "Data" if class_at_owned.is_none_or(|at| a.start < at) => Some(
                Finding::new(
                    a.start,
                    a.end(),
                    Severity::Warning,
                    "jpa-lombok-data",
                    "@Data on an entity generates equals, hashCode and toString over every field, lazy associations included.",
                )
                .fix("Replace with @Getter and @Setter", if owned_class.is_some() {lombok_data_fix_owned(text,&masked,a).unwrap_or_default()} else {lombok_data_fix(text,a,kotlin)}),
            ),
            "EqualsAndHashCode" if class_at_owned.is_none_or(|at| a.start < at) => Some(Finding::new(
                a.start,
                a.end(),
                Severity::Warning,
                "jpa-lombok-equals",
                "The generated equals and hashCode compare mutable fields and a generated id, so the entity's hash code changes once it is persisted.",
            )
            .fix("Remove @EqualsAndHashCode", vec![remove_annotation(text, a)])),
            "ManyToOne" | "OneToOne" if class_at_owned.is_none_or(|at| a.start > at) => if let Some(class)=owned_class {fetch_finding_owned(&masked,a,true,class)} else {fetch_finding(text,&masked,a,true,kotlin)},
            "OneToMany" | "ManyToMany" | "ElementCollection" if class_at_owned.is_none_or(|at| a.start > at) => if let Some(class)=owned_class {fetch_finding_owned(&masked,a,false,class)} else {fetch_finding(text,&masked,a,false,kotlin)},
            _ => None,
        };

        findings.extend(finding.map(|mut f| { if f.fix.as_ref().is_some_and(|(_,edits)|edits.is_empty()) {f.fix=None;} f }));
    }

    findings
}

fn identifier_only_constructor(masked: &str, class_at: usize) -> bool {
    let annotations = annotations(masked);
    let header_end = masked[class_at..].find('{').map_or(masked.len(), |i| i + class_at);
    let Some(open) = masked[class_at..header_end].find('(').map(|i| i + class_at) else { return true };
    let Some(close) = closing(masked.as_bytes(), open) else { return false };

    if masked[open + 1..close].trim().is_empty() {
        return true;
    }

    crate::model::split_top_level(&masked[open + 1..close]).iter().filter(|(_, part)| !part.trim().is_empty()).all(|(offset, part)| {
        let from = open + 1 + offset;
        let to = from + part.len();

        annotations.iter().any(|a| a.start >= from && a.start < to && matches!(a.name, "Id" | "EmbeddedId"))
    })
}

fn overrides(masked: &str, name: &str) -> bool {
    words(masked, name).any(|i| masked[..i].trim_end().ends_with("fun") && masked[i + name.len()..].trim_start().starts_with('('))
}

const IMMUTABLE_COLLECTIONS: &[&str] = &["List", "Set", "Map", "Collection", "Iterable"];

/// Kotlin entity attributes JPA cannot populate: `val` properties and read-only collection types on associations.
#[cfg_attr(not(test), allow(dead_code))] // Legacy lexical fixture; live collection hints require canonical Kotlin ownership.
pub fn inspect_kotlin_entity(text: &str) -> Vec<Finding> {
    kotlin_entity_findings(text, &|_| true, &|_| true, false)
}

pub fn inspect_kotlin_entity_owned(text: &str, class: &Class) -> Vec<Finding> {
    let masked = mask(text);
    let plain = class.superclass.is_none() && class.body_end.is_some()
        && words(&masked, "class").count() == 1
        && ["interface", "enum", "object", "typealias"].iter().all(|w| words(&masked, w).next().is_none())
        && declaration_keyword(&masked, 0, &["class"]).and_then(|(at, _)| {
            let open = masked[at..].find('{')? + at;
            let header = masked[at + 5..open].trim();
            Some(header == class.name)
        }) == Some(true);
    if !plain || !crate::attribute_info::balanced(&masked) || !crate::live::persistence_annotations_only(text) {return vec![];}
    kotlin_entity_findings(text, &|ty| {
        kotlin_source_type(class,ty,&["kotlin.collections.List","kotlin.collections.Set","kotlin.collections.Map","kotlin.collections.Collection","kotlin.collections.Iterable"])
    }, &|ty| {
        kotlin_source_type(class,ty,&["kotlin.String","kotlin.Long","kotlin.Int","kotlin.Short","kotlin.Byte","kotlin.Boolean","kotlin.Double","kotlin.Float","kotlin.Char","kotlin.ByteArray"])
    }, true)
}
fn kotlin_source_type(class: &Class, ty: &str, owners: &[&str]) -> bool {
    let ty=ty.strip_suffix('?').unwrap_or(ty);
    if ty.contains('.') {
        return owners.contains(&ty) && class.name!="kotlin" && !class.imports.iter().any(|i|i.rsplit('.').next()==Some("kotlin"));
    }
    if ty==class.name {return false;}
    let imports:Vec<_>=class.imports.iter().filter(|i|i.rsplit('.').next()==Some(ty)).collect();
    let [import]=imports.as_slice() else {return false;};
    owners.contains(&import.as_str())
}

fn kotlin_entity_findings(text: &str, read_only_owned: &dyn Fn(&str) -> bool, val_owned: &dyn Fn(&str) -> bool, source_hint: bool) -> Vec<Finding> {
    let masked = mask(text);
    let all = annotations(&masked);
    let mut findings = Vec::new();

    let Some((class_at, _)) = declaration_keyword(&masked, 0, &["class"]) else { return findings };
    let members=class_members(&masked,true);
    if source_hint && members.len()>1024 {return findings;}
    let all=if source_hint {declaration_annotations(&masked,all,&members,class_at)} else {all};
    let class_level = |name: &str| all.iter().any(|a| a.start < class_at && a.name == name);

    if !class_level("Entity") {
        return findings;
    }

    let read_only_class = class_level("Immutable") || class_level("Embeddable") || class_level("MappedSuperclass");

    for (start, end) in members {
        let decl = without_annotations(&masked, start, end);
        let left = decl.split('=').next().unwrap_or(&decl);

        if ["fun", "class", "object", "interface", "by"].iter().any(|w| has_word(&decl, w)) {
            continue;
        }

        let Some(keyword) = words(left, "val").chain(words(left, "var")).min() else { continue };
        let before = &left[..keyword];
        let modifiers = ["private", "protected", "public", "internal", "override", "open", "final"];
        if !before.split_whitespace().all(|m| modifiers.contains(&m)) {
            continue;
        }

        let named: Vec<&str> = all.iter().filter(|a| a.start >= start && a.start < end).map(|a| a.name).collect();

        let following = masked[end..].trim_start();
        let computed = ["get", "set"]
            .iter()
            .any(|w| following.strip_prefix(w).is_some_and(|rest| rest.trim_start().starts_with('(') && !rest.starts_with(is_ident_char)));
        let delegated = following.strip_prefix("by").is_some_and(|rest| rest.starts_with(char::is_whitespace));

        let computed = computed || has_word(left, "get") || has_word(left, "set");
        let is_val = left[keyword..].starts_with("val");
        let is_attribute = !named.contains(&"Transient") && !computed && !delegated;

        if is_val && is_attribute && !read_only_class && !named.iter().any(|n| matches!(*n, "Id" | "EmbeddedId"))
            && (!source_hint || declared_type(&decl,true).is_some_and(|ty|val_owned(&ty))) {
            findings.push(Finding::new(
                start + keyword,
                start + keyword + 3,
                if source_hint {Severity::Info} else {Severity::Warning},
                "jpa-val-attribute",
                if source_hint {"This source property uses Kotlin val and exposes read-only property access. Consider var if mutation is required; effective JPA access, XML and generated/provider behavior are unresolved."} else {"A val compiles to a final field, which JPA cannot set when it loads the entity. Declare the attribute with var."},
            ));
        }

        if !is_attribute || !named.iter().any(|n| matches!(*n, "OneToMany" | "ManyToMany" | "ElementCollection")) {
            continue;
        }

        let Some(colon) = left[keyword..].find(':').map(|p| p + keyword + 1) else { continue };
        let ty_start = colon + (left[colon..].len() - left[colon..].trim_start().len());
        let ty_len = left[ty_start..].find(['<', '?']).unwrap_or(left.len() - ty_start);
        let ty = left[ty_start..ty_start + ty_len].trim_end();

        if IMMUTABLE_COLLECTIONS.contains(&last_name(ty)) && read_only_owned(ty) {
            findings.push(Finding::new(
                start + ty_start,
                start + ty_start + ty.len(),
                if source_hint { Severity::Info } else { Severity::Warning },
                "jpa-immutable-collection",
                if source_hint {
                    format!("The source-declared Kotlin {} API exposes read-only operations. Consider a Mutable collection type if this association is mutated from Kotlin.", last_name(ty))
                } else {
                    format!("{} is read-only in Kotlin, so the persistence provider cannot fill it. Use a Mutable collection type.", last_name(ty))
                },
            ));
        }
    }

    findings
}

fn is_ident_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// The entity annotation of `text` when the class has neither `@Id`, `@EmbeddedId` nor `@IdClass`.
#[cfg_attr(not(test), allow(dead_code))] // Legacy full-context fixture; not a live provider path.
pub fn missing_id(text: &str) -> Option<Finding> {
    let masked = mask(text);
    let annotations = annotations(&masked);
    let entity = annotations.iter().find(|a| a.name == "Entity")?;
    words(&masked[entity.end()..], "class").next()?;

    if annotations.iter().any(|a| matches!(a.name, "Id" | "EmbeddedId" | "IdClass")) {
        return None;
    }

    Some(Finding::new(
        entity.start,
        entity.end(),
        Severity::Error,
        "entity-missing-id",
        "This entity declares no identifier: add an @Id or @EmbeddedId attribute, an @IdClass, or inherit one from a mapped superclass.",
    ))
}

/// Offset ranges of the declarations in the first class of `masked`: Kotlin constructor properties, then the body.
fn class_members(masked: &str, kotlin: bool) -> Vec<(usize, usize)> {
    let Some((class_at, _)) = declaration_keyword(masked, 0, &["class"]) else { return Vec::new() };
    let header_end = masked[class_at..].find('{').map_or(masked.len(), |p| p + class_at);
    let mut found = Vec::new();

    if kotlin
        && let Some(paren) = masked[class_at..header_end].find('(').map(|p| p + class_at)
        && let Some(paren_close) = closing(masked.as_bytes(), paren)
    {
        found.extend(members(masked, paren + 1, paren_close, true, true));
    }

    if header_end < masked.len()
        && let Some(close) = closing(masked.as_bytes(), header_end)
    {
        found.extend(members(masked, header_end + 1, close, kotlin, false));
    }

    found
}

const NOT_A_DATE: &[&str] = &[
    "String",
    "Long",
    "long",
    "Integer",
    "int",
    "Short",
    "short",
    "Byte",
    "byte",
    "Boolean",
    "boolean",
    "Double",
    "double",
    "Float",
    "float",
    "Character",
    "char",
    "Int",
    "Char",
    "LocalDate",
    "LocalDateTime",
    "LocalTime",
    "Instant",
    "OffsetDateTime",
    "ZonedDateTime",
    "BigDecimal",
    "BigInteger",
    "UUID",
];

const NOT_A_VERSION: &[&str] = &[
    "String",
    "boolean",
    "Boolean",
    "double",
    "Double",
    "float",
    "Float",
    "byte",
    "Byte",
    "char",
    "Character",
    "Char",
    "BigDecimal",
    "BigInteger",
    "UUID",
];

fn last_name(ty: &str) -> &str {
    ty.rsplit('.').next().unwrap_or(ty).trim()
}

/// `@Temporal`, `@Enumerated` and `@Version` on a type the JPA specification does not allow them on.
/// Only well-known types are judged; library or unresolved types are left alone.
#[cfg_attr(not(test), allow(dead_code))] // Legacy lexical fixture; live scalar types require exact ownership.
pub fn inspect_attribute_types(text: &str, kotlin: bool, is_model: &dyn Fn(&str) -> bool) -> Vec<Finding> {
    attribute_type_findings(text, kotlin, &|ty| is_model(last_name(ty)), &|_| true, false)
}

/// A short type name cannot safely use import/package facts through unknown local or inherited declarations.
pub fn plain_type_owner(text: &str, kotlin: bool, class: &Class) -> bool {
    let masked = mask(text);
    !kotlin && class.superclass.is_none()
        && words(&masked, "class").count() == 1
        && ["interface", "enum", "record", "object", "typealias"].iter().all(|w| words(&masked, w).next().is_none())
        && declaration_keyword(&masked, 0, &["class"]).and_then(|(at, _)| {
            masked[at..].find('{').map(|end| masked[at + 5..at + end].trim() == class.name)
        }) == Some(true)
}

/// Live scalar deny lists require unshadowable primitives or exact JDK source ownership.
/// Independently positive cached source-model facts remain a separate branch.
pub fn inspect_attribute_types_owned(text: &str, kotlin: bool, class: &Class, is_model: &dyn Fn(&str) -> bool) -> Vec<Finding> {
    if owned_class_start(text, &mask(text), class).is_none() { return vec![]; }
    let plain_owner = plain_type_owner(text, kotlin, class);
    attribute_type_findings(text, kotlin, is_model, &|ty| scalar_type_owned(plain_owner, kotlin, class, ty), true)
}

fn scalar_type_owned(plain_owner: bool, kotlin: bool, class: &Class, ty: &str) -> bool {
    const PRIMITIVES: &[&str] = &["long", "int", "short", "byte", "boolean", "double", "float", "char"];
    const JDK: &[&str] = &[
        "java.lang.String", "java.lang.Long", "java.lang.Integer", "java.lang.Short", "java.lang.Byte",
        "java.lang.Boolean", "java.lang.Double", "java.lang.Float", "java.lang.Character",
        "java.time.LocalDate", "java.time.LocalDateTime", "java.time.LocalTime", "java.time.Instant",
        "java.time.OffsetDateTime", "java.time.ZonedDateTime", "java.math.BigDecimal", "java.math.BigInteger", "java.util.UUID",
    ];
    if !kotlin && PRIMITIVES.contains(&ty) { return true; }
    if !plain_owner { return false; }
    if JDK.contains(&ty) { return ty.split('.').next() != Some(class.name.as_str()); }
    if ty.contains('.') || ty == class.name { return false; }
    let imports: Vec<_> = class.imports.iter().filter(|i| i.rsplit('.').next() == Some(ty)).collect();
    let [import] = imports.as_slice() else { return false; };
    JDK.contains(&import.as_str())
}

fn attribute_type_findings(text: &str, kotlin: bool, is_model: &dyn Fn(&str) -> bool, scalar_owned: &dyn Fn(&str) -> bool, source_only: bool) -> Vec<Finding> {
    let masked = mask(text);
    let all = annotations(&masked);
    let mut findings = Vec::new();

    if !all.iter().any(|a| a.name == "Entity" || a.name == "MappedSuperclass" || a.name == "Embeddable") {
        return findings;
    }

    let members = class_members(&masked, kotlin);
    if source_only && members.len() > 1024 { return findings; }
    let all = if source_only {
        let Some((at, _)) = declaration_keyword(&masked, 0, &["class"]) else { return findings; };
        declaration_annotations(&masked, all, &members, at)
    } else { all };
    for (start, end) in members {
        let Some(ty) = declared_type(&without_annotations(&masked, start, end), kotlin) else { continue };
        let raw = ty.as_str();
        let ty = last_name(raw);
        let scalar = scalar_owned(raw);

        for a in all.iter().filter(|a| a.start >= start && a.start < end) {
            let cached = a.name == "Enumerated" && !(scalar && NOT_A_DATE.contains(&ty)) && is_model(raw);
            let message = match a.name {
                "Temporal" if scalar && NOT_A_DATE.contains(&ty) => {
                    format!("@Temporal only applies to java.util.Date and java.util.Calendar, not {ty}.")
                }
                "Enumerated" if scalar && NOT_A_DATE.contains(&ty) || cached => {
                    if source_only && cached {
                        format!("A supplied source class matches {raw}; @Enumerated expects an enum. Compiler and external mappings are unresolved.")
                    } else {
                        format!("@Enumerated only applies to enum types, and {ty} is not one.")
                    }
                }
                "Version" if scalar && NOT_A_VERSION.contains(&ty) => {
                    format!("{ty} can't hold a @Version: use an int, short, long or timestamp type.")
                }
                _ => continue,
            };

            findings.push(Finding::new(a.start, a.end(), if source_only && cached { Severity::Info } else { Severity::Warning }, "attribute-type", message));
        }
    }

    findings
}

const RELATION_ANNOTATIONS: &[&str] = &["OneToOne", "ManyToOne", "OneToMany", "ManyToMany", "ElementCollection"];

const MAPPING_ANNOTATIONS: &[&str] = &[
    "Id",
    "EmbeddedId",
    "Column",
    "Basic",
    "JoinColumn",
    "JoinColumns",
    "JoinTable",
    "Enumerated",
    "Temporal",
    "Lob",
    "Version",
    "Embedded",
    "OneToOne",
    "ManyToOne",
    "OneToMany",
    "ManyToMany",
    "ElementCollection",
];

const COLLECTION_TYPES: &[&str] = &[
    "List",
    "Set",
    "Collection",
    "Map",
    "SortedSet",
    "SortedMap",
    "ArrayList",
    "HashSet",
    "HashMap",
    "MutableList",
    "MutableSet",
    "MutableMap",
    "MutableCollection",
];

fn base_type(ty: &str) -> &str {
    last_name(ty.split('<').next().unwrap_or(ty))
}

/// Attribute declarations that break the JPA specification: two associations on one attribute, `@Id` with `@EmbeddedId`,
/// an association on a type that cannot hold it, and annotations on getters and fields mixed.
#[cfg_attr(not(test), allow(dead_code))] // Legacy lexical fixture; live relationships require exact type ownership.
pub fn inspect_attribute_signatures(text: &str, kotlin: bool) -> Vec<Finding> {
    attribute_signature_findings(text, kotlin, &|_| true, &|_| true, Some(false), false, false)
}

fn literal_property_access(text: &str, class: &Class) -> bool {
    let masked = mask(text);
    let Some((class_at, _)) = declaration_keyword(&masked, 0, &["class"]) else { return false; };
    let all = annotations(&masked);
    let access: Vec<_> = all.iter().filter(|a| a.name == "Access").collect();
    let [access] = access.as_slice() else { return false; };
    if access.start >= class_at { return false; }
    let written = &masked[access.start + 1..access.name_end];
    let owner = if written.contains('.') { written } else {
        let imports: Vec<_> = class.imports.iter().filter(|i| i.rsplit('.').next() == Some("Access")).collect();
        let [import] = imports.as_slice() else { return false; };
        import.as_str()
    };
    let Some(namespace @ ("jakarta.persistence" | "javax.persistence")) = owner.strip_suffix(".Access") else { return false; };
    if namespace.split('.').next() == Some(class.name.as_str()) { return false; }
    let Some((start, end)) = access.args else { return false; };
    let args = masked[start..end].trim();
    let value = if let Some(rest) = args.strip_prefix("value") {
        let Some(value) = rest.trim_start().strip_prefix('=') else { return false; };
        value.trim()
    } else { args };
    value == format!("{namespace}.AccessType.PROPERTY")
}

pub fn inspect_attribute_signatures_owned(text: &str, kotlin: bool, class: &Class, persistence_only: bool) -> Vec<Finding> {
    let masked = mask(text);
    if owned_class_start(text,&masked,class).is_none() {return vec![];}
    let plain_owner = plain_type_owner(text, kotlin, class);
    let local_setters = persistence_only && plain_owner && class.body_end.is_some() && class.kind == crate::model::Kind::Entity
        && declaration_keyword(&masked, 0, &["class"]).is_some_and(|(at, _)| words(&masked[..at], "abstract").next().is_none());
    let mut findings = attribute_signature_findings(text, kotlin, &|ty| {
        !kotlin && ty.ends_with("[]") || scalar_type_owned(plain_owner, kotlin, class, ty)
    }, &|ty| {
        const JDK_COLLECTIONS: &[&str] = &[
            "java.util.List", "java.util.Set", "java.util.Collection", "java.util.Map", "java.util.SortedSet",
            "java.util.SortedMap", "java.util.ArrayList", "java.util.HashSet", "java.util.HashMap",
        ];
        if !plain_owner { return false; }
        let ty = ty.split('<').next().unwrap_or(ty).trim();
        if JDK_COLLECTIONS.contains(&ty) { return ty.split('.').next() != Some(class.name.as_str()); }
        if ty.contains('.') || ty == class.name { return false; }
        let imports: Vec<_> = class.imports.iter().filter(|i| i.rsplit('.').next() == Some(ty)).collect();
        let [import] = imports.as_slice() else { return false; };
        JDK_COLLECTIONS.contains(&import.as_str())
    }, local_setters.then_some(true), local_setters && literal_property_access(text, class), true);
    if class.kind == crate::model::Kind::Embeddable && class.body_end.is_some() {
        let Some(at) = owned_class_start(text, &masked, class) else { return findings; };
        let members = class_members(&masked, kotlin);
        if members.len() <= 1024 {
            let all = declaration_annotations(&masked, annotations(&masked), &members, at);
            for annotation in all.iter().filter(|a| a.start > at && matches!(a.name, "Id" | "EmbeddedId")) {
                findings.push(Finding::new(annotation.start, annotation.end(), Severity::Warning,
                    "jpa-embeddable-identifier", "An embeddable source class must not declare Id or EmbeddedId; its owning entity declares the identifier. Effective XML mappings are not resolved."));
            }
        }
    }
    findings
}

fn declaration_annotations<'a>(masked: &str, all: Vec<Annotation<'a>>, members: &[(usize, usize)], class_at: usize) -> Vec<Annotation<'a>> {
    let bounds: Vec<_> = members.iter().map(|&(start, end)| {
        let decl = without_annotations(masked, start, end);
        let stop = decl.find(['(', '=', '{', ';']).unwrap_or(decl.len());
        let nested_type = declaration_keyword(&decl[..stop], 0, &["class", "interface", "enum", "record", "object", "typealias"]).is_some();
        (start, end, start + stop, !nested_type)
    }).collect();
    let mut member = 0;
    let mut argument_ends = Vec::new();
    let mut kept = Vec::new();
    for annotation in all {
        while argument_ends.last().is_some_and(|end| annotation.start >= *end) { argument_ends.pop(); }
        let nested_argument = !argument_ends.is_empty();
        if let Some((_, end)) = annotation.args { argument_ends.push(end); }
        let complete = annotation.args.is_some() || !masked[annotation.name_end..].trim_start().starts_with('(');
        while member < bounds.len() && annotation.start >= bounds[member].1 { member += 1; }
        let attached = annotation.start < class_at || bounds.get(member).is_some_and(|&(start, _, stop, ordinary)| {
            ordinary && annotation.start >= start && annotation.end() <= stop
        });
        if complete && !nested_argument && attached { kept.push(annotation); }
    }
    kept
}

fn attribute_signature_findings(text: &str, kotlin: bool, basic_owned: &dyn Fn(&str) -> bool, collection_owned: &dyn Fn(&str) -> bool, setters: Option<bool>, explicit_property: bool, owned: bool) -> Vec<Finding> {
    let masked = mask(text);
    let all = annotations(&masked);
    let mut findings = Vec::new();

    let Some((class_at, _)) = declaration_keyword(&masked, 0, &["class"]) else { return findings };
    let members = class_members(&masked, kotlin);
    if owned && members.len() > 1024 { return findings; }
    let all = if owned { declaration_annotations(&masked, all, &members, class_at) } else { all };
    let mut ids: Vec<&Annotation> = Vec::new();
    let mut embedded_ids: Vec<&Annotation> = Vec::new();

    for &(start, end) in &members {
        let in_member: Vec<&Annotation> = all.iter().filter(|a| a.start >= start && a.start < end).collect();
        let decl = without_annotations(&masked, start, end);
        let relations: Vec<&&Annotation> = in_member.iter().filter(|a| RELATION_ANNOTATIONS.contains(&a.name)).collect();

        ids.extend(in_member.iter().copied().filter(|a| a.name == "Id"));
        embedded_ids.extend(in_member.iter().copied().filter(|a| a.name == "EmbeddedId"));

        if let Some(second) = relations.get(1) {
            findings.push(Finding::new(
                second.start,
                second.end(),
                Severity::Warning,
                "jpa-attribute-multiple-relations",
                format!("An attribute has one association: @{} and @{} can't both map it.", relations[0].name, second.name),
            ));
            continue;
        }

        let (Some(relation), Some(ty)) = (relations.first(), declared_type(&decl, kotlin)) else { continue };
        let base = base_type(&ty);
        let basic = (NOT_A_DATE.contains(&base) || ty.ends_with("[]") || base == "ByteArray") && basic_owned(&ty);
        let to_one = matches!(relation.name, "OneToOne" | "ManyToOne");
        let to_many = matches!(relation.name, "OneToMany" | "ManyToMany");

        let wrong = if to_one && (basic || COLLECTION_TYPES.contains(&base) && collection_owned(&ty)) {
            Some(format!("@{} maps a single entity, but {base} is {}.", relation.name, if basic { "a basic type" } else { "a collection" }))
        } else if to_many && basic {
            Some(format!("@{} maps a collection of entities, but {base} is a basic type.", relation.name))
        } else {
            None
        };

        if let Some(message) = wrong {
            findings.push(Finding::new(relation.start, relation.end(), Severity::Warning, "jpa-relationship-type", message));
        }
    }

    let id_class = all.iter().any(|a| a.start < class_at && a.name == "IdClass");
    for embedded in &embedded_ids {
        if let Some(other) = ids.first().map(|a| a.name).or(id_class.then_some("IdClass")) {
            findings.push(Finding::new(
                embedded.start,
                embedded.end(),
                Severity::Warning,
                "jpa-multiple-id-kinds",
                format!("@EmbeddedId can't be combined with @{other}: use one kind of identifier."),
            ));
        }
    }

    if !kotlin {
        let has_access = all.iter().any(|a| a.name == "Access");
        if !has_access { findings.extend(mixed_access(&masked, &all, &members, owned)); }
        if !has_access || explicit_property {
            if let Some(source_only) = setters { findings.extend(missing_setters(&masked, &all, &members, source_only)); }
        }
    }

    findings
}

fn mixed_access(masked: &str, all: &[Annotation], members: &[(usize, usize)], owned: bool) -> Vec<Finding> {
    let mut fields: Vec<&Annotation> = Vec::new();
    let mut getters: Vec<&Annotation> = Vec::new();

    for &(start, end) in members {
        let decl = without_annotations(masked, start, end);
        let left = decl.split('=').next().unwrap_or(&decl);
        let signature_end = left.find('(').map(|p| start + p);
        let mapped = all
            .iter()
            .filter(|a| a.start >= start && a.start < end && MAPPING_ANNOTATIONS.contains(&a.name))
            .filter(|a| signature_end.is_none_or(|e| a.start < e));

        match signature_end {
            Some(_) => getters.extend(mapped),
            None => fields.extend(mapped),
        }
    }

    let (kept, ignored, place) = match (fields.iter().any(|a| a.name == "Id" || owned && a.name == "EmbeddedId"), getters.iter().any(|a| a.name == "Id" || owned && a.name == "EmbeddedId")) {
        (true, false) => ("field", getters, "getter"),
        (false, true) => ("getter", fields, "field"),
        _ => return Vec::new(),
    };

    ignored
        .into_iter()
        .map(|a| {
            Finding::new(
                a.start,
                a.end(),
                if owned { Severity::Info } else { Severity::Warning },
                "jpa-mixed-access",
                if owned {
                    format!("Current source places primary-key mapping on a {kept} and mapping metadata on a {place}. Effective hierarchy/XML access is unresolved; review @Access.")
                } else {
                    format!("The @Id is on a {kept}, so access is by {kept} and this annotation on a {place} is ignored. Move it, or add @Access.")
                },
            )
        })
        .collect()
}

fn source_getter(head: &str) -> bool {
    let tokens: Vec<_> = head.split_whitespace().collect();
    if tokens.len() < 3 { return false; }
    let name = tokens[tokens.len()-1];
    let ty = tokens[tokens.len()-2];
    let modifiers = &tokens[..tokens.len()-2];
    if modifiers.iter().filter(|m| matches!(**m, "public" | "protected")).count() != 1
        || modifiers.iter().any(|m| !["public", "protected", "final", "synchronized", "strictfp"].contains(m))
    { return false; }
    let mut base = ty;
    while let Some(inner) = base.strip_suffix("[]") { base = inner; }
    if base == "void" || !base.split('.').all(|part| !part.is_empty() && part.bytes().enumerate().all(|(i,b)| b.is_ascii_alphabetic() || b == b'_' || b == b'$' || i > 0 && b.is_ascii_digit())) { return false; }
    !name.starts_with("is") || ty == "boolean"
}

/// Annotated getters of an entity with property access (the `@Id` sits on a getter) that have no setter.
fn missing_setters(masked: &str, all: &[Annotation], members: &[(usize, usize)], source_only: bool) -> Vec<Finding> {
    let mut getters: Vec<(String, &Annotation)> = Vec::new();
    let mut field_mapped = false;
    let mut getter_id = false;

    for &(start, end) in members {
        let decl = without_annotations(masked, start, end);
        let left = decl.split('=').next().unwrap_or(&decl);
        let mapped = all.iter().filter(|a| a.start >= start && a.start < end && MAPPING_ANNOTATIONS.contains(&a.name));
        let Some(paren) = left.find('(') else {
            field_mapped |= mapped.clone().next().is_some();
            continue;
        };

        if source_only && (!source_getter(&left[..paren])
            || all.iter().any(|a| a.start >= start && a.end() <= start + paren && a.name == "Transient")) { continue; }
        let mapped = mapped.filter(|a| !source_only || a.end() <= start + paren);
        let name: String = left[..paren]
            .trim_end()
            .bytes()
            .rev()
            .take_while(|&c| is_ident(c))
            .map(char::from)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        let property =
            name.strip_prefix("get").or_else(|| name.strip_prefix("is")).filter(|p| p.starts_with(|c: char| c.is_ascii_uppercase()));
        let (Some(property), Some(first)) = (property, mapped.clone().next()) else { continue };
        if left[paren + 1..].trim_start().starts_with(')') {
            getter_id |= mapped.clone().any(|a| a.name == "Id" || source_only && a.name == "EmbeddedId");
            getters.push((property.to_owned(), first));
        }
    }

    if field_mapped || !getter_id {
        return Vec::new();
    }

    getters
        .into_iter()
        .filter(|(property, _)| words(masked, &format!("set{property}")).next().is_none())
        .map(|(property, first)| {
            Finding::new(
                first.start,
                first.end(),
                Severity::Info,
                "jpa-missing-setter",
                if source_only {
                    format!("Current source declares no setter named set{property}() for this property. Generated and external mappings are unresolved.")
                } else {
                    format!("Property access needs a setter for '{property}': add set{property}().")
                },
            )
        })
        .collect()
}

/// Kotlin entity constructors against the `kotlin-jpa` compiler plugin: a missing plugin, a redundant no-arg constructor, a redundant `open`.
pub fn inspect_kotlin_build(text: &str, build: &Build) -> Vec<Finding> {
    let masked = mask(text);
    let all = annotations(&masked);
    let mut findings = Vec::new();

    let Some((class_at, _)) = declaration_keyword(&masked, 0, &["class"]) else { return findings };
    let Some(entity) = all.iter().find(|a| a.start < class_at && a.name == "Entity") else { return findings };
    let after_keyword = &masked[class_at + "class".len()..];
    let name_end =
        masked.len() - after_keyword.trim_start().len() + after_keyword.trim_start().bytes().take_while(|&c| is_ident(c)).count();
    let header_end = masked[class_at..].find('{').map_or(masked.len(), |p| p + class_at);
    let header = &masked[name_end..header_end];
    let modifiers = without_annotations(&masked, entity.end(), class_at);

    if build.all_open_entities
        && let Some(open) = words(&modifiers, "open").next().map(|i| i + entity.end())
    {
        let removed = open + "open".len() + masked[open + "open".len()..].bytes().take_while(u8::is_ascii_whitespace).count();
        findings.push(
            Finding::new(
                open,
                open + "open".len(),
                Severity::Info,
                "jpa-redundant-open",
                "The allopen plugin already opens entity classes, so this modifier is redundant.",
            )
            .fix("Remove open", vec![Edit { start: open, end: removed, text: String::new() }]),
        );
    }

    if has_word(header, "constructor") {
        return findings;
    }

    let primary = header
        .trim_start()
        .starts_with('(')
        .then(|| header.find('('))
        .flatten()
        .and_then(|p| closing(header.as_bytes(), p).map(|c| &header[p + 1..c]));
    let needs_arguments =
        primary.is_some_and(|params| split_top_level(params).iter().any(|(_, p)| !p.trim().is_empty() && !p.contains('=')));

    let secondary: Vec<(usize, usize)> = (header_end < masked.len())
        .then(|| closing(masked.as_bytes(), header_end))
        .flatten()
        .map(|close| members(&masked, header_end + 1, close, true, false))
        .unwrap_or_default()
        .into_iter()
        .filter(|&(s, e)| {
            without_annotations(&masked, s, e).trim_start().strip_prefix("constructor").is_some_and(|r| r.trim_start().starts_with('('))
        })
        .collect();
    let no_arg = secondary.iter().copied().find(|&(s, e)| {
        let decl = without_annotations(&masked, s, e);
        decl.trim_start().strip_prefix("constructor").is_some_and(|r| r.trim_start().starts_with("()"))
    });

    match build.plugin {
        Plugin::Missing if needs_arguments && no_arg.is_none() => findings.push(Finding::new(
            entity.start,
            entity.end(),
            Severity::Info,
            "jpa-kotlin-plugin-missing",
            "This entity has no constructor without arguments, which JPA needs. Apply the kotlin-jpa (no-arg) compiler plugin.",
        )),
        Plugin::Applied => {
            if let Some((s, e)) = no_arg {
                let decl = without_annotations(&masked, s, e);
                let rest = decl.trim_start().trim_start_matches("constructor").trim_start().trim_start_matches("()").trim();
                let at = s + decl.find("constructor").unwrap_or(0);
                let body_is_empty =
                    rest.is_empty() || rest.strip_prefix('{').and_then(|r| r.strip_suffix('}')).is_some_and(|r| r.trim().is_empty());
                let annotated = all.iter().any(|a| a.start >= s && a.start < e);

                if body_is_empty && !annotated {
                    let line_start = masked[..at].rfind('\n').map_or(0, |p| p + 1);
                    let line_end = e + masked[e..].bytes().take_while(|&c| c == b' ' || c == b'\t').count();
                    let line_end = line_end + usize::from(masked[line_end..].starts_with('\n'));
                    findings.push(
                        Finding::new(
                            at,
                            at + "constructor".len(),
                            Severity::Info,
                            "jpa-redundant-constructor",
                            "The kotlin-jpa plugin generates a constructor without arguments, so this one is redundant.",
                        )
                        .fix("Remove constructor", vec![Edit { start: line_start, end: line_end, text: String::new() }]),
                    );
                }
            }
        }
        _ => {}
    }

    findings
}

/// An attribute name written as a string in a mapping annotation, and the class it must be an attribute of.
#[cfg_attr(not(test), allow(dead_code))]
pub struct NameReference {
    pub owner: String,
    pub value: String,
    pub start: usize,
    pub end: usize,
    pub code: &'static str,
    pub annotation: &'static str,
}

#[cfg_attr(not(test), allow(dead_code))] // Legacy full-context fixture; not a live provider path.
fn name_reference(
    text: &str,
    masked: &str,
    a: &Annotation,
    key: &str,
    owner: &str,
    kotlin: bool,
    code: &'static str,
    annotation: &'static str,
) -> Option<NameReference> {
    let (start, value) = plain_literal(text, value_range(text, masked, a, key)?, kotlin)?;
    Some(NameReference { owner: owner.to_owned(), value: value.to_owned(), start, end: start + value.len(), code, annotation })
}

/// The attribute names that `@AttributeOverride`, `@MapsId`, `@MapKey` and `@NamedAttributeNode` of `class` spell out.
#[cfg_attr(not(test), allow(dead_code))] // Legacy full-context fixture; not a live provider path.
pub fn name_references(text: &str, kotlin: bool, class: &Class) -> Vec<NameReference> {
    let masked = mask(text);
    let all = annotations(&masked);
    let mut found = Vec::new();

    let Some((class_at, _)) = declaration_keyword(&masked, 0, &["class"]) else { return found };
    let members = class_members(&masked, kotlin);
    let owner_of = |a: &Annotation| -> Option<(String, bool)> {
        let &(start, end) = members.iter().find(|&&(s, e)| a.start >= s && a.start < e)?;
        let (_, _, ty) = declaration(&without_annotations(&masked, start, end), kotlin)?;
        Some((element_type(&ty), true))
    };

    for a in &all {
        match a.name {
            "AttributeOverride" => {
                let owner = if a.start < class_at { class.superclass.clone().map(|s| (s, false)) } else { owner_of(a) };
                let Some((owner, _)) = owner else { continue };
                found.extend(name_reference(
                    text,
                    &masked,
                    a,
                    "name",
                    &owner,
                    kotlin,
                    "jpa-unknown-attribute-override",
                    "@AttributeOverride",
                ));
            }
            "MapsId" => {
                let Some(embedded) = class.attributes.iter().find(|x| x.id && x.relation == Relation::Embedded) else { continue };
                found.extend(name_reference(
                    text,
                    &masked,
                    a,
                    "value",
                    &element_type(&embedded.ty),
                    kotlin,
                    "jpa-unknown-maps-id",
                    "@MapsId",
                ));
            }
            "MapKey" => {
                let Some(&(start, end)) = members.iter().find(|&&(s, e)| a.start >= s && a.start < e) else { continue };
                let to_many = all.iter().any(|o| o.start >= start && o.start < end && matches!(o.name, "OneToMany" | "ManyToMany"));
                let Some((owner, _)) = owner_of(a).filter(|_| to_many) else { continue };
                found.extend(name_reference(text, &masked, a, "name", &owner, kotlin, "jpa-unknown-map-key", "@MapKey"));
            }
            "NamedAttributeNode" => {
                let inside = |outer: &str| all.iter().any(|o| o.name == outer && o.args.is_some_and(|(s, e)| a.start >= s && a.start < e));
                if !inside("NamedEntityGraph") || inside("NamedSubgraph") {
                    continue;
                }
                found.extend(name_reference(
                    text,
                    &masked,
                    a,
                    "value",
                    &class.name,
                    kotlin,
                    "jpa-unknown-entity-graph-node",
                    "@NamedAttributeNode",
                ));
            }
            _ => {}
        }
    }

    found
}

/// Findings for the references whose owner's attribute names are known; the first segment of a dotted path is judged.
#[cfg_attr(not(test), allow(dead_code))] // Legacy full-context fixture; not a live provider path.
pub fn inspect_name_references(references: &[NameReference], names: &dyn Fn(&str) -> Option<Vec<String>>) -> Vec<Finding> {
    references
        .iter()
        .filter_map(|r| {
            let first = r.value.split('.').next().unwrap_or(&r.value);
            if first.is_empty() || matches!(first, "key" | "value") && r.annotation == "@AttributeOverride" {
                return None;
            }

            let known = names(&r.owner)?;
            if known.iter().any(|n| n == first) {
                return None;
            }

            Some(Finding::new(
                r.start,
                r.end,
                Severity::Warning,
                r.code,
                format!("{} has no attribute '{first}' (named in {}).", r.owner, r.annotation),
            ))
        })
        .collect()
}

/// `:name` parameters of the `@Query` string literals in a repository that the method does not declare.
#[cfg_attr(not(test), allow(dead_code))] // Legacy lexical fixture; live native SQL uses bounded source-only comment handling.
pub fn inspect_query_parameters(text: &str, kotlin: bool) -> Vec<Finding> {
    query_parameter_findings(text, kotlin, false)
}

pub fn inspect_query_parameters_owned(text: &str, kotlin: bool) -> Vec<Finding> {
    query_parameter_findings(text, kotlin, true)
}

fn query_parameter_findings(text: &str, kotlin: bool, owned_native: bool) -> Vec<Finding> {
    let masked = mask(text);
    let all = annotations(&masked);
    let mut findings = Vec::new();

    let Some(interface) = words(&masked, "interface").next() else { return findings };
    let Some(open) = masked[interface..].find('{').map(|p| p + interface) else { return findings };
    let Some(close) = closing(masked.as_bytes(), open) else { return findings };

    for (start, end) in members(&masked, open + 1, close, kotlin, false) {
        let Some(query) = all.iter().find(|a| a.start >= start && a.start < end && a.name == "Query") else { continue };
        let Some((content_at, query_text)) = value_range(text, &masked, query, "value").and_then(|r| plain_literal(text, r, kotlin)) else {
            continue;
        };

        let decl = without_annotations(&masked, start, end);
        let Some(paren) = decl.find('(') else { continue };
        let Some(paren_close) = closing(decl.as_bytes(), paren) else { continue };
        let Some(declared) = parameter_names(text, &masked, &all, start + paren + 1, start + paren_close, kotlin) else { continue };

        let parameters = if owned_native {
            // Source escapes require a decoded-offset map, which this lexical
            // provider does not have. Dynamic flag/expression contexts are unknown.
            if query_text.contains('\\') || query_text.contains("#{") || query_text.contains("${") { continue; }
            let native = value_range(text, &masked, query, "nativeQuery").map(|(start, end)| masked[start..end].trim());
            match native {
                Some("true") => { let Some(parameters) = native_query_parameters(query_text) else { continue; }; parameters }
                None | Some("false") => query_parameters(query_text),
                _ => continue,
            }
        } else { query_parameters(query_text) };
        for (offset, name) in parameters {
            if declared.iter().any(|d| *d == name) {
                continue;
            }

            let at = content_at + offset;
            findings.push(Finding::new(
                at,
                at + 1 + name.len(),
                Severity::Warning,
                "jpa-unknown-query-parameter",
                format!("The method has no parameter named '{name}'. Name the parameter or annotate it with @Param(\"{name}\")."),
            ));
        }
    }

    findings
}

/// The names the parameters in `start..end` bind under: a `@Param` value, else the parameter name; `None` when one is not spelled out.
pub(crate) fn parameter_names(text: &str, masked: &str, all: &[Annotation], start: usize, end: usize, kotlin: bool) -> Option<Vec<String>> {
    let mut names = Vec::new();

    for (offset, part) in split_top_level(&masked[start..end]).into_iter().filter(|(_, p)| !p.trim().is_empty()) {
        let at = start + offset;
        let param = all.iter().find(|a| a.name == "Param" && a.start >= at && a.start < at + part.len());

        if let Some(param) = param {
            let (_, value) = plain_literal(text, value_range(text, masked, param, "value")?, kotlin)?;
            names.push(value.to_owned());
            continue;
        }

        let bare = without_annotations(masked, at, at + part.len());
        let bare = if kotlin { bare.split(':').next()?.to_owned() } else { bare };
        let name: String =
            bare.trim_end().bytes().rev().take_while(|&c| is_ident(c)).map(char::from).collect::<Vec<_>>().into_iter().rev().collect();
        if name.is_empty() {
            return None;
        }
        names.push(name);
    }

    Some(names)
}

/// Standard SQL literal/comment subset only, preserving byte offsets. Unknown
/// dialect quoting, dynamic expressions, nested/unclosed comments/quotes are quiet.
fn native_query_parameters(query: &str) -> Option<Vec<(usize, String)>> {
    let bytes = query.as_bytes();
    let mut masked = bytes.to_vec();
    let mut i = 0;
    while i < bytes.len() {
        let start = i;
        match bytes[i] {
            b'\'' | b'"' => {
                let quote = bytes[i]; i += 1;
                loop {
                    let next = *bytes.get(i)?;
                    if next == quote {
                        if bytes.get(i+1) == Some(&quote) { i += 2; continue; }
                        i += 1; break;
                    }
                    i += 1;
                }
                masked[start..i].fill(b' ');
            }
            b'-' if bytes.get(i+1) == Some(&b'-') => {
                i += 2;
                while i < bytes.len() && !matches!(bytes[i], b'\n' | b'\r') { i += 1; }
                masked[start..i].fill(b' ');
            }
            b'/' if bytes.get(i+1) == Some(&b'*') => {
                if matches!(bytes.get(i+2), Some(b'!' | b'+')) { return None; }
                i += 2;
                loop {
                    let next = *bytes.get(i)?;
                    if next == b'/' && bytes.get(i+1) == Some(&b'*') { return None; }
                    if next == b'*' && bytes.get(i+1) == Some(&b'/') { i += 2; break; }
                    i += 1;
                }
                masked[start..i].fill(b' ');
            }
            b'$' | b'`' | b'[' => return None,
            b'#' if bytes.get(i+1) == Some(&b'{') => return None,
            b'*' if bytes.get(i+1) == Some(&b'/') => return None,
            _ => i += 1,
        }
    }
    Some(query_parameters(std::str::from_utf8(&masked).ok()?))
}

/// `(offset of the colon, name)` of each named parameter of a query string, skipping quoted text, `::` casts and SpEL.
fn query_parameters(query: &str) -> Vec<(usize, String)> {
    let b = query.as_bytes();
    let mut found = Vec::new();
    let mut quoted = false;
    let mut i = 0;

    while i < b.len() {
        match b[i] {
            b'\'' => quoted = !quoted,
            b':' if !quoted => {
                if b.get(i + 1) == Some(&b':') {
                    i += 2;
                    continue;
                }

                let length = b[i + 1..].iter().take_while(|&&c| is_ident(c)).count();
                if length > 0 && !b[i + 1].is_ascii_digit() && (i == 0 || b[i - 1] != b':') {
                    found.push((i, query[i + 1..i + 1 + length].to_owned()));
                }
            }
            _ => {}
        }

        i += 1;
    }

    found
}

/// Names `@Query(name = "...")` and `createNamedQuery("...")` refer to, as (name, start, end) of the literal's content.
#[cfg_attr(not(test), allow(dead_code))] // Legacy full-context fixture; not a live provider path.
pub fn named_query_references(text: &str, kotlin: bool) -> Vec<(String, usize, usize)> {
    let masked = mask(text);
    let mut found = Vec::new();

    for a in annotations(&masked).iter().filter(|a| a.name == "Query") {
        if let Some((at, name)) = value_range(text, &masked, a, "name").and_then(|r| plain_literal(text, r, kotlin)) {
            found.push((name.to_owned(), at, at + name.len()));
        }
    }

    found.extend(call_literals(text, &masked, &["createNamedQuery", "createNamedStoredProcedureQuery"], kotlin));

    found
}

/// The first string literal argument of each call to one of `calls`, as (value, start, end) of the literal's content.
#[cfg_attr(not(test), allow(dead_code))] // Legacy full-context fixture; not a live provider path.
fn call_literals(text: &str, masked: &str, calls: &[&str], kotlin: bool) -> Vec<(String, usize, usize)> {
    let mut found = Vec::new();

    for call in calls {
        for at in words(masked, call) {
            let open = at + call.len();
            if !masked[open..].trim_start().starts_with('(') {
                continue;
            }

            let paren = open + masked[open..].find('(').unwrap_or(0);
            let end = closing(masked.as_bytes(), paren).unwrap_or(masked.len());
            let first = text[paren + 1..end].split(',').next().unwrap_or("");
            let lead = first.len() - first.trim_start().len();
            let from = paren + 1 + lead;
            if let Some((content_at, name)) = plain_literal(text, (from, from + first.trim().len()), kotlin) {
                found.push((name.to_owned(), content_at, content_at + name.len()));
            }
        }
    }

    found
}

/// Names `@EntityGraph("...")` and `getEntityGraph("...")` refer to, as (name, start, end) of the literal's content.
#[cfg_attr(not(test), allow(dead_code))] // Legacy full-context fixture; not a live provider path.
pub fn entity_graph_references(text: &str, kotlin: bool) -> Vec<(String, usize, usize)> {
    let masked = mask(text);
    let mut found = Vec::new();

    for a in annotations(&masked).iter().filter(|a| a.name == "EntityGraph") {
        if let Some((at, name)) = value_range(text, &masked, a, "value").and_then(|r| plain_literal(text, r, kotlin))
            && !name.is_empty()
        {
            found.push((name.to_owned(), at, at + name.len()));
        }
    }

    found.extend(call_literals(text, &masked, &["getEntityGraph"], kotlin));

    found
}

/// The first segment of each `@EntityGraph(attributePaths = ...)` literal, judged against `entity`.
#[cfg_attr(not(test), allow(dead_code))] // Legacy full-context fixture; not a live provider path.
pub fn entity_graph_paths(text: &str, kotlin: bool, entity: &str) -> Vec<NameReference> {
    let masked = mask(text);
    let mut found = Vec::new();

    for a in annotations(&masked).iter().filter(|a| a.name == "EntityGraph") {
        let Some((start, end)) = value_range(text, &masked, a, "attributePaths") else { continue };
        let mut at = start;

        while let Some(open) = text[at..end].find('"').map(|p| p + at) {
            let Some(close) = text[open + 1..end].find('"').map(|p| p + open + 1) else { break };
            at = close + 1;

            if let Some((content_at, value)) = plain_literal(text, (open, close + 1), kotlin) {
                found.push(NameReference {
                    owner: entity.to_owned(),
                    value: value.to_owned(),
                    start: content_at,
                    end: content_at + value.len(),
                    code: "jpa-unknown-entity-graph-path",
                    annotation: "@EntityGraph",
                });
            }
        }
    }

    found
}

#[cfg_attr(not(test), allow(dead_code))] // Legacy full-context fixture; not a live provider path.
pub fn unknown_entity_graph(name: &str, start: usize, end: usize) -> Finding {
    Finding::new(start, end, Severity::Warning, "jpa-unknown-entity-graph", format!("No @NamedEntityGraph named '{name}' in the project."))
}

/// Names declared by `@NamedEntityGraph` in `text`; `None` when one has no literal name.
pub fn declared_entity_graphs(text: &str, kotlin: bool) -> Option<Vec<String>> {
    let masked = mask(text);
    let mut names = Vec::new();

    for a in annotations(&masked).iter().filter(|a| a.name == "NamedEntityGraph") {
        let (_, name) = value_range(text, &masked, a, "name").and_then(|r| plain_literal(text, r, kotlin))?;
        names.push(name.to_owned());
    }

    Some(names)
}

#[cfg_attr(not(test), allow(dead_code))] // Legacy full-context fixture; not a live provider path.
pub fn unknown_named_query(name: &str, start: usize, end: usize) -> Finding {
    Finding::new(
        start,
        end,
        Severity::Warning,
        "jpa-unknown-named-query",
        format!("No @NamedQuery or @NamedNativeQuery named '{name}' in the project."),
    )
}

/// Names declared by `@NamedQuery`, `@NamedNativeQuery` and `@NamedStoredProcedureQuery` in `text`; `None` when one is not a literal.
pub fn declared_named_queries(text: &str, kotlin: bool) -> Option<Vec<String>> {
    let masked = mask(text);
    let mut names = Vec::new();

    for a in annotations(&masked).iter().filter(|a| matches!(a.name, "NamedQuery" | "NamedNativeQuery" | "NamedStoredProcedureQuery")) {
        let (_, name) = value_range(text, &masked, a, "name").and_then(|r| plain_literal(text, r, kotlin))?;
        names.push(name.to_owned());
    }

    Some(names)
}

const DECLARATIONS: &[&str] = &["class", "interface", "enum", "record"];

fn declaration_start(masked: &str, at: usize) -> usize {
    masked[..at].rfind(['{', '}', ';']).map_or(0, |p| p + 1)
}

fn enclosing_open(masked: &str, at: usize) -> Option<usize> {
    let mut open = Vec::new();

    for (i, c) in masked[..at].bytes().enumerate() {
        match c {
            b'{' => open.push(i),
            b'}' => {
                open.pop();
            }
            _ => {}
        }
    }

    open.pop()
}

/// What JPA requires of the declaration an `@Entity` sits on: a class, with one persistence role, not an inner class.
pub fn inspect_class_signature(text: &str, kotlin: bool) -> Vec<Finding> {
    class_signature_findings(text,kotlin,false)
}
pub fn inspect_class_signature_owned(text: &str, kotlin: bool) -> Vec<Finding> {
    class_signature_findings(text,kotlin,true)
}
fn class_signature_findings(text: &str, kotlin: bool, owned: bool) -> Vec<Finding> {
    let masked = mask(text);
    let all = annotations(&masked);
    let mut findings = Vec::new();
    if owned && (all.len()>256 || !crate::attribute_info::balanced(&masked)) {return findings;}
    let direct=|a:&Annotation<'_>| !all.iter().any(|outer|outer.args.is_some_and(|(s,e)|a.start>=s && a.start<e));
    for entity in all.iter().filter(|a| a.name == "Entity") {
        if owned && !direct(entity) {continue;}
        let Some((keyword_at, keyword)) = declaration_keyword(&masked, entity.end(), DECLARATIONS) else { continue };
        let keyword_end = keyword_at + keyword.len();
        let start = declaration_start(&masked, entity.start);
        let modifiers = &masked[start..keyword_at];
        if owned {
            let prefix=without_annotations(&masked,start,keyword_at);
            if prefix.split_whitespace().any(|word| !["public","private","protected","static","final","abstract","sealed","non-sealed","strictfp","annotation","data","inner","open","expect","actual","internal","value","@"].contains(&word)) {continue;}
        }
        for role in all.iter().filter(|a| a.start >= start && a.start < keyword_at && matches!(a.name, "Embeddable" | "MappedSuperclass") && (!owned || direct(a))) {
            let second = if role.start > entity.start { role } else { entity };
            findings.push(Finding::new(
                second.start,
                second.end(),
                Severity::Warning,
                "jpa-entity-multiple-roles",
                format!("A class is either an entity or @{}, not both.", role.name),
            ));
        }

        let annotation_type = (keyword == "interface" && masked[..keyword_at].trim_end().ends_with('@'))
            || (kotlin && keyword == "class" && has_word(modifiers, "annotation"));
        let not_a_class = match keyword {
            _ if annotation_type => Some("an annotation"),
            "interface" => Some("an interface"),
            "enum" => Some("an enum"),
            _ => None,
        };

        if let Some(what) = not_a_class {
            findings.push(Finding::new(
                keyword_at,
                keyword_end,
                Severity::Warning,
                "jpa-entity-not-a-class",
                format!("An entity must be a class, but this is {what}."),
            ));
            continue;
        }

        if keyword != "class" {
            continue;
        }

        let Some(open) = enclosing_open(&masked, entity.start) else { continue };
        let header = &masked[declaration_start(&masked, open)..open];
        if declaration_keyword(header, 0, DECLARATIONS).is_none_or(|(_, k)| k != "class") {
            continue;
        }

        let nested_ok = if kotlin { !has_word(modifiers, "inner") } else { has_word(modifiers, "static") };
        if !nested_ok {
            let how = if kotlin { "not be inner" } else { "be static" };
            findings.push(Finding::new(
                keyword_at,
                keyword_end,
                Severity::Warning,
                "jpa-entity-nested",
                format!("A nested entity class must {how}: an inner class cannot be instantiated without its outer instance."),
            ));
        }
    }

    findings
}

/// Simple class name and byte range of each `X.class` (Kotlin `X::class`) inside `@EntityListeners(...)`.
#[cfg_attr(not(test), allow(dead_code))] // Legacy full-context fixture; not a live provider path.
pub fn listener_references(text: &str) -> Vec<(String, usize, usize)> {
    let masked = mask(text);
    let mut found = Vec::new();

    for listeners in annotations(&masked).iter().filter(|a| a.name == "EntityListeners") {
        let Some((start, end)) = listeners.args else { continue };

        for at in words(&masked[start..end], "class") {
            let before = masked[..start + at].trim_end();
            let Some(before) = before.strip_suffix("::").or_else(|| before.strip_suffix('.')) else { continue };
            let before = before.trim_end();
            let length = before.bytes().rev().take_while(|&c| is_ident(c)).count();
            if length == 0 {
                continue;
            }

            let name_start = before.len() - length;
            found.push((before[name_start..].to_owned(), name_start, before.len()));
        }
    }

    found
}

#[cfg_attr(not(test), allow(dead_code))] // Legacy full-context fixture; not a live provider path.
pub fn has_callbacks(text: &str) -> bool {
    annotations(&mask(text)).iter().any(|a| CALLBACKS.contains(&a.name))
}

/// Whether `text` declares class `name` with no callback and no superclass that could supply one.
#[cfg_attr(not(test), allow(dead_code))] // Legacy full-context fixture; not a live provider path.
pub fn is_empty_listener(text: &str, name: &str, kotlin: bool) -> bool {
    if has_callbacks(text) {
        return false;
    }

    let masked = mask(text);
    let Some((at, "class")) = declaration_keyword(&masked, 0, DECLARATIONS) else { return false };
    let header_end = masked[at..].find('{').map_or(masked.len(), |p| p + at);
    let header = masked[at + "class".len()..header_end].trim_start();

    let Some(rest) = header.strip_prefix(name) else { return false };
    if rest.bytes().next().is_some_and(is_ident) {
        return false;
    }

    let extended = if kotlin { kotlin_supertype(rest).is_some() } else { has_word(rest, "extends") };
    !extended
}

/// Whether class `name` in `text` declares constructors but none that is public and takes no argument.
#[cfg_attr(not(test), allow(dead_code))] // Legacy full-context fixture; not a live provider path.
pub fn lacks_default_constructor(text: &str, name: &str, kotlin: bool) -> bool {
    let masked = mask(text);
    let Some((at, "class")) = declaration_keyword(&masked, 0, DECLARATIONS) else { return false };
    let Some(rest) = masked[at + "class".len()..].trim_start().strip_prefix(name) else { return false };
    if rest.bytes().next().is_some_and(is_ident) {
        return false;
    }

    let rest_at = masked.len() - rest.len();
    let Some(open) = masked[rest_at..].find('{').map(|p| p + rest_at) else { return false };
    let Some(close) = closing(masked.as_bytes(), open) else { return false };
    let header = &masked[rest_at..open];
    let body = members(&masked, open + 1, close, kotlin, false);

    if kotlin {
        let secondary_default = body.iter().any(|&(s, e)| {
            let decl = without_annotations(&masked, s, e);
            let decl = decl.trim_start();
            decl.strip_prefix("constructor").is_some_and(|r| r.trim_start().starts_with("()"))
        });
        if has_word(header, "constructor") {
            return false;
        }

        let primary = header
            .trim_start()
            .starts_with('(')
            .then(|| header.find('('))
            .flatten()
            .and_then(|p| closing(header.as_bytes(), p).map(|c| &header[p + 1..c]));
        let primary_needs_arguments =
            primary.is_some_and(|params| split_top_level(params).iter().any(|(_, p)| !p.trim().is_empty() && !p.contains('=')));

        return primary_needs_arguments && !secondary_default;
    }

    let mut constructors = Vec::new();
    for &(s, e) in &body {
        let decl = without_annotations(&masked, s, e);
        let Some(paren) = decl.find('(') else { continue };
        let head = decl[..paren].trim_end();
        if !head.ends_with(name) || head.len() > name.len() && is_ident(head.as_bytes()[head.len() - name.len() - 1]) {
            continue;
        }

        let modifiers = head[..head.len() - name.len()].split_whitespace().collect::<Vec<_>>();
        if !modifiers.iter().all(|m| matches!(*m, "public" | "protected" | "private")) {
            continue;
        }

        let params_end = closing(decl.as_bytes(), paren).unwrap_or(decl.len());
        constructors.push(modifiers.contains(&"public") && decl[paren + 1..params_end].trim().is_empty());
    }

    !constructors.is_empty() && !constructors.contains(&true)
}

#[cfg_attr(not(test), allow(dead_code))] // Legacy full-context fixture; not a live provider path.
pub fn listener_without_constructor(start: usize, end: usize, name: &str) -> Finding {
    Finding::new(
        start,
        end,
        Severity::Info,
        "jpa-listener-constructor",
        format!(
            "{name} has no public constructor without arguments, which JPA needs to create the listener (a Spring-managed listener is exempt)."
        ),
    )
}

#[cfg_attr(not(test), allow(dead_code))] // Legacy full-context fixture; not a live provider path.
pub fn unused_listener(start: usize, end: usize, name: &str) -> Finding {
    Finding::new(
        start,
        end,
        Severity::Warning,
        "jpa-listener-without-callbacks",
        format!("{name} declares none of the lifecycle callbacks (@PrePersist, @PostLoad, ...), so this listener never runs."),
    )
}

pub const CALLBACKS: &[&str] = &["PrePersist", "PostPersist", "PreUpdate", "PostUpdate", "PreRemove", "PostRemove", "PostLoad"];

/// Lifecycle callbacks with a signature JPA rejects: on an entity none take arguments, on a listener exactly one.
#[cfg_attr(not(test), allow(dead_code))] // Legacy lexical fixture; live checks require proved current Entity ownership.
pub fn inspect_callbacks(text: &str, kotlin: bool) -> Vec<Finding> {
    let masked = mask(text);
    let all = annotations(&masked);

    let on_entity = all.iter().any(|a| a.name == "Entity" || a.name == "MappedSuperclass");
    if all.iter().any(|a| a.name == "Embeddable") || !(on_entity || masked.contains("persistence")) {
        return Vec::new();
    }

    let expected = usize::from(!on_entity);
    let mut findings = Vec::new();

    for (start, end) in class_members(&masked, kotlin) {
        if !all.iter().any(|a| a.start >= start && a.start < end && CALLBACKS.contains(&a.name)) {
            continue;
        }

        let decl = without_annotations(&masked, start, end);
        let Some(method) = method_signature(&decl, kotlin) else { continue };
        let role = if on_entity { "An entity callback takes no parameters" } else { "A listener callback takes exactly one parameter" };

        let problem = match method.returns_void {
            Some(false) => Some(format!("{role} and returns void, but {} returns a value.", method.name)),
            _ if method.parameters != expected => {
                Some(format!("{role} and returns void, but {} has {} parameters.", method.name, method.parameters))
            }
            _ => None,
        };

        if let Some(message) = problem {
            let at = start + method.name_at;
            findings.push(Finding::new(at, at + method.name.len(), Severity::Error, "entity-listener-signature", message));
        }
    }

    findings
}

struct Method<'a> {
    name: &'a str,
    name_at: usize,
    parameters: usize,
    /// `None` when the return type is inferred and can't be read.
    returns_void: Option<bool>,
}

fn method_signature(decl: &str, kotlin: bool) -> Option<Method<'_>> {
    let open = decl.find('(')?;
    let close = closing(decl.as_bytes(), open)?;
    let before = decl[..open].trim_end();
    let name_len = before.bytes().rev().take_while(|&c| is_ident(c)).count();
    let name_at = before.len() - name_len;
    let name = &before[name_at..];
    let head = before[..name_at].trim_end();

    if name.is_empty() || (kotlin && !has_word(head, "fun")) {
        return None;
    }

    let parameters = split_top_level(&decl[open + 1..close]).iter().filter(|(_, p)| !p.trim().is_empty()).count();
    let tail = decl[close + 1..].trim_start();

    let returns_void = if kotlin {
        match tail.strip_prefix(':') {
            Some(ty) => Some(ty.split(['{', '=']).next().unwrap_or("").trim() == "Unit"),
            None => (!tail.starts_with('=')).then_some(true),
        }
    } else {
        let is_void = head.ends_with("void") && !head[..head.len() - 4].ends_with(|c: char| c.is_alphanumeric() || c == '_');
        if head.is_empty() || head.ends_with(['.', ',']) {
            return None;
        }
        Some(is_void)
    };

    Some(Method { name, name_at, parameters, returns_void })
}

/// Derived queries naming unknown properties, and an id type that isn't the entity's.
#[cfg_attr(not(test), allow(dead_code))] // Legacy full-context fixture; not a live provider path.
pub fn inspect_repository(repo: &Repository, entity_id: Option<&str>, props: Props) -> Vec<Finding> {
    let mut findings = Vec::new();

    if props(&repo.entity).is_none() {
        return findings;
    }

    let same = |a: &str, b: &str| {
        let java = |t: &str| {
            boxed(match t {
                "Int" => "Integer",
                "Char" => "Character",
                other => other,
            })
        };
        java(a) == java(b)
    };

    if let Some(id) = entity_id.filter(|id| !same(id, &repo.id_type)) {
        findings.push(Finding::new(
            repo.id_start,
            repo.id_end,
            Severity::Error,
            "jpa-repository-id",
            format!("The id of {} is {id}, not {}.", repo.entity, repo.id_type),
        ));
    }

    for method in repo.methods.iter().filter(|m| !m.declared_query) {
        if let Some(message) = problem(&method.name, &repo.entity, props) {
            findings.push(Finding::new(method.start, method.end, Severity::Error, "jpa-derived-query", message));
        }
    }

    findings
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::queries::{Prop, parse_repository};

    const ENTITY: &str = "package shop;

import jakarta.persistence.Entity;
import jakarta.persistence.Id;
import jakarta.persistence.ManyToOne;
import lombok.Data;

// @OneToMany(fetch = FetchType.EAGER) in a comment is ignored
@Data
@Entity
public final class Order {
    @Id
    private Long id;

    @ManyToOne
    private Customer customer;

    @ManyToOne(fetch = FetchType.LAZY)
    private Customer lazy;

    @OneToOne(mappedBy = \"order\")
    private Invoice invoice;
}
";

    fn codes(text: &str, kotlin: bool) -> Vec<&'static str> {
        inspect_entity(text, kotlin).iter().map(|f| f.code).collect()
    }

    fn apply(text: &str, mut edits: Vec<Edit>) -> String {
        let mut out = text.to_string();
        edits.sort_by_key(|e| std::cmp::Reverse(e.start));
        for e in edits {
            out.replace_range(e.start..e.end, &e.text);
        }
        out
    }

    fn fixed(text: &str, code: &str, kotlin: bool) -> String {
        let finding = inspect_entity(text, kotlin).into_iter().find(|f| f.code == code).unwrap();
        apply(text, finding.fix.unwrap().1)
    }

    #[test]
    fn reports_entity_problems_and_ignores_comments_and_inverse_sides() {
        assert_eq!(codes(ENTITY, false), ["jpa-final-entity", "jpa-lombok-data", "jpa-eager-to-one"]);
        assert!(codes(&ENTITY.replace("@Entity", "@Embeddable"), false).is_empty());
    }

    #[test]
    fn entity_without_id_warns() {
        let found = missing_id("@Entity\nclass A {\n  String name;\n}\n").unwrap();
        assert_eq!(found.code, "entity-missing-id");
        assert_eq!(&"@Entity\nclass A {\n  String name;\n}\n"[found.start..found.end], "@Entity");
        assert!(missing_id("@Entity\ndata class A(val name: String)\n").is_some());
    }

    #[test]
    fn entity_with_embedded_id_is_quiet() {
        assert!(missing_id("@Entity\nclass A {\n  @EmbeddedId Key id;\n}\n").is_none());
        assert!(missing_id("@Entity\nclass A(@EmbeddedId val id: Key)\n").is_none());
        assert!(missing_id("@Entity\n@IdClass(Key.class)\nclass A {}\n").is_none());
        assert!(missing_id("@Entity\nclass A {\n  @Id\n  public Long getId() { return 1L; }\n}\n").is_none());
        assert!(missing_id("@Embeddable\nclass A {}\n").is_none());
    }

    fn types(text: &str) -> Vec<String> {
        inspect_attribute_types(text, false, &|n| n == "Customer").iter().map(|f| f.message.clone()).collect()
    }

    #[test]
    fn attribute_type_flags_documented_mismatches() {
        let text = "@Entity\nclass A {\n  @Temporal(TemporalType.DATE) LocalDate day;\n  @Enumerated String kind;\n  @Enumerated(EnumType.STRING) private Customer who;\n  @Version String stamp;\n}\n";
        let found = inspect_attribute_types(text, false, &|n| n == "Customer");
        assert_eq!(found.len(), 4);
        assert!(found.iter().all(|f| f.code == "attribute-type"));
        assert_eq!(&text[found[0].start..found[0].end], "@Temporal(TemporalType.DATE)");
    }

    #[test]
    fn attribute_type_is_quiet_for_library_or_dynamic_name() {
        let text = "@Entity\nclass A {\n  @Temporal(TemporalType.DATE) Date a;\n  @Temporal(TemporalType.DATE) Calendar b;\n  @Temporal(TemporalType.DATE) Vendor.Stamp c;\n  @Enumerated Status d;\n  @Enumerated(EnumType.STRING) com.acme.Kind e;\n  @Version long f;\n  @Version Integer g;\n  @Version java.sql.Timestamp h;\n  @Version Stamp i;\n}\n";
        assert!(types(text).is_empty(), "{:?}", types(text));
        assert!(inspect_attribute_types("@Entity\nclass A(@Version val v: Long, @Enumerated val e: Mode)\n", true, &|_| false).is_empty());
        assert_eq!(inspect_attribute_types("@Entity\nclass A(@Version val v: Boolean)\n", true, &|_| false).len(), 1);
    }

    #[test]
    fn attribute_type_range_counts_utf16_columns() {
        let text = "@Entity\nclass A {\n  /* \u{1F600} */ @Version String v;\n}\n";
        let d = inspect_attribute_types(text, false, &|_| false)[0].diagnostic(text);
        assert_eq!(d.range.start.character, 11);
    }

    fn callbacks(text: &str) -> Vec<String> {
        inspect_callbacks(text, false).iter().map(|f| f.message.clone()).collect()
    }

    #[test]
    fn listener_method_with_wrong_parameters_warns() {
        let entity = "@Entity\nclass A {\n  @Id Long id;\n  @PrePersist void ok() {}\n  @PostLoad void bad(String s) {}\n  @PreUpdate public int worse() { return 1; }\n}\n";
        let found = inspect_callbacks(entity, false);
        assert_eq!(found.len(), 2);
        assert!(found.iter().all(|f| f.code == "entity-listener-signature"));
        assert_eq!(&entity[found[0].start..found[0].end], "bad");

        let listener = "import jakarta.persistence.*;\nclass Audit {\n  @PrePersist void none() {}\n  @PostPersist void one(Object o) {}\n  @PreRemove void two(Object o, int x) {}\n  @PostRemove int ret(Object o) { return 1; }\n}\n";
        assert_eq!(callbacks(listener).len(), 3);

        let kotlin = "@Entity\nclass A {\n  @PrePersist fun a(x: Int) {}\n  @PostLoad fun b(): String = \"\"\n  @PostUpdate fun c(): Unit {}\n  @PreRemove fun d() = Unit\n}\n";
        assert_eq!(inspect_callbacks(kotlin, true).len(), 2);
    }

    #[test]
    fn entity_listener_signature_is_quiet_for_library_or_dynamic_name() {
        assert!(callbacks("@Entity\nclass A {\n  @PrePersist void a() {}\n  void other(String s) {}\n}\n").is_empty());
        assert!(
            callbacks("import jakarta.persistence.*;\nclass L {\n  @PrePersist void a(Object o) {}\n  @Autowired void b(String s) {}\n}\n")
                .is_empty()
        );
        assert!(callbacks("class Other {\n  @PrePersist void a(Object o, Object p) {}\n}\n").is_empty(), "no persistence import");
        assert!(callbacks("@Embeddable\nclass E {\n  @PrePersist void a(Object o) {}\n}\n").is_empty());
    }

    #[test]
    fn fixes_fetching_with_the_import() {
        let lazy = fixed(ENTITY, "jpa-eager-to-one", false);
        assert!(lazy.contains("@ManyToOne(fetch = FetchType.LAZY)\n    private Customer customer;"));
        assert!(lazy.contains("import lombok.Data;\nimport jakarta.persistence.FetchType;\n"));

        let eager = "import javax.persistence.*\n\n@Entity\nclass A {\n  @Id var id: Long? = null\n  @OneToMany(mappedBy = \"a\", fetch = FetchType.EAGER) val b: List<B> = listOf()\n}\n";
        let fixed_eager = fixed(eager, "jpa-eager-fetch", true);
        assert!(fixed_eager.contains("@OneToMany(mappedBy = \"a\", fetch = FetchType.LAZY)"));
        assert!(fixed_eager.starts_with("import javax.persistence.*\n\n@Entity"), "no import for a wildcard");

        let with_args = "@Entity class A { @Id Long id; @ManyToOne(optional = false) B b; }";
        assert!(fixed(with_args, "jpa-eager-to-one", false).contains("@ManyToOne(fetch = FetchType.LAZY, optional = false)"));
    }

    #[test]
    fn equals_and_hash_code_fix_removes_the_annotation_line_or_only_the_annotation() {
        let alone = "@Entity\n@EqualsAndHashCode(of = \"id\")\nclass A { @Id Long id; }";
        assert_eq!(fixed(alone, "jpa-lombok-equals", false), "@Entity\nclass A { @Id Long id; }");
        let inline = "@Entity @EqualsAndHashCode class A { @Id Long id; }";
        assert_eq!(fixed(inline, "jpa-lombok-equals", false), "@Entity  class A { @Id Long id; }");
    }

    #[test]
    fn replaces_data_and_drops_final() {
        let getters = fixed(ENTITY, "jpa-lombok-data", false);
        assert!(getters.contains("\n@Getter\n@Setter\n@Entity\n"));
        assert!(getters.contains("import lombok.Data;\nimport lombok.Getter;\nimport lombok.Setter;\n"));
        assert!(fixed(ENTITY, "jpa-final-entity", false).contains("\npublic class Order {"));
    }

    fn props(class: &str) -> Option<Vec<Prop>> {
        (class == "Order").then(|| vec![Prop { name: "number".into(), ty: "String".into(), fqcn: None, target: None }])
    }

    #[test]
    fn checks_repository_methods_and_id_type() {
        let text = "interface R extends JpaRepository<Order, Integer> {\n  List<Order> findByNumber(String n);\n  List<Order> findByCode(String c);\n  @Query(\"x\") List<Order> findByWhatever();\n}";
        let repo = parse_repository(text, false).unwrap();
        let found = inspect_repository(&repo, Some("long"), &props);
        let messages: Vec<&str> = found.iter().map(|f| f.message.as_str()).collect();
        assert_eq!(messages, ["The id of Order is long, not Integer.", "Order has no property 'code' (at 'Code')."]);
        assert_eq!(&text[found[1].start..found[1].end], "findByCode");

        assert!(inspect_repository(&repo, Some("Integer"), &props).len() == 1);
        let kotlin = parse_repository("interface R : JpaRepository<Order, Int>", true).unwrap();
        assert!(inspect_repository(&kotlin, Some("Integer"), &props).is_empty());
        let unknown = parse_repository("interface R : JpaRepository<Other, Int> { fun findByX(): List<Other> }", true).unwrap();
        assert!(inspect_repository(&unknown, None, &props).is_empty());
    }
}
