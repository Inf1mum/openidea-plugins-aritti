//! Public staticStrings location-map form. Parsing never opens destination files.
use super::*;
pub(super) fn locations(
    source: &str,
    value: &Value,
    names: &mut Names,
    issues: &mut Vec<String>,
    inputs: &mut BTreeSet<String>,
) {
    let Some(entries) = value.as_object() else {
        issues.push(format!("{source}: stringsWithLocation must be an object"));
        return;
    };
    for (name, value) in entries {
        if name.chars().any(char::is_control) {
            issues.push(format!("{source}: invalid stringsWithLocation key"));
            continue;
        }
        let (path, line) = if value.is_null() {
            (String::new(), 0)
        } else if let Some(location) = value.as_str() {
            let (path, line) = if let Some((file, line)) = location.rsplit_once(':') {
                let Some(line) = line
                    .parse::<u32>()
                    .ok()
                    .filter(|line| *line > 0 && *line <= 1_000_000)
                else {
                    issues.push(format!(
                        "{source}: invalid stringsWithLocation line for {name}"
                    ));
                    continue;
                };
                (file, line - 1)
            } else {
                (location, 0)
            };
            let Some(path) = resolve_path(source, path) else {
                issues.push(format!(
                    "{source}: invalid stringsWithLocation path for {name}"
                ));
                continue;
            };
            inputs.insert(path.clone());
            (path, line)
        } else {
            issues.push(format!(
                "{source}: stringsWithLocation {name} must be a path or null"
            ));
            continue;
        };
        names.remove(name);
        names.add(
            name,
            Some("ide.json stringsWithLocation".into()),
            &path,
            Position::new(line, 0),
            0,
        );
    }
}
