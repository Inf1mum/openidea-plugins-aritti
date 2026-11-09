//! Current-text local schema facts only; no filesystem, project or network calls.
use crate::{hover,spec,yaml_pointer};
use openidea_plugin::{Position,hover::Hover,providers::Document,text::offset};
use yaml_rust2::Yaml;

fn reference<'a>(root:&'a Yaml,node:&'a Yaml,pointer:&mut String,source:&yaml_pointer::Source<'_>,text:&str,selected:&spec::Key)->Option<&'a str> {
    for (name,value) in node.as_hash()? {
        let Some(name)=name.as_str() else {continue;};
        let before=pointer.len();pointer.push('/');pointer.push_str(&name.replace('~',"~0").replace('/',"~1"));
        if name=="$ref" {
            if let Some((actual,key))=yaml_pointer::resolve(root,source,text,pointer) {
                if key.at==selected.at && key.end==selected.end {
                    let value=actual.as_str()?;
                    return (text.get(key.value_at..key.value_end)?==value).then_some(value);
                }
            }
        }
        // Mapping-only source ownership: array/flow/transformed source forms
        // without an exact scanner chain intentionally do not establish facts.
        if let Some(found)=reference(root,value,pointer,source,text,selected) {return Some(found);}
        pointer.truncate(before);
    }
    None
}

