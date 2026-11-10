//! Bounded Monolog records. Stack paths are candidates, never navigation authority.
use serde_json::{Value, json};
const MAX_RECORDS: usize = 100;
const MAX_FRAMES: usize = 64;
const MAX_EXCEPTIONS: usize = 8;
const MAX_TEXT: usize = 65536;
const MAX_MESSAGE: usize = 4096;

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Frame { pub path: String, pub line: u32, pub call: String }
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Exception { pub class: String, pub message: String }
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Record {
    pub timestamp: String, pub channel: String, pub level: String,
    pub message: String, pub frames: Vec<Frame>, pub exceptions:Vec<Exception>, pub truncated: bool,
}
fn level(s: &str) -> bool { matches!(s, "DEBUG"|"INFO"|"NOTICE"|"WARNING"|"ERROR"|"CRITICAL"|"ALERT"|"EMERGENCY") }
/// Redact recognizable assignment-style credentials without retaining arbitrary JSON context.
/// This is deliberately not a claim to recognize every secret embedded in application messages.
fn display(value:&str,limit:usize)->(String,bool) {
    let mut end=value.len().min(limit);while !value.is_char_boundary(end){end-=1;}
    let input:String=value[..end].chars().map(|c|if c.is_control()&&!matches!(c,'\n'|'\t'){'�'}else{c}).collect();
    let lower=input.to_ascii_lowercase();let mut output=String::new();let mut cursor=0;
    while cursor<input.len() {
        let mut candidate=None;
        for key in ["password","passwd","authorization","api_key","api-key","access_token","refresh_token","secret","token"] {
            let mut search=cursor;
            while let Some(relative)=lower[search..].find(key) {
                let start=search+relative;search=start+key.len();let before=start.checked_sub(1).and_then(|at|input.as_bytes().get(at)).copied();
                if before.is_some_and(|b|b.is_ascii_alphanumeric()||b==b'_'){continue;}
                let mut at=start+key.len();
                if input.as_bytes().get(at)==Some(&b'"')||input.as_bytes().get(at)==Some(&b'\''){at+=1;}
                while input.as_bytes().get(at).is_some_and(|b|b.is_ascii_whitespace()&&*b!=b'\n'){at+=1;}
                if !input.as_bytes().get(at).is_some_and(|b|matches!(b,b':'|b'=')){continue;}at+=1;
                while input.as_bytes().get(at).is_some_and(|b|b.is_ascii_whitespace()&&*b!=b'\n'){at+=1;}
                if at==input.len(){continue;}
                if candidate.as_ref().is_none_or(|(old,_,_)|start<*old){candidate=Some((start,at,key=="authorization"));}
                break;
            }
        }
        let Some((_,start,authorization))=candidate else{output.push_str(&input[cursor..]);break;};
        output.push_str(&input[cursor..start]);let bytes=input.as_bytes();let mut finish=start;
        if matches!(bytes[start],b'"'|b'\'') {
            let quote=bytes[start];finish+=1;
            while finish<bytes.len(){if bytes[finish]==b'\\'{finish=(finish+2).min(bytes.len());continue;}if bytes[finish]==quote{finish+=1;break;}finish+=1;}
        } else {
            while finish<bytes.len()&&!matches!(bytes[finish],b'\n'|b'\r'|b','|b'}'|b']'|b';')&&(authorization||!bytes[finish].is_ascii_whitespace()){finish+=1;}
        }
        if finish==start {output.push_str(&input[start..start+1]);cursor=start+1;}else{output.push_str("[redacted]");cursor=finish;}
    }
    let truncated=end<value.len()||output.len()>limit;
    if output.len()>limit{let mut end=limit;while !output.is_char_boundary(end){end-=1;}output.truncate(end);}
    (output,truncated)
}
fn record(timestamp:&str,channel:&str,severity:&str,message:&str)->Record {
    let (message,truncated)=display(message,MAX_MESSAGE);
    Record{timestamp:display(timestamp,64).0,channel:display(channel,128).0,level:severity.into(),message,frames:Vec::new(),exceptions:Vec::new(),truncated}
}
fn valid_frame(path:&str,line:u32,call:&str)->Option<Frame> {
    if line==0||path.len()>4096||!path.starts_with('/')||path.chars().any(char::is_control){return None;}
    // PHP trace arguments can contain arbitrary credentials: retain the callable only.
    let call=call.split_once('(').map_or_else(||call.to_owned(),|(name,_)|format!("{name}(…)"));
    Some(Frame{path:path.into(),line,call:display(&call,512).0})
}
fn location(value:&str,call:&str)->Option<Frame> {
    let (path,line)=value.rsplit_once(':')?;
    valid_frame(path,line.parse().ok()?,call)
}
fn frame(line:&str)->Option<Frame> {
    if let Some(tail)=line.strip_prefix('#') {
        let (number,rest)=tail.split_once(' ')?;
        if number.is_empty()||!number.bytes().all(|b|b.is_ascii_digit()){return None;}
        let (source,call)=rest.split_once("): ")?;let (path,line)=source.rsplit_once('(')?;
        return valid_frame(path,line.parse().ok()?,call);
    }
    // Standard Monolog NormalizerFormatter trace and exception-origin strings.
    location(line,"")
}
fn structured_frame(value:&Value)->Option<Frame> {
    if let Some(text)=value.as_str(){return frame(text);}
    let file=value["file"].as_str()?;
    let call=match (value["class"].as_str(),value["function"].as_str()) {
        (Some(class),Some(function))=>format!("{class}{}{function}",if value["type"].as_str()==Some("->"){"->"}else{"::"}),
        (_,Some(function))=>function.into(),_=>String::new(),
    };
    if let Some(line)=value["line"].as_u64().and_then(|n|u32::try_from(n).ok()){valid_frame(file,line,&call)}else{location(file,&call)}
}
fn push_frame(record:&mut Record,frame:Frame) {
    if record.frames.iter().any(|old|old.path==frame.path&&old.line==frame.line){return;}
    if record.frames.len()<MAX_FRAMES{record.frames.push(frame);}else{record.truncated=true;}
}
fn trace(record:&mut Record,value:&Value) {
    if let Some(text)=value.as_str(){for line in text.lines().take(MAX_FRAMES+1){if let Some(frame)=frame(line){push_frame(record,frame);}}}
    if let Some(frames)=value.as_array(){if frames.len()>MAX_FRAMES{record.truncated=true;}for value in frames.iter().take(MAX_FRAMES){if let Some(frame)=structured_frame(value){push_frame(record,frame);}}}
}
fn exception(record:&mut Record,value:&Value,depth:usize) {
    if value.is_null(){return;}
    if depth>=MAX_EXCEPTIONS{record.truncated=true;return;}
    if let Some(text)=value.as_str(){
        trace(record,value);let summary=text.lines().next().unwrap_or("");
        if let Some((_,origin))=summary.rsplit_once(" at "){if let Some(frame)=location(origin.trim_end_matches(')'),""){push_frame(record,frame);}}
        if let Some(summary)=summary.strip_prefix("[object] ("){
            if let Some((class,rest))=summary.split_once("(code:"){
                if let Some((_,message))=rest.split_once("): "){
                    let message=message.rsplit_once(" at ").map_or(message,|(message,_)|message);
                    let(class,class_cut)=display(class,256);let(message,message_cut)=display(message,MAX_MESSAGE);record.truncated|=class_cut||message_cut;record.exceptions.push(Exception{class,message});
                }
            }
        }
        return;
    }
    let class=value["class"].as_str().unwrap_or("");let message=value["message"].as_str().unwrap_or("");
    if !class.is_empty()||!message.is_empty(){let (message,truncated)=display(message,MAX_MESSAGE);let(class,class_cut)=display(class,256);record.truncated|=truncated||class_cut;record.exceptions.push(Exception{class,message});}
    if let Some(frame)=structured_frame(value){push_frame(record,frame);}
    trace(record,&value["trace"]);
    exception(record,&value["previous"],depth+1);
}
fn header(line:&str)->Option<Record> {
    if line.starts_with('{') {
        let value=crate::strict_json::parse(line,MAX_TEXT).ok()?;
        let severity=value["level_name"].as_str().or_else(||value["level"].as_str())?.to_ascii_uppercase();if !level(&severity){return None;}
        let message=value["message"].as_str()?;
        let timestamp=value["datetime"].as_str().or_else(||value["datetime"]["date"].as_str()).unwrap_or("");
        let mut record=record(timestamp,value["channel"].as_str().unwrap_or(""),&severity,message);
        exception(&mut record,&value["context"]["exception"],0);
        if value["context"]["exception"].is_null(){exception(&mut record,&value["exception"],0);}
        trace(&mut record,&value["trace"]);trace(&mut record,&json!(message));
        return Some(record);
    }
    let tail=line.strip_prefix('[')?;let(timestamp,rest)=tail.split_once("] ")?;
    if timestamp.len()>64||!timestamp.as_bytes().first()?.is_ascii_digit(){return None;}
    let(label,message)=rest.split_once(": ")?;let(channel,severity)=label.rsplit_once('.')?;
    if channel.len()>128||!level(severity){return None;}
    Some(record(timestamp,channel,severity,message))
}
pub(super) fn parse(lines:&[String],tail_truncated:bool)->Vec<Record> {
    let mut records:Vec<Record>=Vec::new();let mut bytes=0;
    for line in lines.iter().take(200) {
        bytes+=line.len();if bytes>MAX_TEXT{if let Some(last)=records.last_mut(){last.truncated=true;}break;}
        if let Some(record)=header(line) {
            if records.len()==MAX_RECORDS{if let Some(last)=records.last_mut(){last.truncated=true;}break;}records.push(record);
        }else if let Some(record)=records.last_mut(){
            if let Some(frame)=frame(line){push_frame(record,frame);}else{
                let remaining=MAX_MESSAGE.saturating_sub(record.message.len());let (text,truncated)=display(line,remaining.saturating_sub(1));record.truncated|=truncated;
                if remaining>0{record.message.push('\n');record.message.push_str(&text);}
            }
        }
    }
    if tail_truncated||lines.len()>200{for record in &mut records{record.truncated=true;}}
    records
}
pub(super) fn rows(lines:&[String],tail_truncated:bool)->Vec<Value> {
    parse(lines,tail_truncated).into_iter().enumerate().map(|(index,record)|{
        let mut details=record.message.clone();
        for (index,error) in record.exceptions.iter().enumerate(){details.push_str(&format!("\n{}{}: {}",if index==0{""}else{"Caused by "},error.class,error.message));}
        json!({"key":format!("runtime:log-error:{index}"),"cells":["Errors",format!("{} · {}.{}",record.timestamp,record.channel,record.level),details],"activationEnabled":false,"error":{"timestamp":record.timestamp,"channel":record.channel,"level":record.level,"truncated":record.truncated,"exceptions":record.exceptions.into_iter().map(|e|json!({"class":e.class,"message":e.message})).collect::<Vec<_>>(),"frames":record.frames.into_iter().map(|f|json!({"path":f.path,"line":f.line,"call":f.call})).collect::<Vec<_>>()}})
    }).collect()
}

