//! Current-buffer line folds; no process, project discovery or settings reads.
use crate::*;
use openidea_plugin::language_features::{FoldRange, LanguageFeatures};
impl LanguageFeatures for Properties {
    fn folding(&mut self, doc: &Document) -> Vec<FoldRange> {
        if doc.language != "properties"
            || doc.text.len() > 128 << 10
            || doc.text.lines().take(4097).count() > 4096
        {
            return vec![];
        }
        let items = props::items(&doc.text);
        let mut result = Vec::new();
        let mut comments: Option<(u32, u32)> = None;
        let mut group: Option<(String, u32, u32)> = None;
        let flush_comments = |state: &mut Option<(u32, u32)>, out: &mut Vec<FoldRange>| {
            if let Some((start, end)) = state.take() {
                if end > start {
                    out.push(FoldRange {
                        start_line: start,
                        end_line: end,
                        kind: "comment".into(),
                    });
                }
            }
        };
        let flush_group = |state: &mut Option<(String, u32, u32)>, out: &mut Vec<FoldRange>| {
            if let Some((_, start, end)) = state.take() {
                if end > start {
                    out.push(FoldRange {
                        start_line: start,
                        end_line: end,
                        kind: "region".into(),
                    });
                }
            }
        };
        for item in items.into_iter().take(4096) {
            let span = item.span();
            let start = openidea_plugin::text::position(&doc.text, span.start).line;
            let end = openidea_plugin::text::position(&doc.text, span.end).line;
            match item {
                props::Item::Comment(_) => {
                    flush_group(&mut group, &mut result);
                    if let Some((_, last)) =
                        comments.as_mut().filter(|(_, last)| start == *last + 1)
                    {
                        *last = end;
                    } else {
                        flush_comments(&mut comments, &mut result);
                        comments = Some((start, end));
                    }
                }
                props::Item::Prop(prop) => {
                    flush_comments(&mut comments, &mut result);
                    if end > start {
                        result.push(FoldRange {
                            start_line: start,
                            end_line: end,
                            kind: "region".into(),
                        });
                    }
                    let prefix = prop
                        .exact_key(&doc.text)
                        .and_then(|key| key.rsplit_once('.').map(|(prefix, _)| prefix.to_owned()))
                        .filter(|p| !p.is_empty());
                    if let Some(prefix) = prefix {
                        if let Some((_, _, last)) = group
                            .as_mut()
                            .filter(|(p, _, last)| *p == prefix && start == *last + 1)
                        {
                            *last = end;
                        } else {
                            flush_group(&mut group, &mut result);
                            group = Some((prefix, start, end));
                        }
                    } else {
                        flush_group(&mut group, &mut result);
                    }
                }
            }
            if result.len() > 1024 {
                return vec![];
            }
        }
        flush_comments(&mut comments, &mut result);
        flush_group(&mut group, &mut result);
        result.sort_by_key(|r| (r.start_line, r.end_line));
        result.dedup_by_key(|r| (r.start_line, r.end_line));
        result
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn comments_groups_and_continuations_use_unsaved_source_lines() {
        let h = openidea_plugin::mock::MockHost::install().with_capabilities(&[]);
        let mut p = Properties::default();
        let mut d=Document{path:"/project/a.properties".into(),language:"properties".into(),version:2,text:"# one\n! two\na.one=first\\\n continued\na.two=second\n\na.three=third\nb.one=other\n".into()};
        let folds = LanguageFeatures::folding(&mut p, &d);
        assert_eq!(
            folds
                .iter()
                .map(|f| (f.start_line, f.end_line))
                .collect::<Vec<_>>(),
            [(0, 1), (2, 3), (2, 4)]
        );
        d.text = "single=value\n".into();
        assert!(LanguageFeatures::folding(&mut p, &d).is_empty());
        d.language = "java".into();
        assert!(LanguageFeatures::folding(&mut p, &d).is_empty());
        assert!(h.process_runs().is_empty());
    }
}
