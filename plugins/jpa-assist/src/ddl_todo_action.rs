//! The two actions on the `// TODO` block that "Generate entities from SQL file" leaves for a column of an
//! unmapped SQL type: "Uncomment as is" and "Remove column mapping". Both edit only this document.
use super::*;
use crate::model::{import_edit, Edit};
use openidea_plugin::text::offset;

pub(super) const UNCOMMENT_TITLE: &str = "Uncomment as is";
pub(super) const REMOVE_TITLE: &str = "Remove column mapping";

struct Block {
    start: usize,
    end: usize,
    mapping: Vec<String>,
}

fn find(doc: &Document, selection: Range) -> Option<Block> {
    let at = offset(&doc.text, selection.start)?;
    let mut lines = Vec::new();
    let mut position = 0;
    for line in doc.text.split_inclusive('\n') {
        lines.push((position, line));
        position += line.len();
    }
    let first = lines.iter().position(|(_, line)| line.trim_start().starts_with("// TODO: column "))?;
    let rest = lines.get(first + 1..first + 3)?;
    if !rest.iter().all(|(_, line)| line.trim_start().starts_with("// ")) {
        return None;
    }
    let (start, end) = (lines[first].0, rest[1].0 + rest[1].1.len());
    if at < start || at >= end {
        return None;
    }
    let mapping = rest.iter().map(|(_, line)| line.trim_start().trim_start_matches("// ").trim_end().to_owned()).collect();
    Some(Block { start, end, mapping })
}

pub(super) fn eligible(doc: &Document, selection: Range) -> bool {
    matches!(doc.language.as_str(), "java" | "kotlin") && find(doc, selection).is_some()
}

fn namespace(text: &str) -> Option<&'static str> {
    ["jakarta.persistence", "javax.persistence"].into_iter().find(|ns| text.contains(&format!("import {ns}.")))
}

pub(super) fn resolve(pending: &Pending, doc: &Document, uncomment: bool) -> Result<FileAction, Error> {
    current(pending, doc)?;
    let block = find(doc, pending.selection).ok_or(Error::Stale)?;
    let kotlin = doc.language == "kotlin";
    let mut edits = Vec::new();
    if uncomment {
        let ns = namespace(&doc.text).ok_or_else(|| Error::from("The persistence namespace of this file is not known."))?;
        let indent: String = doc.text[block.start..].chars().take_while(|c| c.is_whitespace() && *c != '\n').collect();
        let text: String = block.mapping.iter().map(|line| format!("{indent}{line}\n")).collect();
        edits.push(Edit { start: block.start, end: block.end, text }.to_text_edit(&doc.text));
        if let Some(import) = import_edit(&doc.text, &[&format!("{ns}.Column")], kotlin) {
            edits.push(import.to_text_edit(&doc.text));
        }
    } else {
        let end = block.end + usize::from(doc.text[block.end..].starts_with('\n'));
        edits.push(Edit { start: block.start, end, text: String::new() }.to_text_edit(&doc.text));
    }
    let after = checked_after(doc, &edits)?;
    let title = if uncomment { UNCOMMENT_TITLE } else { REMOVE_TITLE };
    if !ui_kit::prompts::confirm_exact(&format!("{title}?"), &format!("Target: {}\nSource version: {}\n\n{after}", doc.path, doc.version))? {
        return Err(Error::Cancelled);
    }
    current(pending, doc)?;
    Ok(FileAction { title: title.into(), kind: Some("refactor.rewrite".into()), edits: vec![(doc.path.clone(), edits)], ..FileAction::default() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;
    use openidea_plugin::Position;

    const TEXT: &str = "package shop;\n\nimport jakarta.persistence.Entity;\n\n@Entity\npublic class Place {\n    private Long id;\n\n    // TODO: column area has the unmapped SQL type geometry; choose a Java type and uncomment\n    // @Column(name = \"area\")\n    // private Object area;\n\n}\n";

    fn doc() -> Document {
        Document { path: "/project/src/main/java/shop/Place.java".into(), language: "java".into(), version: 1, text: TEXT.into() }
    }
    fn on(line: u32) -> Range {
        Range::new(Position::new(line, 6), Position::new(line, 6))
    }

    #[test]
    fn offered_only_inside_the_todo_block() {
        let _host = MockHost::install();
        assert!(eligible(&doc(), on(8)) && eligible(&doc(), on(10)));
        assert!(!eligible(&doc(), on(6)) && !eligible(&doc(), on(11)));
    }

    #[test]
    fn block_text_is_the_mapping_without_comment_markers() {
        let _host = MockHost::install();
        let block = find(&doc(), on(9)).unwrap();
        assert_eq!(block.mapping, ["@Column(name = \"area\")", "private Object area;"]);
        assert_eq!(&TEXT[block.start..block.start + 14], "    // TODO: c");
    }
}
