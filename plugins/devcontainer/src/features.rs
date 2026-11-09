//! Feature ids for `features` keys, from the public containers.dev index. The index is fetched by a command only and kept
//! in plugin storage; completion reads the stored copy.

use openidea_plugin::host::{self, storage};
use openidea_plugin::{Error, HttpRequest, NotificationLevel};

pub const INDEX_URL: &str = "https://containers.dev/static/devcontainer-index.json";
const CACHE_KEY: &str = "feature-index";
const MAX_CACHE_BYTES: usize = 900 * 1024;

/// One `reference<TAB>name` line per feature, the reference written as a `features` key (`ghcr.io/x/features/node:1`).
pub fn parse_index(body: &str) -> Option<String> {
    let index: serde_json::Value = serde_json::from_str(body).ok()?;
    let mut lines = String::new();
    for collection in index.get("collections")?.as_array()? {
        for feature in collection.get("features").and_then(|f| f.as_array()).into_iter().flatten() {
            let Some(id) = feature.get("id").and_then(|v| v.as_str()) else {
                continue;
            };
            let reference = match feature.get("majorVersion").and_then(|v| v.as_str()) {
                Some(major) => format!("{id}:{major}"),
                None => id.to_owned(),
            };
            let name = feature.get("name").and_then(|v| v.as_str()).unwrap_or_default().replace(['\t', '\n'], " ");
            let line = format!("{reference}\t{name}\n");
            if lines.len() + line.len() > MAX_CACHE_BYTES {
                break;
            }
            lines.push_str(&line);
        }
    }

    (!lines.is_empty()).then_some(lines)
}

pub fn cached() -> Vec<(String, String)> {
    let Ok(Some(lines)) = storage::get(CACHE_KEY) else {
        return Vec::new();
    };

    lines
        .lines()
        .filter_map(|line| line.split_once('\t'))
        .map(|(reference, name)| (reference.to_owned(), name.to_owned()))
        .collect()
}

/// Creation deliberately offers a small metadata subset with explicit numeric
/// major tags. It does not fetch or infer feature options/availability.
pub(crate) fn creation_candidates() -> Vec<(String, String)> {
    let Ok(Some(lines)) = storage::get(CACHE_KEY) else { return Vec::new(); };
    creation_entries(&lines)
}

pub(crate) fn creation_entries(lines: &str) -> Vec<(String, String)> {
    if lines.len() > MAX_CACHE_BYTES { return Vec::new(); }
    let mut entries = std::collections::BTreeMap::new();
    for line in lines.lines().take(4096) {
        let Some((reference, name)) = line.split_once('\t') else { continue; };
        if !creation_reference(reference) { continue; }
        let name = name.chars().take(128).map(|c| if c.is_control() { ' ' } else { c }).collect::<String>();
        entries.entry(reference.to_owned()).or_insert(name);
        if entries.len() == 128 { break; }
    }
    entries.into_iter().collect()
}

fn creation_reference(reference: &str) -> bool {
    if reference.len() > 256 { return false; }
    let Some((path, major)) = reference.rsplit_once(':') else { return false; };
    let Some(path) = path.strip_prefix("ghcr.io/") else { return false; };
    !major.is_empty() && major.len() <= 10 && major.bytes().all(|c| c.is_ascii_digit())
        && path.split('/').count() >= 2
        && path.split('/').all(|part| {
            let bytes = part.as_bytes();
            let alnum = |c: u8| c.is_ascii_lowercase() || c.is_ascii_digit();
            bytes.first().is_some_and(|c| alnum(*c)) && bytes.last().is_some_and(|c| alnum(*c))
                && bytes.iter().all(|c| alnum(*c) || b"._-".contains(c))
                && bytes.windows(2).all(|pair| alnum(pair[0]) || alnum(pair[1]))
        })
}

#[cfg(test)]
mod creation_tests {
    use super::*;
    use openidea_plugin::mock::MockHost;

    #[test]
    fn creation_metadata_is_bounded_deduplicated_and_conservative() {
        for good in ["ghcr.io/devcontainers/features/node:1", "ghcr.io/acme/feature-name:20"] {
            assert!(creation_reference(good));
        }
        for bad in ["https://ghcr.io/acme/feature:1", "ghcr.io/a/f:latest", "ghcr.io/a/f", "ghcr.io/a/../f:1", "ghcr.io/a/-f:1", "ghcr.io/a/f..x:1", "ghcr.io/a/f:1\n", "ghcr.io/A/f:1"] {
            assert!(!creation_reference(bad), "{bad}");
        }
        let host = MockHost::install().with_capabilities(&["storage"]);
        let mut raw = "ghcr.io/devcontainers/features/node:1\tOne\rName\nghcr.io/devcontainers/features/node:1\tDuplicate\nmalformed\n".to_owned();
        for n in 0..256 { raw.push_str(&format!("ghcr.io/acme/feature{n}:1\t{}\n", "😀".repeat(256))); }
        storage::set(CACHE_KEY, &raw).unwrap();
        let choices = creation_candidates();
        assert_eq!(choices.len(), 128);
        assert!(choices.iter().all(|(_, name)| name.chars().count() <= 128 && !name.chars().any(char::is_control)));
        assert_eq!(choices.iter().filter(|(reference, _)| reference.ends_with("/node:1")).count(), 1);
        storage::set(CACHE_KEY, &"x".repeat(MAX_CACHE_BYTES + 1)).unwrap();
        assert!(creation_candidates().is_empty());
        let _host = host.with_capabilities(&[]);
        assert!(creation_candidates().is_empty());
    }
}

pub fn refresh() -> Result<Option<String>, Error> {
    let response = host::fetch(&HttpRequest::get(INDEX_URL))?;
    if response.status != 200 {
        return Err(Error::Failed(format!("containers.dev answered {}.", response.status)));
    }
    let lines = parse_index(&response.text()).ok_or("The containers.dev index has an unexpected shape.")?;
    storage::set(CACHE_KEY, &lines)?;
    // Optional metadata has an exact index-text binding. A failed second write
    // cannot make a different index consume stale descriptors.
    let metadata = crate::feature_options::cache_from_index(&response.text(), &lines).unwrap_or_else(|| "{}".into());
    if let Err(error) = storage::set(crate::feature_options::CACHE_KEY, &metadata) {
        host::notify(NotificationLevel::Warning, "Dev Container feature options", &format!("Feature names were refreshed, but optional cached option metadata could not be stored: {error}"));
    }
    host::notify(NotificationLevel::Info, "Dev Container", &format!("Loaded {} features.", lines.lines().count()));

    Ok(None)
}
