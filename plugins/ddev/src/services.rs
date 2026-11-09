use crate::{Main, PROJECT, VIEW, model, runtime};
use openidea_plugin::details::{Details, Pane, Tab};
use openidea_plugin::views::{Body, Button, TreeItem, View, ViewEvent, Viewport, Views};
use openidea_plugin::{Error, Invocation, Plugin, host};

impl Views for Main {
    fn render(
        &mut self,
        view: &str,
        _viewport: Option<Viewport>,
        ctx: &Invocation,
    ) -> Result<View, Error> {
        if view == crate::support::SETUP {
            return self.render_setup(ctx);
        }
        if view != VIEW {
            return Err(Error::NotFound(view.into()));
        }
        let Some(project) = ctx.project() else {
            return Ok(View::message("Open a project to use DDEV."));
        };
        let configured = project.exists(crate::CONFIG)?;
        let result = self.description(&project).cloned();
        let can_start = self.can_start(&project);
        let selected = self.selected.as_deref().unwrap_or(PROJECT);
        let desc = result.as_ref().ok();
        let state = desc.map_or(
            if configured {
                "Runtime unavailable"
            } else {
                "Not configured"
            },
            |d| &d.status,
        );
        let name = desc.map_or("DDEV", |d| &d.name);
        if desc.is_none() {
            runtime::publish(None, configured);
            let _ = host::set_context("can-start", can_start);
            let _ = host::set_context("available", can_start);
            let _ = host::set_context("can-restart-stop", can_start);
        }
        // Inherit the provider SVG; row icon paths are not resolved by Services.
        let mut items = vec![TreeItem::new(PROJECT, name).detail(state).expandable(true)];
        if let Some(desc) = desc {
            for service in &desc.services {
                // Keep runtime state visible beside URL-bearing services too.
                let status = model::text(&service.info, "status");
                let status = if status.is_empty() { state } else { status };
                let detail = service
                    .url
                    .as_ref()
                    .map_or_else(|| status.into(), |url| format!("{status} — {url}"));
                items.push(
                    TreeItem::new(&format!("service:{}", service.name), &service.name)
                        .parent(PROJECT)
                        .icon(if service.name == "db" {
                            "database"
                        } else if !service.logs {
                            "external-link"
                        } else {
                            "run"
                        })
                        .detail(&detail),
                );
            }
        }
        let service = desc.and_then(|d| d.service(selected));
        let root = selected == PROJECT;
        let has_url = desc.is_some_and(|d| {
            if root {
                d.url.is_some()
            } else {
                service.is_some_and(|s| s.url.is_some())
            }
        });
        let running = desc.is_some_and(|d| d.status == "running");
        let container = root || service.is_some_and(|s| s.logs);
        let status = match &result {
            Ok(d) => format!("{} — {}", d.name, d.status),
            Err(e) => format!("{state}: {e}. Configuration and Help remain available."),
        };
        Ok(View::new(Body::Tree(items))
            .button(Button::new("refresh", "Refresh").icon("refresh"))
            .button(Button::new("start", "Start").icon("run").enabled(can_start))
            .button(
                Button::new("stop", "Stop")
                    .icon("stop")
                    .enabled(running || desc.is_none() && can_start),
            )
            .button(Button::new("shell", "Shell").enabled(running && container))
            .button(Button::new("logs", "Logs").enabled(desc.is_some() && container))
            .button(Button::new("open-service", "Browser").enabled(running && has_url))
            .button(Button::new("actions", "Actions…").icon("settings"))
            .button(
                Button::new(
                    if configured { "open-config" } else { "config" },
                    "Configuration",
                )
                .icon("settings"),
            )
            .status(&status))
    }

