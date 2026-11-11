//! Application metadata and explicitly captured Artisan information. Never runs PHP.
use openidea_plugin::{Error, Project, host, views::Body};
use serde_json::Value;
use std::collections::BTreeMap;

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
fn read(project: &Project, name: &str, limit: usize) -> Option<Value> {
    let root = project.root().ok()?;
    let path = format!("{root}/{name}");
    if !crate::owns_current_path(project, &path) {
        return None;
    }
    let source = project.read_file(name).ok()?;
    crate::strict_json::parse(&source, limit).ok()
}
fn value(value: &Value) -> Option<String> {
    value
        .as_str()
        .filter(|text| text.len() <= 4096 && !text.chars().any(|c| c.is_control() && c != '\n'))
        .map(str::to_owned)
}
fn group(title: &str, entries: Vec<(&str, String)>) -> String {
    if entries.is_empty() {
        return String::new();
    }
    let fields = entries
        .into_iter()
        .map(|(key, value)| {
            format!(
                "<dt>{}</dt><dd>{}</dd>",
                escape(&label(key)),
                escape(&value)
            )
        })
        .collect::<String>();
    format!(
        "<section class=\"about-section\"><h3 class=\"ui-group-header\">{}</h3><dl>{fields}</dl></section>",
        escape(&label(title))
    )
}
pub(crate) fn body(project: &Project, cached: Option<&Value>) -> Result<Body, Error> {
    let root = project.root()?;
    if host::project_of(&format!("{root}/composer.json"))
        .and_then(|owner| owner.root().ok())
        .as_deref()
        != Some(root.as_str())
    {
        return Err(Error::Stale);
    }
    let composer = read(project, "composer.json", 128 << 10).unwrap_or(Value::Null);
    let mut runtime = BTreeMap::new();
    for snapshot in cached
        .into_iter()
        .flat_map(|cached| cached["snapshots"].as_array().into_iter().flatten())
        .filter(|snapshot| snapshot["section"] == "about")
    {
        for row in snapshot["rows"].as_array().into_iter().flatten() {
            if let (Some(key), Some(text)) = (row["cells"][1].as_str(), value(&row["cells"][2])) {
                runtime.insert(key.to_owned(), text);
            }
        }
    }
    let title = runtime
        .get("environment.application_name")
        .cloned()
        .or_else(|| value(&composer["name"]))
        .unwrap_or_else(|| {
            crate::ui_text::t("final.about.applicationFallback", "Laravel application")
        });
    let description = value(&composer["description"]).unwrap_or_default();
    let mut sections = group(
        "Application",
        [
            Some(("Project", root)),
            value(&composer["name"]).map(|name| ("Package", name)),
            value(&composer["version"]).map(|version| ("Application version", version)),
        ]
        .into_iter()
        .flatten()
        .collect(),
    );
    let mut versions = Vec::new();
    if let Some(php) = runtime.get("environment.php_version") {
        versions.push(("PHP", php.clone()));
    }
    if let Some(framework) = runtime.get("environment.laravel_version") {
        versions.push(("Laravel", framework.clone()));
    }
    if let Some(version) = runtime.get("environment.composer_version") {
        versions.push(("Composer", version.clone()));
    }
    sections.push_str(&group("Runtime versions", versions));
    let mut versions = Vec::new();
    let lock = read(project, "composer.lock", 2 << 20).unwrap_or(Value::Null);
    for (package, label) in [
        ("laravel/framework", "Laravel"),
        ("livewire/livewire", "Livewire"),
        ("inertiajs/inertia-laravel", "Inertia"),
        ("filament/filament", "Filament"),
        ("laravel/nova", "Nova"),
        ("laravel/sanctum", "Sanctum"),
        ("laravel/passport", "Passport"),
    ] {
        let matches: Vec<_> = lock["packages"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|entry| entry["name"] == package)
            .collect();
        if let [entry] = matches.as_slice() {
            if let Some(version) = value(&entry["version"]) {
                versions.push((label, version));
            }
        }
    }
    sections.push_str(&group("Dependencies (lockfile)", versions));
    let requirements = [
        ("php", "PHP requirement"),
        ("laravel/framework", "Laravel requirement"),
    ]
    .into_iter()
    .filter_map(|(name, label)| value(&composer["require"][name]).map(|version| (label, version)))
    .collect();
    sections.push_str(&group("Requirements", requirements));
    for (title, keys) in [
        (
            "Environment",
            vec![
                ("environment.environment", "Environment"),
                ("environment.debug_mode", "Debug mode"),
                ("environment.maintenance_mode", "Maintenance mode"),
                ("environment.timezone", "Timezone"),
                ("environment.locale", "Locale"),
            ],
        ),
        (
            "Cache",
            vec![
                ("cache.config", "Configuration"),
                ("cache.events", "Events"),
                ("cache.routes", "Routes"),
                ("cache.views", "Views"),
            ],
        ),
        (
            "Drivers",
            vec![
                ("drivers.cache", "Cache"),
                ("drivers.database", "Database"),
                ("drivers.queue", "Queue"),
                ("drivers.session", "Session"),
                ("drivers.mail", "Mail"),
                ("drivers.broadcasting", "Broadcasting"),
            ],
        ),
    ] {
        sections.push_str(&group(
            title,
            keys.into_iter()
                .filter_map(|(key, label)| runtime.get(key).cloned().map(|text| (label, text)))
                .collect(),
        ));
    }
    let note = if runtime.is_empty() {
        crate::ui_text::text(
            "final.about.emptyNote",
            "Refresh App Info to load the application name, PHP version, environment, cache and driver settings.",
        )
    } else {
        crate::ui_text::text(
            "final.about.capturedNote",
            "Runtime information comes from the last explicit application refresh. Refresh after environment changes.",
        )
    };
    let aria = escape(&label("Application information"));
    Ok(Body::Html(format!(
        r#"<link rel="stylesheet" href="./components.css"><style>[data-laravel-about]{{padding:var(--ui-space-3);color:var(--ui-fg);min-width:0}}[data-laravel-about] h2{{font-size:18px;margin:0 0 var(--ui-space-2)}}.about-description,.about-note{{color:var(--ui-muted);overflow-wrap:anywhere}}.about-grid{{display:grid;grid-template-columns:repeat(auto-fit,minmax(min(100%,300px),1fr));gap:var(--ui-space-4)}}.about-section{{min-width:0}}.about-section h3{{font-size:var(--ui-text);font-weight:500;margin:var(--ui-space-3) 0}}.about-section dl{{display:grid;grid-template-columns:minmax(90px,1fr) minmax(0,2fr);gap:var(--ui-space-2) var(--ui-space-3);margin:0}}.about-section dt{{color:var(--ui-muted)}}.about-section dd{{margin:0;overflow-wrap:anywhere;white-space:pre-wrap}}</style><section data-laravel-about aria-label="{aria}"><h2>{}</h2><p class="about-description">{}</p><div class="about-grid">{sections}</div><p class="about-note">{note}</p></section>"#,
        escape(&title),
        escape(&description)
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn locale_changes_only_authored_about_labels_and_keeps_project_metadata_exact() {
        let host = openidea_plugin::mock::MockHost::install();
        host.add_file(
            "composer.json",
            r#"{"name":"Application","description":"Runtime versions {root}"}"#,
        );
        let project = host::projects().unwrap().remove(0);
        for language in ["ja", "ko", "zh-CN"] {
            host.set_setting("uiLanguage", &serde_json::to_string(language).unwrap());
            let Body::Html(html) = body(&project, None).unwrap() else {
                panic!("about HTML")
            };
            assert!(html.contains("<h2>Application</h2>"));
            assert!(html.contains("Runtime versions {root}</p>"));
            assert!(!html.contains("<h3 class=\"ui-group-header\">Application</h3>"));
            assert!(!html.contains("Refresh App Info to load"));
        }
        assert!(host.requests().is_empty() && host.process_runs().is_empty());
    }
    #[test]
    fn application_metadata_is_escaped_and_does_not_read_env_or_run_php() {
        let mock = openidea_plugin::mock::MockHost::install();
        mock.add_file("composer.json", r#"{"name":"acme/shop","description":"<script>bad</script>","require":{"php":"^8.3","laravel/framework":"^12"},"password":"private"}"#);
        mock.add_file(
            "composer.lock",
            r#"{"packages":[{"name":"laravel/framework","version":"v12.3.0"}]}"#,
        );
        mock.add_file(".env", "APP_KEY=private-secret");
        let project = host::projects().unwrap().remove(0);
        let Body::Html(html) = body(&project, None).unwrap() else {
            panic!("expected about page")
        };
        assert!(
            html.contains("acme/shop")
                && html.contains("v12.3.0")
                && html.contains("PHP requirement")
        );
        assert!(html.contains("&lt;script&gt;") && !html.contains("<script>"));
        assert!(!html.contains("private") && !html.contains("APP_KEY"));
        let cached = serde_json::json!({"snapshots":[{"section":"about","rows":[{"cells":["Application","environment.application_name","Shop"]},{"cells":["Application","environment.php_version","8.4"]},{"cells":["Application","environment.environment","local"]}]},{"section":"logs","rows":[{"cells":["Logs","secret","secret-log"]}]}]});
        let Body::Html(html) = body(&project, Some(&cached)).unwrap() else {
            panic!("expected about page")
        };
        assert!(html.contains("<h2>Shop</h2>") && html.contains("8.4") && html.contains("local"));
        assert!(!html.contains("secret-log"));
        if let Ok(path) = std::env::var("LARAVEL_ABOUT_QA_HTML") {
            std::fs::write(path, &html).unwrap();
        }
        assert!(mock.process_runs().is_empty() && mock.requests().is_empty());
    }
}

// These keys originate in this module; runtime/application values are never passed here.
fn label(value: &str) -> String {
    match value {
        "Application" => crate::ui_text::t("final.about.label.application", value),
        "Project" => crate::ui_text::t("final.about.label.project", value),
        "Package" => crate::ui_text::t("final.about.label.package", value),
        "Application version" => crate::ui_text::t("final.about.label.application-version", value),
        "Runtime versions" => crate::ui_text::t("final.about.label.runtime-versions", value),
        "Dependencies (lockfile)" => {
            crate::ui_text::t("final.about.label.dependencies-lockfile", value)
        }
        "Requirements" => crate::ui_text::t("final.about.label.requirements", value),
        "PHP requirement" => crate::ui_text::t("final.about.label.php-requirement", value),
        "Laravel requirement" => crate::ui_text::t("final.about.label.laravel-requirement", value),
        "Environment" => crate::ui_text::t("final.about.label.environment", value),
        "Debug mode" => crate::ui_text::t("final.about.label.debug-mode", value),
        "Maintenance mode" => crate::ui_text::t("final.about.label.maintenance-mode", value),
        "Timezone" => crate::ui_text::t("final.about.label.timezone", value),
        "Locale" => crate::ui_text::t("final.about.label.locale", value),
        "Cache" => crate::ui_text::t("final.about.label.cache", value),
        "Configuration" => crate::ui_text::t("final.about.label.configuration", value),
        "Events" => crate::ui_text::t("final.about.label.events", value),
        "Routes" => crate::ui_text::t("final.about.label.routes", value),
        "Views" => crate::ui_text::t("final.about.label.views", value),
        "Drivers" => crate::ui_text::t("final.about.label.drivers", value),
        "Database" => crate::ui_text::t("final.about.label.database", value),
        "Queue" => crate::ui_text::t("final.about.label.queue", value),
        "Session" => crate::ui_text::t("final.about.label.session", value),
        "Mail" => crate::ui_text::t("final.about.label.mail", value),
        "Broadcasting" => crate::ui_text::t("final.about.label.broadcasting", value),
        "Application information" => {
            crate::ui_text::t("final.about.label.application-information", value)
        }
        _ => value.into(),
    }
}
