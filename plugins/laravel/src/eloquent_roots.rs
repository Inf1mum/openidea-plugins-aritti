//! Literal Composer PSR-4 roots only; no executable Composer map evaluation.
#[derive(Clone, Debug)]
pub(super) struct Root {
    pub prefix: String,
    pub directory: String,
}
pub(super) fn parse(text: &str) -> Result<Vec<Root>, ()> {
    let value = crate::strict_json::parse(text, 512 * 1024).map_err(|_| ())?;
    let mut roots = vec![];
    for section in ["autoload", "autoload-dev"] {
        let Some(map) = value.get(section).and_then(|section| section.get("psr-4")) else {
            continue;
        };
        let map = map.as_object().ok_or(())?;
        if map.len() > 64 {
            return Err(());
        }
        for (prefix, paths) in map {
            if prefix.is_empty()
                || !prefix.ends_with('\\')
                || prefix.len() > 256
                || !prefix.trim_end_matches('\\').split('\\').all(identifier)
            {
                return Err(());
            }
            let paths = if let Some(path) = paths.as_str() {
                vec![path]
            } else {
                paths
                    .as_array()
                    .filter(|paths| paths.len() <= 8)
                    .ok_or(())?
                    .iter()
                    .map(|path| path.as_str().ok_or(()))
                    .collect::<Result<Vec<_>, _>>()?
            };
            for path in paths {
                let path = path
                    .strip_prefix("./")
                    .unwrap_or(path)
                    .trim_end_matches('/');
                if path.is_empty()
                    || path.len() > 1024
                    || path.starts_with('/')
                    || path.contains('\\')
                    || path.chars().any(|c| {
                        c.is_control() || matches!(c, ':' | '*' | '?' | '[' | ']' | '{' | '}')
                    })
                    || path.split('/').any(|part| {
                        part.is_empty()
                            || matches!(
                                part,
                                "." | ".." | "vendor" | "node_modules" | ".git" | ".openidea"
                            )
                    })
                {
                    return Err(());
                }
                if !roots
                    .iter()
                    .any(|root: &Root| root.prefix == *prefix && root.directory == path)
                {
                    roots.push(Root {
                        prefix: prefix.clone(),
                        directory: path.into(),
                    });
                }
                if roots.len() > 32 {
                    return Err(());
                }
            }
        }
    }
    Ok(roots)
}
fn identifier(value: &str) -> bool {
    let mut chars = value.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}
pub(super) fn matches(roots: &[Root], relative: &str, full: &str) -> bool {
    roots.iter().any(|root| {
        relative
            .strip_prefix(&format!("{}/", root.directory))
            .and_then(|path| path.strip_suffix(".php"))
            .is_some_and(|path| format!("{}{}", root.prefix, path.replace('/', "\\")) == full)
    })
}
