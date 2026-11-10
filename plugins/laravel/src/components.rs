//! What a Blade component or Livewire component accepts: attributes from
//! `@props([...])`, constructor parameters or public properties, and slot names
//! from the variables its view prints.

#[path = "component_metadata.rs"]
mod declarations;
pub(crate) use declarations::{Metadata,parse_metadata as metadata,apply_metadata};
use crate::index::separated;
use crate::php;
use crate::refs::Kind;

#[derive(Debug, PartialEq, Eq)]
pub struct Prop {
    pub name: String,
    pub default: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Context {
    /// An attribute name being typed in `<x-name ` or `<livewire:name `.
    Attribute { kind: Kind, component: String, start: usize },
    /// The slot name of `<x-slot:name`, inside the component `component`.
    Slot { component: String, start: usize },
}

fn word(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_')
}

/// Whether `tag` (from its `<` to the caret) is still open: no `>` outside quotes and echoes.
fn still_open(tag: &str) -> Option<bool> {
    let bytes = tag.as_bytes();
    let mut quote: Option<u8> = None;
    let mut i = 0;

    while i < bytes.len() {
        match (quote, bytes[i]) {
            (Some(q), b) if b == q => quote = None,
            (None, b'"' | b'\'') => quote = Some(bytes[i]),
            (None, b'>') => return None,
            (None, b'{') if bytes[i..].starts_with(b"{{") || bytes[i..].starts_with(b"{!!") => {
                let close = if bytes[i + 1] == b'{' { "}}" } else { "!!}" };
                match tag[i..].find(close) {
                    Some(n) => i += n + close.len() - 1,
                    None => return Some(false),
                }
            }
            _ => {}
        }
        i += 1;
    }

    Some(quote.is_none())
}

fn enclosing_component(text: &str, before: usize,bare:Option<&std::collections::BTreeSet<String>>) -> Option<String> {
    let mut stack: Vec<String> = Vec::new();
    let mut i = 0;

    while let Some(found) = text[i..before].find("<") {
        let at = i + found;
        i = at + 1;
        let rest = &text[at + 1..before];
        let closing = rest.starts_with('/');
        let rest = rest.strip_prefix('/').unwrap_or(rest);
        let (rest,standard)=rest.strip_prefix("x-").map_or((rest,false),|rest|(rest,true));
        let length = rest.bytes().take_while(|b| word(*b) || *b == b':').count();
        let name = &rest[..length];
        if !crate::refs::valid_tag_name(name,false) || name == "slot" || !standard&&!bare.is_some_and(|names|names.contains(name)) {
            continue;
        }

        if closing {
            if let Some(position) = stack.iter().rposition(|open| open == name) {
                stack.truncate(position);
            }
            continue;
        }

        let Some(end) = text[at..before].find('>') else {
            stack.push(name.to_owned());
            continue;
        };
        if !text[..at + end].ends_with('/') {
            stack.push(name.to_owned());
        }
    }

    stack.pop()
}

pub fn context_at(text: &str, at: usize) -> Option<Context> {context_impl(text,at,None)}
pub(crate) fn context_with_metadata(index:&crate::Index,text:&str,at:usize)->Option<Context>{context_impl(text,at,Some(&index.metadata.blade_components.bare_tags))}
fn context_impl(text:&str,at:usize,bare:Option<&std::collections::BTreeSet<String>>)->Option<Context>{
    if at>text.len()||!text.is_char_boundary(at){return None;}
    let visible=crate::refs::without_comments(text);if visible.len()!=text.len(){return None;}let text=visible.as_str();
    let tag_start = text[..at].rfind('<')?;
    let tag = &text[tag_start..at];
    if !still_open(tag)? {
        return None;
    }

    if let Some(typed) = tag.strip_prefix("<x-slot:") {
        if typed.bytes().any(|b| !word(b)) {
            return None;
        }
        let component = enclosing_component(text, tag_start,bare)?;
        return Some(Context::Slot { component, start: at - typed.len() });
    }

    let raw=tag.strip_prefix('<')?;let name_length=raw.bytes().take_while(|b|word(*b)||*b==b':').count();
    let (kind,rest)=if bare.is_some_and(|names|names.contains(&raw[..name_length])){(Kind::Component,raw)}else if let Some(rest)=tag.strip_prefix("<x-"){(Kind::Component,rest)}else{(Kind::Livewire,tag.strip_prefix("<livewire:")?)};

    let length = rest.bytes().take_while(|b| word(*b)||*b==b':').count();
    let component = &rest[..length];
    let after = &rest[length..];
    if !crate::refs::valid_tag_name(component,false) || !after.starts_with(char::is_whitespace) {
        return None;
    }

    let token = after.rsplit(char::is_whitespace).next().unwrap_or_default();
    let mut start = at - token.len();
    if token.starts_with(':') {
        start += 1;
    }

    Some(Context::Attribute { kind, component: component.to_owned(), start })
}

/// Only metadata explicitly declaring no `x-` prefix may own ordinary HTML tags.
fn bare_reference(index:&crate::Index,doc:&openidea_plugin::providers::Document,at:usize)->Option<crate::refs::Reference>{
    if at>doc.text.len()||!doc.text.is_char_boundary(at)||index.metadata.blade_components.bare_tags.is_empty(){return None;}
    let visible=crate::refs::without_comments(&doc.text);if visible.len()!=doc.text.len(){return None;}
    let open=visible[..at].rfind('<')?;let start=open+1+usize::from(visible.as_bytes().get(open+1)==Some(&b'/'));
    let end=start+visible[start..].bytes().take_while(|b|word(*b)||*b==b':').count();if at<start||at>end{return None;}
    let name=&visible[start..end];if !crate::refs::valid_tag_name(name,true)||!index.metadata.blade_components.bare_tags.iter().any(|candidate|candidate.starts_with(name)){return None;}
    Some(crate::refs::Reference{kind:Kind::Component,start,end,class:None,closed:index.metadata.blade_components.bare_tags.contains(name)})
}
pub(crate) fn bare_completion(index:&crate::Index,doc:&openidea_plugin::providers::Document,at:openidea_plugin::Position)->Vec<openidea_plugin::providers::CompletionItem>{
    use openidea_plugin::{providers::{CompletionItem,CompletionKind},text,Range};
    let Some(at)=text::offset(&doc.text,at)else{return vec![];};let Some(reference)=bare_reference(index,doc,at)else{return vec![];};let typed=&doc.text[reference.start..at];
    let Some(project)=crate::project_for(&doc.path)else{return vec![];};
    index.metadata.blade_components.bare_tags.iter().filter(|name|name.starts_with(typed)).filter_map(|name|{
        let entry=index.components.get(name)?;if !entry.path.is_empty()&&!crate::owns_current_path(&project,&entry.path){return None;}
        let mut item=CompletionItem::new(name,CompletionKind::Class);item.detail=entry.detail.clone();item.replace=Some(Range::new(text::position(&doc.text,reference.start),text::position(&doc.text,reference.end)));Some(item)
    }).collect()
}
pub(crate) fn bare_definition(index:&crate::Index,doc:&openidea_plugin::providers::Document,at:openidea_plugin::Position)->Vec<openidea_plugin::Location>{
    let run=||{let at=openidea_plugin::text::offset(&doc.text,at)?;let reference=bare_reference(index,doc,at)?;if !reference.closed{return None;}
        let project=crate::project_for(&doc.path)?;let entry=index.components.get(&doc.text[reference.start..reference.end])?;crate::scoped_location(&project,entry)
    };run().into_iter().collect()
}
pub(crate) fn metadata_view<'a>(index:&'a crate::Index,name:&str)->Option<&'a crate::index::Entry>{index.metadata.blade_components.resolved_views.get(name)}

