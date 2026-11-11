//! Spring Integration Tools for OpenIDEA: indexes the message channels a
//! project declares and the endpoints that read from or send to them (Java
//! annotations, the Java DSL and integration XML), then links producers,
//! channels and consumers. Text-based: channels match by name as written.

mod xml_namespaces;
mod file_actions;
mod structure;
mod navigation;
mod graph;
mod channel_generation;
mod batch_navigation;
mod batch_actions;
mod batch_step_rename;
mod import_tree;
mod batch_runtime;
mod batch_launch;
mod java_hover;
mod source_scope;
mod contexts;
mod dsl_flows;

mod source_provenance;
use source_provenance::{source_type_owned,source_bean_owned};

use openidea_plugin::providers::{CompletionItem, CompletionKind, Document, LineMarker, Providers};

use openidea_plugin::views::{Body, Button, View, ViewEvent, Viewport, Views};
use openidea_plugin::{Diagnostic, Error, Invocation, Location, Plugin, Position, Project, Range, Severity, host};
use spring_text::diagram::{self, Edge, Node};
use spring_text::index::{COMMAND_BUDGET, Index, Indexes, PROVIDER_BUDGET};
use spring_text::java::{Annotation, Str};
use spring_text::{beans, contains, range, xml};
use std::collections::{BTreeMap, BTreeSet};

