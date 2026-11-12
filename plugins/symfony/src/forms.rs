//! Symfony forms from project sources: the fields a form type adds in `buildForm()`, the members of a
//! form view in Twig, and the option names of the built-in field types (from Symfony's public
//! documentation). Matching is textual; unknown types offer only the options every type has.
use std::ops::Range;

use crate::php_text;

/// A field added with `->add('name', Type::class, …)` in `buildForm()`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Field {
    pub name: String,
    /// The field's type class, resolved through `use`.
    pub type_name: Option<String>,
    /// The field name inside its quotes, as offsets in the form type's source.
    pub range: Range<usize>,
}

/// What a form view offers in Twig besides its fields.
pub const VIEW_MEMBERS: [&str; 5] = ["vars", "children", "parent", "rendered", "methodRendered"];

/// The commonly used `form.vars` entries.
pub const VARS: [&str; 24] = [
    "id", "name", "full_name", "value", "data", "errors", "valid", "required", "disabled", "label", "label_attr", "attr", "help", "help_attr", "translation_domain", "compound", "multipart", "submitted",
    "block_prefixes", "unique_block_prefix", "checked", "choices", "expanded", "multiple",
];

fn quoted(text: &str, from: usize) -> Option<(&str, usize)> {
    let start = from + text[from..].find(|c: char| !c.is_whitespace())?;
    let quote = text[start..].chars().next().filter(|quote| matches!(quote, '\'' | '"'))?;
    let inner = &text[start + 1..];
    let end = inner.find(quote)?;
    Some((&inner[..end], start + 1))
}

/// The fields of the first form type in `source`, in the order `buildForm()` adds them.
pub fn fields(source: &str) -> Vec<Field> {
    let Some(model) = php_text::class_model(source) else { return Vec::new() };
    let Some(body) = model.method("buildForm").and_then(|method| method.body.clone()) else { return Vec::new() };
    let masked = php_text::mask(source);
    let resolve = php_text::resolver(source);
    let mut fields = Vec::new();

    for (at, _) in masked[body.clone()].match_indices("->add(") {
        let open = body.start + at + "->add(".len();
        let Some((name, start)) = quoted(source, open) else { continue };
        if name.is_empty() || !name.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-')) {
            continue;
        }
        let after = start + name.len() + 1;
        let type_name = source[after..].trim_start().strip_prefix(',').and_then(|rest| {
            let argument = rest.trim_start().split([',', ')']).next()?.trim();
            let class = argument.strip_suffix("::class")?;
            (!class.is_empty() && class.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'\\'))).then(|| resolve(class))
        });
        fields.push(Field { name: name.to_owned(), type_name, range: start..start + name.len() });
    }
    fields
}

const BASE: &[&str] = &[
    "label", "label_attr", "label_format", "label_html", "translation_domain", "translation_parameters", "required", "disabled", "mapped", "data", "attr", "row_attr", "help", "help_attr", "help_html",
    "empty_data", "error_bubbling", "property_path", "constraints", "by_reference", "inherit_data", "compound", "getter", "setter", "priority", "block_name", "block_prefix", "invalid_message",
    "invalid_message_parameters", "auto_initialize", "trim", "action", "method", "validation_groups",
];

const CHOICE: &[&str] = &[
    "choices", "choice_label", "choice_value", "choice_name", "choice_attr", "choice_translation_domain", "choice_translation_parameters", "choice_loader", "choice_filter", "expanded", "multiple",
    "placeholder", "placeholder_attr", "preferred_choices", "group_by", "separator", "separator_html", "duplicate_preferred_choices",
];

const DATE: &[&str] = &["widget", "format", "input", "input_format", "html5", "years", "months", "days", "placeholder", "model_timezone", "view_timezone", "calendar", "choice_translation_domain"];

