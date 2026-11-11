//! Read-only descriptions; these navigation spans are not rename anchors.
use super::*;
use openidea_plugin::hover::{Hover, HoverProvider};

fn closed_literal(token: &java::Tok<'_>) -> bool {
    let quote = if token.s.starts_with("\"\"\"") { "\"\"\"" } else { "\"" };
    token.s.len() >= 2 * quote.len() && token.s.starts_with(quote) && token.s.ends_with(quote)
}

/// The existing navigation model also sees partial expressions; hover does not evaluate those.
pub(super) fn literal_values(arg: &Arg) -> bool {
    let tokens = java::tokens(&arg.text);
    tokens.iter().filter(|t| closed_literal(t)).count() == arg.strings.len()
        && tokens.iter().all(|t| closed_literal(t) || matches!(t.s, "{" | "}" | "[" | "]" | ","))
}

fn literal_targets(f: &JavaFile, text: &str) -> std::collections::HashSet<(u32, u32)> {
    f.types
        .iter()
        .flat_map(|t| {
            t.methods().filter_map(move |m| {
                let literal = if let Some(a) = m.annotation("ShellMethod").filter(|a| rules::supported(f, text, a)) {
                    a.arg("key").is_none_or(literal_values)
                } else {
                    m.annotation("Command").is_some_and(|a| rules::modern_field(f, text, a).is_some_and(|field| a.arg(field).is_none_or(literal_values)))
                        && t.annotation("Command").is_none_or(|a| a.arg("command").is_none_or(literal_values))
                };
                literal.then(|| {
                    let at = position(text, m.at);
                    (at.line, at.character)
                })
            })
        })
        .collect()
}

/// Accept one whole literal, never a constant/expression or partial concatenation.
fn description<'a>(annotation: &'a Annotation, name: &str) -> Option<&'a str> {
    let arg = annotation.arg(name)?;
    let [literal] = arg.strings.as_slice() else { return None };
    let tokens = java::tokens(&arg.text);
    if tokens.len() != 1 || !closed_literal(&tokens[0]) || arg.text.contains('\\')
        || tokens[0].s.starts_with("\"\"\"") || literal.value.trim().is_empty() || literal.value.len() > 8192 {
        return None;
    }
    Some(literal.value.trim())
}

fn method_description<'a>(f: &'a JavaFile, text: &str, command: &Command) -> Option<&'a str> {
    let target = offset(text, command.target.start)?;
    for t in &f.types {
        for m in t.methods().filter(|m| m.at == target) {
            if let Some(a) = m.annotation("ShellMethod").filter(|a| rules::supported(f, text, a)) {
                return description(a, "value");
            }
            let a = m.annotation("Command").filter(|a| rules::supported(f, text, a))?;
            // An unknown explicit override must not be replaced with the class description.
            return if rules::modern_field(f, text, a) == Some("name") && a.arg("description").is_none() {
                description(a, "value")
            } else if a.arg("description").is_some() {
                description(a, "description")
            } else {
                t.annotation("Command").filter(|a| rules::supported(f, text, a)).and_then(|a| description(a, "description"))
            };
        }
    }
    None
}

/// Annotation descriptions are plain text, not trusted Markdown/HTML.
fn markdown(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '\\' | '`' | '*' | '_' | '[' | ']' | '(' | ')' | '#' | '!' | '|' | '~' => {
                out.push('\\');
                out.push(c);
            }
            _ => out.push(c),
        }
    }
    out
}

impl HoverProvider for ShellTools {
    fn hover(&mut self, doc: &Document, at: Position) -> Option<Hover> {
        let byte = offset(&doc.text, at)?;
        let f = document_model(doc)?;
        let literal_targets = literal_targets(&f, &doc.text);
        let declared: Vec<_> = commands(&f, &doc.text)
            .into_iter()
            .filter(|c| literal_targets.contains(&(c.target.start.line, c.target.start.character)))
            .collect();
        let holds = |r: Range| r.start <= at && at < r.end;
        let render = |commands: Vec<&Command>, span: Range| {
            let first = commands.first()?;
            // Duplicate names/overloads never choose an arbitrary description.
            if commands.iter().any(|c| c.target != first.target) {
                return None;
            }
            let description = method_description(&f, &doc.text, first)?;
            let keys = commands.iter().map(|c| c.key.as_str()).collect::<Vec<_>>().join(", ");
            Some(Hover::new(format!("**Spring Shell command:** {}\n\n{}", markdown(&keys), markdown(description))).range(span))
        };
        if let Some(c) = declared.iter().find(|c| holds(c.target)) {
            return render(declared.iter().filter(|other| other.target == c.target).collect(), c.target);
        }
        let (a, arg, literal) = literal_at(&f, byte)?;
        if !rules::supported(&f, &doc.text, a) {
            return None;
        }
        let span = range(&doc.text, literal.at, literal.end);
        if !holds(span) {
            return None;
        }
        if (a.name == "ShellMethod" && arg.name == "key") || (a.name == "Command" && (rules::modern_field(&f, &doc.text, a) == Some(arg.name.as_str()) || arg.name == "alias")) {
            let method = f.types.iter().flat_map(|t| t.methods()).find(|m| m.annotations.iter().any(|own| own.at == a.at))?;
            let target = range(&doc.text, method.at, method.at + method.name.len());
            return render(declared.iter().filter(|c| c.target == target).collect(), span);
        }
        if a.name == "ShellMethodAvailability"
            && arg.name == "value"
            && literal_values(arg)
            && literal.value != "*"
            && !literal.value.contains("${")
        {
            // On a command this value names an indicator method, not a command key.
            if availability_owner(&f, &doc.text, a).is_some() { return None; }
            let key = words(&[literal]);
            let class = class_at(&f, byte);
            let own: Vec<_> = declared.iter().filter(|c| c.key == key && c.class == class).collect();
            return render(own, span);
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;

    fn doc(text: &str, kotlin: bool) -> Document {
        Document {
            path: if kotlin { "/project/Commands.kt" } else { "/project/Commands.java" }.into(),
            language: if kotlin { "kotlin" } else { "java" }.into(),
            version: 7,
            text: text.into(),
        }
    }
    fn at(text: &str, needle: &str) -> Position {
        position(text, text.find(needle).unwrap())
    }
    fn source<'a>(doc: &'a Document, span: Range) -> &'a str {
        &doc.text[offset(&doc.text, span.start).unwrap()..offset(&doc.text, span.end).unwrap()]
    }

