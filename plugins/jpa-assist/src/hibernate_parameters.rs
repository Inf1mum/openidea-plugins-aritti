//! Parameter option of the Hibernate custom type generator. The JPA Buddy documentation (Entity Designer, Hibernate Types):
//! for a Hibernate Custom Type "you can set whether it will be possible to alter the behavior of types based on parameters or not".
//! In Hibernate 6 that is `org.hibernate.usertype.ParameterizedType#setParameterValues(Properties)`, fed by
//! `@Type(value = X.class, parameters = @Parameter(...))`.
use super::*;

pub(super) fn ask() -> Result<bool, Error> {
    let items = [
        PickItem::new("fixed", "Fixed behaviour, no parameters"),
        PickItem::new("parameters", "Allow parameters to change the behaviour (ParameterizedType)"),
    ];
    Ok(host::pick("User type parameters", &items)?.ok_or(Error::Cancelled)? == "parameters")
}

pub(super) fn parameterize(text: &str, attribute_type: &str, name: &str) -> Result<String, Error> {
    let simple = attribute_type.rsplit('.').next().unwrap_or(attribute_type);
    if ["Properties", "ParameterizedType"].iter().any(|clash| *clash == simple || *clash == name) {
        return Err("The attribute type or class name clashes with Properties or ParameterizedType, which the parameterized user type uses.".into());
    }
    let eol = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let header = text.find("implements UserType<").ok_or(Error::Stale)?;
    let brace = header + text[header..].find(" {").ok_or(Error::Stale)?;
    let body = brace + " {".len();
    let first_import = text.find("import ").ok_or(Error::Stale)?;
    let imports_end = first_import + text[first_import..].find(&format!("{eol}{eol}")).ok_or(Error::Stale)?;
    let mut imports: Vec<&str> = text[first_import..imports_end].split(eol).collect();
    imports.extend(["import java.util.Properties;", "import org.hibernate.usertype.ParameterizedType;"]);
    imports.sort_unstable();
    let members = format!("{eol}    private Properties parameters = new Properties();{eol}{eol}    @Override{eol}    public void setParameterValues(Properties parameters) {{{eol}        this.parameters = parameters;{eol}    }}{eol}");
    Ok(format!(
        "{}{}{}, ParameterizedType {{{members}{}",
        &text[..first_import],
        imports.join(eol),
        &text[imports_end..brace],
        &text[body..]
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLAIN: &str = "package shop;\n\nimport java.io.Serializable;\nimport org.hibernate.usertype.UserType;\n\npublic class T implements UserType<String> {\n    @Override\n    public int getSqlType() {\n        return 1;\n    }\n}\n";

    #[test]
    fn adds_the_interface_imports_field_and_setter_in_sorted_order() {
        let text = parameterize(PLAIN, "String", "T").unwrap();
        assert!(text.contains("import java.io.Serializable;\nimport java.util.Properties;\nimport org.hibernate.usertype.ParameterizedType;\nimport org.hibernate.usertype.UserType;\n\n"), "{text}");
        assert!(text.contains("public class T implements UserType<String>, ParameterizedType {\n    private Properties parameters = new Properties();"), "{text}");
        assert!(text.contains("public void setParameterValues(Properties parameters) {\n        this.parameters = parameters;\n    }\n\n    @Override\n    public int getSqlType()"), "{text}");
    }

    #[test]
    fn keeps_crlf_and_refuses_clashing_names() {
        let text = parameterize(&PLAIN.replace('\n', "\r\n"), "String", "T").unwrap();
        assert!(!text.replace("\r\n", "").contains('\n'), "{text:?}");
        assert!(parameterize(PLAIN, "java.util.Properties", "T").is_err());
        assert!(parameterize(PLAIN, "String", "ParameterizedType").is_err());
    }

    #[test]
    #[ignore = "requires javac and JPA_JAKARTA_API / HIBERNATE_CORE_JAR jar paths"]
    fn parameterized_user_types_compile_against_hibernate_6() {
        let api = std::env::var("JPA_JAKARTA_API").expect("explicit API jar path");
        let core = std::env::var("HIBERNATE_CORE_JAR").expect("hibernate-core 6.x jar path");
        let entity = "package shop;\n\nimport jakarta.persistence.Entity;\nimport jakarta.persistence.Id;\n\n@Entity\npublic class Invoice {\n    @Id\n    private Long id;\n\n    private String title;\n\n    private Money price;\n}\n";
        let document = Document { path: "/project/src/main/java/shop/Invoice.java".into(), language: "java".into(), text: entity.into(), version: 1 };
        for (attribute, sql, name) in [("title", None, "TitleType"), ("price", Some("NUMERIC"), "PriceType")] {
            let rendered = render_type(&document, attribute, name, sql, true).unwrap();
            let text = parameterize(&rendered.text, if attribute == "title" { "String" } else { "Money" }, name).unwrap();
            let dir = std::env::temp_dir().join(format!("jpa-hibparam-{}-{attribute}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            let files = [(format!("{name}.java"), text), ("Invoice.java".into(), patched(entity, &rendered.edits).unwrap()), ("Money.java".into(), "package shop;\npublic class Money {}\n".into())];
            let paths: Vec<_> = files.iter().map(|(file, body)| (std::fs::write(dir.join(file), body).unwrap(), dir.join(file)).1).collect();
            let result = std::process::Command::new("javac").args(["--release", "17", "-proc:none", "-classpath", &format!("{api}:{core}"), "-d"]).arg(&dir).args(&paths).output().expect("javac installed");
            assert!(result.status.success(), "{attribute}: {}", String::from_utf8_lossy(&result.stderr));
            std::fs::remove_dir_all(dir).unwrap();
        }
    }
}