/// Option names of a built-in field type by its short class name, then those of its parents.
pub fn options(type_name: &str) -> Vec<&'static str> {
    let short = type_name.rsplit('\\').next().unwrap_or(type_name);
    let specific: &[&str] = match short {
        "ChoiceType" | "CountryType" | "CurrencyType" | "LanguageType" | "LocaleType" | "TimezoneType" | "UlidType" | "UuidType" => CHOICE,
        "EntityType" => &["class", "query_builder", "em", "choice_label", "choice_value", "group_by", "expanded", "multiple", "placeholder", "preferred_choices", "choice_attr", "choice_name", "choice_translation_domain", "id_reader"],
        "DateType" | "BirthdayType" => DATE,
        "DateTimeType" => &["date_widget", "time_widget", "widget", "format", "date_format", "input", "html5", "with_seconds", "years", "months", "days", "hours", "minutes", "seconds", "placeholder", "model_timezone", "view_timezone"],
        "TimeType" => &["widget", "input", "with_seconds", "with_minutes", "hours", "minutes", "seconds", "placeholder", "html5", "model_timezone", "view_timezone", "reference_date"],
        "IntegerType" => &["grouping", "rounding_mode", "input"],
        "NumberType" => &["scale", "grouping", "rounding_mode", "html5", "input", "input_field"],
        "MoneyType" => &["currency", "scale", "grouping", "rounding_mode", "divisor", "html5"],
        "PercentType" => &["scale", "type", "symbol", "rounding_mode", "html5"],
        "CheckboxType" | "SwitchType" => &["value", "false_values"],
        "PasswordType" => &["always_empty", "hash_property_path"],
        "UrlType" => &["default_protocol"],
        "FileType" => &["multiple", "allow_file_upload"],
        "SubmitType" | "ResetType" | "ButtonType" => &["validate", "clicked"],
        "CollectionType" => &["entry_type", "entry_options", "allow_add", "allow_delete", "delete_empty", "prototype", "prototype_data", "prototype_name", "keep_as_list"],
        "RepeatedType" => &["type", "options", "first_options", "second_options", "first_name", "second_name", "error_bubbling"],
        "RangeType" | "TextType" | "TextareaType" | "EmailType" | "SearchType" | "TelType" | "ColorType" | "HiddenType" | "WeekType" => &[],
        _ => &[],
    };
    let mut names: Vec<&'static str> = specific.to_vec();
    names.extend_from_slice(BASE);
    names.sort_unstable();
    names.dedup();
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    const TYPE: &str = "<?php\nnamespace App\\Form;\nuse Symfony\\Component\\Form\\Extension\\Core\\Type\\TextType;\nuse Symfony\\Component\\Form\\Extension\\Core\\Type\\EmailType as Mail;\nclass PostType extends AbstractType\n{\n    public function buildForm(FormBuilderInterface $builder, array $options): void\n    {\n        $builder\n            ->add('title', TextType::class, ['label' => 'Title'])\n            ->add('author', Mail::class)\n            ->add(\"body\")\n            ->add($dynamic, TextType::class)\n            ->add('has space', TextType::class);\n        $builder->add('tags');\n    }\n    public function configureOptions(OptionsResolver $resolver): void\n    {\n        $resolver->setDefaults(['x' => 1]);\n    }\n}\n";

    #[test]
    fn fields_come_from_the_chained_add_calls_of_build_form() {
        let found = fields(TYPE);
        let summary: Vec<_> = found.iter().map(|field| (field.name.as_str(), field.type_name.as_deref())).collect();
        assert_eq!(
            summary,
            [
                ("title", Some("Symfony\\Component\\Form\\Extension\\Core\\Type\\TextType")),
                ("author", Some("Symfony\\Component\\Form\\Extension\\Core\\Type\\EmailType")),
                ("body", None),
                ("tags", None),
            ]
        );
        assert_eq!(&TYPE[found[0].range.clone()], "title");
        assert!(fields("<?php\nclass NotAForm {}\n").is_empty());
    }

    #[test]
    fn options_merge_the_type_with_the_options_every_type_has() {
        let choice = options("Symfony\\Component\\Form\\Extension\\Core\\Type\\ChoiceType");
        assert!(choice.contains(&"choices") && choice.contains(&"label") && !choice.contains(&"scale"));
        assert!(options("NumberType").contains(&"scale"));
        let unknown = options("App\\Form\\CustomType");
        assert!(unknown.contains(&"required") && !unknown.contains(&"choices"));
        assert!(unknown.windows(2).all(|pair| pair[0] < pair[1]));
    }
}
