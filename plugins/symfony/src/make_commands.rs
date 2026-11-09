//! Commands that run MakerBundle, guide to atomic template creation, or insert form rows.

use openidea_plugin::{Error, Invocation, NotificationLevel, PickItem, Position, Range, TextEdit, host};

use crate::php_checks::{arguments, trimmed};
use crate::php_text::{mask, skip_whitespace};
use crate::{Main, SOURCE};

const STATIC_MAKERS: [(&str, &str); 22] = [
    ("controller", "Create a new controller class"),
    ("entity", "Create or update a Doctrine entity class"),
    ("form", "Create a new form class"),
    ("command", "Create a new console command class"),
    ("crud", "Create CRUD for a Doctrine entity class"),
    ("voter", "Create a new security voter class"),
    ("subscriber", "Create a new event subscriber class"),
    ("validator", "Create a new validator and constraint class"),
    ("twig-extension", "Create a new Twig extension with its runtime class"),
    ("twig-component", "Create a new Twig (or Live) component"),
    ("test", "Create a new test class"),
    ("migration", "Create a new migration based on database changes"),
    ("user", "Create a new security user class"),
    ("auth", "Create a Guard authenticator of different flavors"),
    ("registration-form", "Create a new registration form system"),
    ("reset-password", "Create controllers and entities for reset password"),
    ("message", "Create a new message and handler"),
    ("messenger-middleware", "Create a new messenger middleware"),
    ("serializer:normalizer", "Create a new serializer normalizer class"),
    ("fixtures", "Create a new class to load Doctrine fixtures"),
    ("listener", "Create a new event listener class"),
    ("schedule", "Create a scheduler component"),
];

pub(crate) const PLAIN_BASE_TEMPLATE: &str = "{% block body %}\n{% endblock %}\n";

impl Main {
    pub(crate) fn make(&mut self, ctx: &Invocation) -> Result<Option<String>, Error> {
        let project = ctx.project().ok_or("No project is open.")?;
        let args = ["bin/console".to_owned(), "list".to_owned(), "make".to_owned(), "--raw".to_owned()];
        let listed = project.run_process("php", &args, None).ok().filter(|output| output.status == Some(0));

        let mut makers: Vec<(String, String)> = Vec::new();
        for line in listed.iter().flat_map(|output| output.stdout.lines()) {
            let mut parts = line.trim().splitn(2, char::is_whitespace);
            let Some(name) = parts.next().and_then(|name| name.strip_prefix("make:")) else { continue };
            makers.push((name.to_owned(), parts.next().unwrap_or_default().trim().to_owned()));
        }

        if makers.is_empty() {
            makers = STATIC_MAKERS.iter().map(|(name, description)| ((*name).to_owned(), (*description).to_owned())).collect();
        }

        let items: Vec<PickItem> = makers.iter().map(|(name, description)| PickItem::new(name, name).description(description)).collect();
        let Some(maker) = host::pick("Symfony: Make", &items)? else { return Ok(None) };
        let Some(arguments) = host::input(&format!("Name and options for make:{maker} (empty to be asked)"), None)? else {
            return Ok(None);
        };

        let mut args = vec!["bin/console".to_owned(), format!("make:{maker}")];
        args.extend(arguments.split_whitespace().map(str::to_owned));
        match host::open_terminal(&format!("make:{maker}"), "php", &args) {
            Err(Error::Denied(_)) => Err("Allow the process:php capability in Settings › Plugins.".into()),
            result => result.map(|()| None),
        }
    }

    pub(crate) fn create_template(&mut self, _ctx: &Invocation) -> Result<Option<String>, Error> {
        host::notify(
            NotificationLevel::Info,
            SOURCE,
            "Place the caret inside a literal unqualified Twig extends/include/embed/use template name, then invoke the Create missing Twig template… intention. Review the exact target/content before one atomic non-overwriting Create. Commands cannot return native FileActions; this command does not write a file.",
        );
        Ok(None)
    }

    pub(crate) fn twig_form_fields(&mut self, ctx: &Invocation) -> Result<Option<String>, Error> {
        let editor = ctx.editor().ok_or("Open a Twig template first.")?;
        if !editor.path().ends_with(".twig") {
            return Err("The open file is not a Twig template.".into());
        }

        let index = self.index_for(&editor.path()).ok_or("The file is not in an indexed Symfony project.")?;
        let project = ctx.project().ok_or("No project is open.")?;
        let types: Vec<PickItem> = index
            .classes
            .iter()
            .filter(|(class, _)| class.contains("\\Form\\") && class.ends_with("Type"))
            .map(|(class, entry)| PickItem::new(class, class).description(&entry.path))
            .collect();
        if types.is_empty() {
            return Err("No form types found in the project.".into());
        }

        let Some(class) = host::pick("Form type", &types)? else { return Ok(None) };
        let source =
            index.classes.get(&class).and_then(|entry| project.read_file(&entry.path).ok()).ok_or("The form type cannot be read.")?;
        let fields = form_fields(&source);
        if fields.is_empty() {
            return Err("The form type adds no fields with a literal name.".into());
        }

        let document = editor.document()?;
        let caret = editor.selections()?.first().map_or(Position::new(0, 0), |selection| selection.start);
        let rows: String = fields.iter().map(|field| format!("{{{{ form_row(form.{field}) }}}}\n")).collect();
        let text = format!("{{{{ form_start(form) }}}}\n{rows}{{{{ form_end(form) }}}}\n");
        editor.apply_edits(document.version, &[TextEdit { range: Range::new(caret, caret), text }])?;
        Ok(None)
    }
}

/// Literal field names of `->add('name', ...)` calls in a form type.
pub(crate) fn form_fields(source: &str) -> Vec<String> {
    let masked = mask(source);
    let mut fields = Vec::new();

    for (at, _) in masked.match_indices("->add(") {
        let open = skip_whitespace(&masked, at + "->add".len());
        let Some(first) = arguments(&masked, open).into_iter().next() else { continue };
        let range = trimmed(&masked, first);
        let literal = source[range.clone()].trim_matches(['\'', '"']);
        let quoted = source[range.clone()].starts_with(['\'', '"']);
        let identifier = !literal.is_empty() && literal.chars().all(|c| c.is_alphanumeric() || c == '_');
        if quoted && identifier && !fields.iter().any(|field| field == literal) {
            fields.push(literal.to_owned());
        }
    }

    fields
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_literal_field_names_once() {
        let source = "<?php\nclass T\n{\n    function f($builder)\n    {\n        $builder->add('title', TextType::class)->add(\"body\")->add($dynamic)->add('title')->add('a-b');\n    }\n}\n";
        assert_eq!(form_fields(source), ["title", "body"]);
    }
}