    fn handle(&mut self, view: &str, event: ViewEvent, ctx: &Invocation) -> Result<(), Error> {
        if view == crate::support::SETUP {
            return self.handle_setup(event, ctx);
        }
        if view != VIEW {
            return Err(Error::NotFound(view.into()));
        }
        if let Some(project) = ctx.project() {
            // Button events can precede a render after a project switch.
            self.sync_selection(&project)?;
        }
        match event {
            ViewEvent::Select(key) => {
                let project = ctx.project().ok_or("Open a project to use DDEV.")?;
                if key != PROJECT {
                    self.target(&project, Some(&key))?;
                }
                self.selected = Some(key.clone());
                let _ = host::storage::set(&format!("selection:{}", project.root()?), &key);
            }
            ViewEvent::Activate(key) => {
                self.run_command("open-service", ctx, Some(&key))?;
            }
            ViewEvent::Button(id) => {
                let key = self.selected.clone().unwrap_or(PROJECT.into());
                self.run_command(&id, ctx, Some(&key))?;
            }
            _ => {}
        }
        Ok(())
    }
}

impl Details for Main {
    fn detail(&mut self, view: &str, key: &str, ctx: &Invocation) -> Result<Vec<Tab>, Error> {
        if view != VIEW {
            return Err(Error::NotFound(view.into()));
        }
        let project = ctx.project().ok_or("Open a project to use DDEV.")?;
        let result = self.description(&project).cloned();
        if key != PROJECT {
            let desc = result.as_ref().map_err(|e| Error::from(e.to_string()))?;
            if desc.service(key).is_none() {
                return Err(Error::NotFound(format!("DDEV service {key}")));
            }
        }
        self.selected = Some(key.to_owned());
        let _ = host::storage::set(&format!("selection:{}", project.root()?), key);
        let mut tabs = vec![];
        match result {
            Ok(desc) => {
                let overview = if key == PROJECT {
                    let mut lines = vec![format!("Project: {}", desc.name), format!("Status: {}", desc.status), format!("Folder: {}", project.root()?)];
                    if let Some(url) = &desc.url { lines.push(format!("Website: {url}")); }
                    for (label, field) in [("PHP", "php_version"), ("Node.js", "nodejs_version"), ("Web server", "webserver_type"), ("Docroot", "docroot")] {
                        let value = model::text(&desc.raw, field);
                        if !value.is_empty() { lines.push(format!("{label}: {value}")); }
                    }
                    lines.push(format!("Services: {}", desc.services.len()));
                    lines.push("\nSelect a service for its browser URL, shell or logs.\nActions…: project controls, Database setup and container tools.\nGuided Setup: preferences and execution target.".into());
                    lines.join("\n")
                } else {
                    let service = desc.service(key).unwrap();
                    format!("Service: {}\nStatus: {}\nBrowser: {}\n\n{}", service.name, model::text(&service.info, "status"), service.url.as_deref().unwrap_or("No browser URL"), if service.logs { "Shell and Logs operate on this container." } else { "This is a browser link; it has no separate container shell or logs." })
                };
                tabs.push(Tab::new("overview", "Overview", Pane::Text(overview)));
                let (info, logs) = if key == PROJECT {(model::pretty(&desc.raw),true)} else {
                    let service = desc.service(key).unwrap(); (model::pretty(&service.info),service.logs)
                };
                tabs.push(Tab::new("inspect","Inspect",Pane::Text(info)));
                if logs { tabs.push(Tab::new("logs","Logs",self.log(&project,key)?)); }
                if (key == PROJECT || key == "service:db")
                    && let Ok(summary) = crate::database::summary(&desc)
                {
                    tabs.push(Tab::new("database", "Database", Pane::Text(summary)));
                }
            }
            Err(e) => tabs.push(Tab::new("overview","Overview",Pane::Text(format!("DDEV runtime is unavailable: {e}\n\nAllow process:ddev in Settings > Plugins and check Docker. Configuration can still be read and edited.")))),
        }
        if key == PROJECT {
            let config = match project.read_file(crate::CONFIG) {
                Ok(text) => Pane::Text(text),
                Err(Error::NotFound(_)) => {
                    Pane::Message("No .ddev/config.yaml. Run DDEV: Configure Project.".into())
                }
                Err(e) => Pane::Message(format!("Cannot read configuration: {e}")),
            };
            tabs.push(Tab::new("configuration", "Configuration", config));
            tabs.push(Tab::new("help", "Help", Pane::Text(runtime::HELP.into())));
        }
        Ok(tabs)
    }
}
