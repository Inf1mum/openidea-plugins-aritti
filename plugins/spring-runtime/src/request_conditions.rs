//! Examples come only from literal, structured Actuator mapping conditions.
use serde::{Deserialize,Serialize};
use serde_json::Value;
#[derive(Clone,Debug,Default,PartialEq,Serialize,Deserialize)]
pub struct RequestExamples {
    pub review:Vec<String>,
    pub headers:Vec<(String,String)>,
    pub parameters:Vec<(String,String)>,
}
fn safe(value:&str,limit:usize)->bool {value.len()<=limit&&!value.chars().any(char::is_control)}
fn header(name:&str)->bool {
    !name.is_empty()&&name.bytes().all(|b|b.is_ascii_alphanumeric()||b"!#$%&'*+-.^_`|~".contains(&b))
        &&!matches!(name.to_ascii_lowercase().as_str(),"host"|"content-length"|"transfer-encoding"|"connection"|"upgrade"|"proxy-authorization")
}
pub fn read(conditions:&Value)->RequestExamples {
    let mut out=RequestExamples::default();
    for (field,label,is_header) in [("headers","Headers",true),("params","Parameters",false)] {
        let Some(entries)=conditions.get(field).and_then(Value::as_array).filter(|entries|entries.len()<=32) else {out.review.push(format!("{label}: not reported or unsupported"));continue};
        if entries.is_empty(){out.review.push(format!("{label}: none reported"));}
        for entry in entries {
            let Some(name)=entry["name"].as_str().filter(|name|!name.is_empty()&&safe(name,256)) else {out.review.push(format!("{label}: unsupported condition"));continue};
            let Some(negated)=entry["negated"].as_bool() else {out.review.push(format!("{label}: {name} (negation not reported)"));continue};
            let value=match entry.get("value") {None|Some(Value::Null)=>None,Some(Value::String(value)) if safe(value,1024)=>Some(value.as_str()),_=>{out.review.push(format!("{label}: {name} (unsupported value)"));continue}};
            out.review.push(match (negated,value) {(false,Some(value))=>format!("{label}: {name} equals {value}"),(true,Some(value))=>format!("{label}: {name} must not equal {value}"),(false,None)=>format!("{label}: {name} must be present (value unspecified)"),(true,None)=>format!("{label}: {name} must be absent")});
            if !negated {
                if is_header {if header(name) {out.headers.push((name.into(),value.unwrap_or("<value>").into()));}}
                else {out.parameters.push((name.into(),value.unwrap_or("<value>").into()));}
            }
        }
    }
    for (field,label,request_header) in [("consumes","Consumes","Content-Type"),("produces","Produces","Accept")] {
        let Some(entries)=conditions.get(field).and_then(Value::as_array).filter(|entries|entries.len()<=32) else {out.review.push(format!("{label}: not reported or unsupported"));continue};
        if entries.is_empty(){out.review.push(format!("{label}: none reported"));}
        let mut positive=Vec::new();
        for entry in entries {
            let Some(media)=entry["mediaType"].as_str().filter(|value|safe(value,256)) else {out.review.push(format!("{label}: unsupported media condition"));continue};
            let Some(negated)=entry["negated"].as_bool() else {out.review.push(format!("{label}: {media} (negation not reported)"));continue};
            out.review.push(format!("{label}: {}{media}",if negated{"not "}else{""}));
            if !negated&&!media.contains('*') {positive.push(media);}
        }
        if positive.len()==1&&!out.headers.iter().any(|(name,_)|name.eq_ignore_ascii_case(request_header)) {out.headers.push((request_header.into(),positive[0].into()));}
    }
    out
}
fn encode(value:&str)->String {value.bytes().map(|b|if b.is_ascii_alphanumeric()||b"-._~".contains(&b){(b as char).to_string()}else{format!("%{b:02X}")}).collect()}
impl RequestExamples {
    pub fn query(&self)->String {
        let parameters:Vec<_>=self.parameters.iter().filter(|(name,value)|name.len()<=256&&value.len()<=1024&&!value.contains('<')).take(32).map(|(name,value)|format!("{}={}",encode(name),encode(value))).collect();
        if parameters.is_empty(){String::new()}else{format!("?{}",parameters.join("&"))}
    }
    pub fn header_text(&self)->String {self.headers.iter().take(32).filter(|(name,value)|header(name)&&safe(value,1024)).map(|(name,value)|format!("{name}: {value}")).collect::<Vec<_>>().join("\n")}
    pub fn review_text(&self)->String {if self.review.is_empty(){return "Conditions not reported in this snapshot".into()} self.review.iter().take(128).map(|line|line.chars().take(1400).map(|c|if c.is_control(){'\u{fffd}'}else{c}).collect::<String>()).collect::<Vec<_>>().join("\n")}
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn required_literal_constraints_supply_examples_and_negated_unknown_do_not() {
        let examples=read(&json!({"headers":[{"name":"X-Tenant","value":"demo","negated":false},{"name":"X-Debug","negated":true},{"name":"Host","value":"evil","negated":false}],"params":[{"name":"tag","value":"a&b","negated":false},{"name":"missing","negated":false},{"name":"disabled","value":"true","negated":true}],"consumes":[{"mediaType":"application/json","negated":false}],"produces":[{"mediaType":"text/plain","negated":false}]}));
        assert_eq!(examples.query(),"?tag=a%26b");assert!(examples.header_text().contains("X-Tenant: demo"));assert!(examples.header_text().contains("Content-Type: application/json"));assert!(!examples.header_text().contains("Host:"));assert!(!examples.header_text().contains("X-Debug:"));assert!(examples.review_text().contains("disabled must not equal true"));assert!(examples.review_text().contains("missing must be present"));
    }
    #[test]
    fn unsupported_or_missing_conditions_never_invent_headers_or_parameters() {
        let examples=read(&json!({"headers":[{"name":"Injected","value":"x\r\nBad: y","negated":false}],"params":false,"consumes":[{"mediaType":"application/json"}]}));assert!(examples.headers.is_empty());assert!(examples.parameters.is_empty());assert!(examples.review_text().contains("unsupported"));assert!(examples.review_text().contains("negation not reported"));
    }
}
