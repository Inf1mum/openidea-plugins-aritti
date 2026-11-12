//! Scenario-driven runner for the released-host headless harness. See plugins/jpa-assist/parity/runner/README.md.
//! L1 evidence only: expectations are structural properties taken from a documented claim, never a snapshot of
//! our own output (a `goldenFile` is allowed only when marked `ownOutputSnapshot: true`).
use super::*;
use ide_plugin::{CodeAction, FileOperation};
use regex::Regex;
use std::collections::BTreeMap;

pub(crate) struct Loaded {
    path: PathBuf,
    sha: String,
    value: Value,
}

const KINDS: &[&str] = &["fileaction", "diagnostics", "completion", "hover", "command", "tree", "lineMarkers", "designer"];
const EXPECT_KEYS: &[&str] = &[
    "promptSequence", "promptCount", "editsContain", "resultFile", "createdFiles", "noFilesChanged", "diagnostics", "diagnosticsAbsent",
    "completionsInclude", "completionsExclude", "hoverContains", "clipboardContains", "treeNodes", "treeChildren", "treeAbsent", "markersInclude", "actionOffered",
    "resolveErrorContains", "goldenFile", "designerReplies",
];
const CHECK_KEYS: &[&str] = &["preview", "undo", "cancelEach", "stale"];
const TOP_KEYS: &[&str] = &[
    "id", "rows", "claimIds", "verifies", "fixture", "setup", "preActions", "postSetup", "kind", "target", "action", "prompts", "expect", "checks", "note", "kindExerciser", "warm", "grants",
];

const DESIGNER_OPERATORS: &[&str] = &["equals", "exists", "contains", "matches", "length", "anyItem", "noItem", "countWhere", "handlerError"];
const DESIGNER_FRAME: &str = "0123456789abcdef0123456789abcdef";

fn designer_substitute(v: &Value, project: &str, last: &Value) -> Value {
    match v {
        Value::String(s) => match s.strip_prefix("$last:") {
            Some(pointer) => last.pointer(pointer).cloned().unwrap_or(Value::Null),
            None => Value::String(s.replace("$PROJECT", project)),
        },
        Value::Array(a) => Value::Array(a.iter().map(|x| designer_substitute(x, project, last)).collect()),
        Value::Object(o) => Value::Object(o.iter().map(|(k, x)| (k.clone(), designer_substitute(x, project, last))).collect()),
        other => other.clone(),
    }
}

/// The request frame the web view would post: protocol fields are filled in, `$PROJECT` and `$last:/json/pointer`
/// (a value of the previous reply) are substituted, and `"context": "auto"` is taken from the previous reply's model.
fn designer_request(message: &Value, index: usize, project: &str, last: &Value) -> String {
    let mut request = designer_substitute(message, project, last);
    let object = request.as_object_mut().expect("validated: messages are objects");
    object.entry("channel").or_insert(json!("jpa-designer"));
    object.entry("protocol").or_insert(json!(1));
    object.entry("frame").or_insert(json!(DESIGNER_FRAME));
    object.entry("sequence").or_insert(json!(index as u64 + 1));
    let snapshot = object.get("operation").and_then(|o| o["kind"].as_str()) == Some("snapshot");
    object.entry("root").or_insert(if snapshot { Value::Null } else { json!(project) });
    if object.get("context") == Some(&json!("auto")) {
        let model = &last["model"];
        object.insert("context".into(), json!({"path": model["source"]["path"], "version": model["source"]["version"], "attribute": model["selected"]}));
    }
    request.to_string()
}

fn json_at<'a>(root: &'a Value, path: &str) -> Option<&'a Value> {
    path.split('.').filter(|p| !p.is_empty()).try_fold(root, |value, key| match key.parse::<usize>() {
        Ok(i) if value.is_array() => value.get(i),
        _ => value.get(key),
    })
}

fn subset(item: &Value, want: &Value) -> bool {
    want.as_object().is_some_and(|w| w.iter().all(|(k, x)| item.get(k) == Some(x)))
}

fn tree_event(v: &Value, project: &str) -> ViewEvent {
    let text = |key: &str| v[key].as_str().unwrap_or_default().replace("$PROJECT", project);
    if v["select"].is_string() {
        ViewEvent::Select { key: text("select") }
    } else if v["activate"].is_string() {
        ViewEvent::Activate { key: text("activate") }
    } else {
        ViewEvent::Button { id: text("button") }
    }
}

fn strs(v: &Value) -> Vec<String> {
    v.as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_owned)).collect()).unwrap_or_default()
}

fn safe_relative(path: &str) -> bool {
    !path.is_empty() && !path.starts_with('/') && !path.split('/').any(|s| s == "..")
}

pub(crate) fn load(requested: &[String], only: &[String]) -> R<Vec<Loaded>> {
    let mut files: Vec<PathBuf> = vec![];
    for entry in requested {
        let path = PathBuf::from(entry);
        if path.is_dir() {
            let mut inside: Vec<PathBuf> = std::fs::read_dir(&path)?.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "json")).collect();
            inside.sort();
            files.extend(inside);
        } else {
            files.push(path);
        }
    }
    let mut out = vec![];
    for path in files {
        let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let value: Value = serde_json::from_slice(&bytes).map_err(|e| format!("{}: {e}", path.display()))?;
        if !only.is_empty() && !only.iter().any(|id| value["id"].as_str() == Some(id)) {
            continue;
        }
        out.push(Loaded { path, sha: sha(&bytes), value });
    }
    Ok(out)
}

pub(crate) fn list(scenarios: &[Loaded]) {
    for s in scenarios {
        let v = &s.value;
        println!(
            "{} kind={} fixture={} rows=[{}] claims=[{}]{}",
            v["id"].as_str().unwrap_or("?"),
            v["kind"].as_str().unwrap_or("?"),
            v["fixture"].as_str().unwrap_or("?"),
            strs(&v["rows"]).join(","),
            strs(&v["claimIds"]).join(","),
            if v["kindExerciser"] == true { "  (kind exerciser: covers no documented claim)" } else { "" }
        );
    }
    let claims: std::collections::BTreeSet<String> = scenarios.iter().flat_map(|s| strs(&s.value["claimIds"])).collect();
    println!("{} scenarios, {} distinct claim ids covered", scenarios.len(), claims.len());
}

fn parse_answer(v: &Value) -> Result<Answer, String> {
    if v.as_str() == Some("cancel") {
        return Ok(Answer::Cancel);
    }
    if v.as_str() == Some("default") {
        return Ok(Answer::Default);
    }
    let object = v.as_object().filter(|o| o.len() == 1).ok_or_else(|| format!("answer must be \"cancel\" or a one-key object: {v}"))?;
    let (key, value) = object.iter().next().unwrap();
    let text = || value.as_str().map(str::to_owned).ok_or_else(|| format!("answer {key} needs a string"));
    match key.as_str() {
        "pick" => Ok(Answer::Pick(text()?)),
        "pickLabel" => Ok(Answer::PickLabel(text()?)),
        "pickMany" => {
            let keys = strs(value);
            if keys.is_empty() {
                return Err("pickMany needs a nonempty list of keys".into());
            }
            Ok(Answer::PickMany(keys))
        }
        "input" => Ok(Answer::Input(text()?)),
        "confirm" => match value {
            Value::Bool(b) => Ok(Answer::Confirm(*b)),
            Value::String(s) if s == "yes" => Ok(Answer::Confirm(true)),
            Value::String(s) if s == "no" => Ok(Answer::Confirm(false)),
            _ => Err("confirm needs yes, no or a boolean".into()),
        },
        "cancel" => Ok(Answer::Cancel),
        other => Err(format!("unknown answer type {other}")),
    }
}

