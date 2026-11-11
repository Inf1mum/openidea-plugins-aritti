//! Security checks of one PHP file: request data written straight into a model, and variables in raw SQL.

use openidea_plugin::Severity;

use crate::eloquent_checks::Finding;
use crate::php;

const QUIET_DIRECTORIES: &[&str] = &["/vendor/", "/tests/", "/database/"];
const MASS_ASSIGNMENT_METHODS: &[&str] = &["create", "update", "fill", "forceFill", "updateOrCreate", "firstOrCreate"];
const RAW_QUERY_METHODS: &[&str] = &["whereRaw", "orWhereRaw", "havingRaw", "orHavingRaw", "orderByRaw", "selectRaw", "groupByRaw", "fromRaw"];
const RAW_DB_METHODS: &[&str] = &["raw", "select", "statement", "unprepared", "insert", "update", "delete"];
const ALL_INPUT: &[&str] = &["$request->all()", "request()->all()", "Request::all()", "\\request()->all()"];

/// `relative_path` is relative to the project root.
pub fn findings(relative_path: &str, language: &str, text: &str) -> Vec<Finding> {
    let rooted = format!("/{relative_path}");
    if text.len() > 128 * 1024 || language != "php" || QUIET_DIRECTORIES.iter().any(|directory| rooted.contains(directory)) {
        return Vec::new();
    }

    let masked = php::mask_comments(text);
    let bytes = masked.as_bytes();
    let mut found = Vec::new();
    let mut at = 0;

    while at < bytes.len() {
        match bytes[at] {
            b'\'' | b'"' => at = php::string_end(bytes, at),
            b'<' if bytes[at..].starts_with(b"<<<") => at = crate::function_checks::heredoc_end(&masked, at),
            b'(' => {
                if let Some(finding) = call_finding(&masked, at) {
                    found.push(finding);
                }
                at += 1;
            }
            _ => at += 1,
        }
    }

    found.extend(unguarded_findings(&masked));
    found.extend(csrf_findings(&masked));
    found.sort_by_key(|finding| finding.start);
    found
}

fn call_finding(masked: &str, open: usize) -> Option<Finding> {
    let before = &masked[..open];
    let name_start = before.rfind(|c: char| !(c.is_alphanumeric() || c == '_')).map_or(0, |i| i + 1);
    let name = &before[name_start..];
    let connector = &before[..name_start];
    let is_member = connector.ends_with("->") || connector.ends_with("::");
    if name.is_empty() || !is_member {
        return None;
    }

    let close = php::closing(masked, open)?;
    let first_argument_end = first_argument_end(masked, open + 1, close);
    let argument = &masked[open + 1..first_argument_end];
    let (start, end) = trimmed_range(masked, open + 1, first_argument_end);

    let raw_query = RAW_QUERY_METHODS.contains(&name) || (RAW_DB_METHODS.contains(&name) && connector.trim_end_matches(':').ends_with("DB"));
    if raw_query && interpolates_variable(argument) {
        return Some(Finding {
            start,
            end,
            code: "raw-sql-variable",
            message: "A variable in raw SQL is an injection risk: pass it as a binding (`?` and an array).".into(),
            severity: Severity::Warning,
            replacement: None,
        });
    }

    if MASS_ASSIGNMENT_METHODS.contains(&name) && ALL_INPUT.contains(&argument.trim()) {
        return Some(Finding {
            start,
            end,
            code: "request-all-mass-assignment",
            message: "Every request field reaches the model: write only validated data (`$request->validated()` or `$request->only(...)`).".into(),
            severity: Severity::Warning,
            replacement: (argument.trim() == "$request->all()" && has_form_request_parameter(masked)).then(|| "$request->validated()".into()),
        });
    }

    None
}

/// `validated()` exists on a Form Request only: a `FooRequest $request` parameter that is not the plain `Request`.
fn has_form_request_parameter(masked: &str) -> bool {
    masked.match_indices(" $request").any(|(at, _)| {
        let type_name = masked[..at].rsplit(|c: char| !(c.is_alphanumeric() || c == '_' || c == '\\')).next().unwrap_or("");
        let short = type_name.rsplit('\\').next().unwrap_or("");
        short.len() > "Request".len() && short.ends_with("Request")
    })
}