    #[test]
    fn legacy_current_text_descriptions_cover_method_keys_and_availability() {
        let _host = MockHost::install().with_capabilities(&[]);
        let text = r#"import org.springframework.shell.standard.ShellMethod; import org.springframework.shell.standard.ShellMethodAvailability; @ShellComponent class C {
 @ShellMethod(key = {"hello", "hi"}, value = "Say hello.") void greet() {}
 @ShellMethodAvailability("hello") Availability available() {}
 @ShellMethod("Show users.") void listUsers() {}
}"#;
        let d = doc(text, false);
        let mut tools = ShellTools::default();
        for (needle, expected_span, description) in [
            ("greet()", "greet", "Say hello."),
            ("hello\",", "hello", "Say hello."),
            ("hi\"", "hi", "Say hello."),
            ("hello\")", "hello", "Say hello."),
            ("listUsers()", "listUsers", "Show users."),
        ] {
            let h = tools.hover(&d, at(text, needle)).unwrap();
            assert!(h.markdown.ends_with(description), "{h:?}");
            assert_eq!(source(&d, h.range.unwrap()), expected_span);
        }
        assert!(tools.hover(&d, at(text, "available()")).is_none());
    }

    #[test]
    fn availability_selector_hover_cannot_borrow_another_class_description() {
        let _h=MockHost::install().with_capabilities(&[]);
        let text=r#"import org.springframework.shell.standard.ShellMethod; import org.springframework.shell.standard.ShellMethodAvailability;
@org.springframework.shell.standard.ShellComponent class A { @ShellMethod(key="same",value="A description") void a(){} }
@org.springframework.shell.standard.ShellComponent class B { @ShellMethodAvailability("same") Availability gate(){return null;} }
"#;
        let d=doc(text,false);let mut tools=ShellTools::default();
        let selected=text.rfind("same").unwrap();
        assert!(tools.hover(&d,position(text,selected)).is_none());
        assert!(tools.definition(&d,position(text,selected)).is_empty());
        let changed=Document{version:8,text:text.replace("class B {", "class B { @ShellMethod(key=\"same\",value=\"B description\") void b(){}"),..d};
        let selected=changed.text.rfind("same").unwrap();
        assert!(tools.hover(&changed,position(&changed.text,selected)).unwrap().markdown.ends_with("B description"));
    }
    #[test]
    fn command_side_availability_method_name_is_not_a_command_key_hover() {
        let _h=MockHost::install().with_capabilities(&[]);
        let text=r#"import org.springframework.shell.standard.ShellMethod; import org.springframework.shell.standard.ShellMethodAvailability;
@org.springframework.shell.standard.ShellComponent class A {
 @ShellMethod(key="run",value="Run description") @ShellMethodAvailability("gate") void run(){}
 @ShellMethod(key="gate",value="Unrelated command description") void other(){}
 Availability gate(){return null;}
}"#;
        let d=doc(text,false);let mut tools=ShellTools::default();let selected=text.find("\"gate\"").unwrap()+1;
        assert!(tools.hover(&d,position(text,selected)).is_none());
        let targets=tools.definition(&d,position(text,selected));
        assert_eq!(targets.len(),1);assert_eq!(&d.text[offset(text,targets[0].range.start).unwrap()..offset(text,targets[0].range.end).unwrap()],"gate");
    }
    #[test]
    fn modern_descriptions_inherit_and_override_without_scanning() {
        let host = MockHost::install().with_capabilities(&[]);
        let text = r#"import org.springframework.shell.command.annotation.Command; import org.springframework.shell.standard.ShellMethod;
@Command(command = "db", description = "Database commands.") class C {
 @Command(command = {"dump", "all"}, description = "Export data.") void dumpAll() {}
 @Command(command = "list") void list() {}
 @Command(description = UNKNOWN) void dynamic() {}
 @Command(description = "") void empty() {}
}"#;
        let d = doc(text, false);
        let mut tools = ShellTools::default();
        for needle in ["dumpAll()", "dump\"", "all\""] {
            let h = tools.hover(&d, at(text, needle)).unwrap();
            assert_eq!(h.markdown, "**Spring Shell command:** db dump all\n\nExport data.");
        }
        assert_eq!(tools.hover(&d, at(text, "list()")).unwrap().markdown, "**Spring Shell command:** db list\n\nDatabase commands.");
        assert!(tools.hover(&d, at(text, "dynamic()")).is_none());
        assert!(tools.hover(&d, at(text, "empty()")).is_none());
        assert!(tools.hover(&d, at(text, "db\"")).is_none(), "class prefix is not one command");
        assert!(host.diagnostics().is_empty());
    }

    #[test]
    fn kotlin_utf16_ranges_and_plain_text_are_preserved() {
        let _host = MockHost::install().with_capabilities(&[]);
        let text = "import org.springframework.shell.standard.ShellMethod\n@ShellComponent class C {\n /*😀é*/ @ShellMethod(key = [\"go\"], value = \"Use <b> & [x](url) *now*.\") fun run() {}\n}";
        let d = doc(text, true);
        let mut tools = ShellTools::default();
        let h = tools.hover(&d, at(text, "go\"")).unwrap();
        let span = h.range.unwrap();
        assert_eq!(source(&d, span), "go");
        let line_start = text[..text.find("go\"").unwrap()].rfind('\n').unwrap() + 1;
        assert_eq!(span.start.character, text[line_start..text.find("go\"").unwrap()].encode_utf16().count() as u32);
        assert_eq!(span.end.character - span.start.character, 2);
        assert_eq!(h.markdown, "**Spring Shell command:** go\n\nUse &lt;b&gt; &amp; \\[x\\]\\(url\\) \\*now\\*.");
        assert!(tools.hover(&d, span.end).is_none());
    }

    #[test]
    fn missing_dynamic_ambiguous_and_unrelated_descriptions_stay_quiet() {
        let _host = MockHost::install().with_capabilities(&[]);
        let text = r#"import org.springframework.shell.standard.ShellMethod; import org.springframework.shell.standard.ShellMethodAvailability; @ShellComponent class C {
 @ShellMethod(key="a") void missing() {}
 @ShellMethod(key="b", value="") void empty() {}
 @ShellMethod(key="c", value="prefix" + SUFFIX) void expression() {}
 @ShellMethod(key=PREFIX + "partial", value="Unknown key.") void dynamicKey() {}
 @ShellMethodAvailability("same" + SUFFIX) Availability dynamicAvailability() {}
 @ShellMethod(key="same", value="First.") void first() {}
 @ShellMethod(key="same", value="Second.") void second() {}
 @ShellMethodAvailability({"same", "*", "${dynamic}", "absent"}) Availability available() {}
}"#;
        let d = doc(text, false);
        let mut tools = ShellTools::default();
        for needle in
            ["missing()", "empty()", "expression()", "dynamicKey()", "same\" + SUFFIX", "same\", \"*", "*\"", "${dynamic}", "absent\""]
        {
            assert!(tools.hover(&d, at(text, needle)).is_none(), "{needle}");
        }
        let picocli = r#"import picocli.CommandLine.Command; class C { @Command(description="Not Shell.") void run() {} }"#;
        assert!(tools.hover(&doc(picocli, false), at(picocli, "run()")).is_none());
        let template = "@ShellComponent class C { @ShellMethod(value=\"Hello $name\") fun run() {} }";
        assert!(tools.hover(&doc(template, true), at(template, "run()")).is_none());
        let large = doc(&"x".repeat(512 * 1024 + 1), false);
        assert!(tools.hover(&large, Position::new(0, 0)).is_none());
    }

    #[test]
    fn audit_navigation_spans_are_not_unconditional_exact_rename_anchors() {
        let text = r#"import org.springframework.shell.command.annotation.Command; import org.springframework.shell.standard.ShellMethod;
@Command(command="db") class C {
 @Command(command={"dump", "all"}) void dumpAll() {}
 @ShellMethod(value="Derived.") void listUsers() {}
 @ShellMethod(key=" spaced   name ",value="Spaced.") void spaced() {}
}"#;
        let d = doc(text, false);
        let parsed = parse_source(&d.path, text);
        let declared = commands(&parsed, text);
        for c in &declared {
            assert_ne!(source(&d, c.anchor), c.key, "{c:?}");
        }
        assert_eq!(declared.iter().map(|c| c.key.as_str()).collect::<Vec<_>>(), ["db dump all", "list-users", "spaced name"]);
    }
}