const NAMESPACE: &str = "http://www.springframework.org/schema/integration";
const ENDPOINTS: [&str; 12] = [
    "ServiceActivator",
    "Transformer",
    "Filter",
    "Router",
    "Splitter",
    "Aggregator",
    "BridgeFrom",
    "BridgeTo",
    "InboundChannelAdapter",
    "Publisher",
    "MessagingGateway",
    "Gateway",
];
/// `MessageChannels` factory methods that name the channel they create.
const CHANNEL_FACTORIES: [&str; 7] = ["direct", "queue", "publishSubscribe", "executor", "flux", "priority", "rendezvous"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    /// Creates the channel: a channel `@Bean`, `MessageChannels.queue("x")`, `<int:channel id>`, DSL `.channel("x")`.
    Declares,
    /// An endpoint reading from the channel.
    Consumes,
    /// An endpoint sending to it.
    Produces,
    /// A bean of any kind declared under this name.
    Bean,
    /// Unproved bean spelling retained only for conservative creation collision
    /// checks; never navigation/completion or a proof of project-wide absence.
    BeanCandidate,
    /// An XML `ref` to a bean.
    BeanRef,
    /// Context graph fact (`contexts`): an import/scan edge, a declared type, or an unprovable edge.
    Edge,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ref {
    pub name: String,
    pub role: Role,
    /// Where it is written (`@ServiceActivator handle()`, `<int:bridge>`).
    pub what: String,
    pub range: Range,
    /// Reported when nothing declares or consumes the channel.
    pub checked: bool,
    /// The endpoint (annotation or XML element) it belongs to within its file; 0 for none.
    pub owner: usize,
}

/// The channel and bean names of a Java, Kotlin or XML source.
pub fn parse(path: &str, text: &str) -> Vec<Ref> {
    if path.ends_with(".xml") { parse_xml(text) } else { parse_source(path, text) }
}

fn index_file(path: &str, text: &str) -> Option<Vec<Ref>> {
    Some(parse(path, text)).filter(|refs| !beans::ignored(path) && !refs.is_empty())
}

fn str_ref(text: &str, s: &Str, role: Role, what: String, checked: bool) -> Ref {
    Ref { name: s.value.clone(), role, what, range: range(text, s.at, s.end), checked, owner: 0 }
}

fn annotation_refs(text: &str, a: &Annotation, what: &str, kotlin: bool, out: &mut Vec<Ref>) {
    let mut names=BTreeSet::new();
    if a.args.iter().any(|arg|!names.insert(arg.name.as_str())) {return;}
    for arg in &a.args {
        let role = match (a.name.as_str(), arg.name.as_str()) {
            ("BridgeFrom", "value") => Role::Consumes,
            ("BridgeTo" | "InboundChannelAdapter" | "Publisher", "value") => Role::Produces,
            (_, "inputChannel" | "replyChannel" | "defaultReplyChannel") => Role::Consumes,
            (_, n) if n == "channel" || n.ends_with("Channel") => Role::Produces,
            _ => continue,
        };
        let tokens=spring_text::java::tokens(&arg.text);
        let [one]=tokens.as_slice() else {continue;};
        if one.string().is_none() || one.s.starts_with("\"\"\"") || arg.strings.len()!=1 {continue;}
        for s in &arg.strings {
            if s.value.contains('\\') || kotlin && s.value.contains('$') || text.get(s.at..s.end)!=Some(s.value.as_str()) {continue;}
            out.push(Ref { owner: a.at + 1, ..str_ref(text, s, role, format!("@{} {what}", a.name), role == Role::Produces) });
        }
    }
}

fn integration_annotation_owned(file: &spring_text::java::JavaFile, text: &str, a: &Annotation) -> bool {
    let qualified=format!("org.springframework.integration.annotation.{}",a.name);
    let Some(written)=text.get(a.at+1..a.name_at+a.name.len()) else {return false;};
    source_type_owned(file,written,&qualified)
}
fn channel_bean_owned(file:&spring_text::java::JavaFile,text:&str,bean:&spring_text::java::BeanName,kotlin:bool)->bool {
    let Some(method)=file.types.iter().flat_map(|t|t.methods()).find(|m|m.at==bean.at) else {return false;};
    let annotations=method.annotations.iter().filter(|a|a.name=="Bean").collect::<Vec<_>>();
    let [annotation]=annotations.as_slice() else {return false;};
    let Some(written)=text.get(annotation.at+1..annotation.name_at+annotation.name.len()) else {return false;};
    if !source_type_owned(file,written,"org.springframework.context.annotation.Bean") {return false;}
    let names=annotation.args.iter().filter(|arg|matches!(arg.name.as_str(),"name"|"value")).collect::<Vec<_>>();
    match names.as_slice() {
        []=>if bean.name!=method.name {return false;},
        [name]=>{
            let tokens=spring_text::java::tokens(&name.text);
            let [one]=tokens.as_slice() else {return false;};
            let Some(value)=one.string() else {return false;};
            if one.s.starts_with("\"\"\"") || value.value!=bean.name || value.value.contains('\\') || kotlin && value.value.contains('$') {return false;}
        },
        _=>return false,
    }
    let Some(ty)=method.ty.as_ref().filter(|ty|!ty.array && ty.args.is_empty()) else {return false;};
    ["MessageChannel","PollableChannel","SubscribableChannel"].iter().any(|short|source_type_owned(file,&ty.text,&format!("org.springframework.messaging.{short}")))
}
fn dsl_factory_owned(file:&spring_text::java::JavaFile,text:&str,call:&spring_text::java::Call,short:&str)->bool {
    let qualified=format!("org.springframework.integration.dsl.{short}");
    let tokens=spring_text::java::tokens(text);
    let Some(at)=tokens.iter().position(|t|t.at==call.at) else {return false;};
    if at<2 || tokens[at-1].s!="." {return false;}
    let mut start=at-2;
    while start>=2 && tokens[start-1].s=="." && tokens[start-2].is_ident() {start-=2;}
    let written=tokens[start..at-1].iter().map(|t|t.s).collect::<String>();
    if !source_type_owned(file,&written,&qualified) {return false;}
    let name=if written==qualified {"org"} else {short};
    tokens.iter().enumerate().filter(|(_,t)|t.s==name && !file.imports.iter().any(|i|i.at<=t.at && t.at<i.end)).all(|(i,_)| {
        if name=="org" {
            let parts=qualified.split('.').collect::<Vec<_>>();
            let expected=parts.iter().enumerate().flat_map(|(n,p)|if n==0 {vec![*p]}else{vec![".",*p]}).collect::<Vec<_>>();
            (i==0 || tokens[i-1].s!=".") && tokens.get(i..i+expected.len()).is_some_and(|slice|slice.iter().map(|t|t.s).eq(expected.iter().copied()))
        } else {
            // A value/parameter/field named like the class cannot be borrowed
            // as a static factory. Explicit type uses remain source candidates.
            (i==0 || !matches!(tokens[i-1].s,"."|"val"|"var"|"<")) && tokens.get(i+1).is_some_and(|next|next.s=="." || next.is_ident() && !matches!(next.s,"by"|"extends"|"implements"|"permits"))
        }
    })
}

fn parse_source(path: &str, text: &str) -> Vec<Ref> {
    let mut out=parse_source_refs(path,text);
    if text.len()>512*1024 {return out;}
    let file=spring_text::parse_source(path,text);
    let edges=contexts::source_edges(path,&file,text);
    if !out.is_empty() || !edges.is_empty() {out.extend(edges);out.extend(contexts::source_types(&file));}
    out
}
fn parse_source_refs(path: &str, text: &str) -> Vec<Ref> {
    if text.len()>512*1024 {return vec![Ref{name:String::new(),role:Role::BeanCandidate,what:"Source exceeds admission bound".into(),range:Range::default(),checked:false,owner:0}];}
    let file = spring_text::parse_source(path, text);
    let mut out = Vec::new();
    let kotlin=path.ends_with(".kt");
    let bean_names=spring_text::java::bean_names(&file);
    let annotations_supported=(if kotlin {
        !spring_text::java::tokens(text).iter().any(|t|matches!(t.s,"as"|"typealias"))
    } else { !text.contains("\\u") }) && file.types.iter().all(|owner|owner.supers().next().is_none() && !text[owner.at+owner.name.len()..].trim_start().starts_with('<'));
    if !annotations_supported && (!bean_names.is_empty() || text.contains("integration") || file.imports.iter().any(|import|import.path.rsplit('.').next().is_some_and(|name|name=="Bean" || spring_text::java::STEREOTYPES.contains(&name)))) {
        out.push(Ref{name:String::new(),role:Role::BeanCandidate,what:"Unresolved source annotation/type context".into(),range:Range::default(),checked:false,owner:0});
    }
    let mut bean_ownership=BTreeMap::new();
    let mut seen_beans=BTreeSet::new();
    for b in bean_names {
        if !seen_beans.insert((b.at,b.name.clone())) {continue;}
        let r = range(text, b.at, b.at + b.len);
        let what = format!("{} bean", b.ty);
        // All aliases share one physical declaration and annotation. Validate
        // that whole declaration once rather than re-tokenizing large arrays.
        let (owned,channel)=*bean_ownership.entry(b.at).or_insert_with(||(annotations_supported && source_bean_owned(&file,text,&b,kotlin),annotations_supported && channel_bean_owned(&file,text,&b,kotlin)));
        if channel {
            out.push(Ref { name: b.name.clone(), role: Role::Declares, what: what.clone(), range: r, checked: false, owner: 0 });
        }
        let role=if owned {Role::Bean}else{Role::BeanCandidate};
        out.push(Ref { name: b.name, role, what, range: r, checked: false, owner: 0 });
    }
    if !text.contains("integration") {
        return out;
    }
    for t in &file.types {
        for a in t.annotations.iter().filter(|a| annotations_supported && a.name == "MessagingGateway" && integration_annotation_owned(&file,text,a)) {
            annotation_refs(text, a, &t.name, kotlin, &mut out);
        }
        for m in t.methods() {
            for a in m.annotations.iter().filter(|a| annotations_supported && ENDPOINTS.contains(&a.name.as_str()) && integration_annotation_owned(&file,text,a)) {
                annotation_refs(text, a, &format!("{}()", m.name), kotlin, &mut out);
            }
        }
    }
    if !annotations_supported || kotlin && text.contains('`') || !text.contains("IntegrationFlow") && !text.contains("integrationFlow") && !text.contains("MessageChannels") {
        return out;
    }
    let calls = spring_text::source_calls(path, text);
    for c in &calls {
        let Some(s) = c.args.first().and_then(|a| a.string.as_ref()) else {
            continue;
        };
        let receiver = c.receiver.as_deref();
        if c.args[0].text.trim_start().starts_with("\"\"\"") || s.value.contains('\\') || kotlin && s.value.contains('$') || text.get(s.at..s.end)!=Some(s.value.as_str()) {continue;}
        if CHANNEL_FACTORIES.contains(&c.name.as_str()) && receiver == Some("MessageChannels") && dsl_factory_owned(&file,text,c,"MessageChannels") {
            out.push(str_ref(text, s, Role::Declares, format!("MessageChannels.{}", c.name), false));
        }
    }
    // The DSL creates a channel when no bean has that name, so a flow `.channel("x")` is never unresolved.
    dsl_flows::java_chains(&file, text, &calls, kotlin, &mut out);
    if kotlin {
        dsl_flows::kotlin_flows(&file, text, &calls, &mut out);
    }
    out
}

fn xml_bean_container_owned(elements:&[xml::Element],mut at:usize)->bool {
    while let Some(parent)=elements[at].parent {
        if elements[parent].local()!="beans" || !matches!(xml::namespace(elements,parent),None|Some("http://www.springframework.org/schema/beans")) {return false;}
        at=parent;
    }
    true
}
fn xml_alias_spans(value:&str)->Vec<(usize,usize)> {
    let mut result=Vec::new();let mut start=None;
    for (at,ch) in value.char_indices() {
        if ch.is_ascii_whitespace() || matches!(ch,','|';') {
            if let Some(from)=start.take() {result.push((from,at));}
        } else if start.is_none() {start=Some(at);}
    }
    if let Some(from)=start {result.push((from,value.len()));}result
}
fn parse_xml(text: &str) -> Vec<Ref> {
    if text.len()>512*1024 {return vec![Ref{name:String::new(),role:Role::BeanCandidate,what:"XML source exceeds admission bound".into(),range:Range::default(),checked:false,owner:0}];}
    let els = xml::elements(text);
    let spring=text.contains("www.springframework.org/schema/") || els.first().is_some_and(|root|root.local()=="beans" && matches!(xml::namespace(&els,0),None|Some("http://www.springframework.org/schema/beans")));
    if !spring {return Vec::new();}
    if !xml_namespaces::document_valid(text) {
        return vec![Ref{name:String::new(),role:Role::BeanCandidate,what:"Unresolved XML source envelope".into(),range:Range::default(),checked:false,owner:0}];
    }
    let attr_range = |a: &xml::Attr| range(text, a.value_at, a.value_end);
    let mut out=Vec::new();let mut seen=BTreeSet::new();
    for (i,element) in els.iter().enumerate() {
        let top=element.parent.is_none_or(|parent|els[parent].local()=="beans") || element.local()=="bean";
        if !top || i==0 && element.local()=="beans" {continue;}
        let namespace=xml::namespace(&els,i);
        let known_container=xml_bean_container_owned(&els,i);
        let standard=matches!(namespace,None|Some("http://www.springframework.org/schema/beans")) && matches!(element.local(),"bean"|"alias") && element.parent.is_some();
        let integration=namespace==Some(NAMESPACE) && matches!(element.local(),"channel"|"publish-subscribe-channel"|"bridge"|"service-activator"|"transformer"|"filter"|"splitter"|"aggregator"|"router"|"gateway"|"inbound-channel-adapter"|"outbound-channel-adapter"|"chain");
        let batch=namespace==Some("http://www.springframework.org/schema/batch") && matches!(element.local(),"job"|"flow"|"step");
        for attribute in &element.attrs {
            let alias=element.local()=="alias" && attribute.local()=="alias";
            let names=element.local()=="bean" && attribute.local()=="name";
            if !alias && !names && !(element.local()!="alias" && attribute.local()=="id") {continue;}
            let spans=if names {xml_alias_spans(&attribute.value)}else{vec![(0,attribute.value.len())]};
            for (start,end) in spans {
                let name=&attribute.value[start..end];if name.is_empty() {continue;}
                let owned=known_container && (standard || integration || batch) && attribute.name==attribute.local() && !name.chars().any(char::is_control) && !name.chars().any(|c|c.is_ascii_whitespace()) && !beans::is_expression(name);
                let at=attribute.value_at+start;let to=attribute.value_at+end;
                if !owned || seen.insert((i,name.to_string())) {out.push(Ref{name:name.into(),role:if owned {Role::Bean}else{Role::BeanCandidate},what:"XML bean source name".into(),range:range(text,at,to),checked:false,owner:0});}
            }
        }
    }
    out.extend(contexts::xml_edges(&els,text));
    if !text.contains(NAMESPACE) {
        return out;
    }
    for (i, e) in els.iter().enumerate() {
        if !xml::namespace(&els, i).is_some_and(|ns| ns==NAMESPACE || ns.strip_prefix(NAMESPACE).is_some_and(|rest|rest.starts_with('/'))) {
            continue;
        }
        let outbound = e.local().contains("outbound");
        for a in &e.attrs {
            if a.name!=a.local() {continue;}
            let role = match a.local() {
                "id" if matches!(e.local(), "channel" | "publish-subscribe-channel") => Role::Declares,
                "input-channel" | "default-reply-channel" => Role::Consumes,
                "reply-channel" if !outbound => Role::Consumes,
                "request-channel" | "channel" if outbound => Role::Consumes,
                "output-channel"
                | "default-output-channel"
                | "discard-channel"
                | "error-channel"
                | "default-request-channel"
                | "request-channel"
                | "reply-channel"
                | "channel" => Role::Produces,
                "ref" => Role::BeanRef,
                _ => continue,
            };
            if a.value.chars().any(char::is_control) {
                out.push(Ref{name:String::new(),role:Role::BeanCandidate,what:"Unresolved XML attribute normalization".into(),range:attr_range(a),checked:false,owner:0});continue;
            }
            let checked = matches!(role, Role::Produces | Role::BeanRef);
            out.push(Ref { name: a.value.clone(), role, what: format!("<{}>", e.name), range: attr_range(a), checked, owner: i + 1 });
        }
    }
    out
}

/// Whether a document may hold channels or bean references worth looking at.
fn relevant(doc: &Document) -> bool {
    match doc.language.as_str() {
        "xml" => doc.text.contains(NAMESPACE),
        _ => doc.text.contains("integration"),
    }
}

fn file_name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

pub struct Integration {
    indexes: Indexes<Vec<Ref>>,
    creation: file_actions::Creation,
    graph: graph::External,
    source_creation: channel_generation::Creation,
}

impl Default for Integration {
    fn default() -> Self {
        Self { indexes: Indexes::new(&beans::GLOBS, index_file), creation: Default::default(), graph: Default::default(), source_creation: Default::default() }
    }
}

/// Every reference of the project with its file.
fn all(index: &Index<Vec<Ref>>) -> impl Iterator<Item = (&str, &Ref)> {
    index.iter().flat_map(|(path, refs)| refs.iter().map(move |r| (path, r)))
}

/// The references named like `r` in `roles`, `r` itself excluded.
fn others<'a>(index: &'a Index<Vec<Ref>>, doc: &Document, r: &Ref, roles: &[Role]) -> Vec<(&'a str, &'a Ref)> {
    all(index).filter(|(p, x)| roles.contains(&x.role) && x.name == r.name && !(*p == doc.path && x.range == r.range)).collect()
}

