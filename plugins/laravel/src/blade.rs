//! What a Blade view says about the others: the layout it extends, the views
//! it includes, and its sections, yields, pushes and stacks.

use crate::php;
use crate::refs::blade_regions;

/// A directive argument and where its contents start.
pub type Named = (String, usize);

#[derive(Default, Debug, Clone)]
pub struct Facts {
    pub extends: Option<String>,
    /// Ordered literal fallback candidates; consumers select a proven existing layout.
    pub extends_first: Vec<String>,
    pub includes: Vec<String>,
    pub sections: Vec<Named>,
    pub yields: Vec<Named>,
    pub pushes: Vec<Named>,
    pub stacks: Vec<Named>,
}

pub fn facts(text: &str) -> Facts {
    let mut facts = Facts::default();

    for region in blade_regions(text) {
        let Some(directive) = text[region.clone()].strip_prefix('@') else { continue };
        let name_length = directive.bytes().position(|b| !b.is_ascii_alphabetic()).unwrap_or(directive.len());
        let name = &directive[..name_length];
        let Some(open) = text[region.start..region.end].find('(').map(|i| region.start + i) else { continue };

        let arguments = php::arguments(text, open);
        let argument = |i: usize| arguments.get(i).and_then(|r| php::literal(text, r.clone())).map(|(v, at)| (v.to_owned(), at));

        match name {
            "extends" => facts.extends = argument(0).map(|(v, _)| v),
            "extendsFirst" | "includeFirst" => {
                if let Some(array)=arguments.first().and_then(|range|php::array_open(text,range.clone())) {
                    let names:Option<Vec<_>>=php::items(text,array).into_iter().map(|item|{if item.key.is_some(){return None;}let(name,_)=php::literal(text,item.value)?;(!name.is_empty()&&!name.contains(['\\','$'])).then(||name.to_owned())}).collect();
                    let Some(names)=names else{continue;};
                    if name=="extendsFirst"{facts.extends_first=names;}else{facts.includes.extend(names);}
                }
            },
            "include" | "includeIf" | "includeIsolated" | "component" => facts.includes.extend(argument(0).map(|(v, _)| v)),
            "includeWhen" | "includeUnless" => facts.includes.extend(argument(1).map(|(v, _)| v)),
            "each" => facts.includes.extend([argument(0), argument(3)].into_iter().flatten().map(|(v, _)| v)),
            "section" => facts.sections.extend(argument(0)),
            "yield" => facts.yields.extend(argument(0)),
            "push" | "pushOnce" | "prepend" | "prependOnce" => facts.pushes.extend(argument(0)),
            "stack" => facts.stacks.extend(argument(0)),
            _ => {}
        }
    }

    facts
}

const WIRE_EVENTS: &[&str] = &["click", "dblclick", "submit", "keydown", "keyup", "blur", "focus", "mouseenter", "mouseleave", "init", "poll", "intersect"];
const SCRIPT_GLOBALS: &[&str] = &["alert", "confirm", "prompt", "console", "window", "document", "setTimeout", "setInterval", "true", "false", "null", "undefined", "this"];

/// A component method that a `wire:` attribute calls, and where its name is.
#[derive(Debug, PartialEq, Eq)]
pub struct WireCall {
    pub name: String,
    pub start: usize,
    pub end: usize,
}


