//! One standard "allow this permission" message for denied capabilities.
use openidea_plugin::{Error, NotificationLevel, host};
use std::sync::atomic::{AtomicBool, Ordering};

pub fn message(feature: &str, capability: &str, plugin: &str) -> String {
    format!(
        "{feature} needs the {capability} permission. Allow it in Settings › Plugins › {plugin}."
    )
}

pub fn map(err: Error, feature: &str, capability: &str, plugin: &str) -> Error {
    match err {
        Error::Denied(_) => Error::Invalid(message(feature, capability, plugin)),
        other => other,
    }
}

pub fn notify_once(flag: &AtomicBool, feature: &str, capability: &str, plugin: &str) {
    if flag.swap(true, Ordering::Relaxed) {
        return;
    }
    host::notify(NotificationLevel::Info, plugin, &message(feature, capability, plugin));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_names_capability_and_settings_page() {
        assert_eq!(
            message("Vulnerability lookup", "net:api.osv.dev", "Dependency Audit"),
            "Vulnerability lookup needs the net:api.osv.dev permission. Allow it in Settings › Plugins › Dependency Audit."
        );
    }

    #[test]
    fn map_rewrites_only_denied() {
        assert!(matches!(map(Error::Denied("x".into()), "F", "clipboard", "P"), Error::Invalid(m) if m.contains("clipboard")));
        assert!(matches!(map(Error::Stale, "F", "clipboard", "P"), Error::Stale));
    }

    #[test]
    fn notify_once_notifies_a_single_time() {
        let host = openidea_plugin::mock::MockHost::install();
        let flag = AtomicBool::new(false);
        notify_once(&flag, "Live checks", "events", "Spring Data Tools");
        notify_once(&flag, "Live checks", "events", "Spring Data Tools");
        assert_eq!(host.notifications().len(), 1);
    }
}
