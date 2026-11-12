//! Lombok inspections and the Lombok annotation editor for source-owned Java entities.
//!
//! Strict gate: every Lombok name must come from `lombok.<Name>` (explicit import or qualified use),
//! no wildcard or other Lombok import may appear, and the class must be the single owned entity of the
//! file. Anything else is unknown and the rules stay quiet. `lombok.config` is not consulted.
//! Lombok does not process Kotlin sources here, so Kotlin entities are never inspected.
use super::*;
use crate::checks::{Finding, owned_class_start};
use crate::model::{Annotation, declaration, import_edit, members, without_annotations};
use openidea_plugin::Severity;

pub const TITLE: &str = "Edit Lombok annotations…";
const HANDLED: [&str; 9] =
    ["Data", "Getter", "Setter", "ToString", "EqualsAndHashCode", "Builder", "NoArgsConstructor", "AllArgsConstructor", "RequiredArgsConstructor"];
const EDITABLE: [&str; 7] = ["Getter", "Setter", "NoArgsConstructor", "AllArgsConstructor", "Builder", "ToString", "EqualsAndHashCode"];

struct ClassAnnotation {
    name: &'static str,
    start: usize,
    end: usize,
    args: Option<(usize, usize)>,
}

struct Field {
    name: String,
    name_at: usize,
    member_start: usize,
    is_final: bool,
    initialized: bool,
    notes: Vec<String>,
    association: bool,
    lazy: bool,
    generated_id: bool,
}

struct Facts {
    masked: String,
    imports: Vec<String>,
    insert_at: usize,
    annotations: Vec<ClassAnnotation>,
    fields: Vec<Field>,
    constructors: Vec<bool>,
    methods: Vec<String>,
}

impl Facts {
    fn has(&self, name: &str) -> Option<&ClassAnnotation> {
        self.annotations.iter().find(|a| a.name == name)
    }

    fn declares(&self, method: &str) -> bool {
        self.methods.iter().any(|m| m == method)
    }

    fn imported(&self, name: &str) -> bool {
        self.imports.iter().any(|i| i == &format!("lombok.{name}"))
    }

    /// `(written form, import needed)`: short when imported or collision-free, else qualified.
    fn reference(&self, name: &str) -> (String, Option<String>) {
        if self.imported(name) {
            return (name.to_owned(), None);
        }
        if words(&self.masked, name).next().is_none() {
            return (name.to_owned(), Some(format!("lombok.{name}")));
        }
        (format!("lombok.{name}"), None)
    }
}

/// `Ok(Some(path))` for a verified Lombok name, `Ok(None)` for a foreign one, `Err` when ownership is ambiguous.
fn lombok_path(imports: &[String], written: &str) -> Result<Option<String>, ()> {
    let (qualified, rest) = match written.strip_prefix("lombok.") {
        Some(rest) => (true, rest),
        None => (false, written),
    };
    let first = rest.split('.').next().unwrap_or(rest);
    if !HANDLED.contains(&first) {
        return if qualified { Err(()) } else { Ok(None) };
    }
    if !qualified && !imports.iter().any(|i| i == &format!("lombok.{first}")) {
        return Err(());
    }
    Ok(Some(rest.to_owned()))
}

