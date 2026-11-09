//! Read-only current-document component occurrences. No claims about incoming
//! cross-file references and no automatic string replacement/host Rename.
use crate::*;
pub(crate) fn run(ctx: &Invocation) -> Result<Option<String>, Error> {
    let editor = ctx
        .editor()
        .ok_or("Open an OpenAPI document and select a component key or local reference.")?;
    let path = editor.path();
    let document = editor.document()?;
    let caret = editor
        .selections()?
        .first()
        .map(|s| s.start)
        .ok_or(Error::Stale)?;
    if !is_spec_file(&path) {
        return Err(Error::Invalid("Select an OpenAPI document.".into()));
    }
    let proof = pointer_status::Proof::parse(&document.text)
        .ok_or("Component ownership is unknown in this document.")?;
    let at = offset(&document.text, caret).ok_or(Error::Stale)?;
    let occurrences = proof
        .component_occurrences(at)
        .ok_or("Select a known component key or a supported local component reference.")?;
    let items: Vec<_> = occurrences
        .iter()
        .enumerate()
        .map(|(i, (label, range))| {
            openidea_plugin::PickItem::new(
                &i.to_string(),
                &format!("{label} · line {}", range.start.line + 1),
            )
            .description("Current document only; external incoming references are not searched")
        })
        .collect();
    let Some(choice) = host::pick("OpenAPI component occurrences in this document", &items)? else {
        return Ok(None);
    };
    let destination = occurrences
        .get(choice.parse::<usize>().map_err(|_| Error::Stale)?)
        .ok_or(Error::Stale)?
        .1
        .start;
    let current = ctx.editor().ok_or(Error::Stale)?;
    let snapshot = current.document()?;
    if current.path() != path
        || snapshot.version != document.version
        || snapshot.text != document.text
    {
        return Err(Error::Stale);
    }
    host::open_location(&path, destination.line, destination.character)?;
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;
    #[test]
    fn registered_command_uses_unsaved_source_and_refuses_forged_selection() {
        let h = MockHost::install();
        let text = r##"{"openapi":"3.1.1","components":{"schemas":{"Foo":{}}},"a":{"$ref":"#/components/schemas/Foo"}}"##;
        let at = position(text, text.find("Foo").unwrap());
        h.open_editor(
            "/project/openapi.json",
            "json",
            text,
            &[(at.line, at.character, at.line, at.character)],
        );
        h.answer_next_prompt(Some(&["1"]));
        OpenApi::default()
            .run_command("component-occurrences", &h.invocation(), None)
            .unwrap();
        let location = position(text, text.find("#/components/schemas/Foo").unwrap());
        assert_eq!(
            h.opened_locations().last(),
            Some(&(
                String::from("/project/openapi.json"),
                location.line,
                location.character
            ))
        );
        h.answer_next_prompt(Some(&["999"]));
        assert!(
            OpenApi::default()
                .run_command("component-occurrences", &h.invocation(), None)
                .is_err()
        );
        assert_eq!(h.opened_locations().len(), 1);
        assert!(h.process_runs().is_empty() && h.requests().is_empty());
    }
}