/// `wire:click="save"` and `wire:submit.prevent="save(1)"`: the method name.
/// Expressions (`$set(...)`, `form.save`, `a = 1`) are not calls to check.
pub fn wire_calls(text: &str) -> Vec<WireCall> {
    if text.len()>512*1024{return vec![];}
    let masked = crate::refs::without_comments(text);
    let count=masked.match_indices("wire:").count();if count>128||count.saturating_mul(text.len())>8*1024*1024{return vec![];}
    let bytes = masked.as_bytes();
    let lower=masked.to_ascii_lowercase();
    let mut found = Vec::new();

    for (at, _) in masked.match_indices("wire:") {
        if at == 0 || !bytes[at - 1].is_ascii_whitespace() {
            continue;
        }

        if !wire_attribute(&masked,&lower,at){continue;}

        let attribute_end = masked[at..].find(|c: char| c == '=' || c == '>' || c.is_whitespace()).map_or(masked.len(), |i| at + i);
        let attribute = &masked[at + "wire:".len()..attribute_end];
        if !WIRE_EVENTS.contains(&attribute.split('.').next().unwrap_or_default()) || bytes.get(attribute_end) != Some(&b'=') {
            continue;
        }

        let Some(&quote) = bytes.get(attribute_end + 1).filter(|q| matches!(q, b'"' | b'\'')) else { continue };
        let value_start = attribute_end + 2;
        let Some(value_end) = masked[value_start..].find(quote as char).map(|i| value_start + i) else { continue };

        let value = &masked[value_start..value_end];
        let leading = value.len() - value.trim_start().len();
        let value = value.trim();
        let name_length = value.bytes().position(|b| !(b.is_ascii_alphanumeric() || b == b'_')).unwrap_or(value.len());
        let name = &value[..name_length];
        let rest = &value[name_length..];

        let first = name.bytes().next();
        let callable = first.is_some_and(|b| b.is_ascii_alphabetic() || b == b'_') && (rest.is_empty() || rest.starts_with('(') && rest.ends_with(')'));
        if !callable || SCRIPT_GLOBALS.contains(&name) || rest.contains(';') {
            continue;
        }

        let start = value_start + leading;
        found.push(WireCall { name: name.to_owned(), start, end: start + name.len() });
    }

    found
}

pub(crate) fn wire_attribute(text:&str,lower:&str,at:usize)->bool{
 let Some(open)=text[..at].rfind('<')else{return false;};let mut quote=None;for b in text[open+1..at].bytes(){match(quote,b){(Some(q),b)if q==b=>quote=None,(None,b'\''|b'"')=>quote=Some(b),(None,b'>')=>return false,_=>{}}}if quote.is_some(){return false;}
 let lower=&lower[..at];for tag in ["script","style","textarea","title"]{if let Some(raw)=lower.rfind(&format!("<{tag}")){if lower.rfind(&format!("</{tag}")).is_none_or(|close|raw>close)&&text[raw..at].contains('>'){return false;}}}true
}