/// Attributes named by `@props([...])` of an anonymous component view.
pub fn view_props(view: &str) -> Vec<Prop> {
    let Some(region)=crate::refs::blade_regions(view).into_iter().find(|region|view[region.clone()].starts_with("@props("))else{return Vec::new();};
    let open=region.start+"@props".len();
    let Some(array) = php::arguments(view, open).into_iter().next().and_then(|range| php::array_open(view, range)) else { return Vec::new() };

    php::items(view, array)
        .into_iter()
        .filter_map(|item| {
            let key = item.key.and_then(|key| php::literal(view, key)).map(|(key, _)| key.to_owned());
            match key {
                Some(name) => Some(Prop { name, default: Some(view[php::trimmed(view, item.value)].to_owned()) }),
                None => php::literal(view, item.value).map(|(name, _)| Prop { name: name.to_owned(), default: None }),
            }
        })
        .collect()
}

/// Constructor parameters of a class component.
pub fn constructor_props(class: &str) -> Vec<Prop> {
    let masked = php::mask_comments(class);
    let Some(structure)=crate::package_facts::code(class)else{return Vec::new();};
    let Some(at) = structure.find("function __construct(") else { return Vec::new() };
    let open = at + "function __construct".len();

    php::arguments(&masked, open)
        .into_iter()
        .filter_map(|range| {
            let parameter = masked[range].trim();
            let (declaration, default) = match parameter.split_once('=') {
                Some((declaration, default)) => (declaration.trim(), Some(default.trim().to_owned())),
                None => (parameter, None),
            };
            let name = declaration.rsplit(char::is_whitespace).next()?.strip_prefix('$')?;
            Some(Prop { name: name.to_owned(), default })
        })
        .collect()
}

