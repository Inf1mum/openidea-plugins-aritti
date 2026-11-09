// Pure source-origin admission shared by merged Integration and embedded Batch.
use spring_text::{beans,java::Annotation};
use std::collections::BTreeSet;
pub(crate) fn source_type_owned(file: &spring_text::java::JavaFile, written: &str, qualified: &str) -> bool {
    if written==qualified {
        return !file.types.iter().any(|t|t.name=="org")
            && !file.imports.iter().any(|i|i.path.rsplit('.').next()==Some("org"));
    }
    let short=qualified.rsplit('.').next().unwrap_or("");
    written==short && !file.types.iter().any(|t|t.name==short)
        && !file.imports.iter().any(|i|i.path.ends_with(".*"))
        && file.imports.iter().filter(|i|!i.is_static && i.path.rsplit('.').next()==Some(short)).map(|i|i.path.as_str()).collect::<Vec<_>>()==[qualified]
}
fn bean_literal_names(annotation:&Annotation, default:&str, arrays:bool,kotlin:bool)->Option<Vec<String>> {
    let mut seen=BTreeSet::new();if annotation.args.iter().any(|a|!seen.insert(a.name.as_str())) {return None;}
    let selected=annotation.args.iter().filter(|a|matches!(a.name.as_str(),"name"|"value")).collect::<Vec<_>>();
    let [argument]=selected.as_slice() else {return selected.is_empty().then(||vec![default.into()]);};
    let tokens=spring_text::java::tokens(&argument.text);
    let strings=if tokens.len()==1 {tokens.as_slice()} else if arrays && tokens.len()>=3 && matches!((tokens.first()?.s,tokens.last()?.s),("{","}")|("[","]")) {&tokens[1..tokens.len()-1]} else{return None;};
    let mut names=Vec::new();let mut expect_string=true;
    for token in strings {
        if expect_string {
            let value=token.string()?;
            if token.s.starts_with("\"\"\"") || value.value.is_empty() || value.value.contains('\\') || kotlin && value.value.contains('$') || beans::is_expression(&value.value) || argument.text.get(value.at..value.end)!=Some(value.value.as_str()) {return None;}
            names.push(value.value);expect_string=false;
        } else if token.s=="," {expect_string=true;} else{return None;}
    }
    (!names.is_empty()).then_some(names)
}
pub(crate) fn source_bean_owned(file:&spring_text::java::JavaFile,text:&str,bean:&spring_text::java::BeanName,kotlin:bool)->bool {
    for owner in &file.types {
        let (annotations,qualified,default,arrays)=if owner.at==bean.at {
            let annotations=owner.annotations.iter().filter(|a|spring_text::java::STEREOTYPES.contains(&a.name.as_str())).collect::<Vec<_>>();
            let [annotation]=annotations.as_slice() else{return false;};
            let qualified=match annotation.name.as_str() {
                "Component"|"Service"|"Repository"|"Controller"=>format!("org.springframework.stereotype.{}",annotation.name),
                "Configuration"=>"org.springframework.context.annotation.Configuration".into(),
                "RestController"|"ControllerAdvice"|"RestControllerAdvice"=>format!("org.springframework.web.bind.annotation.{}",annotation.name),
                "SpringBootApplication"=>"org.springframework.boot.autoconfigure.SpringBootApplication".into(),
                "TestConfiguration"=>"org.springframework.boot.test.context.TestConfiguration".into(),
                _=>return false,
            };
            (annotations,qualified,spring_text::decapitalize(&owner.name),false)
        } else if let Some(method)=owner.methods().find(|m|m.at==bean.at) {
            (method.annotations.iter().filter(|a|a.name=="Bean").collect::<Vec<_>>(),"org.springframework.context.annotation.Bean".into(),method.name.clone(),true)
        } else {continue;};
        let [annotation]=annotations.as_slice() else{return false;};
        let Some(written)=text.get(annotation.at+1..annotation.name_at+annotation.name.len()) else{return false;};
        return source_type_owned(file,written,&qualified) && bean_literal_names(annotation,&default,arrays,kotlin).is_some_and(|names|names.contains(&bean.name));
    }
    false
}
