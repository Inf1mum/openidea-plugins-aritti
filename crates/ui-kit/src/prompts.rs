//! Concrete confirmation bodies must fit the pinned host's byte limit.
use openidea_plugin::{Error, host};

/// API 1.3 v0.1.39 clips confirmation bodies to 16 KiB (wit_host::confirm).
/// Refuse a larger concrete preview rather than applying an unseen suffix.
pub const MAX_CONFIRM_BYTES: usize = 16 << 10;

/// Show source as lines, keeping non-layout control characters visible.
/// This is presentation only; callers still approve and revalidate original bytes.
pub fn readable_source(text: &str) -> String {
    let mut shown=String::with_capacity(text.len());
    for c in text.chars() {
        if c.is_control() && !matches!(c,'\n'|'\r'|'\t') { shown.extend(c.escape_default()); }
        else { shown.push(c); }
    }
    shown
}

pub fn confirm_exact(title: &str, body: &str) -> Result<bool, Error> {
    if body.len() > MAX_CONFIRM_BYTES {
        return Err(Error::Invalid(
            "The complete preview exceeds the host's 16 KiB confirmation limit; reduce the operation before applying it.".into(),
        ));
    }
    host::confirm(title, body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;

    #[test]
    fn readable_source_keeps_lines_and_literal_escapes_but_exposes_control_characters() {
        let source="key=one\n\tunicode=\\u006C\r\ncontrol=\u{1b}";
        let shown=readable_source(source);
        assert!(shown.starts_with("key=one\n\tunicode=\\u006C\r\n"));
        assert!(shown.ends_with("control=\\u{1b}"));
        assert!(!shown.contains('\u{1b}'));
    }

    #[test]
    fn exact_host_byte_boundary_is_reviewable() {
        let host = MockHost::install();
        host.answer_next_prompt(Some(&["ok"]));
        assert!(confirm_exact("Preview", &"x".repeat(MAX_CONFIRM_BYTES)).unwrap());
    }

    #[test]
    fn oversized_unicode_preview_refuses_before_consuming_confirmation() {
        let host = MockHost::install();
        host.answer_next_prompt(Some(&["ok"]));
        assert!(matches!(confirm_exact("Preview", &"é".repeat(MAX_CONFIRM_BYTES / 2 + 1)), Err(Error::Invalid(_))));
        assert!(host::confirm("Still pending", "The rejected preview did not ask.").unwrap());
    }
}
