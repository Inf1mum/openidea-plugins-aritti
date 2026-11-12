const MAX_CHARS: usize = 120;
const MASK: &str = "••••••";
const SECRET_HINTS: [&str; 11] = ["SECRET", "PASSWORD", "PASSWD", "PASS", "TOKEN", "KEY", "CREDENTIAL", "AUTH", "PRIVATE", "SALT", "DSN"];

/// Display form of a dotenv value: unquoted, comment stripped, secrets and URL credentials masked, bounded.
/// Only this masked form may be stored or shown.
pub fn display_value(name: &str, raw: &str) -> String {
    let value = unquote(raw.trim());
    if value.is_empty() {
        return value;
    }

    mask_value(name, &value)
}

/// Masks an already unquoted value by name hints and URL credentials, and bounds its length.
pub fn mask_value(name: &str, value: &str) -> String {
    if value.is_empty() {
        return String::new();
    }

    let upper = name.to_ascii_uppercase();
    if SECRET_HINTS.iter().any(|hint| upper.contains(hint)) {
        return MASK.to_owned();
    }

    let value = mask_url_userinfo(value);
    match value.char_indices().nth(MAX_CHARS) {
        Some((end, _)) => format!("{}…", &value[..end]),
        None => value,
    }
}

fn unquote(raw: &str) -> String {
    let Some(quote) = raw.chars().next().filter(|c| *c == '"' || *c == '\'') else {
        return match raw.find(" #") {
            Some(at) => raw[..at].trim_end().to_owned(),
            None => raw.to_owned(),
        };
    };
    let rest = &raw[1..];
    rest.find(quote).map_or(rest, |end| &rest[..end]).to_owned()
}

fn mask_url_userinfo(value: &str) -> String {
    let Some(scheme_end) = value.find("://") else { return value.to_owned() };
    let authority_start = scheme_end + 3;
    let authority_end = value[authority_start..].find(['/', '?', '#']).map_or(value.len(), |at| authority_start + at);
    let Some(at) = value[authority_start..authority_end].rfind('@') else { return value.to_owned() };
    format!("{}***{}", &value[..authority_start], &value[authority_start + at..])
}

#[cfg(test)]
mod tests {
    use super::display_value;

    #[test]
    fn shows_plain_values_unquoted_without_comments() {
        assert_eq!(display_value("APP_ENV", " dev "), "dev");
        assert_eq!(display_value("APP_NAME", "\"My App\" # note"), "My App");
        assert_eq!(display_value("APP_DEBUG", "true # on"), "true");
        assert_eq!(display_value("EMPTY", ""), "");
    }

    #[test]
    fn masks_secret_names_and_url_credentials() {
        assert_eq!(display_value("APP_SECRET", "abc"), "••••••");
        assert_eq!(display_value("MAILER_DSN", "smtp://u:p@host"), "••••••");
        assert_eq!(display_value("DATABASE_URL", "postgresql://app:hunter2@db:5432/app"), "postgresql://***@db:5432/app");
        assert_eq!(display_value("DEFAULT_URI", "http://localhost"), "http://localhost");
    }

    #[test]
    fn bounds_long_values() {
        assert_eq!(display_value("LONG", &"x".repeat(500)).chars().count(), 121);
    }
}
