//! Quoted argv, never shell evaluation. Options and trailing data remain separate.
use openidea_plugin::Error;

pub(crate) fn parse(raw: &str) -> Result<Vec<String>, Error> {
    if raw.len()>8192 || raw.chars().any(|c|c=='\0'||c.is_control()&&!c.is_whitespace()) {
        return Err(Error::Invalid("Additional arguments exceed text limits.".into()));
    }
    let mut words=Vec::new();let mut word=String::new();let mut quote=None;let mut active=false;
    let mut chars=raw.chars().peekable();
    while let Some(c)=chars.next() {
        match (quote,c) {
            (Some('\''),'\'') | (Some('"'),'"') => quote=None,
            (Some('\''),_) => word.push(c),
            (_, '\\') => {
                let Some(&next)=chars.peek() else{return Err(Error::Invalid("Finish the escaped argument.".into()));};
                if next.is_whitespace() || matches!(next,'\\'|'\''|'"') {word.push(chars.next().unwrap());} else {word.push(c);}
                active=true;
            }
            (None,'\''|'"')=>{quote=Some(c);active=true;}
            (None,_) if c.is_whitespace()=>{if active{words.push(std::mem::take(&mut word));active=false;}}
            _=>{word.push(c);active=true;}
        }
        if words.len()>128{return Err(Error::Invalid("Use at most 128 additional arguments.".into()));}
    }
    if quote.is_some(){return Err(Error::Invalid("Close the quoted argument.".into()));}
    if active{words.push(word);}
    if words.len()>128{return Err(Error::Invalid("Use at most 128 additional arguments.".into()));}
    Ok(words)
}

pub(crate) fn merge(mut base:Vec<String>, options:&str, trailing:&str, noninteractive:bool)->Result<Vec<String>,Error>{
    let extra=parse(options)?;let trailing=parse(trailing)?;
    if extra.first().is_some_and(|s|!s.starts_with('-')) || extra.iter().any(|s|s=="--") {
        return Err(Error::Invalid("Additional options must start with an option; put trailing values in Additional arguments.".into()));
    }
    if noninteractive && extra.iter().any(|s|matches!(s.split('=').next().unwrap_or(""),"--ansi"|"--no-ansi"|"--no-interaction"|"-n")) {
        return Err(Error::Invalid("Captured runs manage no-interaction and no-ansi automatically.".into()));
    }
    let boundary=base.iter().position(|s|s=="--");
    if let Some(at)=boundary {base.splice(at..at,extra);} else {base.extend(extra);if !trailing.is_empty(){base.push("--".into());}}
    base.extend(trailing);
    if base.len()>512||base.iter().map(String::len).sum::<usize>()>64*1024{return Err(Error::Invalid("Command arguments exceed limits.".into()));}
    Ok(base)
}


// Exact authored messages are localized only after validation reaches a UI caller.
pub(crate) fn ui_error(error:Error)->Error {
    fn message(message:String)->String{match message.as_str(){
        "Additional arguments exceed text limits." => crate::ui_text::t("native.dockerRoute.98f01d67be",&message),
        "Additional options must start with an option; put trailing values in Additional arguments." => crate::ui_text::t("native.dockerRoute.8188ea2775",&message),
        "Captured runs manage no-interaction and no-ansi automatically." => crate::ui_text::t("native.dockerRoute.7aa9d9330b",&message),
        "Close the quoted argument." => crate::ui_text::t("native.dockerRoute.1049b20ec6",&message),
        "Command arguments exceed limits." => crate::ui_text::t("native.artisan_workspace.eb45400b88",&message),
        "Finish the escaped argument." => crate::ui_text::t("native.dockerRoute.912affd806",&message),
        "Use at most 128 additional arguments." => crate::ui_text::t("native.dockerRoute.b82fcb4ae0",&message),
        _=>message,
    }}
    match error{Error::Invalid(value)=>Error::Invalid(message(value)),Error::Failed(value)=>Error::Failed(message(value)),_=>error}
}
#[cfg(test)]mod tests{
    use super::*;
    #[test]fn quotes_unicode_empty_and_literal_shell_characters_are_preserved(){
        assert_eq!(parse(r#"--label="a b" 'équipe' "" path\ with\ spaces '$HOME' `echo` ;"#).unwrap(),["--label=a b","équipe","","path with spaces","$HOME","`echo`",";"]);
        assert_eq!(parse(r#""C:\work\file.php""#).unwrap(),[r"C:\work\file.php"]);
        assert!(parse("'unfinished").is_err());assert!(parse("trailing\\").is_err());assert!(parse("\0").is_err());
    }
    #[test]fn option_values_precede_protected_structured_and_extra_positionals(){
        let base=vec!["artisan","app:check","--no-interaction","--no-ansi","--","-structured"].into_iter().map(str::to_owned).collect();
        assert_eq!(merge(base,r#"--label "a b""#,r#""-extra" "two words""#,true).unwrap(),["artisan","app:check","--no-interaction","--no-ansi","--label","a b","--","-structured","-extra","two words"]);
        assert!(merge(vec![],"--ansi", "",true).is_err());assert!(merge(vec![],"--", "",false).is_err());assert!(merge(vec![],"value", "",false).is_err());
    }
}
