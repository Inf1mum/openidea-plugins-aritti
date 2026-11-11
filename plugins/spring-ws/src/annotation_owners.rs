//! Explicit written annotation ownership; no classpath or meta-annotation resolution.
use spring_text::java::{self, Annotation, JavaFile, TypeDecl};
use std::collections::{BTreeMap, BTreeSet};
const ENDPOINT: &str = "org.springframework.ws.server.endpoint.annotation.Endpoint";
const ROOT: &str = "org.springframework.ws.server.endpoint.annotation.PayloadRoot";
const ROOTS: &str = "org.springframework.ws.server.endpoint.annotation.PayloadRoots";
struct Import {
    path: String,
    is_static: bool,
    aliased: bool,
}
pub(crate) struct Owners {
    shadowed: BTreeSet<String>,
    imports: BTreeMap<String, Vec<Import>>,
}
fn alias(import: &java::Import, text: &str, kotlin: bool) -> Option<String> {
    if !kotlin {
        return None;
    }
    let tail = text.get(import.end..)?.split('\n').next()?;
    let tokens = java::tokens(tail);
    match tokens.as_slice() {
        [a, name, ..] if a.s == "as" && name.is_ident() => Some(name.s.into()),
        _ => None,
    }
}
fn generic_names(code: &str, names: &mut BTreeSet<String>) {
    let mut depth = 0usize;
    for token in java::tokens(code) {
        match token.s {
            "<" => depth += 1,
            ">" => depth = depth.saturating_sub(1),
            _ if depth > 0 && token.is_ident() => {
                names.insert(token.s.into());
            }
            _ => {}
        }
    }
}
/// Conservative local generic/typealias collisions for a written constant owner.
/// No classpath or lexical type resolution is implied.
pub(crate) fn constant_owner_shadowed(file: &JavaFile, text: &str, name: &str, kotlin: bool) -> bool {
    let mut names = BTreeSet::new();
    for ty in &file.types {
        let header = text.get(ty.at + ty.name.len()..ty.end)
            .and_then(|s| s.split('{').next()).unwrap_or("");
        generic_names(header, &mut names);
        for method in ty.methods() {
            generic_names(text.get(method.start..method.at).unwrap_or(""), &mut names);
        }
    }
    if kotlin {
        for pair in java::tokens(text).windows(2) {
            if pair[0].s == "typealias" && pair[1].is_ident() {
                names.insert(pair[1].s.into());
            }
        }
    }
    names.contains(name)
}
impl Owners {
    /// Parse source ownership once per callback, not once per container member.
    pub(crate) fn new(file: &JavaFile, text: &str, kotlin: bool) -> Self {
        let mut shadowed: BTreeSet<_> = file.types.iter().map(|t| t.name.clone()).collect();
        if kotlin {
            for pair in java::tokens(text).windows(2) {
                if pair[0].s == "typealias" && pair[1].is_ident() {
                    shadowed.insert(pair[1].s.into());
                }
            }
        }
        for ty in &file.types {
            let header = text
                .get(ty.at + ty.name.len()..ty.end)
                .and_then(|s| s.split('{').next())
                .unwrap_or("");
            generic_names(header, &mut shadowed);
            for method in ty.methods() {
                generic_names(
                    text.get(method.start..method.at).unwrap_or(""),
                    &mut shadowed,
                );
            }
        }
        let mut imports: BTreeMap<String, Vec<Import>> = BTreeMap::new();
        for import in &file.imports {
            let alias = alias(import, text, kotlin);
            let name = alias
                .clone()
                .unwrap_or_else(|| import.path.rsplit('.').next().unwrap_or("").into());
            imports.entry(name).or_default().push(Import {
                path: import.path.clone(),
                is_static: import.is_static,
                aliased: alias.is_some(),
            });
        }
        Self { shadowed, imports }
    }
    pub(crate) fn known(&self, text: &str, a: &Annotation, expected: &str) -> bool {
        let Some(tail) = text.get(a.at..) else {
            return false;
        };
        // Kotlin container constructor entries have a synthetic @ token at the
        // actual first identifier's offset, without a written @ in the source.
        let start = a.at + usize::from(tail.starts_with('@'));
        let Some(raw) = text.get(start..a.name_at + a.name.len()) else {
            return false;
        };
        let written: String = java::tokens(raw).iter().map(|t| t.s).collect();
        self.written_known(&written, expected)
    }
    pub(crate) fn resource_constructor(&self, written: &str) -> bool {
        self.written_known(written, "org.springframework.core.io.ClassPathResource")
    }
    fn written_known(&self, written: &str, expected: &str) -> bool {
        let root = expected.split('.').next().unwrap_or("");
        if written == expected {
            return !self.shadowed.contains(root) && !self.imports.contains_key(root);
        }
        let short = expected.rsplit('.').next().unwrap_or("");
        if written != short || self.shadowed.contains(short) {
            return false;
        }
        let Some(imports) = self.imports.get(short) else {
            return false;
        };
        imports
            .iter()
            .any(|i| i.path == expected && !i.is_static && !i.aliased)
            && !imports
                .iter()
                .any(|i| i.path != expected || i.is_static || i.aliased)
    }
    pub(crate) fn endpoint(&self, text: &str, ty: &TypeDecl) -> bool {
        ty.annotations.iter().any(|a| self.known(text, a, ENDPOINT))
    }
    pub(crate) fn payload(&self, text: &str, a: &Annotation) -> bool {
        let expected = match a.name.as_str() {
            "PayloadRoot" => ROOT,
            "PayloadRoots" => ROOTS,
            _ => return false,
        };
        self.known(text, a, expected)
    }
}