fn facts(text: &str, class: &Class) -> Option<Facts> {
    if class.kotlin || class.kind != Kind::Entity || HANDLED.contains(&class.name.as_str()) || class.name == "lombok" {
        return None;
    }
    let masked = mask(text);
    if words(&masked, "class").count() != 1 || ["interface", "enum", "record"].iter().any(|w| words(&masked, w).next().is_some()) {
        return None;
    }
    let at = owned_class_start(text, &masked, class)?;
    let close = class.body_end?;
    let open = at + masked[at..].find('{')?;
    if close <= open {
        return None;
    }

    let imports: Vec<String> = masked
        .lines()
        .filter_map(|l| l.trim().strip_prefix("import "))
        .map(|i| i.trim().trim_end_matches(';').trim().to_owned())
        .collect();
    for import in &imports {
        let simple = import.rsplit('.').next().unwrap_or(import);
        if import.starts_with("lombok.") && !HANDLED.contains(&&import["lombok.".len()..]) {
            return None;
        }
        if HANDLED.contains(&simple) && import != &format!("lombok.{simple}") {
            return None;
        }
    }

    let all = annotations(&masked);
    if all.len() > 256 {
        return None;
    }
    let nested = |a: &Annotation| all.iter().any(|o| o.args.is_some_and(|(s, e)| a.start >= s && a.start < e));
    let after_previous = masked[..at].rfind(['{', '}', ';']).map_or(0, |p| p + 1);
    let first_annotation = after_previous + masked[after_previous..].len() - masked[after_previous..].trim_start().len();
    let last_end = all.iter().filter(|a| a.start >= first_annotation && a.start < at && !nested(a)).map(|a| a.end()).max().unwrap_or(first_annotation);
    let insert_at = last_end + masked[last_end..at].len() - masked[last_end..at].trim_start().len();

    let mut class_annotations: Vec<ClassAnnotation> = Vec::new();
    for a in all.iter().filter(|a| a.start >= first_annotation && a.start < at && !nested(a)) {
        let written = &masked[a.start + 1..a.name_end];
        let Some(path) = lombok_path(&imports, written).ok()? else { continue };
        let name = *HANDLED.iter().find(|n| **n == path)?;
        if class_annotations.iter().any(|c| c.name == name) {
            return None;
        }
        class_annotations.push(ClassAnnotation { name, start: a.start, end: a.end(), args: a.args });
    }

    let mut fields = Vec::new();
    let mut constructors = Vec::new();
    let mut methods = Vec::new();
    for (start, end) in members(&masked, open + 1, close, false, false) {
        let decl = without_annotations(&masked, start, end);
        let trimmed = decl.trim();
        if trimmed.is_empty() {
            continue;
        }
        let declaration_at = decl.len() - decl.trim_start().len();
        let before_equals = decl.find('=').unwrap_or(decl.len());
        if let Some(paren) = decl[..before_equals].find('(') {
            let name: String = decl[..paren].trim_end().chars().rev().take_while(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '$')).collect::<Vec<_>>().into_iter().rev().collect();
            if name == class.name {
                let closed = crate::model::closing(masked.as_bytes(), start + paren)?;
                constructors.push(masked[start + paren + 1..closed].trim().is_empty());
            } else {
                methods.push(name);
            }
            continue;
        }
        let Some((name_at, name, _)) = declaration(&decl, false) else { continue };
        let left = &decl[..before_equals];
        let mut depth = 0i32;
        for c in left.chars() {
            match c {
                '<' => depth += 1,
                '>' => depth -= 1,
                ',' if depth == 0 => return None,
                _ => {}
            }
        }

        let mut notes = Vec::new();
        for a in annotations(&masked[start..end]).into_iter().filter(|a| a.start < declaration_at) {
            let written = &masked[start + a.start + 1..start + a.name_end];
            if let Some(path) = lombok_path(&imports, written).ok()? {
                if !["ToString.Exclude", "ToString.Include", "EqualsAndHashCode.Exclude", "EqualsAndHashCode.Include", "Getter", "Setter"].contains(&path.as_str()) {
                    return None;
                }
                notes.push(path);
            }
        }

        let attribute = class.attributes.iter().find(|a| a.name == name);
        fields.push(Field {
            name,
            name_at: start + name_at,
            member_start: start + first_blank_free(&masked[start..end]),
            is_final: words(left, "final").next().is_some(),
            initialized: before_equals < decl.len(),
            notes,
            association: attribute.is_some_and(|a| {
                matches!(a.relation, Relation::ManyToOne | Relation::OneToOne | Relation::OneToMany | Relation::ManyToMany | Relation::ElementCollection)
            }),
            lazy: attribute.is_some_and(|a| is_lazy(&masked[start..end], declaration_at, a.relation.to_many())),
            generated_id: attribute.is_some_and(|a| a.id && a.generated.is_some()),
        });
    }
    if fields.len() > 512 {
        return None;
    }

    Some(Facts { masked, imports, insert_at, annotations: class_annotations, fields, constructors, methods })
}

/// Explicit `fetch = ...LAZY`, or no `fetch` on a to-many association. Any other fetch expression is unknown.
fn is_lazy(member: &str, declaration_at: usize, to_many: bool) -> bool {
    let association = |a: &crate::model::Annotation| matches!(a.name, "ManyToOne" | "OneToOne" | "OneToMany" | "ManyToMany" | "ElementCollection");
    let found = annotations(member);
    let Some(a) = found.iter().filter(|a| a.start < declaration_at).find(|a| association(a)) else { return false };
    let args = a.args.map_or("", |(from, to)| &member[from..to]);
    let Some(at) = words(args, "fetch").next() else { return to_many };
    words(&args[at..], "LAZY").next().is_some() && words(&args[at..], "EAGER").next().is_none()
}

fn first_blank_free(slice: &str) -> usize {
    slice.len() - slice.trim_start().len()
}

struct Spec {
    only_explicit: bool,
    excluded: Vec<String>,
}

/// `None` when the arguments use a form this module does not model (`of`, non-literal flags).
fn spec(text: &str, facts: &Facts, annotation: Option<&ClassAnnotation>) -> Option<Spec> {
    let mut spec = Spec { only_explicit: false, excluded: Vec::new() };
    let Some((start, end)) = annotation.and_then(|a| a.args) else { return Some(spec) };
    let masked = &facts.masked[start..end];
    if words(masked, "of").next().is_some() {
        return None;
    }
    if let Some(at) = words(masked, "onlyExplicitlyIncluded").next() {
        let value = masked[at + "onlyExplicitlyIncluded".len()..].trim_start().strip_prefix('=')?.trim_start();
        if value.starts_with("true") {
            spec.only_explicit = true;
        } else if !value.starts_with("false") {
            return None;
        }
    }
    let mut parts = text[start..end].split('"');
    parts.next();
    spec.excluded = parts.step_by(2).map(str::to_owned).collect();
    Some(spec)
}