pub(crate) fn validate(repo: &Path, scenarios: &[Loaded]) -> Vec<String> {
    let parity = repo.join(PLUGIN_SOURCE).join("parity");
    let mut errors = vec![];
    let mut ids = BTreeSet::new();
    let mut claims_cache: BTreeMap<String, Option<Value>> = BTreeMap::new();
    for s in scenarios {
        let v = &s.value;
        let id = v["id"].as_str().unwrap_or("").to_owned();
        let mut err = |m: String| errors.push(format!("{}: {m}", if id.is_empty() { s.path.display().to_string() } else { id.clone() }));
        if !Regex::new(r"^RS-\d{3}$").unwrap().is_match(&id) {
            err("id must look like RS-001".into());
        }
        if s.path.file_stem().and_then(|f| f.to_str()).is_some_and(|stem| !stem.starts_with(&id) || id.is_empty()) {
            err(format!("file name {} must start with the id", s.path.display()));
        }
        if !ids.insert(id.clone()) {
            err("duplicate id".into());
        }
        if let Some(object) = v.as_object() {
            for key in object.keys().filter(|k| !TOP_KEYS.contains(&k.as_str())) {
                err(format!("unknown top-level key {key}"));
            }
        }
        let kind = v["kind"].as_str().unwrap_or("");
        if !KINDS.contains(&kind) {
            err(format!("kind must be one of {KINDS:?}"));
        }
        let fixture = v["fixture"].as_str().unwrap_or("");
        let fixture_dir = parity.join("fixtures").join(fixture);
        if fixture.is_empty() || !safe_relative(fixture) || !fixture_dir.is_dir() {
            err(format!("fixture {fixture:?} is not a directory under parity/fixtures"));
        }
        let rows = strs(&v["rows"]);
        let claim_ids = strs(&v["claimIds"]);
        let exerciser = v["kindExerciser"] == true;
        if claim_ids.is_empty() && !(exerciser && rows.is_empty()) {
            err("claimIds is empty: only a scenario marked kindExerciser with no rows may cover no claim".into());
        }
        if exerciser && (!claim_ids.is_empty() || !rows.is_empty()) {
            err("a kindExerciser must list no rows and no claim ids".into());
        }
        if v["note"].as_str().unwrap_or("").trim().is_empty() {
            err("note is required".into());
        }
        let verifies = v["verifies"].as_object().cloned().unwrap_or_default();
        for id in &claim_ids {
            if verifies.get(id).and_then(Value::as_str).is_none_or(|t| t.trim().is_empty()) {
                err(format!("verifies needs one sentence naming the expectation that checks {id}"));
            }
        }
        for key in verifies.keys().filter(|k| !claim_ids.contains(k)) {
            err(format!("verifies names {key} which is not in claimIds"));
        }
        let mut covered_rows = BTreeSet::new();
        for claim in &claim_ids {
            let row = claim.split("-C").next().unwrap_or("").to_owned();
            if !rows.contains(&row) {
                err(format!("claim {claim} belongs to row {row} which is not in rows"));
                continue;
            }
            let file = parity.join("claims").join(format!("{row}.json"));
            let loaded = claims_cache.entry(row.clone()).or_insert_with(|| std::fs::read(&file).ok().and_then(|b| serde_json::from_slice(&b).ok()));
            let Some(doc) = loaded else {
                err(format!("claims/{row}.json is missing or unreadable"));
                continue;
            };
            if doc["row"].as_str() != Some(row.as_str()) {
                err(format!("claims/{row}.json declares row {:?}", doc["row"]));
            }
            match doc["claims"].as_array().and_then(|c| c.iter().find(|c| c["id"].as_str() == Some(claim.as_str()))) {
                None => err(format!("unknown claim id {claim} (not in claims/{row}.json)")),
                Some(found) => {
                    if found["strength"].as_str() != Some("explicit") {
                        err(format!("claim {claim} is not explicit"));
                    }
                    if found["verifiableHeadless"] != true {
                        err(format!("claim {claim} is not marked verifiableHeadless: the headless host cannot show its surface"));
                    }
                    covered_rows.insert(row.clone());
                }
            }
        }
        for row in &rows {
            if !covered_rows.contains(row) {
                err(format!("row {row} has no covered claim id"));
            }
        }
        match kind {
            "fileaction" | "diagnostics" | "completion" | "hover" | "lineMarkers" | "command" | "tree" | "designer" => {}
            _ => {}
        }
        if matches!(kind, "fileaction" | "completion" | "hover" | "lineMarkers") && !safe_relative(v["target"]["file"].as_str().unwrap_or("")) {
            err("target.file (relative path) is required for this kind".into());
        }
        if kind == "fileaction" && v["action"]["title"].as_str().is_none_or(str::is_empty) {
            err("action.title (exact code-action title) is required".into());
        }
        if kind == "command" && v["action"]["command"].as_str().is_none_or(str::is_empty) {
            err("action.command is required".into());
        }
        if kind == "tree" && v["action"]["view"].as_str().is_none_or(str::is_empty) {
            err("action.view is required".into());
        }
        if kind == "designer" {
            let all_messages = v["action"]["messages"].as_array().cloned().unwrap_or_default();
            if all_messages.iter().any(|m| m.get("setup").is_some_and(|s| m.as_object().is_some_and(|o| o.len() != 1) || !s.is_array())) {
                err("a {\"setup\": [steps]} entry between messages has no other key".into());
            }
            let messages: Vec<Value> = all_messages.into_iter().filter(|m| m.get("setup").is_none()).collect();
            if v["action"]["view"].as_str().is_none_or(str::is_empty) || messages.is_empty() || messages.iter().any(|m| !m.is_object()) {
                err("designer scenarios need action.view and a nonempty action.messages array of objects".into());
            }
            for step in v["expect"]["designerReplies"].as_array().cloned().unwrap_or_default() {
                let message = step["message"].as_u64().unwrap_or(messages.len() as u64);
                if message == 0 || message as usize > messages.len() {
                    err(format!("designerReplies message {message} is out of range"));
                }
                if let Some(rx) = step["matches"].as_str() {
                    if Regex::new(rx).is_err() {
                        err(format!("designerReplies matches regex does not compile: {rx}"));
                    }
                }
                let operators = DESIGNER_OPERATORS.iter().filter(|k| step.get(**k).is_some()).count();
                if operators != 1 {
                    err(format!("designerReplies entry needs exactly one of {DESIGNER_OPERATORS:?}: {step}"));
                }
                if step.get("handlerError").is_none() && step["path"].as_str().is_none() {
                    err("designerReplies entry needs a path (or handlerError)".into());
                }
            }
        } else if v["expect"].get("designerReplies").is_some() {
            err("expect.designerReplies needs kind designer".into());
        }
        if strs(&v["grants"]).iter().any(|g| g != "project.write") {
            err("grants may only name project.write".into());
        }
        for event in v["action"]["viewEvents"].as_array().cloned().unwrap_or_default() {
            if event.as_object().is_none_or(|o| o.len() != 1 || !["button", "select", "activate"].iter().any(|k| event[*k].as_str().is_some_and(|t| !t.is_empty()))) {
                err("action.viewEvents entries are one-key objects: button, select or activate with a nonempty string".into());
            }
        }
        for pre in v["preActions"].as_array().cloned().unwrap_or_default() {
            if pre["title"].as_str().is_none_or(str::is_empty) || !safe_relative(pre["target"]["file"].as_str().unwrap_or("")) {
                err("preActions entries need title and target.file".into());
            }
            for answer in pre["prompts"].as_array().cloned().unwrap_or_default() {
                if let Err(m) = parse_answer(&answer) {
                    err(m);
                }
            }
        }
        for answer in v["prompts"].as_array().cloned().unwrap_or_default() {
            if let Err(m) = parse_answer(&answer) {
                err(m);
            }
        }
        if !v["prompts"].is_null() && !v["prompts"].is_array() {
            err("prompts must be an array".into());
        }
        let post_steps = v["postSetup"].as_array().cloned().unwrap_or_default();
        let all_steps = v["setup"].as_array().cloned().unwrap_or_default().into_iter().map(|s| (s, true)).chain(post_steps.into_iter().map(|s| (s, false)));
        for (step, is_setup) in all_steps {
            if !is_setup && step.get("settings").is_some() {
                err("settings steps are only allowed in setup".into());
                continue;
            }
            let object = step.as_object();
            let ok = object.is_some_and(|o| o.len() == 1)
                && match object.unwrap().iter().next().unwrap() {
                    (k, x) if k == "copy" => safe_relative(x["from"].as_str().unwrap_or("")) && safe_relative(x["to"].as_str().unwrap_or("")) && fixture_dir.join(x["from"].as_str().unwrap_or("-")).exists(),
                    (k, x) if k == "swap" => safe_relative(x["file"].as_str().unwrap_or("")) && safe_relative(x["with"].as_str().unwrap_or("")) && fixture_dir.join(x["with"].as_str().unwrap_or("-")).exists(),
                    (k, x) if k == "write" => safe_relative(x["path"].as_str().unwrap_or("")) && x["content"].is_string(),
                    (k, x) if k == "replace" => safe_relative(x["file"].as_str().unwrap_or("")) && x["from"].as_str().is_some_and(|f| !f.is_empty()) && x["to"].is_string(),
                    (k, x) if k == "delete" => safe_relative(x.as_str().unwrap_or("")),
                    (k, x) if k == "settings" => x.is_object(),
                    _ => false,
                };
            if !ok {
                err(format!("bad setup step {step} (copy/swap sources must exist in the fixture; paths must be relative)"));
            }
        }
        if let Some(expect) = v["expect"].as_object() {
            for key in expect.keys().filter(|k| !EXPECT_KEYS.contains(&k.as_str())) {
                err(format!("unknown expect key {key}"));
            }
            if let Some(golden) = expect.get("goldenFile") {
                if golden["ownOutputSnapshot"] != true {
                    err("goldenFile must be marked ownOutputSnapshot: true (never documentation evidence)".into());
                }
            }
            for rx in expect.get("editsContain").map(strs).unwrap_or_default() {
                if Regex::new(&rx).is_err() {
                    err(format!("editsContain regex does not compile: {rx}"));
                }
            }
            let files: Vec<Value> = match expect.get("resultFile") {
                Some(Value::Array(a)) => a.clone(),
                Some(o @ Value::Object(_)) => vec![o.clone()],
                _ => vec![],
            };
            for f in files {
                for rx in strs(&f["matches"]) {
                    if Regex::new(&rx).is_err() {
                        err(format!("resultFile.matches regex does not compile: {rx}"));
                    }
                }
                if !safe_relative(f["path"].as_str().unwrap_or("")) {
                    err("resultFile.path must be relative".into());
                }
            }
            if expect.is_empty() {
                err("expect is empty".into());
            }
        } else {
            err("expect is required".into());
        }
        if let Some(checks) = v["checks"].as_object() {
            for key in checks.keys().filter(|k| !CHECK_KEYS.contains(&k.as_str())) {
                err(format!("unknown checks key {key}"));
            }
            if kind != "fileaction" && (checks.get("stale") == Some(&json!(true)) || checks.get("cancelEach") == Some(&json!(true))) {
                err("checks.stale and checks.cancelEach need kind fileaction".into());
            }
        }
    }
    errors
}

