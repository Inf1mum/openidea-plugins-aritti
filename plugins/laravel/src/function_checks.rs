//! Function-call checks of one PHP file.

use openidea_plugin::Severity;

use crate::eloquent_checks::Finding;
use crate::php;

const QUIET_DIRECTORIES: &[&str] = &["/config/", "/bootstrap/", "/vendor/", "/tests/"];

/// `env()` called outside the configuration files returns null once the configuration is cached.
/// `relative_path` is relative to the project root.
pub fn env_outside_config(relative_path: &str, language: &str, text: &str) -> Vec<Finding> {
    let rooted = format!("/{relative_path}");
    if text.len()>128*1024||language != "php" || QUIET_DIRECTORIES.iter().any(|directory| rooted.contains(directory)) {
        return Vec::new();
    }

    let masked = php::mask_comments(text);
    let bytes = masked.as_bytes();
    // The binding parser need not interpret opaque heredoc values. Mask them
    // in place, preserving every offset and all declarations/imports outside.
    let mut proof=masked.as_bytes().to_vec();let mut cursor=0;
    while cursor<bytes.len(){
        if matches!(bytes[cursor],b'\''|b'"'){cursor=php::string_end(bytes,cursor);continue;}
        if bytes[cursor..].starts_with(b"<<<"){
            let end=heredoc_end(&masked,cursor);if end<=cursor+3{return vec![];}
            for byte in &mut proof[cursor..end]{if !matches!(*byte,b'\r'|b'\n'){*byte=b' ';}}
            proof[cursor]=b'\'';proof[cursor+1]=b'\'';cursor=end;
        }else{cursor+=1;}
    }
    let Ok(proof)=String::from_utf8(proof)else{return vec![];};
    let mut found = Vec::new();
    let mut at = 0;
    let mut calls=0usize;

    while at < bytes.len() {
        if bytes[at] == b'\'' || bytes[at] == b'"' {
            at = php::string_end(bytes, at);
            continue;
        }

        if bytes[at..].starts_with(b"<<<") {
            at = heredoc_end(&masked, at);
            continue;
        }

        if !bytes[at..].starts_with(b"env") {
            at += 1;
            continue;
        }

        let before = &masked[..at];
        let is_identifier = |c: char| c.is_alphanumeric() || c == '_';
        let call_before = match before.strip_suffix('\\') {
            Some(namespace) => namespace.chars().next_back().is_some_and(is_identifier),
            None => before.chars().next_back().is_some_and(|c| is_identifier(c) || matches!(c, '$' | '>' | ':')),
        };
        let is_definition = before.trim_end().ends_with("function") && before.trim_end().len() < before.len();
        let rest = &masked[at + 3..];
        let open = at + 3 + (rest.len() - rest.trim_start().len());
        if call_before || is_definition || bytes.get(open) != Some(&b'(') {
            at += 3;
            continue;
        }

        let Some(close) = php::closing(&masked, open) else { break };
        calls+=1;
        if !crate::view_rename::binding_budget(text.len(),calls){return vec![];}
        if !crate::eloquent_members::metadata_inference::global_helper(&proof,open,"env"){at=close+1;continue;}
        found.push(Finding {
            start: at,
            end: close + 1,
            code: "env-outside-config",
            message: "env() outside config files returns null once the configuration is cached: read a config() value instead.".into(),
            severity: Severity::Warning,
            replacement: None,
        });
        at = close + 1;
    }

    found
}

/// The index after the heredoc or nowdoc that starts at `start` (the end of the text when it isn't closed).
fn heredoc_end(masked: &str, start: usize) -> usize {
    let after = &masked[start + 3..];
    let header_end = after.find('\n').unwrap_or(after.len());
    let label = after[..header_end].trim().trim_matches(|c| c == '\'' || c == '"');
    if label.is_empty() {
        return start + 3;
    }

    let mut line_start = start + 3 + header_end;
    for line in masked[line_start..].split_inclusive('\n').skip(1) {
        line_start += masked[line_start..].find('\n').map_or(0, |i| i + 1);
        let indent = line.len() - line.trim_start().len();
        let tail = &line[indent..];
        let closes = tail.strip_prefix(label).is_some_and(|rest| !rest.starts_with(|c: char| c.is_alphanumeric() || c == '_'));
        if closes {
            return line_start + indent + label.len();
        }
    }

    masked.len()
}

#[cfg(test)]
mod delivery_regressions {
    #[test]
    fn localized_env_inspection_does_not_replace_unrelated_eloquent_messages(){
        let host=openidea_plugin::mock::MockHost::install();host.set_setting("uiLanguage",r#""ja""#);
        let source="<?php $query->orderBy('created_at','desc'); env('APP_KEY');";
        host.add_file("app/Example.php",source);let project=host.invocation().project().unwrap();
        let expected=crate::eloquent_checks::source_findings(source,&|_|None).remove(0).message;
        let findings=crate::eloquent_problems(&project,&crate::Index::default(),"/project/app/Example.php","php",source);
        assert_eq!(findings.len(),1);assert_eq!(findings[0].code.as_deref(),Some("simplify-order-by"));assert_ne!(findings[0].message,expected);assert!(findings[0].message.contains("latest()")&&findings[0].message.contains("created_at")&&findings[0].message.contains("orderBy()"));
        let environment=crate::function_problems(&project,"/project/app/Example.php","php",source);
        assert_eq!(environment.len(),1);assert_eq!(environment[0].code.as_deref(),Some("env-outside-config"));
        assert!(environment[0].message.contains("env()"));assert!(!environment[0].message.starts_with("env() outside config"));
    }
}

#[cfg(test)]
mod binding_regressions{
    #[test]
    fn framework_env_calls_require_proven_function_binding_and_respect_source_bounds(){
        for source in ["<?php use function Vendor\\env; env('APP_KEY');","<?php namespace App; function env($key){} env('APP_KEY');","<?php $client->env('APP_KEY'); Vendor\\env('APP_KEY');"]{
            assert!(super::env_outside_config("app/Test.php","php",source).is_empty(),"{source}");
        }
        assert_eq!(super::env_outside_config("app/Test.php","php","<?php namespace App; \\env('APP_KEY');").len(),1);
        assert_eq!(super::env_outside_config("app/Test.php","php","<?php env('APP_KEY');").len(),1);
        assert!(super::env_outside_config("config/app.php","php","<?php env('APP_KEY');").is_empty());
        assert!(super::env_outside_config("app/Test.php","php",&format!("<?php {} env('APP_KEY');"," ".repeat(128*1024))).is_empty());
    }
}
