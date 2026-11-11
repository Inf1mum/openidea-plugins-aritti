//! Code that breaks on a Laravel major the project has not reached yet.

use openidea_plugin::Severity;

use crate::eloquent_checks::Finding;
use crate::php;

struct Change {
    needle: &'static str,
    removed_in: u32,
    replacement: Option<&'static str>,
    message: &'static str,
}

const CHANGES: &[Change] = &[
    Change { needle: "dispatch_now(", removed_in: 9, replacement: Some("dispatch_sync("), message: "dispatch_now() is removed in Laravel 9: use dispatch_sync()." },
    Change { needle: "Bus::dispatchNow(", removed_in: 9, replacement: Some("Bus::dispatchSync("), message: "Bus::dispatchNow() is removed in Laravel 9: use dispatchSync()." },
    Change { needle: "->assertDeleted(", removed_in: 9, replacement: Some("->assertModelMissing("), message: "assertDeleted() is removed in Laravel 9: use assertModelMissing()." },
    Change { needle: "$routeMiddleware", removed_in: 11, replacement: Some("$middlewareAliases"), message: "$routeMiddleware is renamed $middlewareAliases (the HTTP kernel is gone in Laravel 11)." },
    Change { needle: "protected $dates", removed_in: 11, replacement: None, message: "The $dates property is removed in Laravel 11: cast the columns to datetime in $casts." },
    Change { needle: "HasUuids", removed_in: 12, replacement: None, message: "HasUuids generates ordered UUIDv7 in Laravel 12: use HasVersion4Uuids to keep UUIDv4." },
];

/// `installed` is the major of `vendor/laravel/framework`; nothing is reported when it is unknown.
pub fn findings(installed: Option<u32>, language: &str, text: &str) -> Vec<Finding> {
    let Some(installed) = installed else {
        return Vec::new();
    };
    if language != "php" || text.len() > 128 * 1024 {
        return Vec::new();
    }

    let masked = php::mask_comments(text);
    let mut found = Vec::new();

    for change in CHANGES.iter().filter(|change| change.removed_in > installed) {
        for (start, _) in masked.match_indices(change.needle) {
            found.push(Finding {
                start,
                end: start + change.needle.len(),
                code: "laravel-upgrade",
                message: change.message.into(),
                severity: Severity::Info,
                replacement: change.replacement.map(str::to_owned),
            });
        }
    }

    found
}

#[cfg(test)]
mod tests {
    #[test]
    fn only_changes_ahead_of_the_installed_major_are_reported() {
        let source = "<?php Bus::dispatchNow($job); $this->assertDeleted($m);";
        assert_eq!(super::findings(Some(8), "php", source).len(), 2);
        assert!(super::findings(Some(9), "php", source).is_empty());
        assert!(super::findings(None, "php", source).is_empty());
        assert!(super::findings(Some(8), "php", "<?php // dispatch_now($job);").is_empty());
    }

    #[test]
    fn a_rename_carries_its_replacement() {
        let found = super::findings(Some(8), "php", "<?php dispatch_now($job);");
        assert_eq!(found[0].replacement.as_deref(), Some("dispatch_sync("));
    }
}