/// Where `r` is declared: beans for a bean reference, else channel declarations, else consumers.
fn definitions<'a>(index: &'a Index<Vec<Ref>>, doc: &Document, r: &Ref) -> Vec<(&'a str, &'a Ref)> {
    match r.role {
        Role::BeanRef => others(index, doc, r, &[Role::Bean]),
        _ => Some(others(index, doc, r, &[Role::Declares]))
            .filter(|d| !d.is_empty())
            .unwrap_or_else(|| others(index, doc, r, &[Role::Consumes])),
    }
}

fn location((path, r): (&str, &Ref)) -> Location {
    Location { path: path.into(), range: r.range }
}

impl Integration {
    fn warm_project(&mut self, project: &Project, budget: std::time::Duration) {
        let Some(scope)=project.root().ok().and_then(|root|source_scope::Scope::current(&root)) else{return;};
        scope.prune(self.indexes.get(project,budget));
    }

    fn reset_live(&mut self) {
        self.indexes.reset();
    }

    fn live_diagnostics(&mut self, doc: &Document) -> Vec<Diagnostic> {
        if !relevant(doc) {
            return Vec::new();
        }
        let Some(scope)=source_scope::Scope::for_path(&doc.path) else{return Vec::new();};
        let Some(index)=self.indexes.for_cached_document(&doc.path,&doc.text) else{return Vec::new();};
        let Some(scope)=scope.refresh_for_path(&doc.path) else{return Vec::new();};
        if !scope.prune(index) || !index.fresh() {return Vec::new();}
        problems(index,doc)
    }

