//! Explicit source-owned WebSecurity parameter debug flag, not runtime policy.
use super::*;
const WEB: &str = "org.springframework.security.config.annotation.web.builders.WebSecurity";
pub(crate) fn candidates(doc: &Document) -> Vec<(usize, usize)> {
    if doc.language != "java" || !doc.path.ends_with(".java") || doc.text.len() > 64 << 10
        || !host::setting_bool("springSecurity.checkDebug").unwrap_or(true)
        || !super::path_matcher::closed(&doc.text) { return Vec::new(); }
    let file=java::parse(&doc.text);
    if file.imports.iter().any(|i| i.path.ends_with(".*") || i.is_static && i.path.rsplit('.').next()==Some("WebSecurity"))
        || file.imports.iter().filter(|i| !i.is_static && i.path.rsplit('.').next()==Some("WebSecurity")).count()!=1
        || !file.imports.iter().any(|i| !i.is_static && i.path==WEB)
        || file.types.len()>64 || file.types.iter().any(|t| t.name=="WebSecurity" || t.supers().next().is_some())
    { return Vec::new(); }
    let tokens=java::tokens(&doc.text);
    // Generic type parameters may shadow the imported receiver type.
    if tokens.len()>8192 || tokens.iter().any(|t|t.s=="<") { return Vec::new(); }
    let calls=java::calls(&doc.text);
    if calls.len()>256 { return Vec::new(); }
    let mut out=Vec::new();
    for method in file.types.iter().filter(|t|t.outer.is_none()&&t.kind=="class").flat_map(|t|t.methods()) {
        let params:Vec<_>=method.params.iter().filter(|p|p.ty.text.trim()=="WebSecurity"&&p.ty.name=="WebSecurity"&&p.ty.args.is_empty()).collect();
        let [parameter]=params.as_slice() else {continue;};
        if parameter.name.is_empty() || parameter.name.len()>64 || !parameter.name.bytes().all(|b|b.is_ascii_alphanumeric()||b==b'_') {continue;}
        if tokens.iter().enumerate().filter(|(_,t)|method.at<t.at&&t.at<method.end&&t.s==parameter.name)
            .any(|(i,t)|t.at!=parameter.at&&tokens.get(i+1).is_none_or(|t|t.s!="."))
            || tokens.iter().filter(|t|method.at<t.at&&t.at<method.end).any(|t|matches!(t.s,"new"|"class"|"interface"|"enum"|"record"))
        {continue;}
        for call in calls.iter().filter(|c|c.name=="debug"&&c.receiver.as_deref()==Some(parameter.name.as_str())&&!c.chained&&!c.is_new&&method.at<c.at&&c.end<=method.end&&c.args.len()==1) {
            let Some(index)=tokens.iter().position(|t|t.at==call.at) else {continue;};
            if index<2 || tokens[index-2].s!=parameter.name || index>=3&&tokens[index-3].s=="." {continue;}
            let arg=&call.args[0];let value=java::tokens(&arg.text);
            let [literal]=value.as_slice() else {continue;};
            if literal.s!="true" {continue;}
            out.push((arg.at+literal.at,arg.at+literal.end()));
            if out.len()==4 {return out;}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::{file_actions::FileActions,mock::MockHost};
    const IMPORT:&str="import org.springframework.security.config.annotation.web.builders.WebSecurity;";
    fn doc(body:&str)->Document {Document{path:"/project/S.java".into(),language:"java".into(),version:9,text:format!("{IMPORT}\r\nclass S {{void configure(WebSecurity web) {{ /*😀*/ {body} }} }}")}}
    fn cursor(d:&Document)->Range {let at=openidea_plugin::text::position(&d.text,d.text.find("true").unwrap());Range::new(at,at)}
    #[test]
    fn direct_web_parameter_debug_literal_returns_one_exact_previewed_edit() {
        let h=MockHost::install();let mut tools=SecurityTools::default();let d=doc("web.debug(/*keep*/ true);");h.add_file("S.java",&d.text);
        let found=candidates(&d);assert_eq!(found.len(),1);assert_eq!(&d.text[found[0].0..found[0].1],"true");
        let a=tools.file_actions(&d,cursor(&d)).remove(0);assert!(a.edits.is_empty()&&a.operations.is_empty());
        h.answer_next_prompt(Some(&["yes"]));let fixed=tools.resolve_file_action(&d,a).unwrap();
        assert!(fixed.operations.is_empty());assert_eq!(fixed.edits.len(),1);assert_eq!(fixed.edits[0].1.len(),1);
        let e=&fixed.edits[0].1[0];let mut after=d.text.clone();after.replace_range(offset(&d.text,e.range.start).unwrap()..offset(&d.text,e.range.end).unwrap(),&e.text);
        assert_eq!(after,d.text.replace("/*keep*/ true","/*keep*/ false"));
        assert!(candidates(&Document{text:after,..d.clone()}).is_empty());
        assert_eq!(spring_text::index::project_of(&d.path).unwrap().read_file(&d.path).unwrap(),d.text);
        assert!(h.process_runs().is_empty());assert_eq!(h.fetches(),0);
    }
    #[test]
    fn aliases_field_chains_reassigned_shadowed_dynamic_and_wrong_types_stay_quiet() {
        let _h=MockHost::install();
        for body in ["other.debug(true);","holder.web.debug(true);","this.web.debug(true);","var alias=web; alias.debug(true);","web=other;web.debug(true);","consume(web);web.debug(true);","web.debug((true));","web.debug(true&&FLAG);","web.debug(FLAG);","web.debug(true,false);","web.debug(false);","web.foo().debug(true);","new Other();web.debug(true);","class Local {} web.debug(true);","consume((web)->web.debug(true));"] {assert!(candidates(&doc(body)).is_empty(),"{body}");}
        let d=doc("web.debug(true);");
        for text in [d.text.replace(IMPORT,"import other.WebSecurity;"),d.text.replace(IMPORT,""),format!("import other.*;\n{}",d.text),d.text.replace("class S","class WebSecurity {} class S"),d.text.replace("class S","class S extends Base"),d.text.replace("class S","class S<WebSecurity>"),d.text.replace("WebSecurity web","Other web"),d.text.replace("WebSecurity web","WebSecurity web,WebSecurity other"),d.text.replace("web.debug(true)","// web.debug(true)")] {assert!(candidates(&Document{text,..d.clone()}).is_empty());}
    }
    #[test]
    fn web_debug_cancel_stale_setting_and_unsaved_source_are_owned() {
        for mode in 0..5 {
            let h=MockHost::install();let mut tools=SecurityTools::default();let mut d=doc("web.debug(true);");h.add_file("S.java",&d.text);
            if mode==4 {h.add_file("S.java","different disk");h.open_editor(&d.path,"java",&d.text,&[]);}
            let a=tools.file_actions(&d,cursor(&d)).remove(0);
            match mode {0=>h.answer_next_prompt(None),1=>{d.version+=1;h.answer_next_prompt(Some(&["yes"]));},2=>{h.set_setting("springSecurity.checkDebug","false");h.answer_next_prompt(Some(&["yes"]));},3=>{h.add_file("S.java","changed");h.answer_next_prompt(Some(&["yes"]));},_=>h.answer_next_prompt(Some(&["yes"]))}
            let result=tools.resolve_file_action(&d,a);if mode==4 {assert!(result.is_ok());} else {assert!(result.is_err());}
        }
    }
}
