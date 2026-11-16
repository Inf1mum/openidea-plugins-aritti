//! `where('status', 'refused')` on a column with a backed-enum cast: warn on values the enum lacks
//! and complete the enum's values.
use crate::{eloquent_members, index::Index, php};
use openidea_plugin::{
    Position, Range, Severity, text,
    providers::{CompletionItem, CompletionKind, Document},
};
use std::ops::Range as Span;

const VALUE_METHODS: &[&str] = &["where", "orWhere", "whereNot", "orWhereNot", "firstWhere"];
const LIST_METHODS: &[&str] = &["whereIn", "orWhereIn", "whereNotIn", "orWhereNotIn"];
const OPERATORS: &[&str] = &["=", "==", "!=", "<>", "<", ">", "<=", ">=", "<=>"];

pub(crate) struct Finding {
    pub start: usize,
    pub end: usize,
    pub code: &'static str,
    pub severity: Severity,
    pub message: String,
    pub replacement: Option<String>,
}

struct Site {
    enum_name: String,
    column: String,
    cases: Vec<(String, String)>,
    values: Vec<Span<usize>>,
}

fn method_calls(masked: &str) -> impl Iterator<Item = (&str, usize)> {
    masked.match_indices('(').filter_map(|(open, _)| {
        let before = masked[..open].trim_end();
        let name_start = before
            .char_indices()
            .rev()
            .find(|(_, c)| !(c.is_ascii_alphanumeric() || *c == '_'))
            .map_or(0, |(i, c)| i + c.len_utf8());
        let name = &before[name_start..];
        let receiver = masked[..name_start].trim_end();
        let member = receiver.ends_with("->") || receiver.ends_with("?->") || receiver.ends_with("::");
        (member && (VALUE_METHODS.contains(&name) || LIST_METHODS.contains(&name))).then_some((name, open))
    })
}

/// Literal contents ranges that sit in a value position of the call.
fn value_literals(masked: &str, method: &str, open: usize) -> Option<(String, Vec<Span<usize>>)> {
    let arguments = php::arguments(masked, open);
    let (column, _) = php::literal(masked, arguments.first()?.clone())?;
    let column = column.rsplit('.').next()?.to_owned();
    let literal_range = |range: Span<usize>| {
        php::literal(masked, range).map(|(content, start)| start..start + content.len())
    };

    if LIST_METHODS.contains(&method) {
        let array = php::array_open(masked, arguments.get(1)?.clone())?;
        let values = php::items(masked, array)
            .into_iter()
            .filter(|item| item.key.is_none())
            .filter_map(|item| literal_range(item.value))
            .collect();
        return Some((column, values));
    }

    let value = match arguments.len() {
        2 => arguments[1].clone(),
        3 => {
            let (operator, _) = php::literal(masked, arguments[1].clone())?;
            if !OPERATORS.contains(&operator) {
                return None;
            }
            arguments[2].clone()
        }
        _ => return None,
    };

    Some((column, literal_range(value).into_iter().collect()))
}

fn sites(index: &Index, doc: &Document, owns: &dyn Fn(&str) -> bool, masked: &str, around: Option<usize>) -> Vec<Site> {
    let mut found = Vec::new();

    for (method, open) in method_calls(masked).take(256) {
        if let Some(offset) = around {
            let inside = php::arguments(masked, open).last().is_some_and(|last| open < offset && offset <= last.end + 1);
            if !inside {
                continue;
            }
        }
        let Some((column, values)) = value_literals(masked, method, open) else {
            continue;
        };
        if values.is_empty() {
            continue;
        }
        let Some(full) = eloquent_members::semantic::column_enum(index, doc, open, owns, column.as_str()) else {
            continue;
        };
        let Some((_, cases, _)) = eloquent_members::enum_values(index, &full, owns) else {
            continue;
        };
        let cases: Option<Vec<(String, String)>> = cases
            .into_iter()
            .map(|(name, value, _)| {
                let value = match value? {
                    serde_json::Value::String(value) => value,
                    serde_json::Value::Number(value) => value.to_string(),
                    _ => return None,
                };
                Some((name, value))
            })
            .collect();
        let Some(cases) = cases.filter(|cases| !cases.is_empty()) else {
            continue;
        };
        found.push(Site { enum_name: full.rsplit('\\').next().unwrap_or(&full).to_owned(), column, cases, values });
    }

    found
}

fn distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();

    for (i, x) in a.chars().enumerate() {
        let mut previous = row[0];
        row[0] = i + 1;
        for (j, y) in b.iter().enumerate() {
            let current = row[j + 1];
            row[j + 1] = (previous + usize::from(x != *y)).min(row[j] + 1).min(current + 1);
            previous = current;
        }
    }

    row[b.len()]
}

fn closest<'a>(value: &str, cases: &'a [(String, String)]) -> Option<&'a str> {
    let limit = (value.len() / 3).max(2);
    cases
        .iter()
        .map(|(_, case)| (distance(&value.to_lowercase(), &case.to_lowercase()), case))
        .filter(|(d, _)| *d <= limit)
        .min_by_key(|(d, case)| (*d, case.len()))
        .map(|(_, case)| case.as_str())
}

