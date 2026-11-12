//! Per-constraint configuration of the Bean Validation annotations on a generated DTO class: each
//! constraint copied from the entity can be kept or dropped and given its own message, and a few
//! constraints can be added from scratch. Only the constraints the transfer already understands
//! (literal arguments) are involved; the validator itself is proven by the caller.
use super::*;

const MAX_MESSAGE: usize = 200;
const SCRATCH: [&str; 3] = ["NotNull", "NotBlank", "Email"];

fn name_of(annotation: &str) -> &str {
    annotation[1..].split('(').next().unwrap_or_default()
}

fn args_of(annotation: &str) -> &str {
    annotation.split_once('(').map_or("", |(_, rest)| rest.trim_end_matches(')'))
}

fn message_of(annotation: &str) -> Option<String> {
    let parts = dto_action::split_outside_strings(args_of(annotation), b',')?;
    parts.iter().find_map(|part| {
        let (key, value) = part.split_once('=')?;
        (key.trim() == "message").then(|| value.trim().trim_matches('"').to_owned())
    })
}

fn escaped(message: &str) -> Result<String, Error> {
    if message.len() > MAX_MESSAGE || message.chars().any(char::is_control) {
        return Err(format!("A constraint message is at most {MAX_MESSAGE} characters without control characters.").into());
    }
    Ok(message.replace('\\', "\\\\").replace('"', "\\\""))
}

pub(super) fn with_message(annotation: &str, message: &str) -> Result<String, Error> {
    let refusal = || Error::from("The constraint arguments cannot be rewritten safely.");
    let mut kept: Vec<String> = Vec::new();
    for part in dto_action::split_outside_strings(args_of(annotation), b',').ok_or_else(refusal)? {
        let part = part.trim();
        if part.is_empty() || part.split_once('=').is_some_and(|(key, _)| key.trim() == "message") {
            continue;
        }
        kept.push(part.to_owned());
    }
    kept.push(format!("message = \"{}\"", escaped(message)?));
    Ok(format!("@{}({})", name_of(annotation), kept.join(", ")))
}

fn scratch_applicable(annotation: &str, ty: &str, existing: &[String]) -> bool {
    if existing.iter().any(|known| name_of(known) == annotation) {
        return false;
    }
    match annotation {
        "NotNull" => !matches!(ty, "int" | "long" | "short" | "byte" | "char" | "boolean" | "float" | "double"),
        _ => ty == "String",
    }
}

/// Constraints for the chosen fields after the user kept, dropped, added and re-worded them.
pub(super) fn configure(doc: &Document, chosen: &[(String, String)]) -> Result<(HashMap<String, Vec<String>>, String), Error> {
    let (copied, namespace) = dto_action::transferred(doc, chosen)?;
    let mut entries: Vec<(String, String, bool)> = Vec::new();
    for (field, _) in chosen {
        for annotation in copied.get(field).into_iter().flatten() {
            entries.push((field.clone(), annotation.clone(), false));
        }
    }
    if !entries.is_empty() {
        let items: Vec<_> = entries
            .iter()
            .enumerate()
            .map(|(index, (field, annotation, _))| PickItem::new(&index.to_string(), &format!("{field}: {annotation}")))
            .collect();
        let kept = host::pick_many("Constraints copied from the entity (leave one out to disable it)", &items)?.ok_or(Error::Cancelled)?;
        let mut seen = HashSet::new();
        let indexes: Vec<usize> = kept.iter().filter_map(|key| key.parse().ok()).filter(|index| *index < entries.len() && seen.insert(*index)).collect();
        if indexes.len() != kept.len() {
            return Err("Choose only offered constraints, each once.".into());
        }
        entries = entries.into_iter().enumerate().filter(|(index, _)| indexes.contains(index)).map(|(_, entry)| entry).collect();
    }
    let existing = |field: &str, entries: &[(String, String, bool)]| -> Vec<String> {
        entries.iter().filter(|(known, _, _)| known == field).map(|(_, annotation, _)| annotation.clone()).collect()
    };
    let mut offers: Vec<(String, &str)> = Vec::new();
    for (field, ty) in chosen {
        let present = existing(field, &entries);
        let copied_here = copied.get(field).map(Vec::as_slice).unwrap_or_default();
        for annotation in SCRATCH {
            if scratch_applicable(annotation, ty, &present) && !copied_here.iter().any(|known| name_of(known) == annotation) {
                offers.push((field.clone(), annotation));
            }
        }
    }
    if !offers.is_empty() {
        let items: Vec<_> = offers.iter().map(|(field, annotation)| PickItem::new(&format!("{field}|{annotation}"), &format!("{field}: @{annotation}"))).collect();
        let added = host::pick_many("Constraints to add from scratch (none is allowed)", &items)?.ok_or(Error::Cancelled)?;
        let mut seen = HashSet::new();
        for key in &added {
            let (field, annotation) = offers.iter().find(|(field, annotation)| format!("{field}|{annotation}") == *key).ok_or("Choose only offered constraints, each once.")?;
            if !seen.insert(key.as_str()) {
                return Err("Choose only offered constraints, each once.".into());
            }
            entries.push((field.clone(), format!("@{annotation}"), true));
        }
    }
    if entries.len() > 64 {
        return Err("At most 64 constraints can be configured in one DTO.".into());
    }
    let mut found: HashMap<String, Vec<String>> = HashMap::new();
    for (field, annotation, _) in entries {
        let current = message_of(&annotation).unwrap_or_default();
        let prompt = format!("Message for @{} on {field} (empty keeps the default)", name_of(&annotation));
        let message = host::input(&prompt, Some(&current))?.ok_or(Error::Cancelled)?;
        let message = message.trim();
        let text = if message.is_empty() || message == current { annotation } else { with_message(&annotation, message)? };
        found.entry(field).or_default().push(text);
    }
    Ok((found, namespace))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_replace_an_existing_message_and_escape_quotes() {
        assert_eq!(with_message("@NotBlank", "Name needed").unwrap(), "@NotBlank(message = \"Name needed\")");
        assert_eq!(with_message("@Size(min = 2, max = 40, message = \"old, text\")", "say \"hi\"").unwrap(), "@Size(min = 2, max = 40, message = \"say \\\"hi\\\"\")");
        assert!(with_message("@NotBlank", "line\nbreak").is_err());
        assert!(with_message("@NotBlank", &"x".repeat(201)).is_err());
    }

    #[test]
    fn scratch_constraints_fit_the_field_type_and_skip_existing_ones() {
        assert!(scratch_applicable("NotNull", "Long", &[]));
        assert!(!scratch_applicable("NotNull", "long", &[]));
        assert!(!scratch_applicable("Email", "Long", &[]));
        assert!(!scratch_applicable("NotBlank", "String", &["@NotBlank".to_owned()]));
    }

    #[test]
    fn existing_messages_are_read_back() {
        assert_eq!(message_of("@Size(max = 4, message = \"too long\")").as_deref(), Some("too long"));
        assert_eq!(message_of("@NotNull"), None);
    }
}