/// `$guarded = []` and `Model::unguard()` open every attribute to mass assignment.
fn unguarded_findings(masked: &str) -> Vec<Finding> {
    let mut found = Vec::new();
    let message = "Every attribute is mass assignable: list the fillable attributes instead.";

    for (at, _) in masked.match_indices("$guarded") {
        let rest = &masked[at + "$guarded".len()..];
        let Some(value) = rest.trim_start().strip_prefix('=') else {
            continue;
        };
        let value = value.trim_start();
        let empty = value.starts_with("[]") || value.strip_prefix("array(").is_some_and(|inner| inner.trim_start().starts_with(')'));
        if empty {
            found.push(Finding { start: at, end: at + "$guarded".len(), code: "unguarded-model", message: message.into(), severity: Severity::Warning, replacement: None });
        }
    }

    for (at, _) in masked.match_indices("::unguard(") {
        found.push(Finding { start: at + 2, end: at + "::unguard".len(), code: "unguarded-model", message: message.into(), severity: Severity::Warning, replacement: None });
    }

    found
}

/// URIs excluded from CSRF verification, in `$except` of a `VerifyCsrfToken` class or in `validateCsrfTokens(except:)`.
fn csrf_findings(masked: &str) -> Vec<Finding> {
    let mut ranges = Vec::new();

    if masked.contains("VerifyCsrfToken") {
        for (at, _) in masked.match_indices("$except") {
            if let Some(open) = masked[at..].find('[').map(|i| at + i)
                && let Some(close) = php::closing(masked, open)
            {
                ranges.push((open, close));
            }
        }
    }

    for (at, _) in masked.match_indices("validateCsrfTokens(") {
        let open = at + "validateCsrfTokens".len();
        if let Some(close) = php::closing(masked, open) {
            ranges.push((open, close));
        }
    }

    let mut found = Vec::new();
    for (open, close) in ranges {
        for (start, end, value) in string_literals(masked, open, close) {
            let everything = value == "*";
            found.push(Finding {
                start,
                end,
                code: "csrf-excluded",
                message: if everything {
                    "CSRF verification is off for every URI.".into()
                } else {
                    format!("`{value}` is excluded from CSRF verification: keep this for webhooks that sign their requests.")
                },
                severity: if everything { Severity::Warning } else { Severity::Info },
                replacement: None,
            });
        }
    }

    found
}

/// `(start, end, value)` of each quoted string in `text[from..to]`, quotes included in the range.
fn string_literals(text: &str, from: usize, to: usize) -> Vec<(usize, usize, String)> {
    let bytes = text.as_bytes();
    let mut found = Vec::new();
    let mut at = from;

    while at < to {
        if matches!(bytes[at], b'\'' | b'"') {
            let end = php::string_end(bytes, at).min(to);
            if end > at + 2 {
                found.push((at, end, text[at + 1..end - 1].to_owned()));
            }
            at = end;
            continue;
        }
        at += 1;
    }

    found
}

fn trimmed_range(text: &str, start: usize, end: usize) -> (usize, usize) {
    let slice = &text[start..end];
    let leading = slice.len() - slice.trim_start().len();
    (start + leading, start + slice.trim_end().len().max(leading))
}

/// The end of the first argument in `text[start..close]`: the first comma outside brackets and strings.
fn first_argument_end(text: &str, start: usize, close: usize) -> usize {
    let bytes = text.as_bytes();
    let mut depth = 0i32;
    let mut at = start;

    while at < close {
        match bytes[at] {
            b'\'' | b'"' => {
                at = php::string_end(bytes, at);
                continue;
            }
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth -= 1,
            b',' if depth == 0 => return at,
            _ => {}
        }
        at += 1;
    }

    close
}