/// Public properties and `mount()` parameters of a Livewire component.
pub fn livewire_props(class: &str) -> Vec<Prop> {
    let masked = php::mask_comments(class);
    let mut props = Vec::new();
    let Some(structure)=crate::package_facts::code(class)else{return props;};

    for (line,original) in structure.lines().zip(masked.lines()) {
        let line = line.trim();
        let Some(rest) = line.strip_prefix("public ") else { continue };
        if rest.contains("function ") || rest.starts_with("static ") {
            continue;
        }

        let Some(dollar) = rest.find('$') else { continue };
        let name: String = rest[dollar + 1..].chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
        let default = original.split_once('=').map(|(_, value)| value.trim().trim_end_matches(';').to_owned());
        if !name.is_empty() {
            props.push(Prop { name, default });
        }
    }

    if let Some(at) = structure.find("function mount(") {
        for range in php::arguments(&masked, at + "function mount".len()) {
            let parameter = masked[range].split('=').next().unwrap_or_default().trim().to_owned();
            if let Some(name) = parameter.rsplit(char::is_whitespace).next().and_then(|p| p.strip_prefix('$'))
                && !props.iter().any(|prop| prop.name == name)
            {
                props.push(Prop { name: name.to_owned(), default: None });
            }
        }
    }

    props
}

/// Variables the view prints that are neither props nor framework variables.
pub fn slot_names(view: &str, props: &[Prop]) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();

    for region in crate::refs::blade_regions(view) {
        let text = &view[region];
        if !text.starts_with("{{") && !text.starts_with("{!!") {
            continue;
        }

        for (at, _) in text.match_indices('$') {
            let name: String = text[at + 1..].chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
            let after = text[at + 1 + name.len()..].trim_start();
            let is_variable = after.is_empty() || after.starts_with("}}") || after.starts_with("!!}") || after.starts_with("??");
            let is_prop = props.iter().any(|prop| prop.name == name);

            if !name.is_empty() && is_variable && !is_prop && !matches!(name.as_str(), "slot" | "attributes" | "component" | "loop" | "errors") && !names.contains(&name) {
                names.push(name);
            }
        }
    }

    names
}

