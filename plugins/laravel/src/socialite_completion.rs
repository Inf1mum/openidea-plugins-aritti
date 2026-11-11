//! Socialite provider names in `Socialite::driver('…')`.

const PROVIDERS: &[&str] = &[
    "bitbucket", "facebook", "github", "gitlab", "google", "linkedin", "linkedin-openid", "slack", "slack-openid", "twitch", "twitter", "twitter-oauth-2", "x",
];

/// The byte range of the provider name typed at `offset` inside `Socialite::driver('`.
pub fn context(text: &str, offset: usize) -> Option<(usize, usize)> {
    let line_start = text[..offset].rfind('\n').map_or(0, |i| i + 1);
    let before = &text[line_start..offset];
    let quote = before.rfind(['\'', '"'])?;
    let typed = &before[quote + 1..];
    if typed.contains(['\'', '"', ')', ',']) || !before[..quote].trim_end().ends_with("Socialite::driver(") {
        return None;
    }

    Some((line_start + quote + 1, offset))
}

pub fn providers() -> &'static [&'static str] {
    PROVIDERS
}

#[cfg(test)]
mod tests {
    #[test]
    fn only_the_socialite_driver_argument_completes() {
        let at = |text: &str| super::context(text, text.len()).is_some();
        assert!(at("return Socialite::driver('gi"));
        assert!(!at("return Cache::driver('gi"));
        assert!(!at("return Socialite::driver('github')->scopes('a"));
    }
}
