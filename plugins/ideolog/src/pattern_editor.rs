//! Draft settings are instance-owned and never stored until validated Save.
//! Storage remains Ideolog XML for compatibility with existing exports/settings.
use crate::{
    log::{self, Config, Parsed},
    patterns,
};
use fancy_regex::{Regex, RegexBuilder};
use openidea_plugin::views::{Body, Button, Field, FieldValue, View, ViewEvent};
use openidea_plugin::{Error, host};
use std::collections::{BTreeMap, BTreeSet};
use std::cell::RefCell;
use std::hash::BuildHasher;
use std::time::{SystemTime, UNIX_EPOCH};

const PREFIX: &str = "pattern-";
const MAX_PATTERN: usize = 4096;
const MAX_SAMPLE: usize = 8192;
const MAX_STORAGE: usize = 128 << 10;
const MAX_ENTRIES: usize = 128;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Format,
    Highlight,
}
#[derive(Clone)]
struct Model {
    kind: Kind,
    attrs: BTreeMap<String, String>,
}
struct Draft {
    token: String,
    model: Model,
    base: String,
    at: Option<usize>,
    sample: String,
    preview: String,
    rejected: BTreeSet<String>,
    formats: Vec<(String, String)>,
}
#[derive(Default)]
pub struct Editor {
    draft: Option<Draft>,
    next: u64,
    pub(super) section: Option<bool>,
    pub(super) browser: RefCell<(Vec<String>, Vec<usize>, Option<usize>)>,
}

fn invalid(message: impl Into<String>) -> Error {
    Error::Invalid(message.into())
}
fn regex(pattern: &str, flags: &str) -> Result<Regex, Error> {
    if pattern.is_empty() || pattern.len() > MAX_PATTERN || pattern.contains('\0') {
        return Err(invalid("Pattern must contain 1–4096 bytes without NUL."));
    }
    RegexBuilder::new(&format!("{flags}{pattern}"))
        .backtrack_limit(20_000)
        .build()
        .map_err(|error| invalid(format!("Invalid regex: {error}")))
}
fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('\n', "&#10;")
        .replace('\r', "&#13;")
        .replace('\t', "&#9;")
}
impl Model {
    fn from_xml(xml: &str) -> Result<Self, Error> {
        let elements = log::elements(xml);
        let [(name, attrs)] = elements.as_slice() else {
            return Err(invalid("Expected one stored Ideolog pattern."));
        };
        let kind = match *name {
            "LogParsingPattern" => Kind::Format,
            "LogHighlightingPattern" => Kind::Highlight,
            _ => return Err(invalid("Unsupported pattern.")),
        };
        let mut attrs: BTreeMap<_, _> = attrs.iter().cloned().collect();
        attrs.entry("enabled".into()).or_insert_with(|| "true".into());
        Ok(Self { kind, attrs })
    }
    fn get(&self, key: &str) -> &str {
        self.attrs.get(key).map_or("", String::as_str)
    }
    fn xml(&self) -> String {
        let name = match self.kind {
            Kind::Format => "LogParsingPattern",
            Kind::Highlight => "LogHighlightingPattern",
        };
        let attrs = self.attrs.iter().map(|(key, value)| format!(" {key}=\"{}\"", xml_escape(value))).collect::<String>();
        format!("<{name}{attrs} />")
    }
    fn group(&self, key: &str, max: usize) -> Result<Option<usize>, Error> {
        let value = self.get(key).parse::<i32>().map_err(|_| invalid(format!("{key}: use a whole column number, or -1 for none.")))?;
        if value == -1 {
            return Ok(None);
        }
        if !(0..=99).contains(&value) || value as usize >= max {
            return Err(invalid(format!("{key}: column is outside this pattern's {max} capture groups.")));
        }
        Ok(Some(value as usize))
    }
    fn validate(&self) -> Result<(), Error> {
        if self.attrs.iter().any(|(key, value)| {
            key.is_empty()
                || !key.chars().all(|c| c.is_ascii_alphanumeric() || "_:-".contains(c))
                || value.len() > MAX_PATTERN
                || value.contains('\0')
        }) {
            return Err(invalid("Pattern attribute is invalid or exceeds 4096 bytes."));
        }
        if !matches!(self.get("enabled"), "true" | "false") {
            return Err(invalid("Enabled must be true or false."));
        }
        match self.kind {
            Kind::Format => {
                if self.get("name").trim().is_empty() || self.get("name").len() > 128 {
                    return Err(invalid("Give the format a name of 1–128 bytes."));
                }
                let capture = regex(self.get("pattern"), "(?s)")?;
                let count = capture.captures_len().saturating_sub(1);
                if count == 0 {
                    return Err(invalid("Parsing regex needs at least one capture group (one per column)."));
                }
                regex(self.get("linePattern"), "")?;
                if !self.get("timePattern").is_empty() {log::validate_time_pattern(self.get("timePattern")).map_err(invalid)?;}
                for key in ["timeId", "severityId", "categoryId"] {
                    self.group(key, count)?;
                }
            }
            Kind::Highlight => {
                let capture = regex(self.get("pattern"), "(?i)")?;
                if !patterns::ACTIONS.iter().any(|(id, _)| *id == self.get("action")) {
                    return Err(invalid("Select a supported highlighting action."));
                }
                if self.get("action") == "HIGHLIGHT_MATCH" && capture.captures_len() < 2 {
                    return Err(invalid("Match highlighting needs a capture group around the text to colour."));
                }
                self.group("captureGroup", 100)?;
                self.get("fg").parse::<i32>().map_err(|_| invalid("Colour must be a signed 32-bit ARGB value."))?;
            }
        }
        if let Some(error) = Config::with_custom(&self.xml()).errors.first() {
            return Err(invalid(error));
        }
        Ok(())
    }
}