// ---------------------------------------------------------------------------------------------------------------

struct Results {
    items: Vec<Value>,
    scrub: Vec<(String, &'static str)>,
}

fn scrubbed(text: &str, scrub: &[(String, &'static str)]) -> String {
    let text = scrub.iter().fold(text.to_owned(), |acc, (from, to)| acc.replace(from.as_str(), to));
    // the plugin proposes the WASI clock (yyyyMMddHHmmss) as the default of its date prompt
    Regex::new(r"\b20\d{12}\b").unwrap().replace_all(&text, "<clock-proposal>").into_owned()
}

impl Results {
    fn add(&mut self, check: &str, pass: bool, detail: impl Into<String>) {
        let detail: String = scrubbed(&detail.into(), &self.scrub);
        println!("    {} {check}: {}", if pass { "pass" } else { "FAIL" }, detail.chars().take(240).collect::<String>());
        self.items.push(json!({"check": check, "pass": pass, "detail": detail}));
    }
    fn all_pass(&self) -> bool {
        self.items.iter().all(|i| i["pass"] == true)
    }
}

#[derive(Default)]
struct Obs {
    prompts: Vec<Value>,
    problems: Vec<String>,
    resolve_error: Option<String>,
    offered: Vec<String>,
    action_offered: Option<bool>,
    edit_texts: Vec<String>,
    edits_json: Vec<Value>,
    preview: String,
    clipboard: Vec<String>,
    diagnostics: Vec<Value>,
    completions: Vec<String>,
    hovers: Vec<String>,
    markers: Vec<Value>,
    nodes: Vec<String>,
    tree_raw: Value,
    designer: Vec<Value>,
}

struct Artifacts {
    dir: PathBuf,
    rel: String,
    list: Vec<Value>,
    scrub: Vec<(String, &'static str)>,
}

impl Artifacts {
    fn put(&mut self, name: &str, bytes: &[u8]) {
        let scrubbed_bytes = std::str::from_utf8(bytes).map(|t| scrubbed(t, &self.scrub).into_bytes());
        let bytes = scrubbed_bytes.as_deref().unwrap_or(bytes);
        let _ = std::fs::create_dir_all(&self.dir);
        if std::fs::write(self.dir.join(name), bytes).is_ok() {
            self.list.push(json!({"path": format!("{}/{name}", self.rel), "sha256": sha(bytes)}));
        }
    }
}

fn snapshot(root: &Path) -> BTreeMap<String, String> {
    let mut files = vec![];
    files_under(root, &mut files);
    files.into_iter().map(|f| (f.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/"), sha(&std::fs::read(&f).unwrap_or_default()))).collect()
}

fn changed_files(before: &BTreeMap<String, String>, after: &BTreeMap<String, String>) -> Vec<Value> {
    let mut out = vec![];
    for (path, hash) in after {
        match before.get(path) {
            None => out.push(json!({"path": path, "change": "created"})),
            Some(old) if old != hash => out.push(json!({"path": path, "change": "modified"})),
            _ => {}
        }
    }
    for path in before.keys().filter(|p| !after.contains_key(*p)) {
        out.push(json!({"path": path, "change": "removed"}));
    }
    out
}

fn toml_value(v: &Value) -> String {
    match v {
        Value::Array(items) => format!("[{}]", items.iter().map(toml_value).collect::<Vec<_>>().join(", ")),
        Value::Object(map) => format!("{{ {} }}", map.iter().map(|(k, x)| format!("{} = {}", Value::String(k.clone()), toml_value(x))).collect::<Vec<_>>().join(", ")),
        Value::Null => "\"\"".into(),
        other => other.to_string(),
    }
}

fn apply_setup(project: &Path, steps: &[Value]) -> Result<Option<String>, String> {
    let mut settings = serde_json::Map::new();
    for step in steps {
        let (kind, x) = step.as_object().and_then(|o| o.iter().next()).ok_or("empty setup step")?;
        let io = |e: std::io::Error| format!("setup {kind}: {e}");
        match kind.as_str() {
            "copy" | "swap" => {
                let (from, to) = if kind == "copy" { (x["from"].as_str().unwrap(), x["to"].as_str().unwrap()) } else { (x["with"].as_str().unwrap(), x["file"].as_str().unwrap()) };
                let to = project.join(to);
                if let Some(parent) = to.parent() {
                    std::fs::create_dir_all(parent).map_err(io)?;
                }
                let source = project.join(from);
                if source.is_dir() {
                    copy_tree(&source, &to, &[]).map_err(io)?;
                } else {
                    std::fs::copy(source, to).map_err(io)?;
                }
            }
            "write" => {
                let to = project.join(x["path"].as_str().unwrap());
                if let Some(parent) = to.parent() {
                    std::fs::create_dir_all(parent).map_err(io)?;
                }
                std::fs::write(to, x["content"].as_str().unwrap()).map_err(io)?;
            }
            "replace" => {
                let file = project.join(x["file"].as_str().unwrap());
                let text = std::fs::read_to_string(&file).map_err(io)?;
                let from = x["from"].as_str().unwrap();
                if !text.contains(from) {
                    return Err(format!("setup replace: {from:?} not found in {}", file.display()));
                }
                std::fs::write(&file, text.replacen(from, x["to"].as_str().unwrap(), 1)).map_err(io)?;
            }
            "delete" => {
                let target = project.join(x.as_str().unwrap());
                if target.is_dir() {
                    std::fs::remove_dir_all(target).map_err(io)?;
                } else {
                    std::fs::remove_file(target).map_err(io)?;
                }
            }
            "settings" => settings.extend(x.as_object().unwrap().clone()),
            _ => unreachable!(),
        }
    }
    if settings.is_empty() {
        return Ok(None);
    }
    let mut text = format!("[plugins.\"{ID}\"]\n");
    for (key, value) in &settings {
        text.push_str(&format!("{} = {}\n", Value::String(key.clone()), toml_value(value)));
    }
    Ok(Some(text))
}

fn char_col(line: &str, byte: usize) -> u32 {
    line[..byte].chars().count() as u32
}

fn anchor_pos(text: &str, anchor: &str, end: bool) -> Result<Pos, String> {
    for (index, line) in text.lines().enumerate() {
        if let Some(at) = line.find(anchor) {
            return Ok(Pos { line: index as u32, col: char_col(line, if end { at + anchor.len() } else { at }) });
        }
    }
    Err(format!("anchor {anchor:?} not found in the target file"))
}

fn locate(text: &str, target: &Value) -> Result<Range, String> {
    let one = |v: &Value| -> u32 { v.as_u64().unwrap_or(1).saturating_sub(1) as u32 };
    if let Some(sel) = target.get("selection").filter(|s| s.is_object()) {
        if let (Some(from), Some(to)) = (sel["fromAnchor"].as_str(), sel["toAnchor"].as_str()) {
            return Ok(Range { start: anchor_pos(text, from, false)?, end: anchor_pos(text, to, true)? });
        }
        return Ok(Range { start: Pos { line: one(&sel["startLine"]), col: one(&sel["startCol"]) }, end: Pos { line: one(&sel["endLine"]), col: one(&sel["endCol"]) } });
    }
    if let Some(p) = target.get("position").filter(|p| p.is_object()) {
        let pos = Pos { line: one(&p["line"]), col: one(&p["col"]) };
        return Ok(Range { start: pos, end: pos });
    }
    if let Some(anchor) = target["anchor"].as_str() {
        let pos = anchor_pos(text, anchor, target["at"].as_str() == Some("end"))?;
        return Ok(Range { start: pos, end: pos });
    }
    Ok(Range::default())
}

fn language_for(path: &Path, target: &Value) -> String {
    target["language"].as_str().map(str::to_owned).unwrap_or_else(|| match path.extension().and_then(|e| e.to_str()) {
        Some("sql") => "sql".into(),
        Some("properties") => "properties".into(),
        _ => language_of(path).into(),
    })
}

fn source_files(project: &Path) -> Vec<PathBuf> {
    let mut all = vec![];
    files_under(project, &mut all);
    all.retain(|p| p.extension().is_some_and(|e| e == "java" || e == "kt" || e == "xml") && p.file_name().is_some_and(|n| n != "pom.xml"));
    all
}

fn warm(rt: &Rt, files: &[PathBuf]) {
    for path in files {
        if let Ok(doc) = provider_doc(&rt.host, path, language_of(path)) {
            rt.plugins.provide_diagnostics(&doc, &ide_core::CancellationToken::new());
        }
    }
    let events = files.iter().map(|p| WorkbenchEvent::FileOpened(p.display().to_string())).collect();
    rt.plugins.post_events(Some(CLIENT), events);
    rt.plugins.flush_events();
}

struct ActionRun {
    resolved: Result<CodeAction, String>,
    seen: Vec<Value>,
    problems: Vec<String>,
}

type Hook = Option<(usize, Box<dyn FnOnce() + Send>)>;

fn run_action(rt: &Rt, invocation: &Invocation, path: &Path, lang: &str, range: Range, title: &str, answers: Vec<Answer>, hook: Hook) -> R<Result<ActionRun, Vec<String>>> {
    let doc = provider_doc(&rt.host, path, lang)?;
    let offered: Vec<(String, CodeAction)> = rt.plugins.provide_code_actions(&doc, range).into_iter().filter(|(o, _)| o == ID).collect();
    let Some(action) = offered.iter().find(|(_, a)| a.title == title).map(|(_, a)| a.clone()) else {
        return Ok(Err(offered.into_iter().map(|(_, a)| a.title).collect()));
    };
    *rt.script.lock().unwrap() = Script { answers: answers.into(), seen: vec![], problems: vec![], hook };
    let resolved = rt.plugins.resolve_code_action(ID, &doc, action, invocation.clone()).map_err(|e| e.to_string());
    let mut script = rt.script.lock().unwrap();
    Ok(Ok(ActionRun { resolved, seen: std::mem::take(&mut script.seen), problems: std::mem::take(&mut script.problems) }))
}

fn edit_texts(action: &CodeAction) -> (Vec<String>, Vec<Value>) {
    let mut texts = vec![];
    let mut json = vec![];
    for operation in &action.operations {
        match operation {
            FileOperation::Create { path, text, overwrite } => {
                texts.push(text.clone());
                json.push(json!({"op": "create", "path": path, "overwrite": overwrite, "text": text}));
            }
            FileOperation::Rename { from, to } => json.push(json!({"op": "rename", "from": from, "to": to})),
            FileOperation::Delete { path } => json.push(json!({"op": "delete", "path": path})),
        }
    }
    for (path, edits) in &action.edits {
        for edit in edits {
            texts.push(edit.new_text.clone());
            json.push(json!({"op": "edit", "path": path, "range": [edit.range.start.line, edit.range.start.col, edit.range.end.line, edit.range.end.col], "newText": edit.new_text}));
        }
    }
    (texts, json)
}

fn workspace_edit(action: &CodeAction) -> WorkspaceEdit {
    let mut files = vec![];
    for operation in &action.operations {
        match operation {
            FileOperation::Create { path, text, overwrite } => {
                files.push(FileEdit::Create { path: path.clone().into(), overwrite: *overwrite });
                files.push(FileEdit::Edit { path: path.clone().into(), edits: vec![TextEdit { range: Range::default(), new_text: text.clone() }], version: None });
            }
            FileOperation::Rename { from, to } => files.push(FileEdit::Rename { from: from.clone().into(), to: to.clone().into() }),
            FileOperation::Delete { path } => files.push(FileEdit::Delete { path: path.clone().into(), recursive: false }),
        }
    }
    for (path, edits) in &action.edits {
        files.push(FileEdit::Edit { path: path.clone().into(), edits: edits.clone(), version: None });
    }
    WorkspaceEdit { files, ..Default::default() }
}

fn preview_of(seen: &[Value]) -> String {
    seen.iter().filter(|p| p["kind"] == "confirm").filter_map(|p| p["message"].as_str()).collect::<Vec<_>>().join("\n=====\n")
}

fn prompt_summary(p: &Value) -> String {
    let title = p["title"].as_str().or(p["prompt"].as_str()).unwrap_or("");
    let options: Vec<&str> = p["items"].as_array().map(|a| a.iter().filter_map(|i| i["key"].as_str()).collect()).unwrap_or_default();
    format!("{} {title:?} options {options:?}", p["kind"].as_str().unwrap_or("?"))
}

// ---------------------------------------------------------------------------------------------------------------

fn evaluate(expect: &Value, obs: &Obs, project: &Path, before: &BTreeMap<String, String>, after: &BTreeMap<String, String>, applied_events: usize, res: &mut Results) {
    let item_matches = |item: &Value, wanted: &str| item["key"].as_str() == Some(wanted) || item["label"].as_str().is_some_and(|l| l.contains(wanted));
    for spec in expect["designerReplies"].as_array().cloned().unwrap_or_default() {
        let number = spec["message"].as_u64().unwrap_or(obs.designer.len() as u64) as usize;
        let name = spec["name"].as_str().map(str::to_owned).unwrap_or_else(|| format!("designer message {number}: {}", spec["path"].as_str().unwrap_or("handler")));
        let Some(step) = number.checked_sub(1).and_then(|i| obs.designer.get(i)) else {
            res.add(&name, false, "that message was not sent");
            continue;
        };
        if let Some(wanted) = spec["handlerError"].as_str() {
            let error = step["error"].as_str().unwrap_or("");
            res.add(&name, !error.is_empty() && error.contains(wanted), format!("handler error {error:?}, wanted {wanted:?}"));
            continue;
        }
        let Some(reply) = step["reply"].as_object().map(|_| &step["reply"]) else {
            res.add(&name, false, format!("no reply to message {number} (handler error {:?})", step["error"]));
            continue;
        };
        let got = json_at(reply, spec["path"].as_str().unwrap_or(""));
        let shown = got.map(|g| g.to_string().chars().take(160).collect::<String>()).unwrap_or_else(|| "<absent>".into());
        let items = || got.and_then(Value::as_array).cloned().unwrap_or_default();
        let pass = if let Some(want) = spec.get("equals") {
            got == Some(want)
        } else if let Some(want) = spec["exists"].as_bool() {
            got.is_some_and(|g| !g.is_null()) == want
        } else if let Some(want) = spec.get("contains") {
            match got {
                Some(Value::String(text)) => want.as_str().is_some_and(|w| text.contains(w)),
                Some(Value::Array(list)) => list.contains(want),
                _ => false,
            }
        } else if let Some(rx) = spec["matches"].as_str() {
            got.and_then(Value::as_str).is_some_and(|t| Regex::new(rx).is_ok_and(|r| r.is_match(t)))
        } else if let Some(n) = spec["length"].as_u64() {
            got.is_some_and(|g| g.as_array().map(Vec::len).or(g.as_str().map(str::len)).or(g.as_object().map(|o| o.len())) == Some(n as usize))
        } else if let Some(want) = spec.get("anyItem") {
            items().iter().any(|i| subset(i, want))
        } else if let Some(want) = spec.get("noItem") {
            got.is_some() && !items().iter().any(|i| subset(i, want))
        } else if let Some(want) = spec.get("countWhere") {
            items().iter().filter(|i| subset(i, &want["where"])).count() as u64 == want["count"].as_u64().unwrap_or(u64::MAX)
        } else {
            false
        };
        res.add(&name, pass, format!("{} = {shown}", spec["path"].as_str().unwrap_or("")));
    }
    if let Some(count) = expect["promptCount"].as_u64() {
        res.add("prompt count", obs.prompts.len() as u64 == count, format!("expected {count}, saw {}", obs.prompts.len()));
    }
    for (i, spec) in expect["promptSequence"].as_array().cloned().unwrap_or_default().iter().enumerate() {
        let Some(p) = obs.prompts.get(i) else {
            res.add(&format!("prompt {} exists", i + 1), false, format!("only {} prompts were shown", obs.prompts.len()));
            continue;
        };
        let mut failures = vec![];
        let kind_ok = match spec["kind"].as_str() {
            Some("pick") => p["kind"] == "quickPick" && p["multiple"] == false,
            Some("pickMany") => p["kind"] == "quickPick" && p["multiple"] == true,
            Some("input") => p["kind"] == "input",
            Some("confirm") => p["kind"] == "confirm",
            None => true,
            Some(other) => {
                failures.push(format!("unknown kind {other}"));
                false
            }
        };
        if !kind_ok {
            failures.push(format!("kind is {}", p["kind"]));
        }
        let title = p["title"].as_str().or(p["prompt"].as_str()).unwrap_or("").to_lowercase();
        if let Some(t) = spec["titleContains"].as_str() {
            if !title.contains(&t.to_lowercase()) {
                failures.push(format!("title {title:?} lacks {t:?}"));
            }
        }
        if let Some(t) = spec["messageContains"].as_str() {
            if !p["message"].as_str().unwrap_or("").contains(t) {
                failures.push(format!("message lacks {t:?}"));
            }
        }
        if let Some(default) = spec["defaultIs"].as_str() {
            if p["value"].as_str() != Some(default) {
                failures.push(format!("default value is {} not {default:?}", p["value"]));
            }
        }
        let items = p["items"].as_array().cloned().unwrap_or_default();
        for wanted in strs(&spec["optionsInclude"]) {
            if !items.iter().any(|it| item_matches(it, &wanted)) {
                failures.push(format!("option {wanted:?} absent"));
            }
        }
        if let Some(wanted) = spec["itemDescriptions"].as_object() {
            for (key, text) in wanted {
                let text = text.as_str().unwrap_or("");
                if !items.iter().any(|it| it["key"].as_str() == Some(key.as_str()) && it["description"].as_str().is_some_and(|d| d.contains(text))) {
                    failures.push(format!("option {key:?} has no description containing {text:?}"));
                }
            }
        }
        for banned in strs(&spec["optionsExclude"]) {
            if items.iter().any(|it| item_matches(it, &banned)) {
                failures.push(format!("option {banned:?} present"));
            }
        }
        res.add(&format!("prompt {} {}", i + 1, spec["name"].as_str().unwrap_or("matches the documented shape")), failures.is_empty(), if failures.is_empty() { prompt_summary(p) } else { format!("{failures:?}; actual {}", prompt_summary(p)) });
    }
    if let Some(wanted) = expect["actionOffered"].as_bool() {
        res.add("action offered state", obs.action_offered == Some(wanted), format!("expected {wanted}; offered {:?}", obs.offered));
    }
    if let Some(text) = expect["resolveErrorContains"].as_str() {
        let got = obs.resolve_error.clone().unwrap_or_default();
        res.add("action is refused with a message", got.to_lowercase().contains(&text.to_lowercase()), format!("error {got:?}"));
    }
    let combined = obs.edit_texts.join("\n");
    for rx in strs(&expect["editsContain"]) {
        let matched = Regex::new(&rx).map(|r| r.is_match(&combined)).unwrap_or(false);
        res.add(&format!("resolved edits contain /{rx}/"), matched, format!("{} edit texts, {} bytes", obs.edit_texts.len(), combined.len()));
    }
    let files: Vec<Value> = match &expect["resultFile"] {
        Value::Array(a) => a.clone(),
        o @ Value::Object(_) => vec![o.clone()],
        _ => vec![],
    };
    for f in files {
        let path = f["path"].as_str().unwrap_or("");
        let text = std::fs::read_to_string(project.join(path));
        let mut failures = vec![];
        match &text {
            Err(e) => failures.push(format!("cannot read: {e}")),
            Ok(text) => {
                failures.extend(strs(&f["contains"]).into_iter().filter(|s| !text.contains(s)).map(|s| format!("lacks {s:?}")));
                failures.extend(strs(&f["notContains"]).into_iter().filter(|s| text.contains(s)).map(|s| format!("unexpectedly contains {s:?}")));
                failures.extend(strs(&f["matches"]).into_iter().filter(|rx| !Regex::new(rx).map(|r| r.is_match(text)).unwrap_or(false)).map(|rx| format!("no match for /{rx}/")));
            }
        }
        res.add(&format!("result file {path}"), failures.is_empty(), if failures.is_empty() { format!("{} bytes", text.map(|t| t.len()).unwrap_or(0)) } else { failures.join("; ") });
    }
    for f in expect["createdFiles"].as_array().cloned().unwrap_or_default() {
        let path = f["path"].as_str().unwrap_or("");
        let created = !before.contains_key(path) && after.contains_key(path);
        let text = std::fs::read_to_string(project.join(path)).unwrap_or_default();
        let missing: Vec<String> = strs(&f["contains"]).into_iter().filter(|s| !text.contains(s)).collect();
        res.add(&format!("created file {path}"), created && missing.is_empty(), format!("created {created}; missing {missing:?}"));
    }
    if expect["noFilesChanged"] == true {
        res.add("no files changed", before == after && applied_events == 0, format!("{} changed, {applied_events} workspace edits applied", changed_files(before, after).len()));
    }
    for want in expect["diagnostics"].as_array().cloned().unwrap_or_default() {
        let hit = obs.diagnostics.iter().any(|d| diag_matches(d, &want));
        res.add(&format!("diagnostic {} {}:{}", want["rule"].as_str().unwrap_or("?"), want["file"].as_str().unwrap_or("?"), want["line"]), hit, format!("{} diagnostics in total", obs.diagnostics.len()));
    }
    for want in expect["diagnosticsAbsent"].as_array().cloned().unwrap_or_default() {
        let hit = obs.diagnostics.iter().any(|d| diag_matches(d, &want));
        res.add(&format!("no diagnostic {} {}", want["rule"].as_str().unwrap_or("?"), want["file"].as_str().unwrap_or("?")), !hit, "absence check".to_owned());
    }
    for want in strs(&expect["completionsInclude"]) {
        res.add(&format!("completion offers {want:?}"), obs.completions.iter().any(|c| c.contains(&want)), format!("{} items: {:?}", obs.completions.len(), obs.completions.iter().take(8).collect::<Vec<_>>()));
    }
    for banned in strs(&expect["completionsExclude"]) {
        res.add(&format!("completion does not offer {banned:?}"), !obs.completions.iter().any(|c| c.contains(&banned)), format!("{} items", obs.completions.len()));
    }
    let hover = obs.hovers.join("\n");
    for want in strs(&expect["hoverContains"]) {
        res.add(&format!("hover contains {want:?}"), hover.contains(&want), format!("{} hovers, {} bytes", obs.hovers.len(), hover.len()));
    }
    let clip = obs.clipboard.join("\n----\n");
    for want in strs(&expect["clipboardContains"]) {
        res.add(&format!("clipboard contains {want:?}"), clip.to_lowercase().contains(&want.to_lowercase()), format!("{} clipboard writes, {} bytes", obs.clipboard.len(), clip.len()));
    }
    for want in strs(&expect["treeAbsent"]) {
        res.add(&format!("tree has no node {want:?}"), !obs.nodes.iter().any(|n| n.contains(&want)), format!("{} nodes", obs.nodes.len()));
    }
    for chain in expect["treeChildren"].as_array().cloned().unwrap_or_default() {
        let labels = strs(&chain);
        let items = obs.tree_raw["body"]["value"].as_array().cloned().unwrap_or_default();
        let mut parents: Vec<Option<String>> = vec![None];
        for label in &labels {
            let next: Vec<Option<String>> = items
                .iter()
                .filter(|n| n["label"].as_str() == Some(label.as_str()) && parents.iter().any(|p| p.as_deref() == n["parent"].as_str()))
                .map(|n| n["key"].as_str().map(str::to_owned))
                .collect();
            parents = next;
        }
        res.add(&format!("tree path {}", labels.join(" > ")), !parents.is_empty(), format!("{} nodes", items.len()));
    }
    for want in strs(&expect["treeNodes"]) {
        res.add(&format!("tree node {want:?}"), obs.nodes.iter().any(|n| n.contains(&want)), format!("{} nodes", obs.nodes.len()));
    }
    for want in strs(&expect["markersInclude"]) {
        let hit = obs.markers.iter().any(|m| m["tooltip"].as_str().is_some_and(|t| t.contains(&want)));
        res.add(&format!("line marker {want:?}"), hit, format!("{} markers", obs.markers.len()));
    }
    if let Some(golden) = expect.get("goldenFile").filter(|g| g.is_object()) {
        let expected = golden["file"].as_str().and_then(|f| std::fs::read(f).ok());
        let actual = golden["actual"].as_str().and_then(|f| std::fs::read(project.join(f)).ok());
        res.add("own-output snapshot (ownOutputSnapshot: true, NOT documentation evidence)", expected.is_some() && expected == actual, "byte comparison".to_owned());
    }
}

fn diag_matches(d: &Value, want: &Value) -> bool {
    want["rule"].as_str().is_none_or(|r| d["rule"].as_str() == Some(r))
        && want["file"].as_str().is_none_or(|f| d["file"].as_str().is_some_and(|x| x.ends_with(f)))
        && want["line"].as_u64().is_none_or(|l| d["line"].as_u64() == Some(l))
}

// ---------------------------------------------------------------------------------------------------------------

async fn run_one(repo: &Path, out: &Path, wasm: &Path, wasm_sha: &str, loaded: &Loaded) -> R<Value> {
    let sc = &loaded.value;
    let id = sc["id"].as_str().unwrap_or("").to_owned();
    let kind = sc["kind"].as_str().unwrap_or("").to_owned();
    let fixture = repo.join(PLUGIN_SOURCE).join("parity/fixtures").join(sc["fixture"].as_str().unwrap_or(""));
    let tmp = tempfile::tempdir()?;
    let base = tmp.path().canonicalize()?;
    let project = base.join("project");
    copy_tree(&fixture, &project, SKIP_COPY)?;
    let fixture_before = tree_hash(&project);
    let steps = sc["setup"].as_array().cloned().unwrap_or_default();
    let settings = apply_setup(&project, &steps)?;
    let mut setup_hash = tree_hash(&project);
    let mut before = snapshot(&project);
    let scrub = vec![(project.display().to_string(), "$PROJECT"), (base.display().to_string(), "$TMP")];
    let mut res = Results { items: vec![], scrub: scrub.clone() };
    let mut arts = Artifacts { dir: out.join("artifacts").join(&id), rel: format!("artifacts/{id}"), list: vec![], scrub };
    let rt = boot(&base, &project, repo, wasm, settings.as_deref(), &strs(&sc["grants"]))?;
    res.add("install into the private released host", rt.install_error.is_none(), format!("{:?}", rt.install_error));
    let mut obs = Obs::default();
    let mut after_hash = setup_hash.clone();
    let mut after_files = before.clone();
    let mut restored_hash = None;
    let mut events = 0usize;

    if rt.install_error.is_none() {
        let invocation = Invocation { client: Some(CLIENT.into()), workspace_root: Some(project.clone()), ..Default::default() };
        let files = source_files(&project);
        if sc["warm"] != false {
            warm(&rt, &files);
        }
        for pre in sc["preActions"].as_array().cloned().unwrap_or_default() {
            let pre_target = project.join(pre["target"]["file"].as_str().unwrap_or(""));
            let lang = language_for(&pre_target, &pre["target"]);
            let outcome: R<String> = (|| {
                let doc = provider_doc(&rt.host, &pre_target, &lang)?;
                let range = locate(&doc.text, &pre["target"])?;
                let pre_answers: Vec<Answer> = pre["prompts"].as_array().cloned().unwrap_or_default().iter().map(|v| parse_answer(v).unwrap_or(Answer::Cancel)).collect();
                let run = run_action(&rt, &invocation, &pre_target, &lang, range, pre["title"].as_str().unwrap_or(""), pre_answers, None)?;
                let run = run.map_err(|titles| format!("not offered; offered {titles:?}"))?;
                let action = run.resolved.map_err(|e| format!("did not resolve: {e}; prompts {:?}", run.seen.iter().map(prompt_summary).collect::<Vec<_>>()))?;
                let documents = rt.host.project(CLIENT)?.documents.clone();
                let mut edit = workspace_edit(&action);
                documents.prepare_workspace_edit(&mut edit)?;
                documents.apply_workspace_edit(&edit)?;
                Ok(format!("{} prompts, {} operations", run.seen.len(), action.operations.len()))
            })();
            res.add(&format!("pre-action {:?} applied", pre["title"].as_str().unwrap_or("")), outcome.is_ok(), outcome.unwrap_or_else(|e| e.to_string()));
        }
        if let Some(steps) = sc["postSetup"].as_array() {
            let snapshot_before_post = snapshot(&project);
            if let Err(e) = apply_setup(&project, steps) {
                res.add("postSetup applied", false, e);
            }
            // the headless host has no file watcher: an already opened buffer would shadow the new disk text
            if let Ok(documents) = rt.host.project(CLIENT).map(|p| p.documents.clone()) {
                for change in changed_files(&snapshot_before_post, &snapshot(&project)) {
                    let path = project.join(change["path"].as_str().unwrap_or(""));
                    if change["change"] == "modified" {
                        if let (Ok(info), Ok(text)) = (documents.open(&path), std::fs::read_to_string(&path)) {
                            let _ = documents.replace_all(info.id, &text);
                            let _ = documents.save(info.id);
                        }
                    }
                }
            }
        }
        if sc["preActions"].is_array() || sc["postSetup"].is_array() {
            setup_hash = tree_hash(&project);
            before = snapshot(&project);
            after_files = before.clone();
            after_hash = setup_hash.clone();
            warm(&rt, &source_files(&project));
        }
        let answers: Vec<Answer> = sc["prompts"].as_array().cloned().unwrap_or_default().iter().map(|v| parse_answer(v).unwrap_or(Answer::Cancel)).collect();
        let target_rel = sc["target"]["file"].as_str().unwrap_or("").to_owned();
        let target = project.join(&target_rel);
        let outcome: R<()> = async {
            match kind.as_str() {
                "fileaction" => {
                    let title = sc["action"]["title"].as_str().unwrap_or("");
                    let lang = language_for(&target, &sc["target"]);
                    let doc = provider_doc(&rt.host, &target, &lang)?;
                    let range = locate(&doc.text, &sc["target"])?;
                    let original_text = std::fs::read_to_string(&target)?;
                    let doc_id = rt.host.project(CLIENT)?.documents.open(&target)?.id;
                    let documents = rt.host.project(CLIENT)?.documents.clone();
                    let happy = run_action(&rt, &invocation, &target, &lang, range, title, answers.clone(), None)?;
                    let happy = match happy {
                        Ok(run) => run,
                        Err(titles) => {
                            obs.action_offered = Some(false);
                            obs.offered = titles;
                            return Ok(());
                        }
                    };
                    obs.action_offered = Some(true);
                    obs.prompts = happy.seen.clone();
                    obs.problems = happy.problems.clone();
                    obs.preview = preview_of(&happy.seen);
                    let mut applied_ok = false;
                    match &happy.resolved {
                        Err(e) => obs.resolve_error = Some(e.clone()),
                        Ok(action) => {
                            let (texts, json) = edit_texts(action);
                            obs.edit_texts = texts;
                            obs.edits_json = json;
                            let mut edit = workspace_edit(action);
                            let before_events = rt.applied.lock().unwrap().len();
                            documents.prepare_workspace_edit(&mut edit)?;
                            documents.apply_workspace_edit(&edit)?;
                            applied_ok = true;
                            events = rt.applied.lock().unwrap().len() - before_events;
                        }
                    }
                    after_files = snapshot(&project);
                    after_hash = tree_hash(&project);
                    let want_checks = sc["checks"].clone();
                    let flag = |k: &str| want_checks[k] == true;
                    if flag("preview") || want_checks["preview"].is_object() {
                        let only = strs(&want_checks["preview"]["paths"]);
                        let missing: Vec<String> = obs
                            .edits_json
                            .iter()
                            .filter(|e| only.is_empty() || e["path"].as_str().is_some_and(|p| only.iter().any(|o| p.contains(o.as_str()))))
                            .filter_map(|e| e["text"].as_str().or(e["newText"].as_str()))
                            .map(str::trim)
                            .filter(|t| !t.is_empty() && !obs.preview.contains(*t))
                            .map(|t| t.chars().take(60).collect())
                            .collect();
                        res.add("confirm preview contains every resulting changed text", applied_ok && !obs.preview.is_empty() && missing.is_empty(), format!("preview {} bytes, {} edits, restricted to {only:?}, missing {missing:?}", obs.preview.len(), obs.edits_json.len()));
                    }
                    // expectations are evaluated while the applied state is on disk; restoring happens afterwards
                    evaluate(&sc["expect"], &obs, &project, &before, &after_files, events, &mut res);
                    for p in &happy.problems {
                        res.add("scripted answer accepted", false, p.clone());
                    }
                    if let (Some(error), true) = (&obs.resolve_error, sc["expect"]["resolveErrorContains"].is_null()) {
                        res.add("action resolved to edits", false, error.clone());
                    }
                    if flag("undo") || flag("cancelEach") || flag("stale") {
                        if applied_ok {
                            let undo_id = rt.applied.lock().unwrap().iter().rev().nth(events.saturating_sub(1)).copied();
                            let undone = undo_id.map(|u| documents.undo_workspace_edit(u));
                            let _ = documents.save(doc_id);
                            let hash = tree_hash(&project);
                            restored_hash = Some(hash.clone());
                            let ok = matches!(undone, Some(Ok(_))) && hash == setup_hash;
                            if flag("undo") {
                                let diff = changed_files(&before, &snapshot(&project));
                                res.add("native grouped Undo restores the disposable copy", ok, format!("undo {:?}; tree {hash} vs setup {setup_hash}; still different {diff:?}", undone.map(|u| u.map_err(|e| e.to_string()))));
                            }
                        } else if flag("undo") {
                            res.add("native grouped Undo restores the disposable copy", false, "nothing was applied".to_owned());
                        }
                    }
                    let n = happy.seen.len();
                    if flag("cancelEach") {
                        for stop in 0..n {
                            let mut a: Vec<Answer> = answers.iter().take(stop).cloned().collect();
                            a.push(Answer::Cancel);
                            let ev = rt.applied.lock().unwrap().len();
                            let run = run_action(&rt, &invocation, &target, &lang, range, title, a, None)?;
                            let ok = match run {
                                Ok(r) => r.resolved.is_err() && r.problems.is_empty() && r.seen.len() == stop + 1 && tree_hash(&project) == setup_hash && rt.applied.lock().unwrap().len() == ev,
                                Err(_) => false,
                            };
                            res.add(&format!("Cancel at prompt {} of {n} leaves the tree unchanged", stop + 1), ok, "no edit applied, tree hash equals the setup state".to_owned());
                        }
                    }
                    if flag("stale") {
                        let points: Vec<(String, usize)> = if n > 1 { vec![("after the first prompt".into(), 1), ("at the final prompt".into(), n - 1)] } else { vec![("at the only prompt".into(), 0)] };
                        for (label, at) in points {
                            let d = documents.clone();
                            let hook: Box<dyn FnOnce() + Send> = Box::new(move || {
                                let _ = d.apply_edits(doc_id, &[TextEdit { range: Range::default(), new_text: "\n".into() }]);
                            });
                            let ev = rt.applied.lock().unwrap().len();
                            let run = run_action(&rt, &invocation, &target, &lang, range, title, answers.clone(), Some((at, hook)))?;
                            let (refused, detail) = match run {
                                Ok(r) => match &r.resolved {
                                    Err(m) => (m.to_lowercase().contains("stale") || m.to_lowercase().contains("changed"), m.clone()),
                                    Ok(_) => (false, "the action resolved against a changed buffer".to_owned()),
                                },
                                Err(_) => (false, "action not offered".to_owned()),
                            };
                            let disk = std::fs::read_to_string(&target).unwrap_or_default();
                            res.add(&format!("stale buffer edit {label} is refused"), refused && rt.applied.lock().unwrap().len() == ev && disk == original_text, detail);
                            documents.replace_all(doc_id, &original_text)?;
                            documents.save(doc_id)?;
                        }
                    }
                    if restored_hash.is_some() || flag("cancelEach") || flag("stale") {
                        res.add("tree is back to the setup state at the end", tree_hash(&project) == setup_hash, "restored".to_owned());
                    }
                    arts.put("prompts.json", &serde_json::to_vec_pretty(&obs.prompts)?);
                    arts.put("preview.txt", obs.preview.as_bytes());
                    arts.put("edits.json", &serde_json::to_vec_pretty(&obs.edits_json)?);
                    return Ok(());
                }
                "diagnostics" => {
                    let mut rows = vec![];
                    for pass in 0..2 {
                        rows.clear();
                        for path in &files {
                            let doc = provider_doc(&rt.host, path, language_of(path))?;
                            if pass == 0 {
                                rt.plugins.post_events(Some(CLIENT), vec![WorkbenchEvent::FileOpened(path.display().to_string())]);
                                rt.plugins.flush_events();
                            }
                            for (owner, found) in rt.plugins.provide_diagnostics(&doc, &ide_core::CancellationToken::new()) {
                                if owner != ID {
                                    continue;
                                }
                                for d in found {
                                    rows.push(json!({"rule": d.code.clone().unwrap_or_default(), "file": path.strip_prefix(&project).unwrap().to_string_lossy(), "line": d.range.start.line + 1, "severity": format!("{:?}", d.severity), "message": d.message}));
                                }
                            }
                        }
                    }
                    obs.diagnostics = rows;
                    arts.put("diagnostics.json", &serde_json::to_vec_pretty(&obs.diagnostics)?);
                }
                "completion" | "hover" | "lineMarkers" => {
                    let lang = language_for(&target, &sc["target"]);
                    let doc = provider_doc(&rt.host, &target, &lang)?;
                    let range = locate(&doc.text, &sc["target"])?;
                    if kind == "completion" {
                        obs.completions = rt.plugins.provide_completion(&doc, range.start, None).into_iter().map(|(_, i)| i.label).collect();
                        arts.put("completions.json", &serde_json::to_vec_pretty(&obs.completions)?);
                    } else if kind == "hover" {
                        obs.hovers = rt.plugins.provide_hover(&doc, range.start).into_iter().map(|h| h.markdown).collect();
                        arts.put("hover.md", obs.hovers.join("\n=====\n").as_bytes());
                    } else {
                        obs.markers = rt.plugins.provide_line_markers(&doc).into_iter().map(|(_, m)| json!({"line": m.line + 1, "icon": m.icon, "tooltip": m.tooltip})).collect();
                        arts.put("markers.json", &serde_json::to_vec_pretty(&obs.markers)?);
                    }
                }
                "command" => {
                    let editor = if target_rel.is_empty() {
                        None
                    } else {
                        let lang = language_for(&target, &sc["target"]);
                        let doc = provider_doc(&rt.host, &target, &lang)?;
                        let range = locate(&doc.text, &sc["target"])?;
                        Some(ide_plugin::EditorContext { path: target.clone(), language: lang, selections: vec![range] })
                    };
                    let with_editor = Invocation { editor, ..invocation.clone() };
                    *rt.script.lock().unwrap() = Script { answers: answers.clone().into(), ..Script::default() };
                    let command = sc["action"]["command"].as_str().unwrap_or("");
                    let arg = sc["action"]["arg"].as_str().map(|a| a.replace("$PROJECT", &project.display().to_string()));
                    let result = rt.plugins.run_command(ID, command, arg.as_deref(), with_editor);
                    let mut script = rt.script.lock().unwrap();
                    obs.prompts = std::mem::take(&mut script.seen);
                    obs.problems = std::mem::take(&mut script.problems);
                    drop(script);
                    obs.clipboard = rt.clipboard.lock().unwrap().clone();
                    if let Err(e) = &result {
                        obs.resolve_error = Some(e.to_string());
                    }
                    res.add("command ran", result.is_ok(), format!("{:?}", result.as_ref().err()));
                    arts.put("prompts.json", &serde_json::to_vec_pretty(&obs.prompts)?);
                    arts.put("clipboard.txt", obs.clipboard.join("\n----\n").as_bytes());
                }
                "tree" => {
                    let view = sc["action"]["view"].as_str().unwrap_or("");
                    let editor = if target_rel.is_empty() {
                        None
                    } else {
                        let lang = language_for(&target, &sc["target"]);
                        let doc = provider_doc(&rt.host, &target, &lang)?;
                        let range = locate(&doc.text, &sc["target"])?;
                        Some(ide_plugin::EditorContext { path: target.clone(), language: lang, selections: vec![range] })
                    };
                    let with_editor = Invocation { editor, ..invocation.clone() };
                    *rt.script.lock().unwrap() = Script { answers: answers.clone().into(), ..Script::default() };
                    if sc["action"]["viewEvents"].as_array().is_some_and(|events| !events.is_empty()) {
                        let _ = rt.plugins.render_view(ID, view, None, with_editor.clone());
                    }
                    for event in sc["action"]["viewEvents"].as_array().cloned().unwrap_or_default() {
                        let event = tree_event(&event, &project.display().to_string());
                        let sent = rt.plugins.view_event(ID, view, event.clone(), with_editor.clone());
                        res.add(&format!("view event {event:?} handled"), sent.is_ok(), format!("{:?}", sent.as_ref().err()));
                    }
                    {
                        let mut script = rt.script.lock().unwrap();
                        obs.prompts = std::mem::take(&mut script.seen);
                        obs.problems = std::mem::take(&mut script.problems);
                    }
                    arts.put("prompts.json", &serde_json::to_vec_pretty(&obs.prompts)?);
                    after_files = snapshot(&project);
                    after_hash = tree_hash(&project);
                    let model = rt.plugins.render_view(ID, view, None, with_editor.clone());
                    match model {
                        Ok(model) => {
                            obs.tree_raw = serde_json::to_value(&model)?;
                            let nodes = obs.tree_raw["body"]["value"].as_array().cloned().unwrap_or_default();
                            obs.nodes = nodes.iter().map(|n| format!("{} {}", n["label"].as_str().unwrap_or(""), n["detail"].as_str().unwrap_or(""))).collect();
                            arts.put("tree-labels.json", &serde_json::to_vec_pretty(&obs.nodes)?);
                        }
                        Err(e) => res.add("view renders a model", false, e.to_string()),
                    }
                }
                "designer" => {
                    let view = sc["action"]["view"].as_str().unwrap_or("");
                    let editor = if target_rel.is_empty() {
                        None
                    } else {
                        let lang = language_for(&target, &sc["target"]);
                        let doc = provider_doc(&rt.host, &target, &lang)?;
                        let range = locate(&doc.text, &sc["target"])?;
                        Some(ide_plugin::EditorContext { path: target.clone(), language: lang, selections: vec![range] })
                    };
                    let with_editor = Invocation { editor, ..invocation.clone() };
                    *rt.script.lock().unwrap() = Script { answers: answers.clone().into(), ..Script::default() };
                    let _ = rt.plugins.render_view(ID, view, None, with_editor.clone());
                    let project_text = project.display().to_string();
                    let mut last = Value::Null;
                    let mut index = 0usize;
                    for message in sc["action"]["messages"].as_array().cloned().unwrap_or_default() {
                        if let Some(steps) = message["setup"].as_array() {
                            let snapshot_before = snapshot(&project);
                            if let Err(e) = apply_setup(&project, steps) {
                                res.add("between-message setup applied", false, e);
                            }
                            let documents = rt.host.project(CLIENT)?.documents.clone();
                            for change in changed_files(&snapshot_before, &snapshot(&project)) {
                                let path = project.join(change["path"].as_str().unwrap_or(""));
                                if change["change"] == "modified" {
                                    if let (Ok(info), Ok(text)) = (documents.open(&path), std::fs::read_to_string(&path)) {
                                        let _ = documents.replace_all(info.id, &text);
                                        let _ = documents.save(info.id);
                                    }
                                }
                            }
                            continue;
                        }
                        let text = designer_request(&message, index, &project_text, &last);
                        index += 1;
                        rt.view_messages.lock().unwrap().clear();
                        let sent = rt.plugins.view_event(ID, view, ViewEvent::Message { text: text.clone() }, with_editor.clone());
                        let replies: Vec<Value> = std::mem::take(&mut *rt.view_messages.lock().unwrap())
                            .into_iter()
                            .filter(|(v, _)| v == view)
                            .filter_map(|(_, m)| serde_json::from_str(&m).ok())
                            .collect();
                        let reply = replies.iter().rev().find(|r| r.get("frame").is_some()).cloned().unwrap_or(Value::Null);
                        if !reply.is_null() {
                            last = reply.clone();
                        }
                        obs.designer.push(json!({
                            "message": index, "request": serde_json::from_str::<Value>(&text).unwrap_or(Value::Null),
                            "handled": sent.is_ok(), "error": sent.as_ref().err().map(|e| e.to_string()), "replyCount": replies.len(), "reply": reply,
                        }));
                    }
                    arts.put("designer-exchange.json", &serde_json::to_vec_pretty(&obs.designer)?);
                    after_files = snapshot(&project);
                    after_hash = tree_hash(&project);
                }
                _ => {}
            }
            Ok(())
        }
        .await;
        if let Err(e) = outcome {
            res.add("scenario ran without a harness error", false, e.to_string());
        }
        if kind != "fileaction" {
            if kind == "command" {
                after_files = snapshot(&project);
                after_hash = tree_hash(&project);
            }
            evaluate(&sc["expect"], &obs, &project, &before, &after_files, events, &mut res);
            for p in &obs.problems {
                res.add("scripted answer accepted", false, p.clone());
            }
        } else if obs.action_offered == Some(false) {
            evaluate(&sc["expect"], &obs, &project, &before, &after_files, events, &mut res);
            if sc["expect"]["actionOffered"].as_bool() != Some(false) {
                res.add("action is offered at the target", false, format!("{:?} not among {:?}", sc["action"]["title"], obs.offered));
            }
        }
    }
    // artifacts for any file a check read
    for f in match &sc["expect"]["resultFile"] {
        Value::Array(a) => a.clone(),
        o @ Value::Object(_) => vec![o.clone()],
        _ => vec![],
    } {
        if let Some(p) = f["path"].as_str() {
            if let Ok(b) = std::fs::read(project.join(p)) {
                arts.put(&format!("result-{}", p.replace('/', "__")), &b);
            }
        }
    }
    let status = if res.all_pass() { "pass" } else { "fail" };
    Ok(json!({
        "scenario": id, "kind": kind, "rows": sc["rows"], "claimIds": sc["claimIds"], "kindExerciser": sc["kindExerciser"] == true,
        "scenarioSha256": loaded.sha, "wasmSha256": wasm_sha, "hostRev": HOST_REV, "fixture": sc["fixture"],
        "fixtureTreeHashBefore": fixture_before, "setupTreeHash": setup_hash, "fixtureTreeHashAfter": after_hash, "treeHashAfterRestore": restored_hash,
        "changedFiles": changed_files(&before, &after_files),
        "scope": "L1 headless released host; structural expectations taken from the documented claims; no parity claim",
        "results": res.items, "status": status, "artifacts": arts.list,
    }))
}

/// Returns true when any scenario failed.
pub(crate) async fn run_all(repo: &Path, out: &Path, receipts: &Path, wasm: &Path, scenarios: &[Loaded]) -> R<bool> {
    std::fs::create_dir_all(receipts)?;
    let wasm_sha = sha(&std::fs::read(wasm)?);
    let mut summary = vec![];
    let mut failed = false;
    for loaded in scenarios {
        let id = loaded.value["id"].as_str().unwrap_or("?").to_owned();
        println!("== {id} {} [{}]", loaded.value["kind"].as_str().unwrap_or(""), strs(&loaded.value["claimIds"]).join(","));
        let receipt = match run_one(repo, out, wasm, &wasm_sha, loaded).await {
            Ok(r) => r,
            Err(e) => json!({"scenario": id, "status": "fail", "results": [{"check": "scenario aborted", "pass": false, "detail": e.to_string()}], "wasmSha256": wasm_sha, "hostRev": HOST_REV, "claimIds": loaded.value["claimIds"], "rows": loaded.value["rows"]}),
        };
        let pass = receipt["status"] == "pass";
        failed |= !pass;
        println!("{} {id}", if pass { "PASS" } else { "FAIL" });
        std::fs::write(receipts.join(format!("{id}.json")), serde_json::to_vec_pretty(&receipt)?)?;
        summary.push(json!({"scenario": id, "status": receipt["status"], "claimIds": loaded.value["claimIds"], "checks": receipt["results"].as_array().map(|a| a.len()), "failedChecks": receipt["results"].as_array().map(|a| a.iter().filter(|r| r["pass"] != true).count())}));
    }
    let passed = summary.iter().filter(|s| s["status"] == "pass").count();
    std::fs::write(
        receipts.join("summary.json"),
        serde_json::to_vec_pretty(&json!({"total": summary.len(), "passed": passed, "failed": summary.len() - passed, "wasmSha256": wasm_sha, "hostRev": HOST_REV, "scenarios": summary}))?,
    )?;
    println!("scenarios: {passed}/{} passed", summary.len());
    Ok(failed)
}