fn included(field: &Field, owner: &str, spec: &Spec) -> bool {
    !field.name.starts_with('$')
        && !field.notes.iter().any(|n| n == &format!("{owner}.Exclude"))
        && !spec.excluded.contains(&field.name)
        && (!spec.only_explicit || field.notes.iter().any(|n| n == &format!("{owner}.Include")))
}

fn eol(text: &str) -> &'static str {
    if text.contains("\r\n") { "\r\n" } else { "\n" }
}

fn prefix_edit(text: &str, at: usize, items: &[String]) -> Edit {
    let line_start = text[..at].rfind('\n').map_or(0, |p| p + 1);
    let indent = &text[line_start..at];
    let eol = eol(text);
    let inserted: String = if indent.trim().is_empty() {
        items.iter().map(|i| format!("{i}{eol}{indent}")).collect()
    } else {
        items.iter().map(|i| format!("{i} ")).collect()
    };
    Edit::insert(at, &inserted)
}

fn import_edits(text: &str, facts: &Facts, names: &[&str]) -> Vec<Edit> {
    let wanted: Vec<String> = names.iter().filter(|n| !facts.imported(n)).map(|n| format!("lombok.{n}")).collect();
    let refs: Vec<&str> = wanted.iter().map(String::as_str).collect();
    import_edit(text, &refs, false)
        .map(|mut e| {
            e.text = e.text.replace("\r\n", "\n").replace('\n', eol(text));
            e
        })
        .into_iter()
        .collect()
}

/// Annotation texts and import edits to add `names` as class annotations.
fn added(text: &str, facts: &Facts, names: &[&str]) -> Vec<Edit> {
    let mut items = Vec::new();
    let mut imports = Vec::new();
    for name in names {
        let (written, import) = facts.reference(name);
        items.push(format!("@{written}"));
        imports.extend(import);
    }
    let import_names: Vec<&str> = names.iter().copied().filter(|n| imports.iter().any(|i| i == &format!("lombok.{n}"))).collect();
    let mut edits = vec![prefix_edit(text, facts.insert_at, &items)];
    edits.extend(import_edits(text, facts, &import_names));
    edits
}

pub fn findings(text: &str, class: &Class, equality: bool, to_string: bool, constructors: bool) -> Vec<Finding> {
    let Some(facts) = facts(text, class) else { return vec![] };
    let mut found = Vec::new();
    if to_string {
        found.extend(to_string_findings(text, &facts));
    }
    if equality {
        found.extend(equality_findings(text, &facts));
    }
    if constructors {
        found.extend(constructor_findings(text, &facts));
    }
    found
}

fn exclusion(text: &str, facts: &Facts, field: &Field, owner: &str, title: &'static str, message: String) -> Finding {
    let (written, import) = facts.reference(owner);
    let mut edits = vec![prefix_edit(text, field.member_start, &[format!("@{written}.Exclude")])];
    edits.extend(import_edits(text, facts, &import.iter().map(|_| owner).collect::<Vec<_>>()));
    let code = if owner == "ToString" { "jpa-lombok-to-string" } else { "jpa-lombok-equals-field" };
    Finding {
        start: field.name_at,
        end: field.name_at + field.name.len(),
        severity: Severity::Warning,
        code,
        message,
        fix: Some((title, edits)),
    }
}

fn to_string_findings(text: &str, facts: &Facts) -> Vec<Finding> {
    let annotation = facts.has("ToString");
    if annotation.is_none() || facts.declares("toString") {
        return vec![];
    }
    let Some(spec) = spec(text, facts, annotation) else { return vec![] };
    facts
        .fields
        .iter()
        .filter(|f| f.lazy && included(f, "ToString", &spec))
        .map(|f| {
            let message = format!(
                "The generated toString includes the association `{}`: it can load lazy data or recurse through a bidirectional association.",
                f.name
            );
            if spec.only_explicit {
                return Finding { start: f.name_at, end: f.name_at + f.name.len(), severity: Severity::Warning, code: "jpa-lombok-to-string", message, fix: None };
            }
            exclusion(text, facts, f, "ToString", "Exclude from toString", message)
        })
        .collect()
}