pub(crate) fn hover(doc:&Document,at:Position)->Option<Hover> {
    if doc.language!="yaml" || spec::is_json(&doc.text) {return None;}
    let root=yaml_pointer::model(&doc.text)?;
    let openapi=root["openapi"].as_str();
    let swagger=root["swagger"].as_str();
    if !matches!(root["openapi"],Yaml::BadValue) && !matches!(root["swagger"],Yaml::BadValue) {return None;}
    let modern=openapi.is_some_and(|v|v.rsplit_once('.').is_some_and(|(minor,patch)|matches!(minor,"3.0"|"3.1")&&!patch.is_empty()&&patch.bytes().all(|b|b.is_ascii_digit())));
    if swagger!=Some("2.0") && !modern {return None;}
    let keys=crate::yaml_scan::keys(&doc.text,8192,4<<20,48)?;
    if keys.len()>8192 {return None;}
    let source=yaml_pointer::source(&keys);
    yaml_pointer::resolve(&root,&source,&doc.text,if swagger==Some("2.0") {"/swagger"} else {"/openapi"})?;
    let caret=offset(&doc.text,at)?;
    let selected=keys.iter().find(|key|doc.text.get(key.at..key.end)==Some("$ref")&&((key.at<=caret&&caret<key.end)||(key.value_at<=caret&&caret<key.value_end)))?;
    let target=reference(&root,&root,&mut String::new(),&source,&doc.text,selected)?;
    let pointer=target.strip_prefix('#')?;
    let prefix=if swagger==Some("2.0") {"/definitions/"} else {"/components/schemas/"};
    let rest=pointer.strip_prefix(prefix)?;
    let (value,_)=yaml_pointer::resolve(&root,&source,&doc.text,pointer)?;
    let boolean=matches!(value,Yaml::Boolean(_));
    if value.as_hash().is_none() && !(boolean&&openapi.is_some_and(|v|v.starts_with("3.1."))) {return None;}
    let heading=if rest.contains('/') {
        if !hover::nested_schema_pointer(rest) {return None;}
        format!("Local schema: #{pointer}")
    } else {format!("Schema: {}",rest.replace("~1","/").replace("~0","~"))};
    let facts=if let Yaml::Boolean(value)=value {serde_json::Value::Bool(*value)} else {
        let mut facts=serde_json::Map::new();
        for name in ["title","summary","description","type","format","operationId"] {
            if let Some(value)=value[name].as_str() {facts.insert(name.into(),serde_json::Value::String(value.into()));}
        }
        if value["deprecated"]==Yaml::Boolean(true) {facts.insert("deprecated".into(),serde_json::Value::Bool(true));}
        serde_json::Value::Object(facts)
    };
    Some(Hover::new(hover::facts(&heading,&facts)).range(spring_text::range(&doc.text,selected.value_at,selected.value_end)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::{hover::HoverProvider,mock::MockHost,text::{position,offset}};
    const SPEC:&str="openapi: 3.1.1\ninfo:\n  title: 😀\ncomponents:\n  schemas:\n    'A/B':\n      properties:\n        '~id':\n          type: integer\n          description: Current <id> [link](https://evil.test)\nx:\n  $ref: '#/components/schemas/A~1B/properties/~0id'\n";
    fn doc(text:&str)->Document {Document {path:"/project/openapi.yaml".into(),language:"yaml".into(),text:text.into(),version:7}}
    fn run(text:&str)->Option<Hover> {hover(&doc(text),position(text,text.rfind("#/").unwrap()))}
    #[test]
    fn registered_yaml_hover_reads_current_text_only_with_exact_utf16_reference_range() {
        let host=MockHost::install().with_capabilities(&[]);
        let mut plugin=crate::OpenApi::default();
        let found=plugin.hover(&doc(SPEC),position(SPEC,SPEC.rfind("#/").unwrap())).unwrap();
        assert!(found.markdown.contains("Declared type: integer"));assert!(found.markdown.contains("Local schema:"));
        let range=found.range.unwrap();
        assert_eq!(&SPEC[offset(SPEC,range.start).unwrap()..offset(SPEC,range.end).unwrap()],"#/components/schemas/A~1B/properties/~0id");
        let unicode=SPEC.replace("'A/B'","'😀A/B'").replace("A~1B","😀A~1B");
        let unicode_range=run(&unicode).unwrap().range.unwrap();
        assert_eq!((unicode_range.end.character-unicode_range.start.character) as usize,"#/components/schemas/😀A~1B/properties/~0id".encode_utf16().count());
        let changed=SPEC.replace("Current <id>","Unsaved changed");
        let mut current=doc(&changed);current.version+=1;
        assert!(plugin.hover(&current,position(&changed,changed.rfind("#/").unwrap())).unwrap().markdown.contains("Unsaved changed"));
        assert!(host.requests().is_empty());assert!(host.process_runs().is_empty());assert!(host.opened_locations().is_empty());
    }
    #[test]
    fn duplicate_flattened_base_alias_encoded_and_array_owners_withhold_yaml_facts() {
        let _host=MockHost::install().with_capabilities(&[]);
        for text in [
            SPEC.replace("components:","components: {}\ncomponents:"),
            SPEC.replace("components:","components.schemas.A/B.properties.~id: {}\ncomponents:"),
            SPEC.replace("openapi: 3.1.1","openapi: 3.0.4\nopenapi: 3.1.1"),
            SPEC.replace("info:","$id: other.yaml\ninfo:"),
            SPEC.replace("'A/B':","'A/B': &schema"),
            SPEC.replace("'A/B':","'A/B': !!map"),
            SPEC.replace("A~1B/properties/~0id'","A%2FB/properties/~0id'"),
            SPEC.replace("A~1B/properties/~0id'","A~2B/properties/~0id'"),
            SPEC.replace("x:\n  $ref:","x:\n  - $ref:"),
            SPEC.replace("x:\n  $ref:","x.$ref:"),
        ] {assert!(run(&text).is_none(),"{text}");}
        let external=SPEC.replace("#/components","common.yaml#/components");
        assert!(run(&external).is_none());
    }
    #[test]
    fn swagger_and_version_owned_boolean_schemas_use_declared_facts_only() {
        let swagger="swagger: '2.0'\ndefinitions:\n  Pet:\n    type: object\nx:\n  $ref: '#/definitions/Pet'\n";
        assert!(run(swagger).unwrap().markdown.contains("Declared type: object"));
        let boolean="openapi: 3.1.1\ncomponents:\n  schemas:\n    Never: false\nx:\n  $ref: '#/components/schemas/Never'\n";
        assert!(run(boolean).unwrap().markdown.contains("Boolean schema: false"));
        assert!(run(&boolean.replace("3.1.1","3.0.4")).is_none());
        assert!(run(&boolean.replace("3.1.1","4.0.0")).is_none());
        let custom="openapi: 3.1.1\ncomponents:\n  schemas:\n    Pet:\n      x-custom:\n        description: Not a schema\nx:\n  $ref: '#/components/schemas/Pet/x-custom'\n";
        assert!(run(custom).is_none());
        let array="openapi: 3.1.1\ncomponents:\n  schemas:\n    Pet:\n      allOf:\n        - type: string\nx:\n  $ref: '#/components/schemas/Pet/allOf/0'\n";
        assert!(run(array).is_none());
    }
    #[test]
    fn yaml_markdown_and_source_size_limits_remain_bounded() {
        let found=run(SPEC).unwrap();
        assert!(!found.markdown.contains("<id>"));assert!(!found.markdown.contains("[link]("));
        let long=SPEC.replace("Current <id> [link](https://evil.test)",&"😀".repeat(10000));
        assert!(run(&long).unwrap().markdown.len()<=8192);
        let oversized=format!("{SPEC}# {}\n","a".repeat(512<<10));assert!(run(&oversized).is_none());
        for end in (0..SPEC.len()).filter(|end|SPEC.is_char_boundary(*end)) {
            let _=hover(&doc(&SPEC[..end]),Position::new(0,0));
        }
    }
}
