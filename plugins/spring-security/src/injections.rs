//! Physical method-security SpEL ranges; the core Spring plugin owns `spel` syntax.
use super::*;
use openidea_plugin::injections::{Document, Injection, Injections};
use openidea_plugin::text::position;
impl Injections for SecurityTools {
    fn injections(&mut self, doc: &Document) -> Vec<Injection> {
        let kotlin = doc.language == "kotlin";
        if doc.text.len() > 128 * 1024
            || !matches!(doc.language.as_str(), "java" | "kotlin")
            || kotlin && !doc.path.ends_with(".kt")
            || !kotlin && !doc.path.ends_with(".java")
        {
            return vec![];
        }
        if kotlin
            && doc
                .text
                .lines()
                .any(|line| line.trim_start().starts_with("import ") && line.contains(" as "))
        {
            return vec![];
        }
        let file = spring_text::parse_source(&doc.path, &doc.text);
        if file.types.iter().any(|t| t.name == "org")
            || file
                .imports
                .iter()
                .any(|i| i.path.ends_with(".*") || i.path.rsplit('.').next() == Some("org"))
        {
            return vec![];
        }
        let mut out = Vec::new();
        for annotation in annotations(&file).filter(|a| METHOD_SECURITY.contains(&a.name.as_str()))
        {
            let expected = format!(
                "org.springframework.security.access.prepost.{}",
                annotation.name
            );
            let written: String = java::tokens(
                &doc.text[annotation.at + 1..annotation.name_at + annotation.name.len()],
            )
            .iter()
            .map(|token| token.s)
            .collect();
            if written != expected {
                let imports: Vec<_> = file
                    .imports
                    .iter()
                    .filter(|i| i.path.rsplit('.').next() == Some(annotation.name.as_str()))
                    .collect();
                if written != annotation.name
                    || file.types.iter().any(|t| t.name == annotation.name)
                    || imports.len() != 1
                    || imports[0].is_static
                    || imports[0].path != expected
                {
                    continue;
                }
            }
            let args: Vec<_> = annotation
                .args
                .iter()
                .filter(|a| a.name == "value")
                .collect();
            let [argument] = args.as_slice() else {
                continue;
            };
            let [literal] = argument.strings.as_slice() else {
                continue;
            };
            let Some(body) = doc.text.get(literal.at..literal.end) else {
                continue;
            };
            let Some(raw) = doc.text.get(argument.at..argument.end) else {
                continue;
            };
            if body.is_empty()
                || body != literal.value
                || body.contains(['\\', '\r', '\n'])
                || body.contains("${")
                || body.contains("#{")
                || kotlin && body.contains('$')
                || raw.trim() != format!("\"{body}\"")
            {
                continue;
            }
            out.push(Injection {
                range: Range::new(
                    position(&doc.text, literal.at),
                    position(&doc.text, literal.end),
                ),
                language: "spel".into(),
            });
            if out.len() >= 128 {
                return out;
            }
        }
        out
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::text::offset;
    fn doc(text: &str) -> Document {
        Document {
            path: "/project/S.java".into(),
            language: "java".into(),
            text: text.into(),
            version: 1,
        }
    }
    #[test]
    fn canonical_method_security_literal_is_an_exact_spel_span() {
        let d = doc(
            "// 🦉\r\nimport org.springframework.security.access.prepost.PreAuthorize; class S { @PreAuthorize(\"hasRole('ADMIN') and #id > 0\") void save(long id){} }",
        );
        let ranges = SecurityTools::default().injections(&d);
        assert_eq!(ranges.len(), 1);
        let r = &ranges[0];
        assert_eq!(r.language, "spel");
        assert_eq!(
            &d.text[offset(&d.text, r.range.start).unwrap()..offset(&d.text, r.range.end).unwrap()],
            "hasRole('ADMIN') and #id > 0"
        );
        for text in [
            d.text.replace(
                "org.springframework.security.access.prepost.PreAuthorize",
                "vendor.PreAuthorize",
            ),
            d.text.replace("@PreAuthorize", "@vendor.PreAuthorize"),
            d.text
                .replace("hasRole('ADMIN') and #id > 0", r"hasRole('\u0041DMIN')"),
            d.text.replace("hasRole('ADMIN') and #id > 0", "${dynamic}"),
        ] {
            assert!(SecurityTools::default().injections(&doc(&text)).is_empty());
        }
    }
}