fn equality_findings(text: &str, facts: &Facts) -> Vec<Finding> {
    let annotation = facts.has("EqualsAndHashCode");
    if (annotation.is_none() && facts.has("Data").is_none()) || facts.declares("equals") || facts.declares("hashCode") {
        return vec![];
    }
    let Some(spec) = spec(text, facts, annotation) else { return vec![] };
    let mut found = Vec::new();
    for f in facts.fields.iter().filter(|f| included(f, "EqualsAndHashCode", &spec)) {
        if f.association {
            let message = format!(
                "The generated equals and hashCode include the association `{}`: comparing it can load lazy data or recurse.",
                f.name
            );
            if spec.only_explicit {
                found.push(Finding { start: f.name_at, end: f.name_at + f.name.len(), severity: Severity::Warning, code: "jpa-lombok-equals-field", message, fix: None });
            } else {
                found.push(exclusion(text, facts, f, "EqualsAndHashCode", "Exclude from equals and hashCode", message));
            }
        } else if f.generated_id {
            found.push(Finding {
                start: f.name_at,
                end: f.name_at + f.name.len(),
                severity: Severity::Warning,
                code: "jpa-lombok-equals-field",
                message: format!("The generated equals and hashCode include the generated id `{}`, which is null before persist, so the hash code changes.", f.name),
                fix: None,
            });
        }
    }
    found
}

fn constructor_findings(text: &str, facts: &Facts) -> Vec<Finding> {
    let trigger = [facts.has("Builder"), facts.has("AllArgsConstructor")].into_iter().flatten().min_by_key(|a| a.start);
    let Some(trigger) = trigger else { return vec![] };
    if facts.has("NoArgsConstructor").is_some() || facts.has("RequiredArgsConstructor").is_some() || facts.constructors.iter().any(|no_arg| *no_arg) {
        return vec![];
    }
    let builder_needs_all = facts.has("Builder").is_some() && facts.has("AllArgsConstructor").is_none();
    let fixable = facts.constructors.is_empty() && !facts.fields.iter().any(|f| f.is_final && !f.initialized);
    let (title, names): (&'static str, Vec<&str>) = if builder_needs_all {
        ("Add @NoArgsConstructor and @AllArgsConstructor", vec!["NoArgsConstructor", "AllArgsConstructor"])
    } else {
        ("Add @NoArgsConstructor", vec!["NoArgsConstructor"])
    };
    vec![Finding {
        start: trigger.start,
        end: trigger.end,
        severity: Severity::Error,
        code: "jpa-lombok-constructors",
        message: format!(
            "@{} leaves this entity without a no-arg constructor, which JPA requires (public or protected).",
            trigger.name
        ),
        fix: fixable.then(|| (title, added(text, facts, &names))),
    }]
}

/// Items for the editor: one toggle per editable annotation, labelled with the effect.
pub fn toggle_items(text: &str, class: &Class) -> Option<Vec<(&'static str, bool)>> {
    let facts = facts(text, class)?;
    Some(EDITABLE.iter().map(|n| (*n, facts.has(n).is_some())).collect())
}

pub fn eligible(text: &str, class: &Class) -> bool {
    facts(text, class).is_some_and(|f| f.has("Data").is_none())
}

