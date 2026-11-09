//! Only native SQL with a direct physical Java string. JPQL stays plugin-owned.
use openidea_plugin::{
    Range,
    injections::{Document, Injection, Injections},
    text::position,
};
use spring_text::java;

impl Injections for crate::DataTools {
    fn injections(&mut self, doc: &Document) -> Vec<Injection> {
        if doc.language != "java" || !doc.path.ends_with(".java") || doc.text.len() > 128 << 10 {
            return vec![];
        }
        // This existing source model removes ambiguous annotation/repository owners.
        let file = crate::live::repository_file(doc);
        let mut out = vec![];
        for ty in &file.types {
            for method in ty.methods() {
                for annotation in &method.annotations {
                    if annotation.name != "Query"
                        || annotation
                            .args
                            .iter()
                            .filter(|a| a.name == "nativeQuery")
                            .count()
                            != 1
                        || !annotation
                            .arg("nativeQuery")
                            .is_some_and(|a| a.text.trim() == "true")
                    {
                        continue;
                    }
                    // NativeQuery is JPA-specific. A foreign Query must not borrow its flag.
                    let written: String = java::tokens(
                        &doc.text[annotation.at + 1..annotation.name_at + annotation.name.len()],
                    )
                    .iter()
                    .map(|t| t.s)
                    .collect();
                    if written != "org.springframework.data.jpa.repository.Query"
                        && !(written == "Query"
                            && file.imports.iter().any(|i| {
                                !i.is_static
                                    && i.path == "org.springframework.data.jpa.repository.Query"
                            }))
                    {
                        continue;
                    }
                    if annotation.args.iter().filter(|a| a.name == "value").count() != 1 {
                        continue;
                    }
                    let Some(arg) = annotation.arg("value") else {
                        continue;
                    };
                    let [literal] = arg.strings.as_slice() else {
                        continue;
                    };
                    let Some(body) = doc.text.get(literal.at..literal.end) else {
                        continue;
                    };
                    let Some(raw) = doc.text.get(arg.at..arg.end) else {
                        continue;
                    };
                    if body.is_empty()
                        || body != literal.value
                        || body.contains(['\\', '\r', '\n'])
                        || body.contains("#{")
                        || body.contains("${")
                        || raw.trim() != format!("\"{body}\"")
                    {
                        continue;
                    }
                    out.push(Injection {
                        range: Range::new(
                            position(&doc.text, literal.at),
                            position(&doc.text, literal.end),
                        ),
                        language: "sql".into(),
                    });
                    if out.len() >= 128 {
                        return out;
                    }
                }
            }
        }
        out
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn doc(query: &str) -> Document {
        Document {
            path: "/p/Repo.java".into(),
            language: "java".into(),
            version: 1,
            text: format!(
                "// 😀\r\nimport org.springframework.data.jpa.repository.JpaRepository;\nimport org.springframework.data.jpa.repository.Query;\ninterface Repo extends JpaRepository<User,Long> {{ @Query({query}) User one(); }}"
            ),
        }
    }
    #[test]
    fn native_sql_is_a_physical_utf16_range() {
        let d = doc("value = \"select café from users\", nativeQuery = true");
        let ranges = crate::DataTools::default().injections(&d);
        assert_eq!(ranges.len(), 1);
        assert_eq!(
            ranges[0].range.start,
            position(&d.text, d.text.find("select").unwrap())
        );
        assert_eq!(ranges[0].language, "sql");
    }
    #[test]
    fn jpql_transformed_and_foreign_strings_stay_quiet() {
        for arg in [
            "\"select u from User u\"",
            "value=\"select u\",nativeQuery=false",
            "value=\"select \\u0061\",nativeQuery=true",
            "value=\"select \"+\"a\",nativeQuery=true",
            "value=\"select :#{#x}\",nativeQuery=true",
            "value=\"select a\",nativeQuery=true,nativeQuery=true",
        ] {
            assert!(
                crate::DataTools::default().injections(&doc(arg)).is_empty(),
                "{arg}"
            );
        }
        let mut d = doc("value=\"select a\",nativeQuery=true");
        d.text = d.text.replace(
            "org.springframework.data.jpa.repository.Query",
            "vendor.Query",
        );
        assert!(crate::DataTools::default().injections(&d).is_empty());
        d = doc("value=\"select a\",nativeQuery=true");
        d.text = d.text.replace("@Query", "@vendor.Query");
        assert!(crate::DataTools::default().injections(&d).is_empty());
    }
}