/// Resolve a captured frame against current owned files. No compiled line-number guessing.
pub(super) fn source(project:&openidea_plugin::Project, frame:&Frame) -> Option<(String,u32)> {
    let root=project.root().ok()?;
    let relative=frame.path.strip_prefix(&format!("{root}/"))?;
    if relative.split('/').any(|s|matches!(s,""|"."|"..")) || !crate::owns_current_path(project,&frame.path) { return None; }
    let text=project.read_file(&frame.path).ok()?;
    if text.len()>1024*1024 { return None; }
    if relative.starts_with("storage/framework/views/") {
        let (_,suffix)=text.rsplit_once("/**PATH ")?;
        let (original,rest)=suffix.split_once(" ENDPATH**/")?;
        if !rest.trim().is_empty() || !original.ends_with(".blade.php") {return None;}
        let rel=original.strip_prefix(&format!("{root}/"))?;
        if rel.split('/').any(|s|matches!(s,""|"."|"..")) || !crate::owns_current_path(project,original) || project.read_file(original).is_err() {return None;}
        return Some((original.into(),0));
    }
    if frame.line as usize>text.lines().count().max(1) {return None;}
    Some((frame.path.clone(),frame.line-1))
}
#[cfg(test)] mod tests {
    use super::*;
    #[test] fn multiline_and_frames_keep_timestamp_zone() {
        let lines=vec!["orphan tail".into(),"[2026-11-10T08:00:00+01:00] local.ERROR: Failed {\"id\":42}".into(),"[stacktrace]".into(),"#0 /app/app/Service.php(12): App\\Service->run()".into(),"[2026-11-10 08:01:00] local.INFO: Done".into()];
        let records=parse(&lines,false);assert_eq!(records.len(),2);assert!(records[0].timestamp.ends_with("+01:00"));assert_eq!(records[0].frames[0].line,12);assert!(records[0].message.contains("[stacktrace]"));
    }
    #[test] fn json_records_and_unknown_headers() {
        let records=parse(&[r#"{"datetime":"2026-11-10T00:00:00Z","channel":"worker","level_name":"error","message":"<script>x</script>"}"#.into(),"[bad] local.MADEUP: no".into()],false);
        assert_eq!(records.len(),1);assert!(records[0].message.starts_with("<script>"));
        let record=parse(&[json!({"datetime":"today","level_name":"ERROR","message":"x","context":{"exception":{"trace":["#0 /app/Test.php(3): run()"]}}}).to_string()],false);
        assert_eq!(record[0].frames[0].line,3);
        let rows=rows(&["[2026-11-10] local.ERROR: test".into()],true);assert_eq!(rows[0]["activationEnabled"],false);assert_eq!(rows[0]["error"]["truncated"],true);
    }
    #[test] fn limits_and_bad_frames() {
        assert!(frame("#1 /app/a.php(0): run()").is_none());assert!(frame("#x /app/a.php(1): run()").is_none());
        let mut lines=vec!["[2026-11-10] local.ERROR: x".into()];lines.extend((0..100).map(|i|format!("#{i} /app/a.php({}): run()",i+1)));let r=parse(&lines,false);assert_eq!(r[0].frames.len(),64);assert!(r[0].truncated);
    }
    #[test] fn real_monolog_json_fixture_has_origin_trace_previous_and_redaction() {
        let raw=include_str!("../tests/fixtures/monolog-json-errors.jsonl");
        let parsed=parse(&[raw.trim_end().into()],false);assert_eq!(parsed.len(),1);let record=&parsed[0];
        assert_eq!(record.timestamp,"2026-11-10T08:00:00+01:00");assert_eq!(record.exceptions.len(),2);
        assert_eq!(record.exceptions[0].class,"RuntimeException");assert_eq!(record.exceptions[1].class,"InvalidArgumentException");
        assert_eq!(record.frames.iter().map(|f|f.line).collect::<Vec<_>>(),vec![18,20,15,17]);
        assert!(record.frames.iter().all(|f|f.path=="/project/app/MonologFixture.php"));
        let host=openidea_plugin::mock::MockHost::install();host.add_file("app/MonologFixture.php",&"<?php\n".repeat(30));let project=host.invocation().project().unwrap();
        assert_eq!(source(&project,&record.frames[0]),Some(("/project/app/MonologFixture.php".into(),17)));
        host.add_file("app/MonologFixture.php","<?php");assert!(source(&project,&record.frames[0]).is_none());
        let rendered=json!(rows(&[raw.trim_end().into()],false)).to_string();
        for private in ["PRIVATE_TOKEN_SENTINEL","PRIVATE_PASSWORD_SENTINEL","PRIVATE_CONTEXT_SENTINEL","PRIVATE_REQUEST_SENTINEL","PRIVATE_PROCESSOR_SENTINEL"] {assert!(!rendered.contains(private),"leaked {private}");}
        assert!(rendered.contains("outer operation failed"));assert!(rendered.contains("Caused by InvalidArgumentException"));assert!(rendered.contains("[redacted]"));
    }
    #[test] fn structured_and_php_frames_exclude_args_and_bound_previous_chain() {
        let mut previous=Value::Null;
        for depth in 0..12 {previous=json!({"class":"RuntimeException","message":format!("depth {depth}"),"file":"/project/a.php","line":depth+1,"previous":previous});}
        let raw=json!({"datetime":"today","channel":"test","level_name":"ERROR","message":"broken","context":{"exception":{"class":"Exception","message":"outer","file":"/project/main.php:8","trace":[{"file":"/project/a.php","line":12,"class":"Service","type":"->","function":"run","args":["PRIVATE_ARGUMENT"]},"#2 /project/b.php(4): run('PRIVATE_CALL_ARGUMENT')","/project/c.php:5",{"file":"relative.php","line":4}],"previous":previous}}});
        let parsed=parse(&[raw.to_string()],false);let record=&parsed[0];assert_eq!(record.exceptions.len(),8);assert!(record.truncated);assert_eq!(record.frames[0].line,8);
        assert!(record.frames.iter().any(|f|f.path=="/project/c.php"&&f.line==5));
        let rendered=json!(rows(&[raw.to_string()],false)).to_string();assert!(!rendered.contains("PRIVATE_ARGUMENT"));assert!(!rendered.contains("PRIVATE_CALL_ARGUMENT"));
        assert!(frame("/project/a.php:0").is_none());assert!(frame("relative.php:3").is_none());assert!(frame("/project/a.php:4294967296").is_none());
    }
    #[test] fn display_redacts_credentials_and_preserves_unicode() {
        let text=r#"é password='private value' token=abc api_key="xyz" Authorization: Bearer private-auth
