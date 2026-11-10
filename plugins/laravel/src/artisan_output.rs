//! Plain, bounded failure summaries retain the cause before console stack frames.

fn plain(text: &str) -> String {
    let mut out = String::new();
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\u{1b}' {
            match chars.next() {
                Some('[') => {
                    for next in chars.by_ref() {
                        if ('\u{40}'..='\u{7e}').contains(&next) {
                            break;
                        }
                    }
                }
                Some(']') => {
                    while let Some(next) = chars.next() {
                        if next == '\u{7}' {
                            break;
                        }
                        if next == '\u{1b}' && chars.peek() == Some(&'\\') {
                            chars.next();
                            break;
                        }
                    }
                }
                _ => {}
            }
        } else if ch == '\r' {
            if chars.peek() != Some(&'\n') {
                out.push('\n');
            }
        } else if !ch.is_control() || ch == '\n' || ch == '\t' {
            out.push(ch);
        }
    }
    out
}

fn excerpt(text: &str) -> String {
    let text = plain(text);
    let mut lines = text.lines().map(str::trim).filter(|line| !line.is_empty());
    let head = lines.by_ref().take(12).collect::<Vec<_>>().join("\n");
    let clipped = head.chars().count() > 2000;
    let mut result: String = head.chars().take(2000).collect();
    if clipped || lines.next().is_some() {
        result.push_str("\n…");
    }
    result
}

pub(crate) fn failure(command: &str, status: Option<i32>, stdout: &str, stderr: &str) -> String {
    let stdout = excerpt(stdout);
    let stderr = excerpt(stderr);
    let detail = match (stdout.is_empty(), stderr.is_empty()) {
        (true, true) => "No error output was captured.".to_owned(),
        (false, true) => stdout,
        (true, false) => stderr,
        (false, false) if stdout == stderr => stderr,
        _ => format!("Standard error:\n{stderr}\n\nStandard output:\n{stdout}"),
    };
    let exit = status.map_or_else(
        || "no exit status".to_owned(),
        |code| format!("exit {code}"),
    );
    format!("artisan {command} failed ({exit}):\n{detail}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retains_the_exception_before_long_formatted_stack_traces() {
        let output = format!(
            "\x1b[31mRuntimeException\x1b[0m\nUnable to write cache directory.\n{}\n\x1b[2m+47 vendor frames\x1b[22m\n48 artisan:16",
            "vendor frame\n".repeat(50)
        );
        let message = failure("optimize:clear", Some(1), "", &output);
        assert!(message.contains("RuntimeException\nUnable to write cache directory."));
        assert!(!message.contains('\x1b'));
        assert!(!message.contains("artisan:16"));
        assert!(message.contains("exit 1"));
    }

    #[test]
    fn keeps_both_streams_and_strips_osc_and_control_sequences() {
        let message = failure(
            "optimize:clear",
            Some(2),
            "SQLSTATE[HY000] Connection refused",
            "\x1b]8;;https://example.test\x1b\\Exception\x1b]8;;\x07\r\n\x1b[2m+47 vendor frames\x1b[22m\0",
        );
        assert!(message.contains("SQLSTATE[HY000] Connection refused"));
        assert!(message.contains("Exception\n+47 vendor frames"));
        assert!(!message.contains("https://"));
        assert!(
            !message
                .chars()
                .any(|c| c.is_control() && c != '\n' && c != '\t')
        );
    }

    #[test]
    fn bounds_unicode_output_and_explains_missing_diagnostics() {
        let message = failure("cache:clear", None, &"é".repeat(10000), "");
        assert!(message.chars().count() < 2100);
        assert!(message.ends_with('…'));
        assert!(failure("cache:clear", Some(1), "", "").contains("No error output was captured."));
    }
}