impl Editor {
    pub(super) fn selected(&self, all: &[String]) -> Result<Option<usize>, Error> {
        let binding = self.browser.borrow();
        if binding.2.is_some() && binding.0 != all { return Err(Error::Stale); }
        Ok(binding.2)
    }
    pub(super) fn bind_rows(&self, all: Vec<String>, rows: Vec<usize>) {
        let mut binding=self.browser.borrow_mut();
        let selected=if binding.0==all { binding.2.filter(|n|rows.contains(n)) } else {None};
        *binding=(all,rows,selected);
    }
    pub(super) fn select_row(&self, row: usize) -> Result<(),Error> {
        let mut binding=self.browser.borrow_mut();
        if binding.0 != patterns::entries() { return Err(Error::Stale); }
        binding.2=Some(*binding.1.get(row).ok_or(Error::Stale)?);
        Ok(())
    }
    pub(crate) fn control_key(&self, key: &str) -> String {
        self.draft.as_ref().map_or_else(|| key.to_owned(), |draft| format!("{key}@{}", draft.token))
    }
    fn scoped_id<'a>(&self, id: &'a str) -> Result<&'a str, Error> {
        if !id.starts_with(PREFIX) {
            return Ok(id);
        }
        let (key, token) = id.rsplit_once('@').ok_or(Error::Stale)?;
        if self.draft.as_ref().is_none_or(|draft| draft.token != token) {
            return Err(Error::Stale);
        }
        Ok(key)
    }
    pub fn render(&self) -> Option<View> {
        let draft = self.draft.as_ref()?;
        let model = &draft.model;
        let mut fields = vec![
            Field::new("pattern-enabled", "Enabled", FieldValue::Checkbox(model.get("enabled") != "false")),
        ];
        let text = |key: &str, label: &str| Field::new(&format!("{PREFIX}{key}"), label, FieldValue::Text(model.get(key).into()));
        match model.kind {
            Kind::Format => fields.extend([
                text("name", "Format name"),
                text("pattern", "Event regex (one capture group per column)"),
                text("linePattern", "Event start regex"),
                text("timePattern", "Timestamp format (empty: automatic)"),
                text("timeId", "Timestamp column (0 based; -1 none)"),
                text("severityId", "Level column (0 based; -1 none)"),
                text("categoryId", "Category column (0 based; -1 none)"),
            ]),
            Kind::Highlight => {
                fields.push(text("pattern", "Highlight regex (case insensitive)"));
                fields.push(Field::new(
                    "pattern-action",
                    "Action",
                    FieldValue::Choice(
                        patterns::ACTIONS.iter().map(|(_, label)| label.to_string()).collect(),
                        patterns::ACTIONS
                            .iter()
                            .find(|(id, _)| *id == model.get("action"))
                            .map_or(model.get("action"), |(_, label)| *label)
                            .into(),
                    ),
                ));
                fields.push(text("captureGroup", "Apply to column (0 based; -1 all)"));
                let selected = patterns::COLOURS
                    .iter()
                    .find(|(.., argb)| argb.to_string() == model.get("fg"))
                    .map_or_else(|| format!("ARGB {}", model.get("fg")), |(_, label, _)| label.to_string());
                let mut colours: Vec<String> = patterns::COLOURS.iter().map(|(_, label, _)| label.to_string()).collect();
                if !colours.contains(&selected) {
                    colours.push(selected.clone());
                }
                fields.push(Field::new("pattern-fg", "Colour", FieldValue::Choice(colours, selected)));
                let current=model.get("formatId");
                let selected=draft.formats.iter().find(|(id,_)|id==current).map(|(_,label)|label.as_str()).unwrap_or("Any log format");
                fields.push(Field::new("pattern-formatId", "Log format", FieldValue::Choice(draft.formats.iter().map(|(_,label)|label.clone()).collect(), selected.into())));
            }
        }
        fields.push(Field::new(
            "pattern-sample",
            "Sample log event (preview only; never saved)",
            FieldValue::Multiline(draft.sample.clone()),
        ));
        fields.push(Field::new(
            "pattern-preview-output",
            "Capture / highlighting preview",
            FieldValue::readonly_text(&draft.preview, true),
        ));
        let mut view = View::new(Body::Form(fields))
            .button(Button::new("pattern-preview", "Preview"))
            .button(Button::new("pattern-save", "Save").icon("check"))
            .button(Button::new("pattern-cancel", "Cancel"))
            .status(match model.kind {
                Kind::Format => "Log format settings · timestamp / level / message captures",
                Kind::Highlight => "Highlighting settings · line / field / captured match",
            });
        if let Body::Form(fields) = &mut view.body {
            for field in fields {
                field.key = self.control_key(&field.key);
            }
        }
        for button in &mut view.toolbar {
            button.id = self.control_key(&button.id);
        }
        Some(view)
    }
    fn start(&mut self, kind: Option<Kind>) -> Result<(), Error> {
        if self.draft.is_some() {
            return Err(invalid("Save or cancel the current pattern first."));
        }
        let base = host::storage::get("patterns")?.unwrap_or_default();
        if base.len() > MAX_STORAGE {
            return Err(invalid("Stored patterns exceed the 128 KiB editing limit."));
        }
        let all: Vec<String> = base.lines().filter(|line| !line.trim().is_empty()).map(str::to_owned).collect();
        let (at, xml) = if let Some(kind) = kind {
            if all.len() >= MAX_ENTRIES {
                return Err(invalid("At most 128 patterns can be edited here."));
            }
            (
                None,
                match kind {
                    Kind::Format => patterns::format_element("New format", r"^(\S+ \S+) (\w+) (.*)$", r"^\d", "0", "1", "-1"),
                    Kind::Highlight => patterns::highlight_element("ERROR", "HIGHLIGHT_LINE", patterns::COLOURS[0].2),
                },
            )
        } else {
            let Some(at) = self.selected(&all)?.or(patterns::pick_entry_if_unselected(self.selected(&all)?, "Edit pattern", &all)?) else {
                return Ok(());
            };
            (Some(at), all[at].clone())
        };
        let mut model = Model::from_xml(&xml)?;
        if at.is_none() {
            let ids: BTreeSet<String> = all
                .iter()
                .flat_map(|entry| log::elements(entry))
                .filter_map(|(_, attrs)| log::attr(&attrs, "uuid").map(str::to_owned))
                .collect();
            let candidate = (0..=MAX_ENTRIES)
                .map(|counter| patterns::uuid(&format!("{xml}:{base}:{counter}")))
                .find(|id| !ids.contains(id))
                .ok_or_else(|| invalid("Cannot allocate a unique pattern identity."))?;
            model.attrs.insert("uuid".into(), candidate);
        }
        self.next = self.next.checked_add(1).ok_or_else(|| invalid("Pattern edit identity limit reached; reopen the plugin."))?;
        let now = SystemTime::now().duration_since(UNIX_EPOCH).map_err(|_| invalid("Cannot establish pattern edit identity."))?.as_nanos();
        let random = std::collections::hash_map::RandomState::new();
        let token = format!(
            "{:016x}{:016x}",
            random.hash_one((now, self.next, "pattern draft")),
            random.hash_one((now, self.next, "pattern controls"))
        );
        let setting=host::setting_string("customPatterns").unwrap_or_default();
        if setting.len()>MAX_STORAGE { return Err(invalid("Custom formats exceed the 128 KiB limit.")); }
        let config=Config::with_custom(&format!("{setting}\n{base}"));
        let mut formats=vec![(String::new(),"Any log format".into())];
        let options=config.format_names();
        let mut counts=BTreeMap::<String,usize>::new();
        for (_,name) in &options {*counts.entry(name.clone()).or_default()+=1;}
        let mut names=BTreeSet::new();
        for (id,name) in options {
            if !names.insert(id.clone()) {continue;}
            let label=if counts.get(&name)==Some(&1) && name!="Any log format" {name} else {format!("{name} · {id}")};
            formats.push((id,label));
        }
        let current=model.get("formatId");
        if !current.is_empty() && !formats.iter().any(|(id,_)|id==current) { formats.push((current.into(),format!("Unavailable format · {current}"))); }
        self.draft=Some(Draft{token,model,base,at,formats,rejected:BTreeSet::new(),sample:"2024-05-01 10:00:00 ERROR Example failure\n    at demo.Foo.run(Foo.java:42)".into(),preview:"Enter a sample event, then Preview to inspect captures. Column numbers are 0 based; -1 means none. Changes are saved only by Save.".into()});
        Ok(())
    }
    pub fn handle(&mut self, event: &ViewEvent) -> Result<bool, Error> {
        if let ViewEvent::Button(id) = event {
            let id = self.scoped_id(id)?;
            match id {
                "add-format" => {
                    self.start(Some(Kind::Format))?;
                    return Ok(true);
                }
                "add-highlight" => {
                    self.start(Some(Kind::Highlight))?;
                    return Ok(true);
                }
                "edit" => {
                    self.start(None)?;
                    return Ok(true);
                }
                "pattern-cancel" => {
                    self.draft = None;
                    return Ok(true);
                }
                "pattern-preview" | "pattern-save" => {
                    let Some(draft) = self.draft.as_mut() else {
                        return Err(invalid("No pattern is being edited."));
                    };
                    let result = if id == "pattern-preview" { preview(draft) } else { save(draft) };
                    match result {
                        Ok(message) if id == "pattern-preview" => draft.preview = message,
                        Ok(_) => self.draft = None,
                        Err(error) => {
                            draft.preview = format!("Cannot {}: {error}", if id == "pattern-preview" { "preview" } else { "save" })
                        }
                    }
                    return Ok(true);
                }
                _ => {}
            }
        }
        if let ViewEvent::Field(key, value) = event {
            let key = self.scoped_id(key)?;
            let Some(key) = key.strip_prefix(PREFIX) else {
                return Ok(false);
            };
            let Some(draft) = self.draft.as_mut() else {
                return Err(invalid("No pattern is being edited."));
            };
            let limit = if key == "sample" { MAX_SAMPLE } else { MAX_PATTERN };
            if value.len() > limit || value.contains('\0') {
                draft.rejected.insert(key.into());
                draft.preview = format!("Field exceeds its {limit}-byte limit or contains NUL. Correct it before saving.");
                return Ok(true);
            }
            draft.rejected.remove(key);
            if key == "sample" {
                draft.sample = value.clone();
            } else {
                let allowed = match draft.model.kind {
                    Kind::Format => ["enabled", "name", "pattern", "linePattern", "timePattern", "timeId", "severityId", "categoryId"].contains(&key),
                    Kind::Highlight => ["enabled", "pattern", "action", "captureGroup", "fg", "formatId"].contains(&key),
                };
                if !allowed {
                    return Err(invalid("Unknown pattern field."));
                }
                let value = if key=="formatId" {
                    draft.formats.iter().find(|(_,label)|label==value).map(|(id,_)|id.clone()).or_else(||draft.formats.iter().find(|(id,_)|id==value).map(|(id,_)|id.clone())).ok_or_else(||invalid("Select a listed log format."))?
                } else if key == "action" {
                    patterns::ACTIONS.iter().find(|(_, label)| *label == value).map_or_else(|| value.clone(), |(id, _)| id.to_string())
                } else if key == "fg" {
                    patterns::COLOURS
                        .iter()
                        .find(|(_, label, _)| *label == value)
                        .map_or_else(|| value.strip_prefix("ARGB ").unwrap_or(value).to_string(), |(.., argb)| argb.to_string())
                } else {
                    value.clone()
                };
                draft.model.attrs.insert(key.into(), value);
            }
            draft.preview = "Draft changed. Preview again to inspect the current captures.".into();
            return Ok(true);
        }
        if self.draft.is_some() {
            return Ok(true);
        }
        Ok(false)
    }
}