/// Every method name of a Livewire component class, or `None` when the class
/// can have methods the text does not show (a trait, a parent other than
/// `Component`, `__call`, Volt, attributes that register listeners).
pub fn livewire_methods(text: &str) -> Option<Vec<String>> {
    let masked = crate::package_facts::code(text)?;
    let full=crate::eloquent_members::declaration(text)?;let short=full.rsplit('\\').next()?;
    let class_at=masked.match_indices("class").find_map(|(at,_)|{if at>0&&(php::is_identifier(masked.as_bytes()[at-1])||masked.as_bytes()[at-1]==b':'){return None;}let rest=&masked[at+5..];if !rest.starts_with(char::is_whitespace){return None;}let name=rest.trim_start().split(|c:char|c.is_whitespace()||c=='{').next()?;(name==short).then_some(at)})?;

    let open = masked[class_at..].find('{').map(|i| class_at + i)?;
    let header = masked[class_at..open].split_whitespace().collect::<Vec<_>>();
    let parent = header.iter().position(|w| *w == "extends").and_then(|i| header.get(i + 1))?;
    if crate::eloquent_members::metadata_inference::class_identity(text,parent).as_deref()!=Some("Livewire\\Component") {
        return None;
    }

    let body = &masked[open..];
    let mut depth=0usize;let mut has_trait=false;
    for (at,b) in body.bytes().enumerate(){match b {b'{'=>depth+=1,b'}'=>depth=depth.saturating_sub(1),b'u' if depth==1&&body[at..].starts_with("use ")&&(at==0||!php::is_identifier(body.as_bytes()[at-1]))=>has_trait=true,_=>{}}}

    if has_trait || body.contains("__call") || !stable_attributes(text,&masked,open..masked.len()) {
        return None;
    }

    let mut methods = Vec::new();
    for (at, _) in body.match_indices("function ") {
        let name: String = body[at + "function ".len()..].chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
        if !name.is_empty() {
            methods.push(name);
        }
    }

    Some(methods)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fallback_layouts_and_isolated_includes_keep_order_without_guessing_dynamic_layouts() {
        let facts=facts("@extendsFirst(['missing','layouts.app']) @includeFirst(['parts.a','parts.b']) @includeIsolated('parts.card')");
        assert!(facts.extends.is_none());assert_eq!(facts.extends_first,["missing","layouts.app"]);assert_eq!(facts.includes,["parts.a","parts.b","parts.card"]);
        assert!(super::facts("@extendsFirst([$dynamic,'layouts.app'])").extends_first.is_empty());
    }
    #[test]
    fn reads_layout_sections_and_stacks() {
        let text = "@extends('layouts.app')\n@section('content')\n@include('partials.nav')\n@each('row', $rows, 'r', 'empty')\n@push('scripts')\n@endpush\n@stack('styles')\n@yield('title')\n@section('title', 'Home')";
        let facts = facts(text);
        assert_eq!(facts.extends.as_deref(), Some("layouts.app"));
        assert_eq!(facts.includes, ["partials.nav", "row", "empty"]);
        let names = |named: &[Named]| named.iter().map(|(n, at)| (n.clone(), text[*at..].starts_with(n.as_str()))).collect::<Vec<_>>();
        assert_eq!(names(&facts.sections), [("content".into(), true), ("title".into(), true)]);
        assert_eq!(names(&facts.pushes), [("scripts".into(), true)]);
        assert_eq!(names(&facts.stacks), [("styles".into(), true)]);
        assert_eq!(names(&facts.yields), [("title".into(), true)]);
    }

    #[test]
    fn wire_calls_name_methods_and_skip_expressions() {
        let text = "<button wire:click=\"save\">a</button>\n<form wire:submit.prevent='store(1, $x)'></form>\n<a wire:click=\"$set('a', 1)\"></a>\n<a wire:click=\"form.save\"></a>\n<input wire:model=\"title\">\n{{-- <a wire:click=\"gone\"> --}}\n<a wire:click=\"count = 1\"></a>\n<a wire:click=\"alert('x')\"></a>\n<b wire:click=\"é\"></b>";
        let calls = wire_calls(text);
        let names: Vec<(&str, &str)> = calls.iter().map(|c| (c.name.as_str(), &text[c.start..c.end])).collect();
        assert_eq!(names, [("save", "save"), ("store", "store")]);
    }

    #[test]
    fn wire_calls_keep_exact_offsets_with_crlf_and_multibyte() {
        let text = "<p>é</p>\r\n<a wire:click=\" go \"></a>";
        let call = &wire_calls(text)[0];
        assert_eq!(&text[call.start..call.end], "go");
    }

    #[test]
    fn livewire_methods_are_listed_only_for_plain_components() {
        let plain = "<?php\nuse Livewire\\Component;\nclass Counter extends Component\n{\n    public $c = 0;\n    public function increment() {}\n    protected function hidden() {}\n    public function render() { return fn () => 1; }\n}\n";
        assert_eq!(livewire_methods(plain).unwrap(), ["increment", "hidden", "render"]);
        for risky in [
            "<?php\nclass A extends Component\n{\n    use WithPagination;\n}\n",
            "<?php\nclass A extends Base\n{\n}\n",
            "<?php\nclass A extends Component\n{\n    public function __call($m, $a) {}\n}\n",
            "<?php\nclass A extends Component\n{\n    #[Computed]\n    public function x() {}\n}\n",
        ] {
            assert!(livewire_methods(risky).is_none(), "{risky}");
        }
    }
}

/// Native Livewire/PHP attributes do not manufacture undeclared action methods;
/// unknown attribute classes retain the conservative absence boundary.
pub(crate) fn stable_attributes(source:&str,code:&str,range:std::ops::Range<usize>)->bool{
 let attrs:Vec<_>=code[range.clone()].match_indices("#[").map(|(i,_)|range.start+i).collect();if attrs.len()>64||attrs.len().saturating_mul(source.len())>4*1024*1024{return false;}
 for at in attrs {for item in php::items(source,at+1){let raw=code[item.value].trim();let name=raw.split(|c:char|!c.is_ascii_alphanumeric()&&!matches!(c,'_'|'\\')).next().unwrap_or("");let Some(full)=crate::eloquent_members::metadata_inference::class_identity(source,name)else{return false;};if ["SensitiveParameter","Override","Deprecated"].contains(&full.as_str()){continue;}let Some(attribute)=full.strip_prefix("Livewire\\Attributes\\")else{return false;};if !["Computed","On","Reactive","Modelable","Locked","Url","Validate","Rule","Session","Renderless","Isolate","Title","Layout","Lazy","Js","Async","Defer","Transition","Bindable"].contains(&attribute){return false;}}}true
}