/// A `$variable` in the code or in a double-quoted part of the argument; single-quoted text is literal.
fn interpolates_variable(argument: &str) -> bool {
    let bytes = argument.as_bytes();
    let mut at = 0;

    while at < bytes.len() {
        match bytes[at] {
            b'\'' => at = php::string_end(bytes, at),
            b'"' => {
                let end = php::string_end(bytes, at);
                if has_interpolation(&argument[at + 1..end.saturating_sub(1).max(at + 1)]) {
                    return true;
                }
                at = end;
            }
            b'$' => return true,
            _ => at += 1,
        }
    }

    false
}

fn has_interpolation(double_quoted: &str) -> bool {
    let bytes = double_quoted.as_bytes();
    bytes.iter().enumerate().any(|(i, &b)| {
        let escaped = i > 0 && bytes[i - 1] == b'\\';
        let next = bytes.get(i + 1).copied().unwrap_or(0);
        !escaped && ((b == b'$' && (next.is_ascii_alphabetic() || next == b'_' || next == b'{')) || (b == b'{' && next == b'$'))
    })
}

#[cfg(test)]
mod tests {
    fn codes(source: &str) -> Vec<&'static str> {
        super::findings("app/Http/Controllers/Example.php", "php", source).into_iter().map(|f| f.code).collect()
    }

    #[test]
    fn request_all_into_a_model_is_reported() {
        assert_eq!(codes("<?php User::create($request->all());"), ["request-all-mass-assignment"]);
        assert_eq!(codes("<?php $user->update(request()->all());"), ["request-all-mass-assignment"]);
        assert!(codes("<?php User::create($request->validated());").is_empty());
    }

    #[test]
    fn variables_in_raw_sql_are_reported_and_bindings_are_not() {
        assert_eq!(codes("<?php DB::select(\"select * from users where id = $id\");"), ["raw-sql-variable"]);
        assert_eq!(codes("<?php $q->whereRaw('a = ' . $value);"), ["raw-sql-variable"]);
        assert!(codes("<?php DB::select('select * from users where id = ?', [$id]);").is_empty());
        assert!(codes("<?php $q->whereRaw('a = 1');").is_empty());
        assert!(codes("<?php $client->select($id);").is_empty());
    }

    #[test]
    fn comments_strings_tests_and_other_languages_are_quiet() {
        assert!(codes("<?php // User::create($request->all());").is_empty());
        assert!(codes("<?php $s = 'User::create($request->all())';").is_empty());
        assert!(super::findings("tests/Feature/ExampleTest.php", "php", "<?php User::create($request->all());").is_empty());
        assert!(super::findings("app/a.js", "javascript", "User::create($request->all());").is_empty());
    }

    #[test]
    fn validated_replaces_all_only_with_a_form_request() {
        let with_form_request = "<?php function store(StoreUserRequest $request) { User::create($request->all()); }";
        let found = super::findings("app/Http/Controllers/Example.php", "php", with_form_request);
        assert_eq!(found[0].replacement.as_deref(), Some("$request->validated()"));

        let plain = "<?php function store(Request $request) { User::create($request->all()); }";
        assert_eq!(super::findings("app/Http/Controllers/Example.php", "php", plain)[0].replacement, None);
    }

    #[test]
    fn empty_guarded_and_unguard_are_reported() {
        assert_eq!(codes("<?php class A { protected $guarded = []; }"), ["unguarded-model"]);
        assert_eq!(codes("<?php class A { protected $guarded = ['id']; }"), Vec::<&str>::new());
        assert_eq!(codes("<?php Model::unguard();"), ["unguarded-model"]);
    }

    #[test]
    fn csrf_exclusions_are_listed_and_the_wildcard_warns() {
        let class = "<?php class VerifyCsrfToken { protected $except = ['stripe/*', '*']; }";
        let found = super::findings("app/Http/Middleware/VerifyCsrfToken.php", "php", class);
        assert_eq!(found.len(), 2);
        assert_eq!(found[1].severity, openidea_plugin::Severity::Warning);

        let bootstrap = "<?php ->withMiddleware(function ($m) { $m->validateCsrfTokens(except: ['webhook/*']); })";
        assert_eq!(codes(bootstrap), ["csrf-excluded"]);
        assert!(codes("<?php class Other { protected $except = ['a']; }").is_empty());
    }
}