pub(crate) fn findings(index: &Index, doc: &Document, owns: &dyn Fn(&str) -> bool) -> Vec<Finding> {
    if index.partial || doc.text.len() > 256 * 1024 {
        return Vec::new();
    }
    let masked = php::mask_comments(&doc.text);
    let mut found = Vec::new();

    for site in sites(index, doc, owns, &masked, None) {
        let allowed = site.cases.iter().map(|(_, value)| format!("'{value}'")).collect::<Vec<_>>().join(", ");
        for range in site.values {
            let value = &masked[range.clone()];
            if value.is_empty() || site.cases.iter().any(|(_, case)| case == value) {
                continue;
            }
            found.push(Finding {
                start: range.start,
                end: range.end,
                code: "laravel.unknown-enum-value",
                severity: Severity::Warning,
                message: format!("Column '{}' can never be '{value}': it is cast to {} ({allowed})", site.column, site.enum_name),
                replacement: closest(value, &site.cases).map(str::to_owned),
            });
        }
    }

    found
}

pub(crate) fn completion(index: &Index, doc: &Document, at: Position, owns: &dyn Fn(&str) -> bool) -> Vec<CompletionItem> {
    let Some(offset) = text::offset(&doc.text, at) else {
        return Vec::new();
    };
    if index.partial || doc.text.len() > 256 * 1024 {
        return Vec::new();
    }
    let masked = php::mask_comments(&doc.text);

    for site in sites(index, doc, owns, &masked, Some(offset)) {
        let Some(range) = site.values.iter().find(|range| range.start <= offset && offset <= range.end) else {
            continue;
        };
        let replace = Range::new(text::position(&doc.text, range.start), text::position(&doc.text, range.end));

        return site
            .cases
            .iter()
            .map(|(case, value)| {
                let mut item = CompletionItem::new(value, CompletionKind::EnumMember);
                item.detail = Some(format!("{}::{case}", site.enum_name));
                item.replace = Some(replace);
                item
            })
            .collect();
    }

    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::{diagnostics::DiagnosticsProvider, mock::MockHost, providers::Providers};

    fn project(host: &MockHost) -> crate::Main {
        host.add_file("composer.json", r#"{"require":{"laravel/framework":"^11.0"},"autoload":{"psr-4":{"App\\":"app/"}}}"#);
        host.add_file("app/Enums/Status.php", "<?php namespace App\\Enums; enum Status: string { case Pending = 'pending'; case Shipped = 'shipped'; case Refunded = 'refunded'; }");
        host.add_file("app/Models/Order.php", "<?php namespace App\\Models; use App\\Enums\\Status; use Illuminate\\Database\\Eloquent\\Model; class Order extends Model { protected $casts = ['status' => Status::class]; }");
        let mut plugin = crate::tests::activated(host);
        plugin.index_project(&host.invocation().project().unwrap());
        crate::tests::finish_indexing(&mut plugin);
        plugin
    }

    fn check(plugin: &mut crate::Main, body: &str) -> Vec<openidea_plugin::Diagnostic> {
        let source = format!("<?php use App\\Models\\Order; {body}");
        plugin
            .diagnostics(&crate::tests::doc("/project/usage.php", "php", &source))
            .into_iter()
            .filter(|d| d.code.as_deref() == Some("laravel.unknown-enum-value"))
            .collect()
    }

    #[test]
    fn unknown_value_warns_in_where_family_only() {
        let host = MockHost::install();
        let mut plugin = project(&host);

        for body in [
            "Order::where('status', 'refused');",
            "Order::where('status', '!=', 'refused');",
            "Order::query()->orWhere('status', 'refused');",
            "Order::whereNot('status', 'refused');",
            "Order::firstWhere('status', 'refused');",
            "Order::whereIn('status', ['pending', 'refused']);",
        ] {
            let found = check(&mut plugin, body);
            assert_eq!(found.len(), 1, "{body}");
            assert!(found[0].message.contains("'refused'"), "{body}");
        }

        for body in [
            "Order::where('status', 'pending');",
            "Order::where('status', '!=', 'shipped');",
            "Order::whereIn('status', ['pending', 'shipped']);",
            "Order::where('other', 'refused');",
            "Order::where('status', $value);",
        ] {
            assert!(check(&mut plugin, body).is_empty(), "{body}");
        }
    }

    #[test]
    fn closest_value_is_the_replacement() {
        let cases = vec![("A".to_owned(), "pending".to_owned()), ("B".to_owned(), "refunded".to_owned())];
        assert_eq!(closest("refused", &cases), Some("refunded"));
        assert_eq!(closest("zzzzzzzz", &cases), None);
    }

    #[test]
    fn value_argument_completes_enum_values() {
        let host = MockHost::install();
        let mut plugin = project(&host);
        let source = "<?php use App\\Models\\Order; Order::where('status', '');";
        let column = source.find("''").unwrap() + 1;
        let items = plugin.completion(&crate::tests::doc("/project/usage.php", "php", source), text::position(source, column), None);
        let labels: Vec<_> = items.iter().map(|item| item.label.as_str()).collect();
        assert_eq!(labels, ["pending", "shipped", "refunded"]);
    }
}
