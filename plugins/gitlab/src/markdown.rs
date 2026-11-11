//! CommonMark presentation from a bounded event stream. Raw HTML is text;
//! only explicit HTTP(S) links survive, and images never create fetchable tags.
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd, html};
const MAX_INPUT: usize = 128 << 10;
pub(crate) const STYLE: &str = r#"
.gl-markdown{font:inherit;color:var(--ui-fg);line-height:1.5;overflow-wrap:anywhere}.gl-markdown>:first-child{margin-top:0}.gl-markdown pre{overflow:auto;padding:var(--ui-space-2,8px);background:var(--ui-bg-raised,var(--ui-bg));border:1px solid var(--ui-border)}.gl-markdown code{font-family:var(--ui-font-mono,monospace)}.gl-markdown blockquote{margin-inline:0;padding-inline-start:var(--ui-space-3,12px);border-inline-start:2px solid var(--ui-border);color:var(--ui-muted)}.gl-markdown table{border-collapse:collapse;max-width:100%}.gl-markdown td,.gl-markdown th{border:1px solid var(--ui-border);padding:4px 8px}.gl-markdown a{color:var(--ui-accent);text-decoration:underline}.gl-markdown img{display:none}
"#;
fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
fn link(destination: &str, base: &str) -> Option<String> {
    if destination.len() > 4096
        || destination
            .chars()
            .any(|c| c.is_control() || c.is_whitespace())
        || destination.contains('\\')
    {
        return None;
    }
    let base = url::Url::parse(base).ok()?;
    let url = base.join(destination).ok()?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return None;
    }
    Some(url.to_string())
}
pub(crate) fn fragment(text: &str, base: &str) -> String {
    let truncated = text.len() > MAX_INPUT;
    let limit = if truncated {
        text.char_indices()
            .map(|(at, _)| at)
            .take_while(|at| *at <= MAX_INPUT)
            .last()
            .unwrap_or(0)
    } else {
        text.len()
    };
    let text = &text[..limit];
    let mut links = Vec::new();
    let parser=Parser::new_ext(text,Options::ENABLE_TABLES|Options::ENABLE_STRIKETHROUGH|Options::ENABLE_TASKLISTS).map(|event|match event{
        Event::Html(raw)|Event::InlineHtml(raw)=>Event::Text(raw),
        Event::Start(Tag::Image{..})=>Event::Text("[Image: ".into()),Event::End(TagEnd::Image)=>Event::Text("]".into()),
        Event::Start(Tag::Link{dest_url,..})=>{let safe=link(&dest_url,base);links.push(safe.is_some());match safe{Some(url)=>Event::Html(format!("<a href=\"{}\" target=\"_blank\" rel=\"noopener noreferrer\" data-markdown-url=\"{}\">",escape(&url),escape(&url)).into()),None=>Event::Text("".into())}},
        Event::End(TagEnd::Link)=>if links.pop()==Some(true){Event::Html("</a>".into())}else{Event::Text("".into())},
        other=>other,
    });
    let mut body = String::new();
    html::push_html(&mut body, parser);
    if body.len() > 768 << 10 {
        body = format!(
            "<pre>{}</pre><p>Markdown layout exceeded its display budget; bounded source shown.</p>",
            escape(text)
        );
    }
    format!(
        "<article class=\"gl-markdown\">{body}{}</article>",
        if truncated {
            "<p>Markdown truncated at128KiB.</p>"
        } else {
            ""
        }
    )
}
pub(crate) fn links(text: &str, base: &str) -> Vec<String> {
    Parser::new(text)
        .filter_map(|event| match event {
            Event::Start(Tag::Link { dest_url, .. }) => link(&dest_url, base),
            _ => None,
        })
        .collect()
}
const LINK_SCRIPT: &str = r#"document.addEventListener('click',function(event){const anchor=event.target.closest&&event.target.closest('a[data-markdown-url]');if(!anchor)return;event.preventDefault();openidea.postMessage(JSON.stringify({type:'markdown-link',url:anchor.dataset.markdownUrl}));});"#;
pub(crate) fn document(text: &str, base: &str) -> String {
    format!(
        "<style>{STYLE}</style>{}<script>{LINK_SCRIPT}</script>",
        fragment(text, base)
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn commonmark_tables_lists_code_and_safe_relative_links_are_rendered() {
        let html = fragment(
            "# Heading\n\n**bold** _emphasis_ ~~removed~~\n\n- [x] complete\n\n|Name|Value|\n|---|---|\n|a|b|\n\n```rust\n<code>\n```\n\n[Issue](/g/p/-/issues/7)",
            "https://gitlab.com/g/p/-/merge_requests/1",
        );
        for expected in [
            "<h1>Heading</h1>",
            "<strong>bold</strong>",
            "<em>emphasis</em>",
            "<del>removed</del>",
            "<table>",
            "&lt;code&gt;",
            "href=\"https://gitlab.com/g/p/-/issues/7\"",
            "noopener noreferrer",
        ] {
            assert!(html.contains(expected), "{expected}: {html}");
        }
    }
    #[test]
    fn untrusted_html_unsafe_links_and_remote_images_cannot_create_active_elements() {
        let html = fragment(
            "<script>alert(1)</script>\n\n<img src=https://evil.test/a onerror=alert(1)>\n\n[bad](javascript:alert%281%29) [data](data:text/html,hello) [creds](https://u:p@evil.test)\n\n![tracking](https://evil.test/pixel)\n\n`<iframe>`",
            "https://gitlab.com/",
        );
        for forbidden in [
            "<script",
            "<img",
            "<iframe",
            "href=\"javascript:",
            "href=\"data:",
            "href=\"https://u:p",
        ] {
            assert!(!html.contains(forbidden), "{html}");
        }
        assert!(html.contains("[Image: tracking]"));
        assert!(html.contains("&lt;script&gt;"));
        assert!(link("java\nscript:alert(1)", "https://gitlab.com/").is_none());
    }
    #[test]
    fn unicode_and_large_documents_are_bounded_without_panics() {
        let html = fragment(&"😀".repeat(100000), "https://gitlab.com/");
        assert!(html.contains("truncated"));
        assert!(html.len() < 800000);
    }
}
