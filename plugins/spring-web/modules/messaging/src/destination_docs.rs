//! Literal annotation destinations; no broker/container/receiver inference.
use super::*;
use openidea_plugin::hover::Hover;
pub(super) fn hover(doc: &Document, at: Position) -> Option<Hover> {
    if doc.language != "java"
        || !doc.path.ends_with(".java")
        || doc.text.len() > 128 << 10
        || doc.text.contains("\\u")
    {
        return None;
    }
    let off = offset(&doc.text, at)?;
    let file = java::parse(&doc.text);
    if file.imports.iter().any(|i| i.path.ends_with(".*")) {
        return None;
    }
    for annotation in file.types.iter().flat_map(|t| {
        t.annotations
            .iter()
            .chain(t.members.iter().flat_map(|m| &m.annotations))
    }) {
        let (fqn, key, label, alternatives) = match annotation.name.as_str() {
            "KafkaListener" => (
                "org.springframework.kafka.annotation.KafkaListener",
                "topics",
                "Kafka topic",
                &["topicPattern", "topicPartitions"][..],
            ),
            "RabbitListener" => (
                "org.springframework.amqp.rabbit.annotation.RabbitListener",
                "queues",
                "RabbitMQ queue",
                &["bindings", "queuesToDeclare"][..],
            ),
            "JmsListener" => (
                "org.springframework.jms.annotation.JmsListener",
                "destination",
                "JMS destination",
                &[][..],
            ),
            _ => continue,
        };
        if file.types.iter().any(|t| t.name == annotation.name)
            || !file.imports.iter().any(|i| !i.is_static && i.path == fqn)
            || file.imports.iter().any(|i| {
                !i.is_static
                    && i.path.rsplit('.').next() == Some(annotation.name.as_str())
                    && i.path != fqn
            })
            || annotation
                .args
                .iter()
                .any(|a| alternatives.contains(&a.name.as_str()))
        {
            continue;
        }
        let args: Vec<_> = annotation.args.iter().filter(|a| a.name == key).collect();
        let [arg] = args.as_slice() else {
            continue;
        };
        if arg.strings.is_empty()
            || !java::tokens(&arg.text)
                .iter()
                .all(|t| t.string().is_some() || matches!(t.s, "{" | "}" | ","))
        {
            continue;
        }
        if arg.strings.iter().any(|s| {
            s.value.is_empty()
                || s.value.len() > 256
                || s.value.contains(['$', '#', '{', '}', '`', '\\', '<', '>'])
                || s.value.chars().any(char::is_control)
                || s.value.len() != s.end - s.at
                || doc.text[s.at..s.end].contains('\\')
        }) {
            continue;
        }
        let selected: Vec<_> = arg
            .strings
            .iter()
            .filter(|s| s.at <= off && off <= s.end)
            .collect();
        let [s] = selected.as_slice() else {
            continue;
        };
        return Some(Hover::new(format!("{label} annotation source\n\nLiteral destination: `{}`.\n\nDeclared by @{} `{key}`; {} literal destination(s) in this attribute. This is current source only. Container registration, inherited/composed metadata, installed framework version, broker availability, subscriptions, delivery and runtime routing are not established. Listener settings beyond this literal destination attribute are not evaluated.",s.value,annotation.name,arg.strings.len())).range(range(&doc.text,s.at,s.end)));
    }
    None
}
#[cfg(test)]
mod tests {
    use super::*;
    fn doc(import: &str, annotation: &str) -> Document {
        Document {
            path: "/project/Listener.java".into(),
            language: "java".into(),
            version: 8,
            text: format!(
                "import {import}; class Listener {{ /* 😀 */ {annotation} void receive(String message) {{}} }}"
            ),
        }
    }
    #[test]
    fn exact_annotation_imports_and_literal_destination_ranges_have_no_broker_claim() {
        for (import, annotation, label) in [
            (
                "org.springframework.kafka.annotation.KafkaListener",
                "@KafkaListener(topics={\"orders\",\"dead-letter\"})",
                "Kafka topic",
            ),
            (
                "org.springframework.amqp.rabbit.annotation.RabbitListener",
                "@RabbitListener(queues=\"orders\")",
                "RabbitMQ queue",
            ),
            (
                "org.springframework.jms.annotation.JmsListener",
                "@JmsListener(destination=\"orders\")",
                "JMS destination",
            ),
        ] {
            let d = doc(import, annotation);
            let at = openidea_plugin::text::position(&d.text, d.text.find("orders").unwrap());
            let h = hover(&d, at).unwrap();
            assert_eq!(h.range.unwrap().start, at);
            assert!(h.markdown.contains(label));
            assert!(h.markdown.contains("not established"));
        }
    }
    #[test]
    fn foreign_dynamic_regex_alternatives_and_concatenation_stay_quiet() {
        for (import, annotation) in [
            ("foreign.KafkaListener", "@KafkaListener(topics=\"orders\")"),
            (
                "org.springframework.kafka.annotation.KafkaListener",
                "@KafkaListener(topics=\"${orders}\")",
            ),
            (
                "org.springframework.kafka.annotation.KafkaListener",
                "@KafkaListener(topics=\"orders\" + suffix)",
            ),
            (
                "org.springframework.kafka.annotation.KafkaListener",
                "@KafkaListener(topics=\"orders\",topicPattern=\".*\")",
            ),
            (
                "org.springframework.amqp.rabbit.annotation.RabbitListener",
                "@RabbitListener(queues=\"orders\",bindings=bindings)",
            ),
        ] {
            let d = doc(import, annotation);
            assert!(
                hover(
                    &d,
                    openidea_plugin::text::position(&d.text, d.text.find("orders").unwrap())
                )
                .is_none()
            );
        }
    }
}
