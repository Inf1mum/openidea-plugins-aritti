//! PHP 8.5 pipe's literal LHS is the first sole argument of a proven first-class callable.
use super::*;
use crate::refs::{Kind, Reference};
pub(super) fn reference(
    index: &Index,
    doc: &openidea_plugin::providers::Document,
    at: usize,
) -> Option<Reference> {
    if index
        .metadata
        .php_version
        .is_some_and(|version| version < (8, 5, 0))
    {
        return None;
    }
    let site = document_site(index, doc, at)?;
    if !site.pipe {
        return None;
    }
    let identity = &site.identity;
    let method = identity.method.to_ascii_lowercase();
    let mut list = false;
    let mut keys = false;
    let kind = if let Some(class) = identity.class.as_deref() {
        let facade = |expected: &str| {
            class.eq_ignore_ascii_case(&format!("Illuminate\\Support\\Facades\\{expected}"))
                || class.eq_ignore_ascii_case(expected)
                    && !crate::php::declares_class(&doc.text, expected)
        };
        if facade("Config") && matches!(method.as_str(), "get" | "has") {
            keys = method == "get";
            Kind::Config
        } else if facade("View") && matches!(method.as_str(), "make" | "exists") {
            Kind::View
        } else if facade("Route") && matches!(method.as_str(), "has" | "is" | "currentroutenamed") {
            list = true;
            Kind::Route
        } else if facade("Lang") && matches!(method.as_str(), "get" | "has" | "choice") {
            Kind::Translation
        } else if facade("Gate")
            && matches!(
                method.as_str(),
                "allows" | "denies" | "check" | "authorize" | "inspect" | "any" | "none"
            )
        {
            list = matches!(method.as_str(), "any" | "none");
            Kind::Ability
        } else if facade("Vite") && method == "asset" {
            Kind::Vite
        } else {
            return None;
        }
    } else {
        let helpers = [
            ("config", Kind::Config),
            ("view", Kind::View),
            ("route", Kind::Route),
            ("env", Kind::Env),
            ("asset", Kind::Asset),
            ("__", Kind::Translation),
            ("trans", Kind::Translation),
            ("trans_choice", Kind::Translation),
            ("action", Kind::Action),
            ("inertia", Kind::Inertia),
        ];
        let (name, kind) = helpers.into_iter().find(|(name, _)| {
            crate::eloquent_members::metadata_inference::global_helper(&doc.text, site.open, name)
        })?;
        keys = name == "config";
        if name == "inertia"
            && !index
                .metadata
                .package_enabled("inertiajs/inertia-laravel", true)
        {
            return None;
        }
        kind
    };
    if !site.arrays.is_empty() {
        if site.arrays.len() != 1 {
            return None;
        }
        let role = &site.arrays[0];
        if keys {
            if !role.is_key {
                return None;
            }
        } else if !list || role.is_key {
            return None;
        }
    }
    // The standard scalar provider uses source spelling as its identifier.
    // Escaped spellings stay with configured metadata, which has a decoded range map.
    if doc.text[site.literal.clone()].contains('\\') {
        return None;
    }
    Some(Reference {
        kind,
        start: site.literal.start,
        end: site.literal.end,
        class: None,
        closed: site.closed,
    })
}
