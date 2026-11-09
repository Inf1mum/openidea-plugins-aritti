//! Packaged resource views backed by the existing typed view/command handlers.
//! Native custom editors continue to use their document/undo integration.
use std::cell::Cell;

use openidea_plugin::views::{Body, FieldValue, ViewEvent, Viewport, Views};
use openidea_plugin::{Error, Invocation, Plugin, host};
use serde::Deserialize;
use serde_json::{Value, json};

thread_local! { static RENDERING: Cell<bool> = const { Cell::new(false) }; }

pub fn is_rendering() -> bool {
    RENDERING.with(Cell::get)
}

struct Rendering(bool);
impl Rendering {
    fn enter() -> Self {
        Self(RENDERING.with(|v| v.replace(true)))
    }
}
impl Drop for Rendering {
    fn drop(&mut self) {
        RENDERING.with(|v| v.set(self.0));
    }
}

#[derive(Deserialize)]
struct Config {
    title: String,
    views: std::collections::BTreeMap<String, String>,
    actions: Vec<Action>,
    #[serde(default)]
    project_independent: bool,
    #[serde(default)]
    project_bound_buttons: Vec<String>,
}

#[derive(Deserialize)]
struct Action {
    view: String,
    id: String,
    label: String,
    kind: String,
}

#[derive(Deserialize)]
struct Request {
    channel: String,
    sequence: u64,
    #[serde(default, rename = "frameId")]
    frame_id: Option<String>,
    project: Option<String>,
    #[serde(flatten)]
    operation: Operation,
}

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "kebab-case")]
enum Operation {
    Render,
    Event { event: Event },
    Command { id: String, key: String },
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
enum Event {
    Button { key: String },
    Select { key: String },
    Activate { key: String },
    Expand { key: String },
    Collapse { key: String },
    Field { key: String, value: String },
    Cell { row: u32, column: u32, value: String },
    TableSelect { row: u32 },
    TableActivate { row: u32 },
    Sort { column: u32, descending: bool },
    Message { value: String },
}

impl From<Event> for ViewEvent {
    fn from(event: Event) -> Self {
        match event {
            Event::Button { key } => Self::Button(key),
            Event::Select { key } => Self::Select(key),
            Event::Activate { key } => Self::Activate(key),
            Event::Expand { key } => Self::Expand(key),
            Event::Collapse { key } => Self::Collapse(key),
            Event::Field { key, value } => Self::Field(key, value),
            Event::Cell { row, column, value } => Self::Cell { row, column, value },
            Event::TableSelect { row } => Self::Other { kind: "table-select".into(), data: json!({"row":row}).to_string() },
            Event::TableActivate { row } => Self::Other { kind: "table-activate".into(), data: json!({"row":row}).to_string() },
            Event::Sort { column, descending } => {
                Self::Other { kind: "sort".into(), data: json!({"column":column,"descending":descending}).to_string() }
            }
            Event::Message { value } => Self::Message(value),
        }
    }
}

fn field(value: FieldValue) -> Value {
    match value {
        FieldValue::Text(text) => json!({"kind":"text","text":text}),
        FieldValue::Multiline(text) => json!({"kind":"multiline","text":text}),
        FieldValue::Checkbox(checked) => json!({"kind":"checkbox","checked":checked}),
        FieldValue::Choice(options, text) => json!({"kind":"choice","options":options,"text":text}),
        FieldValue::Other { kind, data } => {
            json!({"kind":kind,"data":serde_json::from_str::<Value>(&data).unwrap_or(Value::Null)})
        }
        _ => json!({"kind":"unsupported"}),
    }
}

fn body(body: Body) -> Value {
    match body {
        Body::Message(text) => json!({"kind":"message","text":text}),
        Body::Html(html) => json!({"kind":"html","html":html}),
        Body::Tree(items) => {
            json!({"kind":"tree","items":items.into_iter().map(|r|json!({"key":r.key,"parent":r.parent,"label":r.label,"detail":r.detail,"expandable":r.expandable,"expanded":r.expanded})).collect::<Vec<_>>()})
        }
        Body::List(items) => {
            json!({"kind":"list","items":items.into_iter().map(|r|json!({"key":r.key,"label":r.label,"detail":r.detail})).collect::<Vec<_>>()})
        }
        Body::Form(fields) => {
            json!({"kind":"form","fields":fields.into_iter().map(|f|json!({"key":f.key,"label":f.label,"value":field(f.value)})).collect::<Vec<_>>()})
        }
        Body::Table(table) => {
            json!({"kind":"table","columns":table.columns,"rows":table.rows,"firstRow":table.first_row,"totalRows":table.total_rows,"editable":table.editable})
        }
        Body::Other { kind, data } => {
            let mut data = serde_json::from_str::<Value>(&data).unwrap_or(json!({}));
            if let Some(map) = data.as_object_mut() {
                map.insert("kind".into(), kind.into());
            }
            data
        }
        _ => json!({"kind":"message","text":"This view needs a newer UI renderer."}),
    }
}

fn contains_key(body: &Body, key: &str) -> bool {
    match body {
        Body::Tree(rows) => rows.iter().any(|r| r.key == key),
        Body::List(rows) => rows.iter().any(|r| r.key == key),
        Body::Other { kind, data } if kind == "resources" => serde_json::from_str::<Value>(data)
            .ok()
            .and_then(|v| {
                v.get("items").and_then(Value::as_array).map(|rows| rows.iter().any(|r| r.get("key").and_then(Value::as_str) == Some(key)))
            })
            .unwrap_or(false),
        _ => false,
    }
}

fn valid_frame_id(frame: &str) -> bool {
    frame.len() == 32 && frame.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// Handles only the namespaced web protocol. Legacy messages and native events
/// fall through to the existing handler; commands are restricted to the view's
/// declared item actions, and operate only on a currently rendered item.
pub fn handle<T: Plugin + Views>(plugin: &mut T, view: &str, event: &ViewEvent, ctx: &Invocation, config: &str) -> Result<bool, Error> {
    let ViewEvent::Message(text) = event else {
        return Ok(false);
    };
    let Ok(value) = serde_json::from_str::<Value>(text) else {
        return Ok(false);
    };
    if value.get("channel").and_then(Value::as_str) != Some("ui-kit") {
        return Ok(false);
    }
    // Correlates a broadcast reply to its requesting page, not an authorization
    // token. Older native mocks may omit it; current browser pages never do.
    let frame_id = value.get("frameId").cloned().unwrap_or(Value::Null);
    let sequence = value.get("sequence").and_then(Value::as_u64).unwrap_or(0);
    let decode = || -> Result<(Request, Config), Error> {
        if text.len() > 262_144 {
            return Err(Error::Invalid("View request is too large.".into()));
        }
        if !frame_id.is_null() && !frame_id.as_str().is_some_and(valid_frame_id) {
            return Err(Error::Invalid("Invalid view frame identity.".into()));
        }
        let request: Request = serde_json::from_value(value).map_err(|e| Error::Invalid(e.to_string()))?;
        let config: Config = serde_json::from_str(config).map_err(|e| Error::Invalid(e.to_string()))?;
        Ok((request, config))
    };
    let (request, config) = match decode() {
        Ok(decoded) => decoded,
        Err(error) => {
            host::post_view_message(
                view,
                &json!({"channel":"ui-kit","sequence":sequence,"frameId":frame_id,"project":null,"error":error.to_string()}).to_string(),
            )?;
            return Ok(true);
        }
    };
    let Some(title) = config.views.get(view) else {
        return Ok(false);
    };
    // Rendering is always available, including permission/empty-state messages.
    // A denied/missing project is not a common anonymous mutation scope.
    let root = ctx.project().and_then(|p| p.root().ok()).filter(|p| !p.is_empty());
    let project = root.clone().or_else(|| config.project_independent.then(|| "@ui-kit:project-independent".into()));
    let sequence = request.sequence;
    let _rendering = Rendering::enter();
    let result = (|| -> Result<Value, Error> {
        if !matches!(request.operation, Operation::Render) {
            if project.is_none() {
                return Err(Error::Denied(
                    "Project identity is unavailable. Open a project and allow project.read before using project actions.".into(),
                ));
            }
            if request.project != project {
                return Err(Error::Stale);
            }
            if root.is_none()
                && match &request.operation {
                    Operation::Command { .. } => true,
                    Operation::Event { event: Event::Button { key } } => config.project_bound_buttons.contains(key),
                    _ => false,
                }
            {
                return Err(Error::Denied("This action needs an identifiable project. Open a project and allow project.read.".into()));
            }
        }
        match request.operation {
            Operation::Render => {}
            Operation::Event { event } => {
                let current = plugin.render(view, Some(Viewport { first_row: 0, rows: 5000 }), ctx)?;
                let allowed = match &event {
                    Event::Button { key } => current.toolbar.iter().any(|b| b.id == *key && b.enabled),
                    Event::Select { key } | Event::Activate { key } | Event::Expand { key } | Event::Collapse { key } => {
                        contains_key(&current.body, key)
                    }
                    Event::Field { key, .. } => {
                        matches!(&current.body, Body::Form(fields) if fields.iter().any(|f| f.key == *key && !matches!(f.value, FieldValue::Other { .. })))
                    }
                    Event::Cell { row, column, .. } => {
                        matches!(&current.body, Body::Table(t) if t.editable && *row >= t.first_row && (*row - t.first_row) < t.rows.len() as u32 && (*column as usize) < t.columns.len())
                    }
                    Event::TableSelect { row } | Event::TableActivate { row } => match &current.body {
                        Body::Table(t) => *row >= t.first_row && (*row - t.first_row) < t.rows.len() as u32,
                        Body::Other { kind, data } if kind == "table" => serde_json::from_str::<Value>(data).ok().is_some_and(|t| {
                            let first = t.get("firstRow").or_else(|| t.get("first_row")).and_then(Value::as_u64).unwrap_or(0);
                            let count = t.get("rows").and_then(Value::as_array).map_or(0, Vec::len) as u64;
                            u64::from(*row) >= first && u64::from(*row) - first < count
                        }),
                        _ => false,
                    },
                    Event::Sort { column, .. } => match &current.body {
                        Body::Table(t) => (*column as usize) < t.columns.len(),
                        Body::Other { kind, .. } => kind == "table",
                        _ => false,
                    },
                    Event::Message { .. } => matches!(&current.body, Body::Html(_)),
                };
                if !allowed {
                    return Err(Error::Invalid("This control is no longer available. Refresh the view.".into()));
                }
                plugin.handle(view, event.into(), ctx)?;
            }
            Operation::Command { id, key } => {
                let kind = key.split(':').next().unwrap_or_default();
                if !config.actions.iter().any(|a| a.view == view && a.id == id && (a.kind.is_empty() || a.kind == kind)) {
                    return Err(Error::Denied("This action is not declared for the selected resource.".into()));
                }
                let current = plugin.render(view, Some(Viewport { first_row: 0, rows: 5000 }), ctx)?;
                if !contains_key(&current.body, &key) {
                    return Err(Error::Stale);
                }
                if let Some(message) = plugin.run_command(&id, ctx, Some(&key))? {
                    host::notify(openidea_plugin::NotificationLevel::Info, title, &message);
                }
            }
        }
        let model = plugin.render(view, Some(Viewport { first_row: 0, rows: 5000 }), ctx)?;
        Ok(json!({"title":title,"plugin":config.title,"body":body(model.body),"status":model.status,
            "toolbar":model.toolbar.iter().map(|b|json!({"key":b.id,"label":b.label,"enabled":b.enabled})).collect::<Vec<_>>(),
            "actions":config.actions.iter().filter(|a|a.view==view).map(|a|json!({"id":a.id,"label":a.label,"kind":a.kind})).collect::<Vec<_>>()}))
    })();
    let answer = match result {
        Ok(model) => {
            json!({"channel":request.channel,"sequence":sequence,"frameId":request.frame_id,"project":project,"model":model})
        }
        Err(error) => {
            json!({"channel":request.channel,"sequence":sequence,"frameId":request.frame_id,"project":project,"error":error.to_string()})
        }
    };
    host::post_view_message(view, &answer.to_string())?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::views::{ListItem, View, Views};
    use openidea_plugin::{Plugin, mock::MockHost};
    #[derive(Default)]
    struct Demo {
        calls: Vec<String>,
    }
    impl Plugin for Demo {
        fn run_command(&mut self, id: &str, _: &Invocation, arg: Option<&str>) -> Result<Option<String>, Error> {
            self.calls.push(format!("{id}:{}", arg.unwrap_or_default()));
            Ok(None)
        }
    }
    impl Views for Demo {
        fn render(&mut self, _: &str, _: Option<Viewport>, _: &Invocation) -> Result<View, Error> {
            Ok(View::new(Body::List(vec![ListItem::new("route:1", "/orders")])).button(openidea_plugin::views::Button::new("cli", "CLI")))
        }
        fn handle(&mut self, _: &str, event: ViewEvent, _: &Invocation) -> Result<(), Error> {
            self.calls.push(format!("event:{event:?}"));
            Ok(())
        }
    }
    const CONFIG: &str =
        r#"{"title":"Demo","views":{"routes":"Routes"},"actions":[{"view":"routes","id":"copy","label":"Copy","kind":"route"}]}"#;
    #[test]
    fn view_commands_require_current_scope_and_declared_resource() {
        let mock = MockHost::install();
        let ctx = mock.invocation();
        let mut demo = Demo::default();
        for (project, id, key) in [("/wrong", "copy", "route:1"), ("/project", "delete", "route:1"), ("/project", "copy", "route:9")] {
            let event =
                ViewEvent::Message(json!({"channel":"ui-kit","op":"command","sequence":1,"project":project,"id":id,"key":key}).to_string());
            assert!(handle(&mut demo, "routes", &event, &ctx, CONFIG).unwrap());
            assert!(demo.calls.is_empty());
        }
        let event = ViewEvent::Message(
            json!({"channel":"ui-kit","op":"command","sequence":2,"project":"/project","id":"copy","key":"route:1"}).to_string(),
        );
        handle(&mut demo, "routes", &event, &ctx, CONFIG).unwrap();
        assert_eq!(demo.calls, ["copy:route:1"]);
        assert!(!is_rendering());
    }
    #[test]
    fn native_and_legacy_events_are_not_intercepted() {
        let mock = MockHost::install();
        let mut demo = Demo::default();
        for event in [ViewEvent::Button("refresh".into()), ViewEvent::Message("node:1".into())] {
            assert!(!handle(&mut demo, "routes", &event, &mock.invocation(), CONFIG).unwrap());
        }
    }
    #[test]
    fn read_only_fields_keep_their_semantics() {
        let model = field(FieldValue::readonly_text("<unsafe>", true));
        assert_eq!(model["kind"], "readonly-text");
        assert_eq!(model["data"]["text"], "<unsafe>");
    }
    #[test]
    fn anonymous_scopes_cannot_dispatch_commands_or_events_but_can_render() {
        for denied in [false, true] {
            let mock = MockHost::install().with_capabilities(if denied { &[] } else { &["project.read"] });
            if !denied {
                mock.set_project_root(None);
            }
            let mut demo = Demo::default();
            for project in [Value::Null, json!("")] {
                for operation in
                    [json!({"op":"command","id":"copy","key":"route:1"}), json!({"op":"event","event":{"kind":"button","key":"cli"}})]
                {
                    let mut request = json!({"channel":"ui-kit","sequence":1,"project":project});
                    request.as_object_mut().unwrap().extend(operation.as_object().unwrap().clone());
                    handle(&mut demo, "routes", &ViewEvent::Message(request.to_string()), &mock.invocation(), CONFIG).unwrap();
                    assert!(demo.calls.is_empty());
                    let answer: Value = serde_json::from_str(&mock.view_messages().last().unwrap().1).unwrap();
                    assert!(answer.get("error").is_some());
                }
            }
            handle(
                &mut demo,
                "routes",
                &ViewEvent::Message(json!({"channel":"ui-kit","sequence":2,"op":"render"}).to_string()),
                &mock.invocation(),
                CONFIG,
            )
            .unwrap();
            let answer: Value = serde_json::from_str(&mock.view_messages().last().unwrap().1).unwrap();
            assert!(answer.get("model").is_some());
            assert!(answer["project"].is_null());
        }
    }

    #[test]
    fn project_free_forms_opt_in_without_authorizing_project_actions() {
        let mock = MockHost::install().with_capabilities(&[]);
        let mut demo = Demo::default();
        let mut config: Value = serde_json::from_str(CONFIG).unwrap();
        config["project_independent"] = json!(true);
        config["project_bound_buttons"] = json!(["cli"]);
        for (operation, allowed) in [
            (json!({"op":"event","event":{"kind":"select","key":"route:1"}}), true),
            (json!({"op":"event","event":{"kind":"button","key":"cli"}}), false),
            (json!({"op":"command","id":"copy","key":"route:1"}), false),
        ] {
            let before = demo.calls.len();
            let mut request = json!({"channel":"ui-kit","sequence":1,"project":"@ui-kit:project-independent"});
            request.as_object_mut().unwrap().extend(operation.as_object().unwrap().clone());
            handle(&mut demo, "routes", &ViewEvent::Message(request.to_string()), &mock.invocation(), &config.to_string()).unwrap();
            assert_eq!(demo.calls.len() > before, allowed);
        }
    }

    #[test]
    fn previous_project_actions_are_stale_after_a_switch() {
        let mock = MockHost::install();
        let mut demo = Demo::default();
        mock.set_project_root(Some("/second"));
        let request = json!({"channel":"ui-kit","sequence":1,"project":"/project","op":"command","id":"copy","key":"route:1"});
        handle(&mut demo, "routes", &ViewEvent::Message(request.to_string()), &mock.invocation(), CONFIG).unwrap();
        assert!(demo.calls.is_empty());
    }
    #[test]
    fn same_project_and_sequence_replies_preserve_distinct_requesting_frames() {
        let mock = MockHost::install();
        let mut demo = Demo::default();
        let frames = ["00000000000000000000000000000001", "00000000000000000000000000000002"];
        for frame in frames {
            let request = json!({"channel":"ui-kit","sequence":7,"project":"/project","frameId":frame,"op":"render"});
            handle(&mut demo, "routes", &ViewEvent::Message(request.to_string()), &mock.invocation(), CONFIG).unwrap();
            let answer: Value = serde_json::from_str(&mock.view_messages().last().unwrap().1).unwrap();
            assert_eq!(answer["frameId"], frame);
            assert_eq!(answer["sequence"], 7);
            assert_eq!(answer["project"], "/project");
            assert!(answer.get("model").is_some());
        }
        assert!(demo.calls.is_empty());
    }

    #[test]
    fn malformed_frame_replies_echo_identity_without_dispatching_mutations() {
        let mock = MockHost::install();
        let mut demo = Demo::default();
        for frame in [
            json!(""),
            json!("a".repeat(31)),
            json!("a".repeat(33)),
            json!("A".repeat(32)),
            json!("g".repeat(32)),
            json!(12),
            json!({"frame":"invalid"}),
        ] {
            let request = json!({"channel":"ui-kit","sequence":9,"project":"/project","frameId":frame,
                "op":"command","id":"copy","key":"route:1"});
            handle(&mut demo, "routes", &ViewEvent::Message(request.to_string()), &mock.invocation(), CONFIG).unwrap();
            let answer: Value = serde_json::from_str(&mock.view_messages().last().unwrap().1).unwrap();
            assert_eq!(answer["frameId"], frame);
            assert_eq!(answer["sequence"], 9);
            assert!(answer["error"].as_str().unwrap().contains("frame"));
            assert!(answer.get("model").is_none());
            assert!(demo.calls.is_empty());
        }
    }

    #[test]
    fn valid_frame_is_echoed_on_project_membership_and_decode_errors() {
        let mock = MockHost::install();
        let mut demo = Demo::default();
        let frame = "0123456789abcdef0123456789abcdef";
        for operation in [
            json!({"op":"command","id":"copy","key":"route:1","project":"/wrong"}),
            json!({"op":"command","id":"copy","key":"route:missing","project":"/project"}),
            json!({"op":"event","project":"/project","event":{"kind":"button","key":"missing"}}),
            json!({"op":"invalid","project":"/project"}),
        ] {
            let mut request = json!({"channel":"ui-kit","sequence":1,"frameId":frame});
            request.as_object_mut().unwrap().extend(operation.as_object().unwrap().clone());
            handle(&mut demo, "routes", &ViewEvent::Message(request.to_string()), &mock.invocation(), CONFIG).unwrap();
            let answer: Value = serde_json::from_str(&mock.view_messages().last().unwrap().1).unwrap();
            assert_eq!(answer["frameId"], frame);
            assert!(answer.get("error").is_some());
            assert!(answer.get("model").is_none());
            assert!(demo.calls.is_empty());
        }
    }
}