fn save(draft: &Draft) -> Result<String, Error> {
    if !draft.rejected.is_empty() {
        return Err(invalid("Correct fields rejected for exceeding their size limits before saving."));
    }
    draft.model.validate()?;
    if host::storage::get("patterns")?.unwrap_or_default() != draft.base {
        return Err(invalid("Stored patterns changed elsewhere. Cancel and reopen before saving."));
    }
    let mut all: Vec<String> = draft.base.lines().filter(|line| !line.trim().is_empty()).map(str::to_owned).collect();
    let element = draft.model.xml();
    match draft.at {
        Some(at) => {
            let target = all.get_mut(at).ok_or_else(|| invalid("Edited pattern no longer exists."))?;
            *target = element;
        }
        None => all.push(element),
    }
    if all.len() > MAX_ENTRIES || all.join("\n").len() > MAX_STORAGE {
        return Err(invalid("Pattern storage exceeds 128 entries / 128 KiB."));
    }
    patterns::save(&all)?;
    Ok("Saved".into())
}

fn preview(draft: &Draft) -> Result<String, Error> {
    if !draft.rejected.is_empty() {
        return Err(invalid("Correct oversized fields before previewing."));
    }
    draft.model.validate()?;
    if draft.sample.is_empty() || draft.sample.len() > MAX_SAMPLE || draft.sample.lines().any(|line| line.len() > MAX_PATTERN) {
        return Err(invalid("Use a sample of 1–8192 bytes, with lines at most 4096 bytes."));
    }
    let mut model = draft.model.clone();
    model.attrs.insert("enabled".into(), "true".into());
    let config = Config::with_custom(&model.xml());
    if model.kind == Kind::Format {
        let pattern = regex(model.get("pattern"), "(?s)")?;
        let first = draft.sample.lines().next().unwrap_or_default();
        if !pattern.is_match(first).map_err(|_| invalid("Sample exceeded the regex backtracking limit."))? {
            return Err(invalid("Event regex does not match the sample's first line."));
        }
        if !regex(model.get("linePattern"), "")?
            .is_match(first)
            .map_err(|_| invalid("Event-start regex exceeded its backtracking limit."))?
        {
            return Err(invalid("Event-start regex does not recognise the sample's first line."));
        }
        let format = config.format_by_id(model.get("uuid")).ok_or_else(|| invalid("Draft format is unavailable."))?;
        let parsed = Parsed::new(Some(format), &draft.sample);
        let values = parsed.values();
        let field = |id: Option<usize>| id.and_then(|n| values.get(n).copied()).unwrap_or("(none / not captured)");
        let time_preview: String=match parsed.time(Some(format)).and_then(|time|log::time_seconds(time,format.time_pattern.as_deref())) {
            Some(_)=>"Valid timestamp · matches the selected format".into(),
            None=>"Not parsed by the selected timestamp format".into(),
        };
        Ok(format!(
            "Timestamp: {}\nLevel: {}\nCategory: {}\nMessage: {}\nColumns captured: {}\nContinuation lines: {}\nParsed timestamp: {time_preview}",
            field(format.time_column),
            field(format.severity_column),
            field(format.category_column),
            parsed.message(),
            values.len(),
            draft.sample.lines().count().saturating_sub(1)
        ))
    } else {
        let pattern = regex(model.get("pattern"), "(?i)")?;
        // Apply the original action rules to the detected format, with the
        // same all-column/field/match semantics as editor highlighting.
        pattern.is_match(&draft.sample).map_err(|_| invalid("Sample exceeded the regex backtracking limit."))?;
        let settings = host::setting_string("customPatterns").unwrap_or_default();
        if settings.len() > MAX_STORAGE {
            return Err(invalid("Custom-format setting exceeds the 128 KiB preview limit."));
        }
        let config = Config::highlighting_preview(&format!("{settings}\n{}", draft.base), &model.xml()).map_err(invalid)?;
        let format = config.detect(&draft.sample);
        let parsed = Parsed::new(format, &draft.sample);
        let pieces = config.highlight(format, &parsed);
        let mut lines = vec![format!("Detected format: {}", format.map_or("plain text", |format| format.name.as_str()))];
        for (range, kind) in pieces.into_iter().take(24) {
            let value = &draft.sample[range];
            let value: String = value.chars().take(256).collect();
            lines.push(format!("{}: {value}", kind.name()));
        }
        if lines.len() == 1 {
            lines.push("No highlighting rule matched. Check action, column, format UUID and capture groups.".into());
        }
        Ok(lines.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;
    fn click(editor: &mut Editor, id: &str) {
        let id = if id.starts_with(PREFIX) { editor.control_key(id) } else { id.to_owned() };
        assert!(editor.handle(&ViewEvent::Button(id)).unwrap());
    }
    fn field(editor: &mut Editor, key: &str, value: &str) {
        assert!(editor.handle(&ViewEvent::Field(editor.control_key(&format!("pattern-{key}")), value.into())).unwrap());
    }
    fn output(editor: &Editor) -> &str {
        &editor.draft.as_ref().unwrap().preview
    }


    #[test]
    fn named_format_choice_and_timestamp_fields_roundtrip_without_uuid_typing() {
        let host=MockHost::install();
        let format=patterns::format_element("Orders",r"^(.*)$","^","0","-1","-1");
        patterns::save(&[format]).unwrap();
        let mut editor=Editor::default();click(&mut editor,"add-highlight");
        let Body::Form(fields)=editor.render().unwrap().body else {panic!("form")};
        let choice=fields.iter().find(|f|f.label=="Log format").unwrap();
        let FieldValue::Choice(options,selected)=&choice.value else {panic!("choice")};
        assert_eq!(selected,"Any log format");
        let name=options.iter().find(|name|name.as_str()=="Orders").unwrap().clone();
        field(&mut editor,"formatId",&name);click(&mut editor,"pattern-save");
        assert!(patterns::entries()[1].contains("formatId=\"c0570000"));
        click(&mut editor,"add-format");field(&mut editor,"timePattern","yyyy-MM-dd HH:mm:ss");
        field(&mut editor,"sample","2024-05-01 10:00:00 ERROR failure");click(&mut editor,"pattern-preview");
        assert!(editor.draft.as_ref().unwrap().preview.contains("Parsed timestamp:"));
        field(&mut editor,"timePattern","EEE yyyy");click(&mut editor,"pattern-save");
        assert!(editor.draft.is_some(),"unsupported date patterns cannot be saved");
        assert_eq!(patterns::entries().len(),2);
        assert!(host.process_runs().is_empty());
    }

    #[test]
    fn controls_from_cancelled_draft_cannot_modify_save_or_cancel_a_new_draft() {
        let _host = MockHost::install();
        let mut editor = Editor::default();
        click(&mut editor, "add-format");
        let old_name = editor.control_key("pattern-name");
        let old_save = editor.control_key("pattern-save");
        let old_cancel = editor.control_key("pattern-cancel");
        click(&mut editor, "pattern-cancel");
        click(&mut editor, "add-format");
        assert_ne!(old_name, editor.control_key("pattern-name"));
        for event in [ViewEvent::Field(old_name, "Wrong edit session".into()), ViewEvent::Button(old_save), ViewEvent::Button(old_cancel)] {
            assert!(matches!(editor.handle(&event), Err(Error::Stale)));
        }
        assert_eq!(editor.draft.as_ref().unwrap().model.get("name"), "New format");
        assert!(patterns::stored().is_empty());
        field(&mut editor, "name", "Current session");
        click(&mut editor, "pattern-save");
        assert!(patterns::stored().contains("Current session"));
    }

    #[test]
    fn structured_format_preview_uses_actual_captures_and_never_saves_the_sample() {
        let _host = MockHost::install();
        let mut editor = Editor::default();
        click(&mut editor, "add-format");
        let Body::Form(fields) = editor.render().unwrap().body else { panic!("form") };
        assert!(fields.iter().any(|field| field.key.starts_with("pattern-timeId@")));
        field(&mut editor, "name", "Worker");
        field(&mut editor, "sample", "2024-05-01 10:00:00 WARN é😀private-message\r\n    😀 trace\r\n");
        click(&mut editor, "pattern-preview");
        let preview = output(&editor);
        assert!(preview.contains("Timestamp: 2024-05-01 10:00:00"));
        assert!(preview.contains("Level: WARN"));
        assert!(preview.contains("Message: é😀private-message"));
        assert!(preview.contains("Continuation lines: 1"));
        assert!(patterns::stored().is_empty());
        click(&mut editor, "pattern-save");
        assert!(editor.draft.is_none());
        assert!(patterns::stored().contains("Worker"));
        assert!(!patterns::stored().contains("private-message"));
    }

    #[test]
    fn invalid_columns_regexes_and_oversized_fields_cannot_silently_save_old_values() {
        let _host = MockHost::install();
        let mut editor = Editor::default();
        click(&mut editor, "add-format");
        for (key, value) in [("timeId", "nonsense"), ("timeId", "-2"), ("timeId", "3"), ("timeId", "999999999999999999999")] {
            field(&mut editor, key, value);
            click(&mut editor, "pattern-save");
            assert!(patterns::stored().is_empty());
            assert!(output(&editor).contains("Cannot save"));
        }
        field(&mut editor, "timeId", "0");
        field(&mut editor, "pattern", "(");
        click(&mut editor, "pattern-save");
        assert!(output(&editor).contains("Invalid regex"));
        field(&mut editor, "pattern", "no captures");
        click(&mut editor, "pattern-save");
        assert!(output(&editor).contains("at least one capture"));
        field(&mut editor, "pattern", r"^(\S+ \S+) (\w+) (.*)$");
        field(&mut editor, "name", &"x".repeat(MAX_PATTERN + 1));
        click(&mut editor, "pattern-save");
        assert!(patterns::stored().is_empty());
        assert!(output(&editor).contains("Correct fields"));
        field(&mut editor, "name", "Corrected");
        click(&mut editor, "pattern-save");
        assert!(patterns::stored().contains("Corrected"));
    }

    #[test]
    fn sample_mismatch_does_not_fall_back_to_a_different_detected_format() {
        let _host = MockHost::install();
        let mut editor = Editor::default();
        click(&mut editor, "add-format");
        field(&mut editor, "pattern", r"^(CUSTOM) (WARN) (.*)$");
        field(&mut editor, "sample", "2024-05-01T10:00:00Z ERROR docker format\n");
        click(&mut editor, "pattern-preview");
        assert!(output(&editor).contains("does not match"));
        assert!(!output(&editor).contains("Timestamp:"));
        field(&mut editor, "sample", &"x".repeat(MAX_PATTERN + 1));
        click(&mut editor, "pattern-preview");
        assert!(output(&editor).contains("lines at most 4096"));
    }

    #[test]
    fn cancelled_or_conflicting_edits_preserve_stored_patterns_and_extra_attributes() {
        let host = MockHost::install();
        let xml = r#"<LogParsingPattern enabled="false" name="Original" pattern="^(\S+) (WARN) (.*)$" linePattern="^" timeId="0" severityId="1" categoryId="-1" uuid="stable-id" timePattern="HH:mm:ss" fullmatch="false" extension="quoted &quot;value&quot;" />"#;
        patterns::save(&[xml.into()]).unwrap();
        let mut editor = Editor::default();
        host.answer_next_prompt(Some(&["0"]));
        click(&mut editor, "edit");
        field(&mut editor, "name", "Changed");
        click(&mut editor, "pattern-cancel");
        assert_eq!(patterns::stored(), xml);
        host.answer_next_prompt(Some(&["0"]));
        click(&mut editor, "edit");
        field(&mut editor, "name", "Changed");
        click(&mut editor, "pattern-save");
        let stored = patterns::stored();
        let model = Model::from_xml(&stored).unwrap();
        assert_eq!(model.get("enabled"), "false");
        assert_eq!(model.get("uuid"), "stable-id");
        assert_eq!(model.get("timePattern"), "HH:mm:ss");
        assert_eq!(model.get("fullmatch"), "false");
        assert_eq!(model.get("extension"), "quoted \"value\"");
        host.answer_next_prompt(Some(&["0"]));
        click(&mut editor, "edit");
        patterns::save(&[patterns::highlight_element("external", "HIGHLIGHT_LINE", -39836)]).unwrap();
        let external = patterns::stored();
        click(&mut editor, "pattern-save");
        assert_eq!(patterns::stored(), external);
        assert!(output(&editor).contains("changed elsewhere"));
    }

    #[test]
    fn highlighting_preview_checks_only_the_draft_action_and_supports_column_and_match_captures() {
        let _host = MockHost::install();
        let mut editor = Editor::default();
        click(&mut editor, "add-highlight");
        field(&mut editor, "pattern", "NEVER_MATCH");
        click(&mut editor, "pattern-preview");
        assert!(output(&editor).contains("No highlighting rule matched"), "default ERROR colour must not imply a successful draft");
        field(&mut editor, "action", "Highlight the matching text");
        field(&mut editor, "pattern", "ERROR");
        click(&mut editor, "pattern-save");
        assert!(output(&editor).contains("needs a capture group"));
        field(&mut editor, "pattern", r"user=(\w+)");
        field(&mut editor, "fg", "Blue");
        field(&mut editor, "sample", "10:00:00|INFO|main|user=alice\n");
        click(&mut editor, "pattern-preview");
        assert!(output(&editor).contains("function: alice"));
        field(&mut editor, "action", "Highlight a field");
        field(&mut editor, "captureGroup", "1");
        field(&mut editor, "pattern", "INFO");
        click(&mut editor, "pattern-preview");
        assert!(output(&editor).contains("function: INFO"));
        field(&mut editor, "enabled", "false");
        click(&mut editor, "pattern-save");
        let model = Model::from_xml(&patterns::stored()).unwrap();
        assert_eq!(model.get("fg"), patterns::COLOURS[3].2.to_string());
        assert_eq!(model.get("enabled"), "false");
    }

    #[test]
    fn stale_buttons_invalid_picker_and_denied_storage_cannot_mutate() {
        let host = MockHost::install();
        let mut editor = Editor::default();
        assert!(editor.handle(&ViewEvent::Button("pattern-save".into())).is_err());
        patterns::save(&[patterns::highlight_element("one", "HIGHLIGHT_LINE", -39836)]).unwrap();
        let original = patterns::stored();
        host.answer_next_prompt(Some(&["999"]));
        assert!(editor.handle(&ViewEvent::Button("edit".into())).is_err());
        assert_eq!(patterns::stored(), original);
        click(&mut editor, "add-format");
        click(&mut editor, "remove");
        assert_eq!(patterns::stored(), original, "list mutations blocked while a draft is active");
        click(&mut editor, "pattern-cancel");
        let _denied = host.with_capabilities(&[]);
        assert!(editor.handle(&ViewEvent::Button("add-format".into())).is_err());
        assert!(editor.draft.is_none());
    }
    #[test]
    fn new_pattern_identity_is_unique_and_multiline_attributes_round_trip() {
        let _host = MockHost::install();
        let mut editor = Editor::default();
        click(&mut editor, "add-format");
        let first = editor.draft.as_ref().unwrap().model.get("uuid").to_string();
        click(&mut editor, "pattern-save");
        click(&mut editor, "add-format");
        let second = editor.draft.as_ref().unwrap().model.get("uuid").to_string();
        assert_ne!(first, second, "separate formats need separate restriction identities");
        field(&mut editor, "name", "Two\nLines");
        click(&mut editor, "pattern-save");
        assert_eq!(patterns::entries().len(), 2, "escaped newlines cannot split storage records");
        assert_eq!(Model::from_xml(&patterns::entries()[1]).unwrap().get("name"), "Two\nLines");
    }
}
