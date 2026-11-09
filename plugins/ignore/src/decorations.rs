//! Source syntax markers plus bounded event-admitted historical literal-directory facts.
use crate::{
    IgnorePlugin,
    check::{self, Dialect, Syntax},
    highlight,
    matcher::Rule,
};
use openidea_plugin::{
    providers::{Document, LineMarker},
    semantic_tokens::{SemanticToken, SemanticTokens, TokenKind, TokenModifier},
    syntax::Highlight,
};
const MAX_TEXT: usize = 128 << 10;
const MAX_LINES: usize = 4096;
const MAX_MARKERS: usize = 256;
const MAX_TOKENS: usize = 4096;
const BUILTIN: &[&str] = &[
    ".gitignore",
    ".gitignore_global",
    ".dockerignore",
    ".npmignore",
    ".prettierignore",
    ".eslintignore",
    ".stylelintignore",
    ".hgignore",
    ".ignore",
    ".rgignore",
    ".fdignore",
    ".helmignore",
    ".vscodeignore",
    ".gcloudignore",
    ".slugignore",
    ".cfignore",
    ".bzrignore",
    ".containerignore",
];
fn dialect(doc: &Document) -> Option<Dialect> {
    if doc.text.len() > MAX_TEXT {
        return None;
    }
    let name = doc.path.rsplit(['/', '\\']).next()?;
    match doc.language.as_str() {
        "ignore" if BUILTIN.contains(&name) || name.ends_with(".gitignore") || name.ends_with(".dockerignore") => {
            Some(check::dialect(&doc.path).unwrap_or(Dialect { syntax: Syntax::Glob, switchable: false }))
        }
        "dotignore" => check::dialect(&doc.path),
        _ => None,
    }
}
fn switch(line: &str) -> Option<Option<Syntax>> {
    line.strip_prefix("syntax:").map(|value| match value.trim() {
        "glob" | "rootglob" => Some(Syntax::Glob),
        "re" | "regexp" => Some(Syntax::Regexp),
        _ => None,
    })
}
pub(crate) fn markers(doc: &Document) -> Vec<LineMarker> {
    let Some(dialect) = dialect(doc) else {
        return vec![];
    };
    let mut syntax = Some(dialect.syntax);
    let mut markers = Vec::new();
    for (line, raw) in doc.text.lines().take(MAX_LINES).enumerate() {
        let entry = raw.trim();
        if dialect.switchable {
            if let Some(next) = switch(entry) {
                syntax = next;
                continue;
            }
            if entry.starts_with("include:") || entry.starts_with("subinclude:") {
                continue;
            }
        }
        if syntax != Some(Syntax::Glob) || entry.len() > 1024 || entry.contains(['[', ']', '\\']) || entry.chars().any(char::is_control) {
            continue;
        }
        let Some(rule) = Rule::parse(entry) else {
            continue;
        };
        if !rule.dir_only || rule.pattern.is_empty() {
            continue;
        }
        markers.push(LineMarker {
            line: line as u32,
            icon: Some("icons/directory-pattern.svg".into()),
            tooltip: if rule.negated {
                "Directory re-inclusion pattern; source syntax only, existence not checked"
            } else {
                "Directory ignore pattern; source syntax only, existence not checked"
            }
            .into(),
            target: None,
        });
        if markers.len() >= MAX_MARKERS {
            break;
        }
    }
    markers
}
/// One exact document, admitted outside marker rendering. No negative conclusions.
#[derive(Default)]
pub(crate) struct Directories { watching: bool, sample: Option<DirectorySample> }
struct DirectorySample { root: String, path: String, text: String, at: std::time::Instant, lines: Vec<u32>, targets: Vec<String> }
impl Directories {
    pub(crate) fn configure(&mut self, watching: bool) { self.watching=watching; self.clear(); }
    pub(crate) fn clear(&mut self) { self.sample=None; }
    /// Retain an exact historical observation across unrelated UI/status events
    /// and duplicate save/watcher delivery. Providers remain filesystem-free.
    pub(crate) fn events(&mut self, events: &[openidea_plugin::Event]) {
        use openidea_plugin::{Event,FileKind};
        if events.is_empty() {return;}
        let Some(sample)=self.sample.take() else {return;};
        let started=std::time::Instant::now();
        if !self.watching || events.len()>256 || sample.at.elapsed()>=std::time::Duration::from_secs(30) || !crate::preferences::enabled(&sample.path) {return;}
        let Ok(projects)=openidea_plugin::host::projects() else {return;};
        if projects.len()>64 {return;}
        let mut roots=Vec::new();
        for project in &projects {
            let Ok(root)=project.root() else {return;};let root=root.trim_end_matches('/').to_owned();
            if root.is_empty() || root.len()>4096 || root.contains('\\') || root.chars().any(char::is_control) {return;}
            roots.push(root);
        }
        let owner=|path:&str| roots.iter().filter(|root|path==root.as_str() || path.strip_prefix(root.as_str()).is_some_and(|rest|rest.starts_with('/'))).max_by_key(|root|root.len());
        if std::iter::once(&sample.path).chain(&sample.targets).any(|path|owner(path).map(String::as_str)!=Some(sample.root.as_str())) {return;}
        let Some(index)=roots.iter().position(|root|root==&sample.root) else {return;};let project=&projects[index];
        let related=|changed:&str,target:&str| changed==target || target.strip_prefix(changed).is_some_and(|rest|rest.starts_with('/'));
        let mut recheck=false;let mut paths=0usize;
        for event in events {
            match event {
                Event::FileOpened(path)|Event::FileSaved(path) if path==&sample.path => return, // freshly admitted below
                Event::FilesChanged(batch) => {
                    for path in batch.created.iter().chain(&batch.modified).chain(&batch.removed) {
                        paths+=1;
                        if paths>256 || path.is_empty() || path.len()>4096 || path.chars().any(char::is_control) {return;}
                        recheck |= std::iter::once(&sample.path).chain(&sample.targets).any(|target|related(path.trim_end_matches('/'),target));
                    }
                }
                // SDK1.3 has no typed SettingsChanged; unknown future events
                // invalidate conservatively rather than guessing their schema.
                Event::Other{..} => return,
                _ => {}
            }
        }
        if recheck {
            if !project.stat(&sample.path).is_ok_and(|s|s.kind==FileKind::File && s.size<=MAX_TEXT as u64) || project.read_file(&sample.path).ok().as_deref()!=Some(sample.text.as_str()) {return;}
            for target in &sample.targets {
                if started.elapsed()>std::time::Duration::from_millis(100) || !project.stat(target).is_ok_and(|s|s.kind==FileKind::Directory) {return;}
            }
        }
        if started.elapsed()>std::time::Duration::from_millis(100) {return;}
        // Preserve the original timestamp: watcher echoes never extend TTL.
        self.sample=Some(sample);
    }
    pub(crate) fn admit(&mut self, path: &str, text: &str, ctx: &openidea_plugin::Invocation) {
        self.clear();
        if !self.watching || text.len()>MAX_TEXT || !crate::preferences::enabled(path) {return;}
        let doc=Document {path:path.into(),language:if BUILTIN.contains(&path.rsplit('/').next().unwrap_or("")) {"ignore"} else {"dotignore"}.into(),text:text.into(),version:0};
        let Some(dialect)=dialect(&doc) else {return;};
        let Some((project,root,relative))=crate::project_of(path) else {return;};
        if relative.split('/').any(|p| p.is_empty() || matches!(p,"."|"..")) || relative.contains('\\') || ctx.project().and_then(|p|p.root().ok()).as_deref()!=Some(root.as_str()) {return;}
        let Some(parent)=path.rsplit_once('/').map(|p|p.0) else {return;};
        // Host stat resolves in-root links: directory compatibility, not lstat identity.
        let safe_directory=|target:&str| -> bool {
            let Some((owner,owner_root,rel))=crate::project_of(target) else {return false;};
            if owner_root!=root || rel.split('/').count()>32 {return false;}
            let mut current=root.clone();
            for part in rel.split('/') {current.push('/');current.push_str(part);if owner.stat(&current).ok().map(|s|s.kind)!=Some(openidea_plugin::FileKind::Directory) {return false;}}
            true
        };
        if project.stat(path).ok().map(|s|s.kind)!=Some(openidea_plugin::FileKind::File) || (parent!=root && !safe_directory(parent)) {return;}
        let started=std::time::Instant::now();let mut syntax=Some(dialect.syntax);let mut lines=Vec::new();let mut targets=Vec::new();let mut attempts=0;
        for (line,raw) in text.lines().take(MAX_LINES).enumerate() {
            let entry=raw.trim();
            if dialect.switchable {if let Some(next)=switch(entry) {syntax=next;continue;} if entry.starts_with("include:") || entry.starts_with("subinclude:") {continue;}}
            if syntax!=Some(Syntax::Glob) || entry.len()>1024 || entry.contains(['\\','[',']']) || entry.chars().any(char::is_control) {continue;}
            let Some(rule)=Rule::parse(entry).filter(|r|!r.dir_only && r.is_literal()) else {continue;};
            if rule.pattern.is_empty() || rule.pattern.split('/').any(|p|p.is_empty() || matches!(p,"."|"..")) {continue;}
            attempts+=1;if attempts>64 || started.elapsed()>std::time::Duration::from_millis(100) {break;}
            let target=format!("{parent}/{}",rule.pattern);
            if safe_directory(&target) {lines.push(line as u32);targets.push(target);}
        }
        if project.read_file(path).ok().as_deref()!=Some(text) || ctx.project().and_then(|p|p.root().ok()).as_deref()!=Some(root.as_str()) {return;}
        self.sample=Some(DirectorySample {root,path:path.into(),text:text.into(),at:std::time::Instant::now(),lines,targets});
    }
    pub(crate) fn markers(&self, doc:&Document)->Vec<LineMarker> {
        if dialect(doc).is_none() {return Vec::new();}
        let Some(s)=self.sample.as_ref().filter(|s|s.path==doc.path && s.text==doc.text && s.at.elapsed()<std::time::Duration::from_secs(30)) else {return Vec::new();};
        s.lines.iter().map(|line|LineMarker {line:*line,icon:Some("icons/directory-pattern.svg".into()),tooltip:"Literal pattern matched a project directory when this file opened or saved; cached observation".into(),target:None}).collect()
    }
}
impl SemanticTokens for IgnorePlugin {
    fn semantic_tokens(&mut self, doc: &Document) -> Vec<SemanticToken> {
        // dotignore already has the package lexical highlighter. Do not overlay it.
        if doc.language != "ignore" || !crate::preferences::enabled(&doc.path) {
            return vec![];
        }
        let Some(dialect) = dialect(doc) else {
            return vec![];
        };
        let mut syntax = Some(dialect.syntax);
        let mut tokens = Vec::new();
        for (line_number, raw) in doc.text.split_inclusive('\n').take(MAX_LINES).enumerate() {
            let line = raw.trim_end_matches(['\r', '\n']);
            let content = line.trim();
            let indent = line.len() - line.trim_start().len();
            let mut cursor = 0usize;
            let mut column = line[..indent].encode_utf16().count() as u32;
            let mut push = |from: usize, to: usize, kind, documentation: bool| {
                if from >= to || tokens.len() >= MAX_TOKENS {
                    return;
                }
                // Token spans arrive sorted. Walk each source character once,
                // rather than rescanning the whole document for every range.
                if from < cursor || !content.is_char_boundary(from) || !content.is_char_boundary(to) {
                    return;
                }
                column += content[cursor..from].encode_utf16().count() as u32;
                let start = openidea_plugin::Position::new(line_number as u32, column);
                column += content[from..to].encode_utf16().count() as u32;
                cursor = to;
                let end = openidea_plugin::Position::new(line_number as u32, column);
                let token = SemanticToken::new(openidea_plugin::Range::new(start, end), kind);
                tokens.push(if documentation { token.modifier(TokenModifier::Documentation) } else { token });
            };
            if content.is_empty() {
                continue;
            }
            if content.starts_with('#') {
                push(0, content.len(), TokenKind::Comment, content.starts_with("##"));
                continue;
            }
            if dialect.switchable {
                if let Some(next) = switch(content) {
                    push(0, 6, TokenKind::Keyword, false);
                    let value = content[7..].trim();
                    let from = content.len() - content.trim_start().len() + 7 + content[7..].len() - content[7..].trim_start().len();
                    if next.is_some() && !value.is_empty() {
                        push(from, from + value.len(), TokenKind::Keyword, false);
                    }
                    syntax = next;
                    continue;
                }
                if let Some((keyword, value)) =
                    ["include:", "subinclude:"].iter().find_map(|prefix| content.strip_prefix(prefix).map(|value| (*prefix, value)))
                {
                    push(0, keyword.len() - 1, TokenKind::Keyword, false);
                    let from = keyword.len() + value.len() - value.trim_start().len();
                    let value = value.trim();
                    if !value.is_empty() {
                        push(from, from + value.len(), TokenKind::String, false);
                    }
                    continue;
                }
            }
            match syntax {
                Some(Syntax::Regexp) => push(0, content.len(), TokenKind::Regexp, false),
                Some(Syntax::Glob) => {
                    for token in highlight::highlights(content) {
                        let kind = match token.kind {
                            Highlight::Operator => TokenKind::Operator,
                            Highlight::Regexp => TokenKind::Regexp,
                            Highlight::Escape => TokenKind::String,
                            _ => continue,
                        };
                        push(token.range.start, token.range.end, kind, false);
                    }
                }
                None => {}
            }
            if tokens.len() >= MAX_TOKENS {
                break;
            }
        }
        tokens
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::{Position, mock::MockHost, providers::Providers};
    fn doc(path: &str, language: &str, text: &str) -> Document {
        Document { path: path.into(), language: language.into(), version: 1, text: text.into() }
    }
    fn built(text: &str) -> Document {
        doc("/project/.gitignore", "ignore", text)
    }
    fn excerpts(d: &Document, tokens: &[SemanticToken]) -> Vec<String> {
        tokens
            .iter()
            .map(|t| {
                let start = openidea_plugin::text::offset(&d.text, t.range.start).unwrap();
                let end = openidea_plugin::text::offset(&d.text, t.range.end).unwrap();
                d.text[start..end].into()
            })
            .collect()
    }
    #[test]
    fn directory_markers_describe_source_syntax_without_asserting_disk_existence() {
        let _h = MockHost::install().with_capabilities(&[]);
        let mut p = IgnorePlugin::default();
        let d = built("# out/\n/out/\n!keep/\n**/cache/\nplain\n/\n[bad/\n\\#literal/\n");
        let m = p.line_markers(&d);
        assert_eq!(m.iter().map(|m| m.line).collect::<Vec<_>>(), [1, 2, 3]);
        assert!(m.iter().all(|m| m.icon.as_deref() == Some("icons/directory-pattern.svg")
            && m.target.is_none()
            && m.tooltip.contains("existence not checked")));
        assert!(m[1].tooltip.contains("re-inclusion"));
    }
    #[test]
    fn mercurial_mode_switch_invalid_syntax_includes_and_crlf_are_conservative() {
        let d = doc(
            "/project/.hgignore",
            "ignore",
            "regex/\r\nsyntax: glob\r\nbuild/\r\ninclude: other/\r\nsyntax: bad\r\nunknown/\r\nsyntax: regexp\r\nregexp/\r\nsyntax: rootglob\r\nroot/\r\n",
        );
        assert_eq!(markers(&d).iter().map(|m| m.line).collect::<Vec<_>>(), [2, 9]);
        assert!(markers(&doc("/project/.boringignore", "dotignore", "regex/\n")).is_empty());
    }
    #[test]
    fn language_and_filename_ownership_does_not_claim_unrelated_documents() {
        let _host=MockHost::install();
        for d in [
            doc("/project/ignore.txt", "text", "out/"),
            doc("/project/random.rs", "ignore", "out/"),
            doc("/project/.gitignore", "rust", "out/"),
        ] {
            assert!(markers(&d).is_empty());
            assert!(IgnorePlugin::default().semantic_tokens(&d).is_empty());
        }
        for path in ["/project/.gitignore_global", "/project/team.gitignore", "/project/custom.dockerignore"] {
            assert_eq!(markers(&doc(path, "ignore", "out/")).len(), 1);
        }
        let fallback = doc("/project/.chefignore", "dotignore", "out/");
        assert_eq!(markers(&fallback).len(), 1);
        assert!(IgnorePlugin::default().semantic_tokens(&fallback).is_empty());
    }
    #[test]
    fn honest_semantic_kinds_and_utf16_nonoverlapping_source_spans() {
        let _h = MockHost::install().with_capabilities(&[]);
        let mut p = IgnorePlugin::default();
        let d = built("### section\r\n!😀/**/*.[oa]\\ x\r\n");
        let tokens = p.semantic_tokens(&d);
        assert_eq!(excerpts(&d, &tokens), ["### section", "!", "**", "*", "[oa]", "\\ "]);
        assert!(tokens.iter().all(|t| matches!(t.kind, TokenKind::Comment | TokenKind::Operator | TokenKind::Regexp | TokenKind::String)));
        assert!(tokens[0].modifiers.contains(&TokenModifier::Documentation));
        assert_eq!(tokens[2].range.start, Position::new(1, 4));
        assert!(tokens.iter().all(|t| t.range.start < t.range.end));
        assert!(tokens.windows(2).all(|pair| pair[0].range.end <= pair[1].range.start));
    }
    #[test]
    fn semantic_mercurial_directives_preserve_regex_and_glob_distinction() {
        let _host=MockHost::install();
        let d =
            doc("/project/.hgignore", "ignore", ".*build/\nsyntax: glob\n!build/**\nsyntax: invalid\nunknown*\nsyntax: regexp\n^build/\n");
        let tokens = IgnorePlugin::default().semantic_tokens(&d);
        let text = excerpts(&d, &tokens);
        assert_eq!(text, [".*build/", "syntax", "glob", "!", "**", "syntax", "syntax", "regexp", "^build/"]);
        assert_eq!(tokens[0].kind, TokenKind::Regexp);
        assert_eq!(tokens[1].kind, TokenKind::Keyword);
        assert!(!text.iter().any(|t| t == "unknown*"));
    }
    #[test]
    fn public_open_save_then_watcher_echo_or_focus_keeps_literal_directory_sample() {
        use openidea_plugin::{Plugin,Event,FilesChanged};
        for next in [Event::FilesChanged(FilesChanged {created:vec![],modified:vec!["/project/.gitignore".into()],removed:vec![]}), Event::EditorFocused(Some("/project/.gitignore".into())), Event::FileSaved("/project/unrelated.txt".into()), Event::FilesChanged(FilesChanged {created:vec![],modified:vec!["/project/unrelated.txt".into()],removed:vec![]}), Event::VcsChanged("/project".into())] {
            let h=MockHost::install();h.add_file(".gitignore","build\n!cache\nsub/out\nout/\n");h.add_dir("build");h.add_dir("cache");h.add_dir("sub");h.add_dir("sub/out");
            let mut p=IgnorePlugin::default();p.activate(&h.context("aritti.ignore")).unwrap();let d=built("build\n!cache\nsub/out\nout/\n");
            p.handle_events(&[Event::FileOpened(d.path.clone())],&h.invocation());
            p.handle_events(&[Event::FileSaved(d.path.clone())],&h.invocation());
            assert_eq!(p.line_markers(&d).iter().map(|m|m.line).collect::<Vec<_>>(),[0,1,2,3]);
            p.handle_events(&[next.clone()],&h.invocation());
            assert_eq!(p.line_markers(&d).iter().map(|m|m.line).collect::<Vec<_>>(),[0,1,2,3],"{next:?}");
            assert!(h.process_runs().is_empty()&&h.requests().is_empty());
        }
    }

    #[test]
    fn watcher_retention_never_extends_original_observation_ttl() {
        use openidea_plugin::{Plugin,Event,FilesChanged};
        let h=MockHost::install();h.add_file(".gitignore","build\n");h.add_dir("build");
        let mut p=IgnorePlugin::default();p.activate(&h.context("aritti.ignore")).unwrap();let d=built("build\n");
        p.handle_events(&[Event::FileSaved(d.path.clone())],&h.invocation());
        let original=std::time::Instant::now()-std::time::Duration::from_secs(29);p.directories.sample.as_mut().unwrap().at=original;
        p.handle_events(&[Event::FilesChanged(FilesChanged {created:vec![],modified:vec![d.path.clone()],removed:vec![]})],&h.invocation());
        assert_eq!(p.directories.sample.as_ref().unwrap().at,original);assert_eq!(p.line_markers(&d).len(),1);
        p.directories.sample.as_mut().unwrap().at=std::time::Instant::now()-std::time::Duration::from_secs(31);
        p.handle_events(&[Event::EditorFocused(Some(d.path.clone()))],&h.invocation());assert!(p.line_markers(&d).is_empty());assert!(p.directories.sample.is_none());
    }
    #[test]
    fn retention_still_rejects_source_target_owner_preference_and_overflow_changes() {
        use openidea_plugin::{Plugin,Event,FilesChanged};
        for case in 0..5 {
            let h=MockHost::install();h.add_file(".gitignore","build\n");h.add_dir("build");
            let mut p=IgnorePlugin::default();p.activate(&h.context("aritti.ignore")).unwrap();let d=built("build\n");
            p.handle_events(&[Event::FileSaved(d.path.clone())],&h.invocation());assert_eq!(p.line_markers(&d).len(),1);
            let mut changed=d.path.clone();
            match case {
                0=>h.add_file(".gitignore","different\n"),
                1=>{h.add_file("build","now a regular file");changed="/project/build".into();},
                2=>h.add_project("/project/build"),
                3=>openidea_plugin::host::storage::set("preferences:v2",r#"{"schema":2,"scope":"application:aritti.ignore","overrides":{},"disabledChecks":[],"disabledSupport":[".gitignore"],"disabledCreation":[]}"#).unwrap(),
                _=>{},
            }
            let events=if case==4 {vec![Event::EditorFocused(Some(d.path.clone()));257]} else {vec![Event::FilesChanged(FilesChanged {created:vec![],modified:vec![changed],removed:vec![]})]};
            p.handle_events(&events,&h.invocation());assert!(p.line_markers(&d).is_empty(),"case{case}");assert!(p.directories.sample.is_none(),"case{case}");
            assert!(h.process_runs().is_empty()&&h.requests().is_empty());
        }
    }

    #[test]
    fn event_admitted_literal_directories_add_markers_without_marker_filesystem_reads() {
        use openidea_plugin::{Plugin,Event};
        let h=MockHost::install();h.add_file(".gitignore","build\n!cache\nfile.txt\nmissing\nsub/out\n*.tmp\n../escape\n");
        h.add_dir("build");h.add_dir("cache");h.add_dir("sub");h.add_dir("sub/out");h.add_file("file.txt","");
        let mut p=IgnorePlugin::default();p.activate(&h.context("aritti.ignore")).unwrap();
        let d=built("build\n!cache\nfile.txt\nmissing\nsub/out\n*.tmp\n../escape\n");
        assert!(p.line_markers(&d).is_empty(),"before event admission");
        p.handle_events(&[Event::FileSaved(d.path.clone())],&h.invocation());
        assert_eq!(p.line_markers(&d).iter().map(|m|m.line).collect::<Vec<_>>(),[0,1,4]);
        h.set_project_root(None); // cached render must not require current project or stat
        assert_eq!(p.line_markers(&d).len(),3);
        let mut edited=d.clone();edited.text.push_str("# unsaved\n");assert!(p.line_markers(&edited).is_empty());
        p.handle_events(&[Event::EditorFocused(None)],&h.invocation());assert!(p.line_markers(&d).is_empty());
    }
    #[test]
    fn cached_directory_facts_do_not_cross_document_language_admission() {
        let h=MockHost::install();h.add_file(".gitignore","build\n");h.add_dir("build");
        let mut cache=Directories::default();cache.configure(true);cache.admit("/project/.gitignore","build\n",&h.invocation());
        let mut d=built("build\n");assert_eq!(cache.markers(&d).len(),1);
        d.language="plaintext".into();assert!(cache.markers(&d).is_empty());
    }
    #[test]
    fn directory_admission_is_bounded_owned_and_expires_without_absence_inference() {
        let h=MockHost::install();let text=(0..80).map(|n|format!("d{n}\n")).collect::<String>();h.add_file(".gitignore",&text);
        for n in 0..80 {h.add_dir(&format!("d{n}"));}
        let mut cache=Directories::default();cache.configure(true);cache.admit("/project/.gitignore",&text,&h.invocation());
        assert_eq!(cache.markers(&built(&text)).len(),64);
        cache.sample.as_mut().unwrap().at=std::time::Instant::now()-std::time::Duration::from_secs(31);assert!(cache.markers(&built(&text)).is_empty());
        h.add_project("/project/nested");h.add_dir("nested/dir");h.add_file(".gitignore","nested/dir\n");cache.admit("/project/.gitignore","nested/dir\n",&h.invocation());assert!(cache.markers(&built("nested/dir\n")).is_empty());
        cache.configure(false);cache.admit("/project/.gitignore",&text,&h.invocation());assert!(cache.sample.is_none());
    }
    #[test]
    fn unsaved_replacements_need_no_old_text_cache_or_process() {
        let h = MockHost::install();
        h.add_file(".gitignore", "disk/\n");
        let mut p = IgnorePlugin::default();
        let d = built("current/\n# note\n");
        assert_eq!(p.line_markers(&d).len(), 1);
        let mut changed = built("# changed\n");
        changed.version = 2;
        assert!(p.line_markers(&changed).is_empty());
        assert_eq!(excerpts(&changed, &p.semantic_tokens(&changed)), ["# changed"]);
    }
    #[test]
    fn bounded_output_and_oversized_inputs_never_trigger_unbounded_decoration() {
        let _host=MockHost::install();
        let mut p = IgnorePlugin::default();
        let d = built(&"out/\n".repeat(300));
        assert_eq!(p.line_markers(&d).len(), MAX_MARKERS);
        let d = built(&"!**?*\n".repeat(3000));
        assert!(p.semantic_tokens(&d).len() <= MAX_TOKENS);
        let d = built(&"out/\n".repeat(MAX_TEXT / 5 + 1));
        assert!(p.line_markers(&d).is_empty() && p.semantic_tokens(&d).is_empty());
        let d = built(&format!("{}out/\n", "\n".repeat(MAX_LINES)));
        assert!(p.line_markers(&d).is_empty());
    }
}