    fn index(&mut self, doc: &Document) -> Option<&Index<Vec<Ref>>> {
        if !relevant(doc) {
            return None;
        }
        let scope=source_scope::Scope::for_path(&doc.path)?;
        let index=self.indexes.for_document(&doc.path,&doc.text,PROVIDER_BUDGET)?;
        let scope=scope.refresh_for_path(&doc.path)?;
        scope.prune(index).then_some(&*index)
    }
}

fn here<'a>(index: &'a Index<Vec<Ref>>, doc: &Document) -> &'a [Ref] {
    index.files().get(&doc.path).map_or(&[], |refs| refs.as_slice())
}

fn problems(index: &Index<Vec<Ref>>, doc: &Document) -> Vec<Diagnostic> {
    if !index.complete() || all(index).any(|(_,reference)|reference.role==Role::BeanCandidate) {
        return Vec::new();
    }
    let known = |name: &str, roles: &[Role]| all(index).any(|(_, x)| x.name == name && roles.contains(&x.role));
    let contexts = contexts::build(index);
    let provenance = here(index, doc).iter().filter(|r| matches!(r.role, Role::Consumes | Role::Produces | Role::BeanRef)).filter(|r| !r.name.is_empty() && !beans::is_expression(&r.name)).filter_map(|r| {
        let (found, contexts_spanned) = contexts.scope(&doc.path, definitions(index, doc, r));
        if contexts_spanned < 2 {
            return None;
        }
        let files: BTreeSet<&str> = found.iter().map(|(p, _)| file_name(p)).collect();
        let message = format!("'{}' is defined in {contexts_spanned} contexts ({}); this file belongs to all of them.", r.name, files.into_iter().collect::<Vec<_>>().join(", "));
        Some(Diagnostic::new(r.range, Severity::Info, message).code("multi-context-definition"))
    }).collect::<Vec<_>>();
    here(index, doc)
        .iter()
        .filter(|r| r.checked && !r.name.is_empty() && !beans::is_expression(&r.name))
        .filter(|r| !beans::INFRASTRUCTURE.contains(&r.name.as_str()))
        .filter_map(|r| {
            let (code, message) = match r.role {
                Role::BeanRef if !known(&r.name, &[Role::Bean]) => {
                    ("unknown-bean", format!("No supported indexed literal bean named '{}' was found. Imported/composed/runtime contexts are unresolved.", r.name))
                }
                Role::Produces if !known(&r.name, &[Role::Declares, Role::Consumes, Role::Bean]) => (
                    "unconsumed-channel",
                    format!(
                        "No supported indexed literal consumer or declaration for channel '{}' was found. Imported/composed/runtime wiring is unresolved.",
                        r.name
                    ),
                ),
                _ => return None,
            };
            Some(Diagnostic::new(r.range, Severity::Warning, message).code(code))
        })
        .chain(provenance)
        .collect()
}

impl Providers for Integration {
    fn completion(&mut self, doc: &Document, at: Position, _trigger: Option<&str>) -> Vec<CompletionItem> {
        let Some(index) = self.index(doc) else {
            return Vec::new();
        };
        let Some(r) =
            here(index, doc).iter().find(|r| contains(r.range, at) && matches!(r.role, Role::Consumes | Role::Produces | Role::BeanRef))
        else {
            return Vec::new();
        };
        let (roles, detail) = match r.role {
            Role::BeanRef => (&[Role::Bean][..], "bean"),
            _ => (&[Role::Declares, Role::Consumes, Role::Produces][..], "message channel"),
        };
        let names: BTreeSet<&str> = all(index)
            .filter(|(_, x)| roles.contains(&x.role) && !x.name.is_empty() && !beans::is_expression(&x.name))
            .map(|(_, x)| x.name.as_str())
            .collect();
        names
            .into_iter()
            .map(|name| CompletionItem {
                detail: Some(detail.into()),
                replace: Some(r.range),
                ..CompletionItem::new(name, CompletionKind::Reference)
            })
            .collect()
    }

