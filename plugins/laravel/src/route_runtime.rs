//! Explicit Artisan route capture. Views and editor providers never start PHP.
use crate::{artisan_metadata, index::Index};
use openidea_plugin::{Error, Invocation, Location, Position, Project, Range, host};
use serde_json::{Value, json};
use ui_kit::routes::{Handler, Route};

pub const ARGS: &[&str] = &[
    "artisan",
    "route:list",
    "--json",
    "--no-interaction",
    "--no-ansi",
];
const MAX_OUTPUT: usize = 4 * 1024 * 1024;
pub struct Snapshot {
    pub routes: Vec<Route>,
    pub captured: u64,
}
impl Snapshot {
    pub fn provenance(&self) -> String {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(self.captured, |d| d.as_secs());
        let seconds = now.saturating_sub(self.captured);
        if seconds < 60 {
            "Artisan snapshot · refreshed just now".into()
        } else if seconds < 3600 {
            format!("Artisan snapshot · refreshed {} min ago", seconds / 60)
        } else {
            format!("Artisan snapshot · refreshed {} h ago", seconds / 3600)
        }
    }
}
fn stamp(project: &Project) -> Result<Value, Error> {
    let mut identity = artisan_metadata::identity(project)?;
    identity["argv"] = json!(ARGS);
    let root = project.root()?;
    if !host::projects()?
        .iter()
        .any(|p| p.root().ok().as_deref() == Some(&root))
    {
        return Err(Error::Stale);
    }
    let mut paths = vec![];
    for glob in [
        "routes/**/*.php",
        "config/**/*.php",
        "app/Providers/**/*.php",
        "bootstrap/**/*.php",
    ] {
        let found = project.find_files(glob, 1024)?;
        if found.len() >= 1024 {
            return Err(Error::Invalid("Too many route capture inputs.".into()));
        }
        paths.extend(found);
    }
    for name in [".env", "app/Http/routes.php", "app/routes.php"] {
        let path = format!("{root}/{name}");
        if project.exists(&path)? {
            paths.push(path);
        }
    }
    paths.sort();
    paths.dedup();
    let mut total = 0;
    let mut hashes = vec![];
    for path in paths {
        if !crate::owns_current_path(project, &path) {
            return Err(Error::Stale);
        }
        let text = project.read_file(&path)?;
        total += text.len();
        if total > 8 * 1024 * 1024 {
            return Err(Error::Invalid("Route capture inputs exceed 8 MiB.".into()));
        }
        hashes.push(json!([path, artisan_metadata::hash(&text)]));
    }
    identity["sources"] = json!(hashes);
    Ok(identity)
}
fn field(value: &Value, key: &str, max: usize) -> Result<String, Error> {
    let s = value[key]
        .as_str()
        .ok_or_else(|| Error::Invalid(format!("Route JSON is missing {key}.")))?;
    if s.len() > max || s.chars().any(char::is_control) {
        return Err(Error::Invalid(format!("Route {key} exceeds text limits.")));
    }
    Ok(s.into())
}
pub fn parse(raw: &str, index: &Index) -> Result<Vec<Route>, Error> {
    let data = crate::strict_json::parse_with_budget(raw, MAX_OUTPUT, 200_000).map_err(|_| {
        Error::Invalid("Artisan route:list did not return bounded route JSON.".into())
    })?;
    let entries = data
        .as_array()
        .filter(|a| a.len() <= 5000)
        .ok_or("Expected an Artisan JSON array with at most 5,000 routes.")?;
    let mut routes = vec![];
    for entry in entries {
        let methods = field(entry, "method", 128)?;
        if methods.is_empty()
            || methods.split('|').any(|m| {
                ![
                    "GET", "HEAD", "POST", "PUT", "PATCH", "DELETE", "OPTIONS", "TRACE", "CONNECT",
                ]
                .contains(&m)
            })
        {
            return Err(Error::Invalid("Unsupported route HTTP method.".into()));
        }
        let uri = field(entry, "uri", 4096)?;
        let path = format!("/{}", uri.trim_start_matches('/'));
        let action = field(entry, "action", 2048)?;
        let namespace = host::setting_string("controllersNamespace")
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "App\\Http\\Controllers".into());
        let class_label = |class: &str| {
            class
                .strip_prefix(&format!("{namespace}\\"))
                .unwrap_or(class)
                .to_owned()
        };
        let (class, member) = if let Some((class, method)) = action.rsplit_once('@') {
            (class_label(class), Some(method.to_owned()))
        } else if action == "Closure" {
            ("Closure".into(), None)
        } else {
            (class_label(&action), Some("__invoke".into()))
        };
        let name = if entry["name"].is_null() {
            None
        } else {
            Some(field(entry, "name", 1024)?)
        };
        // Only cached, owned source locations are used; JSON paths are never trusted.
        let exact: Vec<_> = index
            .route_defs
            .iter()
            .filter(|r| {
                r.endpoint_uri.as_deref().unwrap_or(&r.def.uri) == path
                    && r.def.action == action
                    && methods
                        .split('|')
                        .any(|m| r.def.methods.split('|').any(|known| known == m))
            })
            .collect();
        let location = if exact.len() == 1 {
            let e = exact[0];
            Some(Location {
                path: e.path.clone(),
                range: Range::new(e.at, Position::new(e.at.line, e.at.character + e.width)),
            })
        } else {
            member
                .as_deref()
                .and_then(|method| index.methods.get(&class)?.get(method))
                .map(|e| Location {
                    path: e.path.clone(),
                    range: Range::new(e.at, Position::new(e.at.line, e.at.character + e.width)),
                })
        };
        let method = methods
            .split('|')
            .filter(|m| !(*m == "HEAD" && methods.split('|').any(|v| v == "GET")))
            .collect::<Vec<_>>()
            .join("|");
        routes.push(Route {
            method,
            path,
            handler: Handler { class, member },
            name,
            location,
        });
    }
    Ok(routes)
}
pub fn capture_with(
    ctx: &Invocation,
    index: &Index,
    execute: impl FnOnce(&Project) -> Result<String, Error>,
) -> Result<Snapshot, Error> {
    let project = ctx.project().ok_or("Open a Laravel project first.")?;
    let before = stamp(&project)?;
    let output = execute(&project)?;
    if ctx.project().and_then(|p| p.root().ok()) != project.root().ok()
        || stamp(&project)? != before
    {
        return Err(Error::Stale);
    }
    let routes = parse(&output, index)?;
    Ok(Snapshot {
        routes,
        captured: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs()),
    })
}
pub fn capture(ctx: &Invocation, index: &Index) -> Result<Snapshot, Error> {
    capture_with(ctx, index, |project| {
        let task = host::begin_task(
            "Refresh registered Laravel routes (executes project PHP)",
            true,
        );
        let child = host::start(&host::Command::new("php").args(ARGS), Some(project))?;
        if let Err(error) = child.close_stdin() {
            let _ = child.kill();
            return Err(error);
        }
        let result =
            artisan_metadata::collect(|timeout| child.wait(timeout), || task.is_cancelled());
        if result.is_err() {
            let _ = child.kill();
        }
        result
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;
    fn setup() -> MockHost {
        let host = MockHost::install();
        host.add_file("artisan", "<?php");
        host.add_file("composer.json", "{}");
        host
    }
    fn sample() -> String {
        json!([{"method":"GET|HEAD","uri":"api/pregnancy/contractions","name":null,"action":"App\\Http\\Controllers\\Api\\ContractionController@index","middleware":["api"],"path":"/outside/untrusted.php"},{"method":"PUT|PATCH","uri":"api/pregnancy/contractions/{id}","name":"api.update","action":"App\\Http\\Controllers\\Api\\ContractionController@update"}]).to_string()
    }
    #[test]
    fn runtime_routes_preserve_real_paths_controller_names_and_all_methods() {
        let _host = setup();
        let routes = parse(&sample(), &Index::default()).unwrap();
        assert_eq!(routes.len(), 2);
        assert_eq!(routes[0].path, "/api/pregnancy/contractions");
        assert_eq!(routes[0].handler.class, "Api\\ContractionController");
        assert_eq!(routes[0].handler.member.as_deref(), Some("index"));
        assert_eq!(routes[0].method, "GET");
        assert_eq!(routes[0].location, None);
        assert_eq!(routes[1].method, "PUT|PATCH");
        assert_eq!(routes[1].name.as_deref(), Some("api.update"));
    }
    #[test]
    fn capture_refuses_changed_sources_and_malformed_json_without_running_on_read() {
        let host = setup();
        let index = Index::default();
        let ctx = host.invocation();
        let snapshot = capture_with(&ctx, &index, |project| {
            assert_eq!(project.root().unwrap(), "/project");
            Ok(sample())
        })
        .unwrap();
        assert_eq!(snapshot.routes.len(), 2);
        assert!(matches!(
            capture_with(&ctx, &index, |_| {
                host.add_file("routes/web.php", "<?php changed");
                Ok(sample())
            }),
            Err(Error::Stale)
        ));
        assert!(capture_with(&ctx, &index, |_| Ok("not JSON".into())).is_err());
        assert!(host.process_runs().is_empty());
    }
    #[test]
    fn denied_php_and_invalid_methods_are_errors() {
        let host = MockHost::install().with_capabilities(&["project.read"]);
        host.add_file("artisan", "<?php");
        assert!(matches!(
            capture(&host.invocation(), &Index::default()),
            Err(Error::Denied(_))
        ));
        assert!(parse(&sample().replace("GET|HEAD", "SHELL"), &Index::default()).is_err());
    }
    #[test]
    #[ignore = "requires LARAVEL_ROUTE_LIST_FIXTURE captured with artisan route:list --json"]
    fn captured_runtime_route_list_is_complete() {
        let _host = setup();
        let raw =
            std::fs::read_to_string(std::env::var("LARAVEL_ROUTE_LIST_FIXTURE").unwrap()).unwrap();
        let expected = serde_json::from_str::<Value>(&raw)
            .unwrap()
            .as_array()
            .unwrap()
            .len();
        let routes = parse(&raw, &Index::default()).unwrap();
        assert_eq!(routes.len(), expected);
        assert!(
            !routes
                .iter()
                .any(|r| r.path == "/?" || r.handler.class.contains("$controller"))
        );
        println!(
            "Parsed {expected} registered runtime routes without placeholder paths or variable controller labels"
        );
    }
}
