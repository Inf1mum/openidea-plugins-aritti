//! Declared source bean names only; no effective Spring container inference.
use super::*;
use std::collections::BTreeSet;

fn annotation_type(file:&java::JavaFile,text:&str,a:&java::Annotation)->Option<String> {
    let written=java::tokens(text.get(a.at+1..a.name_at+a.name.len())?).iter().map(|t|t.s).collect::<String>();
    if written.contains('.') {
        return (!file.types.iter().any(|t|t.name=="org")).then_some(written);
    }
    if written!=a.name || file.types.iter().any(|t|t.name==a.name) {return None;}
    let imports=file.imports.iter().filter(|i|!i.is_static && i.path.rsplit('.').next()==Some(a.name.as_str())).collect::<Vec<_>>();
    (imports.len()==1).then(||imports[0].path.clone())
}
fn stereotype(name:&str)->Option<(&'static str,&'static [&'static str])> {
    Some(match name {
        "Component"=>("org.springframework.stereotype.Component", &["value"]),
        "Service"=>("org.springframework.stereotype.Service", &["value"]),
        "Repository"=>("org.springframework.stereotype.Repository", &["value"]),
        "Controller"=>("org.springframework.stereotype.Controller", &["value"]),
        "RestController"=>("org.springframework.web.bind.annotation.RestController", &["value"]),
        "Configuration"=>("org.springframework.context.annotation.Configuration", &["value"]),
        "SpringBootApplication"=>("org.springframework.boot.autoconfigure.SpringBootApplication", &[]),
        "TestConfiguration"=>("org.springframework.boot.test.context.TestConfiguration", &["value"]),
        "ControllerAdvice"=>("org.springframework.web.bind.annotation.ControllerAdvice", &["name"]),
        "RestControllerAdvice"=>("org.springframework.web.bind.annotation.RestControllerAdvice", &["name"]),
        "ShellComponent"=>("org.springframework.shell.standard.ShellComponent", &["value"]),
        _=>return None,
    })
}
fn literal_names(a:&java::Annotation,keys:&[&str],default:&str,array:bool)->Option<Vec<String>> {
    let args=a.args.iter().filter(|arg|keys.contains(&arg.name.as_str())).collect::<Vec<_>>();
    if args.is_empty() {
        if keys.is_empty() && a.args.iter().any(|arg|matches!(arg.name.as_str(),"name"|"value")) {return None;}
        return Some(vec![default.to_owned()]);
    }
    let [arg]=args.as_slice() else {return None;};
    let tokens=java::tokens(&arg.text);
    if arg.strings.is_empty() || arg.strings.len()>32 || tokens.iter().any(|t|t.s.starts_with("\"\"\"")) {return None;}
    if tokens.len()!=1 || !tokens[0].s.starts_with('"') {
        if !array || tokens.first().is_none_or(|t|t.s!="{") || tokens.last().is_none_or(|t|t.s!="}") {return None;}
        let mut value=true;
        for token in &tokens[1..tokens.len()-1] {
            if value && token.s.starts_with('"') {value=false;}
            else if !value && token.s=="," {value=true;}
            else {return None;}
        }
    }
    if !array && arg.strings.len()!=1 {return None;}
    let mut names=BTreeSet::new();
    for value in &arg.strings {
        if value.value.len()!=value.end-value.at || value.value.len()>256 || value.value.chars().any(char::is_whitespace)
            || value.value.chars().any(char::is_control) || beans::is_expression(&value.value) {return None;}
        if value.value.is_empty() {
            if array {return None;} else {names.insert(default.to_owned());}
        } else {names.insert(value.value.clone());}
    }
    Some(names.into_iter().collect())
}
fn conditional(annotations:&[java::Annotation])->bool {annotations.iter().any(|a|a.name=="Profile" || a.name.starts_with("Conditional"))}
pub(super) fn java_names(file:&java::JavaFile,text:&str)->(Vec<BeanDecl>,bool) {
    let mut out=Vec::new(); let mut complete=true;
    for (i,ty) in file.types.iter().enumerate() {
        let markers=ty.annotations.iter().filter(|a|java::STEREOTYPES.contains(&a.name.as_str())).collect::<Vec<_>>();
        if !markers.is_empty() {
            let names=(|| {
                let [marker]=markers.as_slice() else {return None;};
                if ty.outer.is_some() {return None;}
                let (namespace,keys)=stereotype(&marker.name)?;
                if annotation_type(file,text,marker).as_deref()!=Some(namespace) {return None;}
                literal_names(marker,keys,&spring_text::decapitalize(&ty.name),false)
            })();
            if let Some(names)=names {for name in names {out.push(BeanDecl{name,class:(!conditional(&ty.annotations)).then(||file.qualified(i)),range:range(text,ty.at,ty.at+ty.name.len())});}}
            else {complete=false;}
        }
        for method in ty.methods() {
            let markers=method.annotations.iter().filter(|a|a.name=="Bean").collect::<Vec<_>>();
            if markers.is_empty() {continue;}
            let names=(|| {
                let [marker]=markers.as_slice() else {return None;};
                if ty.outer.is_some() || annotation_type(file,text,marker).as_deref()!=Some("org.springframework.context.annotation.Bean") {return None;}
                literal_names(marker,&["name","value"],&method.name,true)
            })();
            if let Some(names)=names {
                let class=(!conditional(&ty.annotations) && !conditional(&method.annotations)).then(||method.ty.as_ref().and_then(|value|hierarchy::qualified(file,i,value).ok())).flatten();
                for name in names {out.push(BeanDecl{name,class:class.clone(),range:range(text,method.at,method.at+method.name.len())});}
            } else {complete=false;}
        }
        if out.len()>4096 {return (Vec::new(),false);}
    }
    (out,complete)
}