/// Edits that toggle the chosen annotations; the caller previews the exact result.
pub fn plan(text: &str, class: &Class, toggled: &[String]) -> Result<Vec<Edit>, String> {
    let facts = facts(text, class).ok_or("Lombok annotations on this class are not source-owned enough to edit.")?;
    if facts.has("Data").is_some() {
        return Err("@Data is present; use the 'Replace with @Getter and @Setter' quick fix first.".into());
    }
    let mut chosen: Vec<&str> = EDITABLE.iter().copied().filter(|n| facts.has(n).is_some() != toggled.iter().any(|t| t == n)).collect();
    if toggled.iter().any(|t| !EDITABLE.contains(&t.as_str())) {
        return Err("Unknown Lombok annotation selected.".into());
    }
    if chosen.contains(&"Builder") && chosen.contains(&"NoArgsConstructor") && !chosen.contains(&"AllArgsConstructor") {
        chosen.push("AllArgsConstructor");
    }
    let explicit = !facts.constructors.is_empty();
    if chosen.contains(&"NoArgsConstructor") && facts.has("NoArgsConstructor").is_none() {
        if facts.fields.iter().any(|f| f.is_final && !f.initialized) {
            return Err("@NoArgsConstructor does not compile with uninitialised final fields.".into());
        }
        if explicit {
            return Err("The class has explicit constructors; Lombok constructor annotations are not combined with them.".into());
        }
    }
    if chosen.contains(&"AllArgsConstructor") && facts.has("AllArgsConstructor").is_none() && explicit {
        return Err("The class has explicit constructors; Lombok constructor annotations are not combined with them.".into());
    }

    let to_add: Vec<&str> = EDITABLE.iter().copied().filter(|n| chosen.contains(n) && facts.has(n).is_none()).collect();
    let to_remove: Vec<&ClassAnnotation> = facts.annotations.iter().filter(|a| EDITABLE.contains(&a.name) && !chosen.contains(&a.name)).collect();
    let mut edits = Vec::new();
    if !to_add.is_empty() {
        edits.extend(added(text, &facts, &to_add));
    }
    for a in &to_remove {
        let line_start = text[..a.start].rfind('\n').map_or(0, |p| p + 1);
        let own_line = text[line_start..a.start].trim().is_empty();
        let mut end = a.end;
        end += text[end..].len() - text[end..].trim_start_matches([' ', '\t']).len();
        if own_line {
            if text[end..].starts_with("\r\n") {
                end += 2;
            } else if text[end..].starts_with('\n') {
                end += 1;
            }
        }
        edits.push(Edit { start: if own_line { line_start } else { a.start }, end, text: String::new() });

        let uses = words(&facts.masked, a.name).filter(|at| {
            let line_start = facts.masked[..*at].rfind('\n').map_or(0, |p| p + 1);
            !facts.masked[line_start..*at].trim_start().starts_with("import ") && !to_remove.iter().any(|r| *at >= r.start && *at < r.end)
        });
        if uses.count() == 0 {
            let mut offset = 0;
            for line in text.split_inclusive('\n') {
                if line.trim() == format!("import lombok.{};", a.name) {
                    edits.push(Edit { start: offset, end: offset + line.len(), text: String::new() });
                }
                offset += line.len();
            }
        }
    }
    Ok(edits)
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::file_actions::FileActions;
    use openidea_plugin::mock::MockHost;

    const PATH: &str = "/project/src/main/java/shop/Order.java";

    fn entity(annotations: &str, body: &str) -> String {
        let imports: String = HANDLED
            .iter()
            .filter(|n| annotations.contains(&format!("@{n}")) || body.contains(&format!("@{n}.")))
            .map(|n| format!("import lombok.{n};\n"))
            .collect();
        format!(
            "package shop;\n\nimport jakarta.persistence.Entity;\nimport jakarta.persistence.FetchType;\nimport jakarta.persistence.GeneratedValue;\nimport jakarta.persistence.Id;\nimport jakarta.persistence.ManyToOne;\nimport jakarta.persistence.OneToMany;\nimport java.util.List;\n{imports}\n@Entity\n{annotations}public class Order {{\n{body}}}\n"
        )
    }

    const BODY: &str = "    @Id\n    @GeneratedValue\n    private Long id;\n\n    @ManyToOne(fetch = FetchType.LAZY)\n    private Customer customer;\n\n    @OneToMany(mappedBy = \"order\")\n    private List<Line> lines;\n\n    private String note;\n";

    fn run(text: &str, equality: bool, to_string: bool, ctor: bool) -> Vec<Finding> {
        let class = parse_class(text, PATH).unwrap();
        findings(text, &class, equality, to_string, ctor)
    }

    fn apply(text: &str, edits: &[Edit]) -> String {
        let mut out = text.to_owned();
        let mut sorted: Vec<&Edit> = edits.iter().collect();
        sorted.sort_by_key(|e| std::cmp::Reverse((e.start, e.end)));
        for e in sorted {
            out.replace_range(e.start..e.end, &e.text);
        }
        out
    }

    fn fixed(text: &str, code: &str) -> Vec<String> {
        run(text, true, true, true).iter().filter(|f| f.code == code).filter_map(|f| f.fix.as_ref()).map(|(_, e)| apply(text, e)).collect()
    }

    fn codes(text: &str) -> Vec<&'static str> {
        run(text, true, true, true).iter().map(|f| f.code).collect()
    }

    #[test]
    fn to_string_flags_lazy_associations_and_fixes_with_exclude() {
        let text = entity("@ToString\n", BODY);
        let found = run(&text, false, true, false);
        assert_eq!(found.len(), 2);
        assert!(found.iter().all(|f| f.code == "jpa-lombok-to-string"));
        let first = fixed(&text, "jpa-lombok-to-string");
        assert!(first[0].contains("    @ToString.Exclude\n    @ManyToOne"));
        assert!(first[1].contains("    @ToString.Exclude\n    @OneToMany"));
    }

    #[test]
    fn crlf_is_kept_by_the_to_string_fix() {
        let text = entity("@ToString\n", BODY).replace('\n', "\r\n");
        let after = &fixed(&text, "jpa-lombok-to-string")[0];
        assert!(after.contains("    @ToString.Exclude\r\n    @ManyToOne"));
        assert!(!after.replace("\r\n", "").contains('\n'));
    }

    #[test]
    fn to_string_quiet_cases() {
        let excluded = BODY.replace("    @ManyToOne", "    @ToString.Exclude\n    @ManyToOne").replace("    @OneToMany", "    @ToString.Exclude\n    @OneToMany");
        assert!(run(&entity("@ToString\n", &excluded), false, true, false).is_empty());
        assert!(run(&entity("@ToString(exclude = {\"customer\", \"lines\"})\n", BODY), false, true, false).is_empty());
        assert!(run(&entity("@ToString(onlyExplicitlyIncluded = true)\n", BODY), false, true, false).is_empty());
        assert!(run(&entity("@ToString(of = \"id\")\n", BODY), false, true, false).is_empty());
        assert!(run(&entity("@Getter\n", BODY), false, true, false).is_empty());
        assert!(run(&entity("@Data\n", BODY), false, true, false).is_empty());
        let eager_one = BODY.replace("FetchType.LAZY", "FetchType.EAGER");
        assert_eq!(run(&entity("@ToString\n", &eager_one), false, true, false).len(), 1);
        let default_one = BODY.replace("(fetch = FetchType.LAZY)", "");
        assert_eq!(run(&entity("@ToString\n", &default_one), false, true, false).len(), 1);
        let eager_all = eager_one.replace("@OneToMany(mappedBy", "@OneToMany(fetch = FetchType.EAGER, mappedBy");
        assert!(run(&entity("@ToString\n", &eager_all), false, true, false).is_empty());
        let unknown_fetch = BODY.replace("FetchType.LAZY", "pick()");
        assert_eq!(run(&entity("@ToString\n", &unknown_fetch), false, true, false).len(), 1);
        let own = BODY.to_owned() + "    public String toString() { return \"\"; }\n";
        assert!(run(&entity("@ToString\n", &own), false, true, false).is_empty());
        assert!(run(&entity("@ToString\n", &BODY.replace("private Customer", "private transient Customer")), false, true, false).len() == 1);
    }

    #[test]
    fn to_string_include_only_is_flagged_without_fix() {
        let body = BODY.replace("    @ManyToOne", "    @ToString.Include\n    @ManyToOne");
        let found = run(&entity("@ToString(onlyExplicitlyIncluded = true)\n", &body), false, true, false);
        assert_eq!(found.len(), 1);
        assert!(found[0].fix.is_none());
    }

    #[test]
    fn equality_flags_associations_and_generated_id() {
        let text = entity("@EqualsAndHashCode\n", BODY);
        let found = run(&text, true, false, false);
        assert_eq!(found.iter().filter(|f| f.fix.is_some()).count(), 2);
        assert_eq!(found.iter().filter(|f| f.fix.is_none()).count(), 1);
        assert!(fixed(&text, "jpa-lombok-equals-field")[0].contains("    @EqualsAndHashCode.Exclude\n    @ManyToOne"));
        let data = entity("@Data\n", BODY);
        assert_eq!(run(&data, true, false, false).len(), 3);
    }

    #[test]
    fn equality_quiet_cases() {
        let body = BODY
            .replace("    @Id", "    @EqualsAndHashCode.Exclude\n    @Id")
            .replace("    @ManyToOne", "    @EqualsAndHashCode.Exclude\n    @ManyToOne")
            .replace("    @OneToMany", "    @EqualsAndHashCode.Exclude\n    @OneToMany");
        assert!(run(&entity("@EqualsAndHashCode\n", &body), true, false, false).is_empty());
        assert!(run(&entity("@EqualsAndHashCode(onlyExplicitlyIncluded = true)\n", BODY), true, false, false).is_empty());
        let own = BODY.to_owned() + "    public boolean equals(Object o) { return true; }\n";
        assert!(run(&entity("@EqualsAndHashCode\n", &own), true, false, false).is_empty());
        assert!(run(&entity("@Getter\n", BODY), true, false, false).is_empty());
    }

    #[test]
    fn constructors_builder_and_all_args_need_no_args() {
        let builder = entity("@Builder\n", BODY);
        assert_eq!(codes(&builder).iter().filter(|c| **c == "jpa-lombok-constructors").count(), 1);
        assert_eq!(run(&builder, false, false, true)[0].severity, Severity::Error);
        let after = &fixed(&builder, "jpa-lombok-constructors")[0];
        assert!(after.contains("@Builder\n@NoArgsConstructor\n@AllArgsConstructor\npublic class"), "{after}");
        assert!(after.contains("import lombok.AllArgsConstructor;"));
        let all = entity("@AllArgsConstructor\n", BODY);
        let after = &fixed(&all, "jpa-lombok-constructors")[0];
        assert!(after.contains("@AllArgsConstructor\n@NoArgsConstructor\npublic class"), "{after}");
        let crlf = builder.replace('\n', "\r\n");
        assert!(!fixed(&crlf, "jpa-lombok-constructors")[0].replace("\r\n", "").contains('\n'));
    }

    #[test]
    fn constructors_quiet_or_unfixable() {
        for quiet in ["@Builder\n@NoArgsConstructor\n@AllArgsConstructor\n", "@Builder\n@RequiredArgsConstructor\n", "@Getter\n", "@Data\n"] {
            assert!(run(&entity(quiet, BODY), false, false, true).is_empty(), "{quiet}");
        }
        let explicit = BODY.to_owned() + "    public Order() {}\n";
        assert!(run(&entity("@Builder\n", &explicit), false, false, true).is_empty());
        let with_args = BODY.to_owned() + "    public Order(Long id) { this.id = id; }\n";
        assert!(run(&entity("@Builder\n", &with_args), false, false, true)[0].fix.is_none());
        let final_field = BODY.replace("private String note;", "private final String note;");
        assert!(run(&entity("@AllArgsConstructor\n", &final_field), false, false, true)[0].fix.is_none());
    }

    #[test]
    fn foreign_unknown_and_kotlin_stay_quiet() {
        let foreign = entity("@Data\n", BODY).replace("import lombok.Data;", "import vendor.Data;");
        assert!(codes(&foreign).is_empty());
        let wildcard = entity("@Data\n", BODY).replace("import lombok.Data;", "import lombok.*;");
        assert!(codes(&wildcard).is_empty());
        let other = entity("@Data\n", BODY).replace("import lombok.Data;", "import lombok.Data;\nimport lombok.Value;");
        assert!(codes(&other).is_empty());
        let unimported = entity("@Data\n", BODY).replace("import lombok.Data;\n", "");
        assert!(codes(&unimported).is_empty());
        let kotlin = "package shop\nimport jakarta.persistence.*\nimport lombok.Data\n@Entity\n@Data\nclass Order {\n    @ManyToOne var customer: Customer? = null\n}\n";
        let class = parse_class(kotlin, "/project/src/main/kotlin/shop/Order.kt").unwrap();
        assert!(findings(kotlin, &class, true, true, true).is_empty());
    }

    #[test]
    fn plan_adds_removes_and_cleans_imports() {
        let text = entity("@Getter\n@Setter\n", BODY);
        let class = parse_class(&text, PATH).unwrap();
        let edits = plan(&text, &class, &["Setter".into(), "NoArgsConstructor".into(), "Builder".into()]).unwrap();
        let after = apply(&text, &edits);
        assert!(!after.contains("@Setter") && !after.contains("import lombok.Setter;"));
        assert!(after.contains("@Entity\n@Getter\n@NoArgsConstructor\n@AllArgsConstructor\n@Builder\npublic class"), "{after}");
        assert!(after.contains("import lombok.Builder;") && after.contains("import lombok.AllArgsConstructor;"));
        let crlf = text.replace('\n', "\r\n");
        let class = parse_class(&crlf, PATH).unwrap();
        let after = apply(&crlf, &plan(&crlf, &class, &["Setter".into(), "ToString".into()]).unwrap());
        assert!(!after.replace("\r\n", "").contains('\n') && after.contains("@ToString\r\n"), "{after}");
    }

    #[test]
    fn plan_refuses_data_final_fields_and_explicit_constructors() {
        let data = entity("@Data\n", BODY);
        let class = parse_class(&data, PATH).unwrap();
        assert!(plan(&data, &class, &["Getter".into()]).is_err());
        let finals = entity("@Getter\n", &BODY.replace("private String note;", "private final String note;"));
        let class = parse_class(&finals, PATH).unwrap();
        assert!(plan(&finals, &class, &["NoArgsConstructor".into()]).is_err());
        let explicit = entity("@Getter\n", &(BODY.to_owned() + "    public Order(Long id) {}\n"));
        let class = parse_class(&explicit, PATH).unwrap();
        assert!(plan(&explicit, &class, &["NoArgsConstructor".into()]).is_err());
        assert!(plan(&explicit, &class, &["Setter".into()]).is_ok());
    }

    #[test]
    fn editor_action_previews_and_returns_one_host_edit() {
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        let text = entity("@Getter\n", BODY);
        host.open_editor(PATH, "java", &text, &[]);
        let doc = Document { path: PATH.into(), language: "java".into(), text: text.clone(), version: 1 };
        let mut plugin = JpaAssist::default();
        let pending = plugin.file_actions(&doc, Range::default()).into_iter().find(|a| a.title == TITLE).unwrap();
        host.answer_next_prompt(Some(&["Setter"]));
        host.answer_next_prompt(Some(&[]));
        let result = plugin.resolve_file_action(&doc, pending).unwrap();
        assert!(result.operations.is_empty() && result.edits.len() == 1);
        let after = crate::file_actions::patched(&text, &result.edits[0].1).unwrap();
        assert!(after.contains("@Getter\n@Setter\npublic class"), "{after}");
        assert_eq!(host.editor_text(), text);
    }

    #[test]
    fn editor_action_is_not_offered_for_data_or_unowned_classes() {
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        for text in [entity("@Data\n", BODY), entity("@Data\n", BODY).replace("import lombok.Data;", "import vendor.Data;")] {
            host.open_editor(PATH, "java", &text, &[]);
            let doc = Document { path: PATH.into(), language: "java".into(), text, version: 1 };
            assert!(JpaAssist::default().file_actions(&doc, Range::default()).iter().all(|a| a.title != TITLE));
        }
    }

    #[test]
    fn live_diagnostics_quick_fix_and_settings_toggles() {
        use openidea_plugin::diagnostics::DiagnosticsProvider;
        use openidea_plugin::providers::Providers;
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        let text = entity("@ToString\n@EqualsAndHashCode\n@Builder\n", BODY);
        host.open_editor(PATH, "java", &text, &[]);
        let doc = Document { path: PATH.into(), language: "java".into(), text: text.clone(), version: 1 };
        let mut plugin = JpaAssist::default();
        let codes_of = |plugin: &mut JpaAssist| -> Vec<String> { plugin.diagnostics(&doc).into_iter().filter_map(|d| d.code).collect() };
        let found = codes_of(&mut plugin);
        for code in ["jpa-lombok-to-string", "jpa-lombok-equals-field", "jpa-lombok-constructors"] {
            assert!(found.iter().any(|c| c == code), "{code}: {found:?}");
        }
        let lines = text.lines().count() as u32;
        let actions = plugin.code_actions(&doc, Range::new(Position::new(0, 0), Position::new(lines, 0)));
        for title in ["Exclude from toString", "Exclude from equals and hashCode", "Add @NoArgsConstructor and @AllArgsConstructor"] {
            assert!(actions.iter().any(|a| a.title == title), "{title}");
        }
        for (setting, code) in [("checkLombokToString", "jpa-lombok-to-string"), ("checkLombokEquality", "jpa-lombok-equals-field"), ("checkLombokConstructors", "jpa-lombok-constructors")] {
            host.set_setting(&format!("jpaAssist.{setting}"), "false");
            assert!(!codes_of(&mut plugin).iter().any(|c| c == code), "{setting}");
        }
    }

    fn wizard_source(settings: &[(&str, &str)]) -> String {
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        let path = "/project/src/main/java/shop/Tag.java";
        host.open_editor(path, "java", "", &[]);
        for (key, value) in settings {
            host.set_setting(key, value);
        }
        let doc = Document { path: path.into(), language: "java".into(), text: String::new(), version: 1 };
        let mut plugin = JpaAssist::default();
        let pending = plugin.file_actions(&doc, Range::default()).into_iter().find(|a| a.title.starts_with("Generate JPA")).unwrap();
        for row in [&["jakarta"][..], &["Tag"], &["tags"], &["IDENTITY"], &["version"], &[]] {
            host.answer_next_prompt(Some(row));
        }
        plugin.resolve_file_action(&doc, pending).unwrap().edits[0].1[0].text.clone()
    }

    #[test]
    fn new_entity_wizard_writes_the_lombok_annotations_chosen_in_settings() {
        let source = wizard_source(&[("lombokEntityAnnotations", r#"[{"annotation":"ToString"},{"annotation":"Getter"},{"annotation":"AllArgsConstructor"},{"annotation":"Bogus"}]"#)]);
        assert!(source.contains("@Getter\n@AllArgsConstructor\n@ToString\npublic class"), "{source}");
        assert!(!source.contains("@Setter") && !source.contains("@NoArgsConstructor"), "{source}");
        assert!(!source.contains("getId()") && source.contains("public void setId(") && source.contains("setVersion("), "{source}");
        for import in ["import lombok.Getter;", "import lombok.AllArgsConstructor;", "import lombok.ToString;"] {
            assert!(source.contains(import), "{import}\n{source}");
        }
        let only_setter = wizard_source(&[("lombokEntityAnnotations", r#"[{"annotation":"Setter"}]"#), ("lombokEntityDefaults", "true")]);
        assert!(only_setter.contains("@Setter\npublic class") && only_setter.contains("getId()") && !only_setter.contains("setId("), "{only_setter}");
        let empty_list_keeps_defaults = wizard_source(&[("lombokEntityAnnotations", "[]"), ("lombokEntityDefaults", "true")]);
        assert!(empty_list_keeps_defaults.contains("@Getter\n@Setter\n@NoArgsConstructor\npublic class"));
        let unknown_only = wizard_source(&[("lombokEntityAnnotations", r#"[{"annotation":"Bogus"}]"#)]);
        assert!(!unknown_only.contains("lombok"), "{unknown_only}");
    }

    #[test]
    fn new_entity_wizard_honours_lombok_entity_defaults() {
        for (value, lombok) in [(None, false), (Some("false"), false), (Some("true"), true)] {
            let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
            let path = "/project/src/main/java/shop/Tag.java";
            host.open_editor(path, "java", "", &[]);
            if let Some(value) = value {
                host.set_setting("lombokEntityDefaults", value);
            }
            let doc = Document { path: path.into(), language: "java".into(), text: String::new(), version: 1 };
            let mut plugin = JpaAssist::default();
            let pending = plugin.file_actions(&doc, Range::default()).into_iter().find(|a| a.title.starts_with("Generate JPA")).unwrap();
            for row in [&["jakarta"][..], &["Tag"], &["tags"], &["IDENTITY"], &["version"], &[]] {
                host.answer_next_prompt(Some(row));
            }
            let resolved = plugin.resolve_file_action(&doc, pending).unwrap();
            let source = &resolved.edits[0].1[0].text;
            assert_eq!(source.contains("@NoArgsConstructor"), lombok, "{source}");
            assert_eq!(source.contains("import lombok.Getter;") && source.contains("@Getter\n@Setter\n@NoArgsConstructor\npublic class"), lombok);
            assert_eq!(source.contains("public Long getId()"), !lombok, "{source}");
        }
    }
}