safe unicode 日本語"#;
        let (shown,_)=display(text,4096);assert!(shown.contains("日本語"));assert!(shown.starts_with("é password=[redacted]"));
        for secret in ["private value","abc","xyz","private-auth"] {assert!(!shown.contains(secret));}
        assert_eq!(display("tokenizer and token validation: token=private",4096).0,"tokenizer and token validation: token=[redacted]");
    }
    #[test]
    #[ignore="requires LARAVEL_MONOLOG_AUTOLOAD pointing to an installed Monolog autoloader"]
    fn installed_monolog_formatter_emits_checked_in_fixture() {
        let autoload=std::env::var("LARAVEL_MONOLOG_AUTOLOAD").expect("Set LARAVEL_MONOLOG_AUTOLOAD");
        let generator=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/emit-monolog-json.php");
        let output=std::process::Command::new("php").arg(generator).arg(autoload).output().unwrap();assert!(output.status.success(),"Monolog fixture generator failed");
        let emitted:Value=serde_json::from_slice(&output.stdout).unwrap();let checked:Value=serde_json::from_str(include_str!("../tests/fixtures/monolog-json-errors.jsonl")).unwrap();assert_eq!(emitted,checked);
        let parsed=parse(&[String::from_utf8(output.stdout).unwrap().trim_end().into()],false);assert_eq!(parsed[0].exceptions.len(),2);assert_eq!(parsed[0].frames.len(),4);
    }
    #[test] fn owned_sources_and_blade_mapping_do_not_guess_lines() {
        let host=openidea_plugin::mock::MockHost::install();host.add_file("app/Service.php","<?php\nthrow new Exception();");host.add_file("resources/views/home.blade.php","Hello");
        let project=host.invocation().project().unwrap();let root=project.root().unwrap();
        let f=Frame{path:format!("{root}/app/Service.php"),line:2,call:String::new()};assert_eq!(source(&project,&f),Some((f.path.clone(),1)));
        host.add_file("storage/framework/views/abc.php",&format!("<?php echo 1; ?>\n/**PATH {root}/resources/views/home.blade.php ENDPATH**/"));
        let f=Frame{path:format!("{root}/storage/framework/views/abc.php"),line:80,call:String::new()};assert_eq!(source(&project,&f),Some((format!("{root}/resources/views/home.blade.php"),0)));
        assert!(source(&project,&Frame{path:"/foreign/a.php".into(),line:1,call:String::new()}).is_none());
    }
}
