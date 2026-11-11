//! Pest dataset and group names, read from the project's tests.

use std::collections::BTreeSet;

use openidea_plugin::Project;

const MAX_TEST_FILES: u32 = 513;
const MAX_FILE_BYTES: usize = 128 * 1024;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Dataset,
    Group,
}

/// The quoted argument being typed at `offset`: its kind and the byte range of the text typed so far.
pub fn context(text: &str, offset: usize) -> Option<(Kind, usize, usize)> {
    let line_start = text[..offset].rfind('\n').map_or(0, |i| i + 1);
    let before = &text[line_start..offset];
    let quote = before.rfind(['\'', '"'])?;
    let typed = &before[quote + 1..];
    if typed.contains(['\'', '"', ')', ',']) {
        return None;
    }

    let call = before[..quote].trim_end();
    let call = call.rsplit_once(',').map_or(call, |(_, last)| last).trim_end();
    let kind = if ["->with(", "->group(", "->groups(", "#[Group("].iter().any(|opener| before[..quote].contains(opener)) {
        if call.ends_with("->with(") {
            Kind::Dataset
        } else if call.ends_with("->group(") || call.ends_with("->groups(") || call.ends_with("#[Group(") || before[..quote].contains("->groups(") || before[..quote].contains("#[Group(") {
            Kind::Group
        } else {
            return None;
        }
    } else {
        return None;
    };

    Some((kind, line_start + quote + 1, offset))
}

pub fn names(project: &Project, kind: Kind) -> Vec<String> {
    let Ok(paths) = project.find_files("tests/**/*.php", MAX_TEST_FILES) else {
        return Vec::new();
    };

    let mut found = BTreeSet::new();
    for path in paths {
        let Ok(text) = project.read_file(&path) else {
            continue;
        };
        if text.len() > MAX_FILE_BYTES {
            continue;
        }
        found.extend(declared(&text, kind));
    }

    found.into_iter().collect()
}

fn declared(text: &str, kind: Kind) -> Vec<String> {
    let openers: &[&str] = match kind {
        Kind::Dataset => &["dataset("],
        Kind::Group => &["->group(", "->groups(", "#[Group("],
    };

    let mut found = Vec::new();
    for opener in openers {
        for (at, _) in text.match_indices(opener) {
            let rest = &text[at + opener.len()..];
            let end = rest.find([')', ']']).unwrap_or(rest.len());
            let arguments = &rest[..end];
            let take = if kind == Kind::Dataset { 1 } else { usize::MAX };
            found.extend(quoted(arguments).into_iter().take(take));
        }
    }

    found
}

fn quoted(arguments: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut at = 0;

    while let Some(offset) = arguments[at..].find(['\'', '"']) {
        let start = at + offset;
        let quote = arguments[start..].chars().next().unwrap_or('\'');
        let Some(length) = arguments[start + 1..].find(quote) else {
            break;
        };
        found.push(arguments[start + 1..start + 1 + length].to_owned());
        at = start + 1 + length + 1;
    }

    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_tells_datasets_from_groups_and_ignores_other_strings() {
        let at = |text: &str| context(text, text.len()).map(|(kind, ..)| kind);
        assert_eq!(at("it('x')->with('em"), Some(Kind::Dataset));
        assert_eq!(at("it('x')->group('sl"), Some(Kind::Group));
        assert_eq!(at("it('x')->groups('a', 'b"), Some(Kind::Group));
        assert_eq!(at("#[Group('sl"), Some(Kind::Group));
        assert_eq!(at("it('x')->with([1, 'ab"), None);
        assert_eq!(at("$this->get('ab"), None);
    }

    #[test]
    fn declared_names_come_from_dataset_and_group_calls() {
        let text = "dataset('emails', fn () => ['a']);\nit('x')->group('slow', 'api');\n#[Group('db')]";
        assert_eq!(declared(text, Kind::Dataset), ["emails"]);
        assert_eq!(declared(text, Kind::Group), ["slow", "api", "db"]);
    }
}