    fn definition(&mut self, doc: &Document, at: Position) -> Vec<Location> {
        if doc.language == "xml" && !crate::xml_namespaces::document_valid(&doc.text) { return vec![]; }
        let Some(index) = self.index(doc) else {
            return Vec::new();
        };
        let Some(r) = here(index, doc).iter().find(|r| contains(r.range, at) && matches!(r.role,Role::Declares|Role::Consumes|Role::Produces|Role::BeanRef) && !r.name.is_empty()) else {
            return Vec::new();
        };
        let (found, _) = contexts::build(index).scope(&doc.path, definitions(index, doc, r));
        found.into_iter().map(location).collect()
    }

    fn line_markers(&mut self, doc: &Document) -> Vec<LineMarker> {
        let Some(index) = self.index(doc) else {
            return Vec::new();
        };
        let mut markers = Vec::new();
        let contexts = contexts::build(index);
        for r in here(index, doc).iter().filter(|r| !r.name.is_empty()) {
            let (found, title) = match r.role {
                Role::Consumes => (others(index, doc, r, &[Role::Produces]), format!("Sent to '{}' by", r.name)),
                Role::Produces => (others(index, doc, r, &[Role::Consumes]), format!("'{}' is consumed by", r.name)),
                _ => continue,
            };
            let (found, _) = contexts.scope(&doc.path, found);
            let Some(&first) = found.first() else {
                continue;
            };
            let list: Vec<String> =
                found.iter().map(|(p, x)| format!("{} ({}:{})", x.what, file_name(p), x.range.start.line + 1)).collect();
            markers.push(LineMarker {
                line: r.range.start.line,
                icon: None,
                tooltip: format!("{title}: {}", list.join(", ")),
                target: Some(location(first)),
            });
        }
        markers
    }
}

const VIEW: &str = "diagram";

fn key(prefix: &str, path: &str, range: Range) -> String {
    format!("{prefix}:{}:{}:{path}", range.start.line, range.start.character)
}

/// `in -> out` channel names of an endpoint, to tell endpoints of one kind apart.
fn route(refs: &[&Ref]) -> String {
    let names = |role: Role| refs.iter().filter(|r| r.role == role).map(|r| r.name.as_str()).collect::<Vec<_>>().join(",");
    format!("({} -> {})", names(Role::Consumes), names(Role::Produces))
}

/// Channels as nodes and endpoints as small nodes: input channel -> endpoint -> output channel.
fn diagram_of(index: &Index<Vec<Ref>>) -> (Vec<Node>, Vec<Edge>) {
    let channel_roles = [Role::Declares, Role::Consumes, Role::Produces];
    let mut channels: BTreeMap<&str, (&str, &Ref)> = BTreeMap::new();
    for (path, r) in all(index).filter(|(_, r)| channel_roles.contains(&r.role) && !r.name.is_empty() && !beans::is_expression(&r.name)) {
        let entry = channels.entry(r.name.as_str()).or_insert((path, r));
        if r.role == Role::Declares && entry.1.role != Role::Declares {
            *entry = (path, r);
        }
    }
    let mut endpoints: BTreeMap<(&str, usize), Vec<&Ref>> = BTreeMap::new();
    for (path, r) in all(index).filter(|(_, r)| r.owner != 0 && matches!(r.role, Role::Consumes | Role::Produces)) {
        if channels.contains_key(r.name.as_str()) {
            endpoints.entry((path, r.owner)).or_default().push(r);
        }
    }
    let channel_id = |name: &str| key("b", channels[name].0, channels[name].1.range);
    let mut nodes: Vec<Node> = channels
        .iter()
        .map(|(name, (path, r))| Node::new(&key("b", path, r.range), name).detail("channel").accent(r.role == Role::Declares))
        .collect();
    let mut edges = BTreeSet::new();
    for ((path, _), refs) in &endpoints {
        let id = key("e", path, refs[0].range);
        let same_kind = endpoints.values().filter(|o| o[0].what == refs[0].what).count() > 1;
        let label = if same_kind { format!("{} {}", refs[0].what, route(refs)) } else { refs[0].what.clone() };
        nodes.push(Node::new(&id, &label).detail(file_name(path)));
        for r in refs {
            edges.insert(match r.role {
                Role::Consumes => (channel_id(&r.name), id.clone()),
                _ => (id.clone(), channel_id(&r.name)),
            });
        }
    }
    (nodes, edges.into_iter().map(|(a, b)| Edge::new(&a, &b)).collect())
}

impl Views for Integration {
    fn render(&mut self, _view: &str, _viewport: Option<Viewport>, ctx: &Invocation) -> Result<View, Error> {
        (|| -> Result<View, Error> {
            self.render_graph(ctx)
        })().map(ui_kit::actions::available)
    }

    fn handle(&mut self, view: &str, event: ViewEvent, ctx: &Invocation) -> Result<(), Error> {
        if ui_kit::web::handle(self, view, &event, ctx, include_str!("ui_config.json"))? {
            return Ok(());
        }

        if matches!(&event,ViewEvent::Button(b) if b == "refresh") {
            if let Some(project)=ctx.project() && self.indexes.get(&project,std::time::Duration::ZERO).complete() {self.indexes.reset();}
            refresh(view);return Ok(());
        }
        self.handle_graph(event,ctx)
    }
}

impl Plugin for Integration {
    fn run_command(&mut self, id: &str, ctx: &Invocation, _arg: Option<&str>) -> Result<Option<String>, Error> {
        match id {
            "integration.current-file-channel-occurrences" => {
                if ctx.editor().is_some_and(|editor|editor.language()=="java") {java_hover::occurrences(ctx)} else {navigation::occurrences(ctx)}
            },
            _ => Err(Error::NotFound(format!("command {id}"))),
        }
    }
}

pub const WINDOW_SECTION: &str = "integration";

pub const OWNER_WINDOW: &str = "structure";

fn id(name: &str) -> String {
    format!("{WINDOW_SECTION}.{name}")
}