pub fn attribute_name(name: &str) -> String {
    separated(name, '-')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context(marked: &str) -> Option<Context> {
        let at = marked.find('|').unwrap();
        context_at(&marked.replacen('|', "", 1), at)
    }

    #[test]
    fn component_property_sources_ignore_fake_declarations_inside_php_or_blade_data() {
        let class="<?php class Card { private string $payload = 'function __construct(public string $fake) {}'; }";
        assert!(constructor_props(class).is_empty());
        let class="<?php class Counter { private string $payload = 'function mount($fake) {}\npublic string $invented;';\npublic string $actual = 'function'; }";
        assert_eq!(livewire_props(class).into_iter().map(|prop|prop.name).collect::<Vec<_>>(),["actual"]);
        assert!(view_props("@php $x=\"@props(['fake'])\"; @endphp").is_empty());
        assert!(view_props("{{-- @props(['fake']) --}}").is_empty());
        assert_eq!(view_props("@props(['actual'])").into_iter().map(|prop|prop.name).collect::<Vec<_>>(),["actual"]);
    }
    #[test]
    fn namespaced_tags_preserve_attribute_and_slot_owner_names() {
        assert!(matches!(context("<x-ui::card ti|"),Some(Context::Attribute{component,..})if component=="ui::card"));
        assert!(matches!(context("<livewire:blog::counter ti|"),Some(Context::Attribute{component,kind:Kind::Livewire,..})if component=="blog::counter"));
        assert!(matches!(context("<x-ui::card><x-slot:he|"),Some(Context::Slot{component,..})if component=="ui::card"));
        assert!(context("<x-ui::card></x-ui::card><x-slot:he|").is_none());
        assert!(context("{{-- <x-ui::card ti| --}}").is_none());
        assert!(context("<x-ui:card ti|").is_none());
    }
    #[test]
    fn actual_providers_complete_navigate_and_read_namespaced_component_attributes() {
        use openidea_plugin::{mock::MockHost,providers::{Providers,Document},text,Position};
        use crate::index::Entry;
        let host=MockHost::install().with_capabilities(&["project.read","editor.read"]);
        host.add_file("composer.json",r#"{"require":{"laravel/framework":"^12","livewire/livewire":"^3"},"autoload":{"psr-4":{"App\\":"app/"}}}"#);
        host.add_file("artisan","<?php");host.add_file("ide.json",r#"{"packages":{"livewire":{"namespaces":[{"classNamespace":"App\\Blog\\Livewire","tagPrefix":"blog"}]}}}"#);
        host.add_file("app/Blog/Livewire/Counter.php","<?php\nnamespace App\\Blog\\Livewire;\nclass Counter extends \\Livewire\\Component {\n public string $title;\n}");
        host.add_file("resources/views/components/anonymous.blade.php","@props(['tone']) {{$header}} {{$slot}}");
        host.add_file("app/View/Components/Card.php","<?php\nclass Card { public function __construct(public string $classTitle) {} }");
        let project=host.invocation().project().unwrap();let mut index=crate::Index::build(&project);assert!(index.livewire.contains_key("blog::counter"));
        index.components.insert("ui::card".into(),Entry{path:"/project/resources/views/components/anonymous.blade.php".into(),at:Position::new(0,0),width:0,detail:None});
        let mut main=crate::Main::default();main.indexes.insert("/project".into(),index);
        let doc=|source:&str|Document{path:"/project/resources/views/page.blade.php".into(),language:"blade".into(),text:source.into(),version:1};
        for (source,label) in [("<livewire:blog::cou>","blog::counter"),("</livewire:blog::cou>","blog::counter"),("<x-ui::ca>","ui::card"),("</x-ui::ca>","ui::card")]{let at=text::position(source,source.find('>').unwrap());assert!(main.completion(&doc(source),at,None).iter().any(|i|i.label==label),"{source}");}
        for source in ["<livewire:blog::counter/>","</livewire:blog::counter>"]{let at=text::position(source,source.find("counter").unwrap()+2);let found=main.definition(&doc(source),at);assert!(found.iter().any(|d|d.path.ends_with("/Counter.php")),"{source}");}
        let source="<livewire:blog::counter ti";assert!(main.completion(&doc(source),text::position(source,source.len()),None).iter().any(|i|i.label=="title"));
        let source="<x-ui::card to";assert!(main.completion(&doc(source),text::position(source,source.len()),None).iter().any(|i|i.label=="tone"));
        let source="<x-ui::card><x-slot:he";assert!(main.completion(&doc(source),text::position(source,source.len()),None).iter().any(|i|i.label=="header"));
        main.indexes.get_mut("/project").unwrap().components.insert("ui::card".into(),Entry{path:"/project/app/View/Components/Card.php".into(),at:Position::new(1,6),width:4,detail:None});
        let source="<x-ui::card cl";let items=main.completion(&doc(source),text::position(source,source.len()),None);assert!(items.iter().any(|i|i.label=="class-title"));assert!(!items.iter().any(|i|i.label=="tone"));
        assert!(!crate::problems(main.indexes.get("/project").unwrap(),"blade","<x-external::card/><livewire:external::counter/>",false).iter().any(|d|matches!(d.code.as_deref(),Some("missing-component"|"missing-livewire"))));
        host.add_project("/project/app/Blog");let source="<livewire:blog::counter ti";assert!(main.completion(&doc(source),text::position(source,source.len()),None).is_empty());
    }
    #[test]
    fn attribute_position_inside_a_component_tag() {
        assert_eq!(context("<x-alert ty|"), Some(Context::Attribute { kind: Kind::Component, component: "alert".into(), start: 9 }));
        assert_eq!(context("<x-alert :ty|"), Some(Context::Attribute { kind: Kind::Component, component: "alert".into(), start: 10 }));
        assert_eq!(context("<livewire:counter |"), Some(Context::Attribute { kind: Kind::Livewire, component: "counter".into(), start: 18 }));
    }

    #[test]
    fn no_attribute_completion_on_the_tag_name_inside_values_or_after_the_tag() {
        assert_eq!(context("<x-ale|"), None);
        assert_eq!(context("<x-alert type=\"in|"), None);
        assert_eq!(context("<x-alert type=\"a > b\" ti|").is_some(), true);
        assert_eq!(context("<x-alert a=\"1\"> ti|"), None);
        assert_eq!(context("<div cla|"), None);
        assert_eq!(context("<x-alert :a=\"{{ $x }}\" b|").is_some(), true);
    }

    #[test]
    fn slot_names_belong_to_the_enclosing_component() {
        assert_eq!(context("<x-card>\n  <x-slot:he|"), Some(Context::Slot { component: "card".into(), start: 19 }));
        assert_eq!(context("<x-card></x-card>\n<x-slot:he|"), None);
        assert_eq!(context("<x-card>\n<x-alert />\n<x-slot:he|").map(|c| matches!(c, Context::Slot { component, .. } if component == "card")), Some(true));
    }

    #[test]
    fn props_come_from_the_view_the_constructor_and_the_livewire_class() {
        let view = "@props(['type' => 'info', 'message', \"dismissible\" => false])\n<div>{{ $message }} {{ $footer }} {{ $slot }}</div>\n{!! $title ?? '' !!}";
        let props = view_props(view);
        assert_eq!(props.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), ["type", "message", "dismissible"]);
        assert_eq!(props[0].default.as_deref(), Some("'info'"));
        assert_eq!(slot_names(view, &props), ["footer", "title"]);

        let class = "<?php\nclass Alert extends Component\n{\n    public function __construct(public string $type = 'info', $userName)\n    {\n    }\n}\n";
        let props = constructor_props(class);
        assert_eq!(props.iter().map(|p| (p.name.as_str(), p.default.as_deref())).collect::<Vec<_>>(), [("type", Some("'info'")), ("userName", None)]);
        assert_eq!(attribute_name("userName"), "user-name");

        let livewire = "<?php\nclass Counter extends Component\n{\n    public int $count = 0;\n    public $label;\n    public function mount($start, $label = null) {}\n    public static $x;\n}\n";
        let names: Vec<String> = livewire_props(livewire).into_iter().map(|p| p.name).collect();
        assert_eq!(names, ["count", "label", "start"]);
    }
}