pub(crate) fn refresh(_view: &str) {
    host::refresh_view(OWNER_WINDOW);
    let _ = host::post_view_message(OWNER_WINDOW, r#"{"channel":"ui-kit-refresh"}"#);
}

const BUILD_FILES: [&str; 3] = ["pom.xml", "build.gradle", "build.gradle.kts"];
const DEPENDENCY_MARKERS: [&str; 8] =
    ["spring-integration", "starter-integration", "<module>", "include(", "include '", "include \"", "(libs.", " libs."];

/// True when unsure: a multi-module build or a version catalog hides the dependency from the root build file.
pub fn applies(project: &Project) -> bool {
    let Ok(root) = project.root() else {
        return true;
    };
    BUILD_FILES
        .iter()
        .filter_map(|file| project.read_file(&format!("{root}/{file}")).ok())
        .any(|build| DEPENDENCY_MARKERS.iter().any(|marker| build.contains(marker)))
}

openidea_plugin::register_plugin!(structure::Structure, providers, views, diagnostics, file_actions, hover);

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;

    const CONFIG: &str = r#"package com.acme;

import org.springframework.integration.dsl.IntegrationFlow;
import org.springframework.integration.dsl.MessageChannels; import org.springframework.context.annotation.Configuration; import org.springframework.context.annotation.Bean; import org.springframework.messaging.MessageChannel; import org.springframework.messaging.PollableChannel;

@Configuration
public class FlowConfig {
    @Bean
    public MessageChannel orders() { return new DirectChannel(); }

    @Bean
    public IntegrationFlow shipping() {
        return IntegrationFlow.from("orders")
            .channel("shipping")
            .channel(MessageChannels.queue("audit"))
            .get();
    }

    @Bean
    public PollableChannel late() { return MessageChannels.priority("late").getObject(); }
}
"#;
    const ENDPOINTS_SRC: &str = r#"package com.acme;

import org.springframework.stereotype.Component; import org.springframework.integration.annotation.ServiceActivator; import org.springframework.integration.annotation.Transformer; import org.springframework.integration.annotation.Filter; import org.springframework.integration.annotation.MessagingGateway; import org.springframework.integration.annotation.Gateway;

@Component
public class Billing {
    @ServiceActivator(inputChannel = "shipping", outputChannel = "invoices")
    public Invoice bill(Order order) { return null; }

    @Transformer(inputChannel = "audit", outputChannel = "nowhere")
    public String audit(Order order) { return ""; }

    @Filter(inputChannel = "late", discardChannel = "${discards}")
    public boolean accept(Order order) { return true; }
}

@MessagingGateway(defaultRequestChannel = "orders", errorChannel = "errorChannel")
interface OrderGateway {
    @Gateway(requestChannel = "orders", replyChannel = "confirmations")
    void place(Order order);
}
"#;
    const XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<beans xmlns="http://www.springframework.org/schema/beans"
       xmlns:int="http://www.springframework.org/schema/integration"
       xmlns:int-http="http://www.springframework.org/schema/integration/http">
    <int:channel id="invoices"/>
    <int:service-activator input-channel="invoices" output-channel="lost" ref="billing"/>
    <int:outbound-channel-adapter channel="invoices" ref="mailer"/>
    <int-http:outbound-gateway request-channel="quotes" reply-channel="quoted" url="http://x"/>
    <bean id="pricing" class="com.acme.Pricing"/>
</beans>
"#;
    const EXPECTED_LABELS: &[&str] = &[
        "<int-http:outbound-gateway>",
        "<int:outbound-channel-adapter>",
        "<int:service-activator>",
        "@Filter accept()",
        "@Gateway place()",
        "@MessagingGateway OrderGateway",
        "@ServiceActivator bill()",
        "@Transformer audit()",
        "IntegrationFlow.from",
        "audit",
        "confirmations",
        "errorChannel",
        "invoices",
        "late",
        "lost",
        "nowhere",
        "orders",
        "quoted",
        "quotes",
        "shipping",
    ];
    const EXPECTED_PAIRS: &[(&str, &str)] = &[
        ("shipping", "@ServiceActivator bill()"),
        ("audit", "@Transformer audit()"),
        ("@MessagingGateway OrderGateway", "errorChannel"),
        ("@MessagingGateway OrderGateway", "orders"),
        ("@Gateway place()", "orders"),
        ("confirmations", "@Gateway place()"),
        ("late", "@Filter accept()"),
        ("invoices", "<int:service-activator>"),
        ("invoices", "<int:outbound-channel-adapter>"),
        ("<int:service-activator>", "lost"),
        ("@ServiceActivator bill()", "invoices"),
        ("quotes", "<int-http:outbound-gateway>"),
        ("<int-http:outbound-gateway>", "quoted"),
        ("@Transformer audit()", "nowhere"),
        ("orders", "IntegrationFlow.from"),
        ("IntegrationFlow.from", "shipping"),
    ];
    const CONFIG_PATH: &str = "/project/src/main/java/com/acme/FlowConfig.java";
    const BILLING_PATH: &str = "/project/src/main/java/com/acme/Billing.java";
    const XML_PATH: &str = "/project/src/main/resources/integration.xml";

    fn setup() -> MockHost {
        let host = MockHost::install();
        host.add_file("src/main/java/com/acme/FlowConfig.java", CONFIG);
        host.add_file("src/main/java/com/acme/Billing.java", ENDPOINTS_SRC);
        host.add_file("src/main/resources/integration.xml", XML);
        host.add_file("pom.xml", "<project><int:channel id=\"ignored\"/></project>");
        host
    }

    fn doc(path: &str, text: &str) -> Document {
        let language = if path.ends_with(".xml") { "xml" } else { "java" };
        Document { path: path.into(), language: language.into(), version: 1, text: text.into() }
    }

    fn at(text: &str, needle: &str) -> Position {
        openidea_plugin::text::position(text, text.find(needle).unwrap() + 1)
    }

    fn names(refs: &[Ref], role: Role) -> Vec<&str> {
        refs.iter().filter(|r| r.role == role).map(|r| r.name.as_str()).collect()
    }

    #[test]
    fn reads_channels_from_java_and_xml() {
        let config = parse(CONFIG_PATH, CONFIG);
        assert_eq!(names(&config, Role::Declares), ["orders", "late", "audit", "late", "shipping"]);
        assert_eq!(names(&config, Role::Consumes), ["orders"]);
        assert_eq!(names(&config, Role::Produces), ["shipping"]);

        let billing = parse(BILLING_PATH, ENDPOINTS_SRC);
        assert_eq!(names(&billing, Role::Consumes), ["shipping", "audit", "late", "confirmations"]);
        assert_eq!(names(&billing, Role::Produces), ["invoices", "nowhere", "${discards}", "orders", "errorChannel", "orders"]);

        let xml = parse(XML_PATH, XML);
        assert_eq!(names(&xml, Role::Declares), ["invoices"]);
        assert_eq!(names(&xml, Role::Consumes), ["invoices", "invoices", "quotes"]);
        assert_eq!(names(&xml, Role::Produces), ["lost", "quoted"]);
        assert_eq!(names(&xml, Role::BeanRef), ["billing", "mailer"]);
        assert!(parse(XML_PATH, "<beans><int:channel id=\"x\"/></beans>").iter().all(|reference|reference.role==Role::BeanCandidate));
    }

    #[test]
    fn global_usages_resolve_in_effective_contexts() {
        let host = MockHost::install();
        let head = "<beans xmlns=\"http://www.springframework.org/schema/beans\" xmlns:int=\"http://www.springframework.org/schema/integration\">";
        let root = |d: &str| format!("{head}<import resource=\"shared.xml\"/><import resource=\"{d}.xml\"/></beans>");
        let decl = format!("{head}<int:channel id=\"c\"/></beans>");
        let shared = format!("{head}<int:bridge input-channel=\"in\" output-channel=\"c\"/></beans>");
        let unrelated = format!("{head}<int:channel id=\"c\"/></beans>");
        let base = "/project/src/main/resources/";
        for (n, t) in [("a.xml", root("da")), ("b.xml", root("db")), ("da.xml", decl.clone()), ("db.xml", decl), ("shared.xml", shared.clone()), ("other.xml", unrelated), ("c.xml", root("dc")), ("dc.xml", shared.replace("<int:bridge input-channel=\"in\" output-channel=\"c\"/>", "<int:channel id=\"c\"/>"))] {
            host.add_file(&format!("{base}{n}"), &t);
        }
        let mut p = Integration::default();
        let path = format!("{base}shared.xml");
        let found = p.definition(&doc(&path, &shared), at(&shared, "\"c\"/>"));
        let paths: BTreeSet<&str> = found.iter().map(|l| l.path.as_str()).collect();
        assert!(paths.iter().any(|x| x.ends_with("da.xml")) && paths.iter().any(|x| x.ends_with("db.xml")), "{paths:?}");
        assert!(paths.iter().any(|x| x.ends_with("other.xml")), "unowned files stay visible: {paths:?}");
        let diagnostics = p.index(&doc(&path, &shared)).map(|i| problems(i, &doc(&path, &shared))).unwrap_or_default();
        assert!(diagnostics.iter().any(|d| d.code.as_deref() == Some("multi-context-definition") && d.message.contains("3 contexts")), "{diagnostics:?}");
    }

    #[test]
    fn goes_to_the_channel_declaration_or_its_consumers() {
        let _host = setup();
        let mut p = Integration::default();
        let found = p.definition(&doc(BILLING_PATH, ENDPOINTS_SRC), at(ENDPOINTS_SRC, "\"shipping\""));
        assert_eq!(found.len(), 1);
        assert_eq!((found[0].path.as_str(), found[0].range.start.line), (CONFIG_PATH, 13));

        let found = p.definition(&doc(BILLING_PATH, ENDPOINTS_SRC), at(ENDPOINTS_SRC, "\"invoices\""));
        assert_eq!((found[0].path.as_str(), found[0].range.start.line), (XML_PATH, 4));

        let found = p.definition(&doc(XML_PATH, XML), at(XML, "\"quoted\""));
        assert!(found.is_empty(), "nothing declares or consumes it");

        let found = p.definition(&doc(XML_PATH, XML), at(XML, "\"billing\""));
        assert_eq!((found[0].path.as_str(), found[0].range.start.line), (BILLING_PATH, 5));
    }

    #[test]
    fn marks_producers_and_consumers() {
        let _host = setup();
        let mut p = Integration::default();
        let markers = p.line_markers(&doc(BILLING_PATH, ENDPOINTS_SRC));
        let tips: Vec<_> = markers.iter().map(|m| (m.line, m.tooltip.as_str())).collect();
        assert_eq!(
            tips,
            [
                (6, "Sent to 'shipping' by: flow .channel (FlowConfig.java:14)"),
                (
                    6,
                    "'invoices' is consumed by: <int:service-activator> (integration.xml:6), <int:outbound-channel-adapter> (integration.xml:7)"
                ),
                (16, "'orders' is consumed by: IntegrationFlow.from (FlowConfig.java:13)"),
                (18, "'orders' is consumed by: IntegrationFlow.from (FlowConfig.java:13)"),
            ]
        );
        assert_eq!(markers[1].target.as_ref().unwrap().path, XML_PATH);
    }

    #[test]
    fn warns_about_lost_messages_and_unknown_beans() {
        let _host = setup();
        let mut p = Integration::default();
        let codes = |problems: &[Diagnostic]| -> Vec<(u32, String)> {
            problems.iter().map(|d| (d.range.start.line, d.code.clone().unwrap())).collect()
        };
        p.line_markers(&doc(BILLING_PATH, ENDPOINTS_SRC));
        assert_eq!(codes(&p.live_diagnostics(&doc(BILLING_PATH, ENDPOINTS_SRC))), [(9, "unconsumed-channel".into())]);
        p.line_markers(&doc(XML_PATH, XML));
        let xml_problems = p.live_diagnostics(&doc(XML_PATH, XML));
        assert_eq!(codes(&xml_problems), [(5, "unconsumed-channel".into()), (6, "unknown-bean".into()), (7, "unconsumed-channel".into())]);
        assert!(xml_problems[1].message.contains("'mailer'"));
    }

    #[test]
    fn completes_channel_and_bean_names() {
        let _host = setup();
        let mut p = Integration::default();
        let text = XML.replace("output-channel=\"lost\"", "output-channel=\"\"");
        let caret = openidea_plugin::text::position(&text, text.find("output-channel=\"\"").unwrap() + 16);
        let items = p.completion(&doc(XML_PATH, &text), caret, None);
        let labels: Vec<_> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(labels.contains(&"shipping") && labels.contains(&"invoices") && labels.contains(&"late"));
        assert!(!labels.contains(&"billing") && !labels.iter().any(|l| l.contains('$')));

        let items = p.completion(&doc(XML_PATH, XML), at(XML, "\"billing\""), None);
        let labels: Vec<_> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(labels.contains(&"billing") && labels.contains(&"pricing") && labels.contains(&"flowConfig"));
        assert!(!labels.contains(&"ignored"));
    }

    #[test]
    fn integration_diagram_has_expected_nodes_and_edges() {
        let host = setup();
        let (nodes, edges) = diagram_of(Integration::default().indexes.get(&host.invocation().project().unwrap(), COMMAND_BUDGET));
        let ids: BTreeSet<&str> = nodes.iter().map(|n| n.id.as_str()).collect();
        assert_eq!(ids.len(), nodes.len(), "node ids are unique");
        assert!(edges.iter().all(|e| ids.contains(e.from.as_str()) && ids.contains(e.to.as_str())));
        let label_of = |id: &str| nodes.iter().find(|n| n.id == id).map(|n| n.label.as_str()).unwrap_or("?");
        let mut pairs: Vec<(&str, &str)> = edges.iter().map(|e| (label_of(&e.from), label_of(&e.to))).collect();
        let mut labels: Vec<&str> = nodes.iter().map(|n| n.label.as_str()).collect();
        labels.sort();
        pairs.sort();
        let mut expected = EXPECTED_PAIRS.to_vec();
        expected.sort();
        assert_eq!(labels, EXPECTED_LABELS);
        assert_eq!(pairs, expected);

        let mut p = Integration::default();
        let view = p.render(VIEW, None, &host.invocation()).unwrap();
        let Body::Html(page) = view.body else { panic!("html expected") };
        assert!(page.contains(&format!("data-id=\"e:6:38:{BILLING_PATH}\"")), "{page}");
        p.handle(VIEW, ViewEvent::Message(format!("e:6:38:{BILLING_PATH}")), &host.invocation()).unwrap();
        assert_eq!(host.opened_locations(), [(BILLING_PATH.to_string(), 6, 38)]);
    }

    #[test]
    fn integration_diagram_tells_endpoints_of_one_kind_apart() {
        let host = MockHost::install();
        let xml = "<beans xmlns=\"http://www.springframework.org/schema/beans\" xmlns:int=\"http://www.springframework.org/schema/integration\">\n<int:service-activator input-channel=\"a\" output-channel=\"b\"/>\n<int:service-activator input-channel=\"c\" output-channel=\"d\"/>\n</beans>";
        host.add_file("src/main/resources/two.xml", xml);
        let (nodes, _) = diagram_of(Integration::default().indexes.get(&host.invocation().project().unwrap(), COMMAND_BUDGET));
        let mut labels: Vec<&str> = nodes.iter().filter(|n| n.label.starts_with('<')).map(|n| n.label.as_str()).collect();
        labels.sort();
        assert_eq!(labels, ["<int:service-activator> (a -> b)", "<int:service-activator> (c -> d)"]);
    }

    #[test]
    fn refresh_during_scan_keeps_the_partial_index() {
        let host = setup();
        let mut p = Integration::default();
        let project = host.invocation().project().unwrap();
        p.indexes.get(&project, std::time::Duration::ZERO).update(CONFIG_PATH, CONFIG);
        p.render(VIEW, None, &host.invocation()).unwrap();
        host.refreshed_views();
        p.indexes.reset();
        p.indexes.get(&project, std::time::Duration::ZERO).update(CONFIG_PATH, CONFIG);
        p.handle(VIEW, ViewEvent::Button("refresh".into()), &host.invocation()).unwrap();
        assert_eq!(p.indexes.get(&project, std::time::Duration::ZERO).files().len(), 1, "not reset");

        p.indexes.get(&project, COMMAND_BUDGET);
        p.handle(VIEW, ViewEvent::Button("refresh".into()), &host.invocation()).unwrap();
        assert!(p.indexes.get(&project, std::time::Duration::ZERO).files().is_empty(), "a complete index starts over");
    }

    #[test]
    fn manifest_activates_on_the_structure_view() {
        assert!(include_str!("../plugin.toml").contains("\"on-view:structure\""));
    }

    #[test]
    fn integration_diagram_is_empty_without_channels() {
        let host = MockHost::install();
        let view = Integration::default().render(VIEW, None, &host.invocation()).unwrap();
        let Body::Html(page) = view.body else { panic!("html expected") };
        assert!(page.contains("No message channels found."));
    }

    #[test]
    fn ignores_files_without_integration() {
        let _host = setup();
        let mut p = Integration::default();
        let plain = "<beans xmlns=\"http://www.springframework.org/schema/beans\"><bean id=\"a\"/></beans>";
        assert!(p.line_markers(&doc("/project/src/main/resources/plain.xml", plain)).is_empty());
        assert!(p.completion(&doc("/project/src/main/resources/plain.xml", plain), Position::new(0, 70), None).is_empty());
    }

    #[test]
    fn applies_follows_the_root_build_file() {
        let host = MockHost::install();
        let project = &host::projects().unwrap()[0];
        assert!(!applies(project), "no build file");
        host.add_file("pom.xml", "<project><artifactId>spring-boot-starter-web</artifactId></project>");
        assert!(!applies(project));
        host.add_file("pom.xml", "<project><artifactId>spring-boot-starter-integration</artifactId></project>");
        assert!(applies(project));
        host.add_file("pom.xml", "<project><modules><module>app</module></modules></project>");
        assert!(applies(project), "multi-module is unsure");
        host.add_file("pom.xml", "<project><build><plugins><include>**/*Test.java</include></plugins></build></project>");
        assert!(!applies(project), "surefire include is not a module");
        host.add_file("pom.xml", "");
        host.add_file("build.gradle.kts", "includeBuild(\"tools\")\ndependencies { implementation(\"org.slf4j:slf4j-api\") }");
        assert!(!applies(project), "gradle without the dependency");
        host.add_file("build.gradle.kts", "dependencies { implementation(libs.spring.thing) }");
        assert!(applies(project), "version catalog is unsure");
        host.add_file("build.gradle.kts", "include(\"app\")");
        assert!(applies(project), "gradle multi-module is unsure");
    }

    #[test]
    fn refresh_goes_to_the_owner_window_when_embedded() {
        let host = MockHost::install();
        refresh("diagram");
        assert_eq!(host.refreshed_views(), ["structure"]);
    }
}
