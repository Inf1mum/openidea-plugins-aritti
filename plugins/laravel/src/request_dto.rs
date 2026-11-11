//! Reviewed, non-overwriting DTO creation for an existing source-proven
//! FormRequest. The DTO retains the exact validated payload without inventing
//! scalar casts, requiredness, nested-object types or controller behavior.
use crate::{Index, php, view_creation};
use openidea_plugin::{
    Error, FileKind, Project,
    file_actions::{FileAction, FileOperation},
    providers::Document,
};
const PREFIX: &str = "request-dto:";
fn shape(index: &Index, doc: &Document, owns: &dyn Fn(&str) -> bool) -> Option<(String, String)> {
    if doc.language != "php" || doc.text.len() > 128 * 1024 {
        return None;
    }
    let full = crate::validation_requests::current_class(doc)?;
    if !crate::validation_requests::validated_request(&index.request_fields, doc, &full, owns) {
        return None;
    }
    let (namespace, name) = full.rsplit_once('\\')?;
    if !name.ends_with("Request")
        || name.len() > 96
        || !name.as_bytes().first()?.is_ascii_uppercase()
        || !doc.path.ends_with(&format!("/{name}.php"))
        || !namespace.split('\\').all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        })
    {
        return None;
    }
    Some((name.into(), namespace.into()))
}

fn composer(
    project: &Project,
    root: &str,
    doc: &Document,
    namespace: &str,
) -> Result<String, Error> {
    let file = format!("{root}/composer.json");
    if !crate::owns_current_path(project, &file) {
        return Err(Error::Stale);
    }
    let stat = project.stat(&file)?;
    if stat.kind != FileKind::File || stat.size > 64 * 1024 {
        return Err(Error::Stale);
    }
    let source = project.read_file(&file)?;
    let value = crate::strict_json::parse(&source, 64 * 1024)?;
    let mut selected = None;
    for section in ["autoload", "autoload-dev"] {
        let Some(object) = value.get(section) else {
            continue;
        };
        let object = object.as_object().ok_or(Error::Stale)?;
        if object.contains_key("psr-0")
            || ["files", "classmap"].iter().any(|key| {
                object
                    .get(*key)
                    .is_some_and(|v| !v.as_array().is_some_and(Vec::is_empty))
            })
        {
            return Err(Error::Stale);
        }
        let Some(mappings) = object.get("psr-4") else {
            continue;
        };
        for (prefix, directory) in mappings.as_object().ok_or(Error::Stale)? {
            let full = format!("{namespace}\\");
            if !full
                .to_ascii_lowercase()
                .starts_with(&prefix.to_ascii_lowercase())
            {
                continue;
            }
            if section != "autoload"
                || prefix.is_empty()
                || !prefix.ends_with('\\')
                || !full.starts_with(prefix)
                || selected.is_some()
            {
                return Err(Error::Stale);
            }
            let directory = directory
                .as_str()
                .ok_or(Error::Stale)?
                .trim_end_matches('/');
            if directory.is_empty()
                || directory.starts_with('/')
                || directory.contains('\\')
                || directory.split('/').any(|p| {
                    p.is_empty()
                        || p.starts_with('.')
                        || ["vendor", "node_modules", "storage"].contains(&p)
                })
            {
                return Err(Error::Stale);
            }
            let remainder = full[prefix.len()..]
                .trim_end_matches('\\')
                .replace('\\', "/");
            let parent = if remainder.is_empty() {
                format!("{root}/{directory}")
            } else {
                format!("{root}/{directory}/{remainder}")
            };
            if doc.path.rsplit_once('/').map(|(path, _)| path) != Some(parent.as_str())
                || !crate::owns_current_path(project, &parent)
            {
                return Err(Error::Stale);
            }
            selected = Some(());
        }
    }
    selected.ok_or(Error::Stale)?;
    Ok(source)
}
fn validate(
    index: &Index,
    doc: &Document,
    expected: Option<&str>,
) -> Result<(Project, String, String, String, String), Error> {
    if index.partial {
        return Err(Error::Stale);
    }
    let (project, root) = view_creation::owner(doc, expected)?;
    let (name, namespace) =
        shape(index, doc, &|path| crate::owns_current_path(&project, path)).ok_or(Error::Stale)?;
    if !crate::owns_current_path(&project, &doc.path) || project.read_file(&doc.path)? != doc.text {
        return Err(Error::Stale);
    }
    let full = crate::validation_requests::current_class(doc).ok_or(Error::Stale)?;
    let dependencies = crate::validation_requests::validated_dependencies(
        &index.request_fields,
        doc,
        &full,
        &|path| crate::owns_current_path(&project, path),
    )
    .ok_or(Error::Stale)?;
    for dependency in dependencies {
        let path = crate::validation_requests::dependency_path(&dependency);
        if !crate::owns_current_path(&project, path) {
            return Err(Error::Stale);
        }
        let stat = project.stat(path)?;
        if stat.kind != FileKind::File || stat.size > 256 * 1024 {
            return Err(Error::Stale);
        }
        let current = project.read_file(path)?;
        if !crate::validation_requests::unchanged(&dependency, &current) {
            return Err(Error::Stale);
        }
    }
    let stamp = composer(&project, &root, doc, &namespace)?;
    let parent = doc.path.rsplit_once('/').ok_or(Error::Stale)?.0;
    let target = format!("{parent}/{name}Data.php");
    view_creation::missing(&project, &target)?;
    let relative = parent
        .strip_prefix(&format!("{root}/"))
        .ok_or(Error::Stale)?;
    let inventory = project.find_files(&format!("{relative}/*.php"), 1025)?;
    if inventory.len() >= 1025 || inventory.iter().any(|p| p.eq_ignore_ascii_case(&target)) {
        return Err(Error::Stale);
    }
    for path in &inventory {
        if !crate::owns_current_path(&project, path) {
            return Err(Error::Stale);
        }
        let stat = project.stat(path)?;
        if stat.kind != FileKind::File || stat.size > 256 * 1024 {
            return Err(Error::Stale);
        }
        if php::declares_class(&project.read_file(path)?, &format!("{name}Data")) {
            return Err(Error::Stale);
        }
    }
    Ok((
        project,
        root,
        target,
        stamp,
        content(
            &name,
            &namespace,
            if doc.text.contains("\r\n") {
                "\r\n"
            } else {
                "\n"
            },
        ),
    ))
}
fn content(request: &str, namespace: &str, eol: &str) -> String {
    format!("<?php\n\nnamespace {namespace};\n\n/** Exact validated payload; values and absent keys are preserved without casts. */\nfinal class {request}Data\n{{\n    private array $values;\n\n    public function __construct(array $values)\n    {{\n        $this->values = $values;\n    }}\n\n    public static function fromRequest({request} $request): self\n    {{\n        return new self($request->validated());\n    }}\n\n    public function all(): array\n    {{\n        return $this->values;\n    }}\n\n    public function has(string $key): bool\n    {{\n        return array_key_exists($key, $this->values);\n    }}\n\n    public function get(string $key, $default = null)\n    {{\n        return array_key_exists($key, $this->values) ? $this->values[$key] : $default;\n    }}\n}}\n").replace('\n',eol)
}
fn prompt_title() -> String {
    crate::ui_text::t("dto.confirm-title", "Create validated Request DTO?")
}
fn review(root: &str, source: &str, target: &str, content: &str) -> String {
    crate::ui_text::format(
        "dto.review",
        "Project: {root}\nSource: {source}\nCreate without overwrite: {target}\n\nExact contents:\n{content}\n\nThis DTO stores exactly validated() values, including nested arrays and explicit nulls. has() distinguishes missing keys; get() reads literal top-level keys. No scalar type conversion or requiredness is inferred. No controller/request code changes, policy changes or PHP/Artisan execution.\n\nApply one undoable file creation?",
        &[
            ("root", root),
            ("source", source),
            ("target", target),
            ("content", content),
        ],
    )
}
pub(crate) fn actions(index: &Index, doc: &Document) -> Vec<FileAction> {
    let Ok((_, root, _, stamp, _)) = validate(index, doc, None) else {
        return vec![];
    };
    let data = serde_json::json!({"root":root,"path":doc.path,"version":doc.version,"source":doc.text,"composer":stamp});
    vec![FileAction {
        title: crate::ui_text::t(
            "dto.create-action",
            "Create DTO from validated FormRequest…",
        ),
        kind: Some("refactor.extract".into()),
        data: Some(format!("{PREFIX}{data}")),
        ..Default::default()
    }]
}
pub(crate) fn resolve(
    index: &Index,
    doc: &Document,
    action: FileAction,
) -> Result<FileAction, Error> {
    let data = action
        .data
        .as_deref()
        .and_then(|d| d.strip_prefix(PREFIX))
        .ok_or(Error::Stale)?;
    let value = crate::strict_json::parse(data, 512 * 1024)?;
    if value["path"].as_str() != Some(&doc.path)
        || value["source"].as_str() != Some(&doc.text)
        || value["version"].as_i64() != Some(doc.version as i64)
    {
        return Err(Error::Stale);
    }
    let root = value["root"].as_str().ok_or(Error::Stale)?;
    let (_, _, target, stamp, content) = validate(index, doc, Some(root))?;
    if value["composer"].as_str() != Some(&stamp) {
        return Err(Error::Stale);
    }
    let preview = review(root, &doc.path, &target, &content);
    if !ui_kit::prompts::confirm_exact(&prompt_title(), &preview)? {
        return Err(Error::Cancelled);
    }
    let (_, _, current, current_stamp, current_content) = validate(index, doc, Some(root))?;
    if current != target || current_stamp != stamp || current_content != content {
        return Err(Error::Stale);
    }
    Ok(FileAction {
        title: crate::ui_text::t(
            "dto.created-action",
            "Create DTO from validated FormRequest",
        ),
        operations: vec![FileOperation::Create {
            path: target.clone(),
            text: content,
            overwrite: false,
        }],
        open: Some(target),
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;
    const COMPOSER: &str = r#"{"require":{"laravel/framework":"^13.0"},"autoload":{"psr-4":{"Domain\\":"src/Domain/"}}}"#;
    fn setup() -> (MockHost, Index, Document) {
        let host = MockHost::install();
        for dir in [
            "src",
            "src/Domain",
            "src/Domain/Http",
            "src/Domain/Http/Requests",
        ] {
            host.add_dir(dir);
        }
        host.add_file("composer.json", COMPOSER);
        let source = "<?php\r\nnamespace Domain\\Http\\Requests;\r\nuse Illuminate\\Foundation\\Http\\FormRequest as BaseRequest;\r\nclass UpdateRequest extends BaseRequest\r\n{\r\n public function rules(): array {return ['email'=>'required|email', 'optional'=>'nullable', 'nested.*.name'=>'string'];}\r\n}\r\n";
        host.add_file("src/Domain/Http/Requests/UpdateRequest.php", source);
        let doc = Document {
            path: "/project/src/Domain/Http/Requests/UpdateRequest.php".into(),
            language: "php".into(),
            version: 3,
            text: source.into(),
        };
        (host, Index::default(), doc)
    }
    #[test]
    fn creation_is_one_reviewed_nonoverwriting_operation_with_no_source_change_or_execution() {
        let (host, index, doc) = setup();
        let lazy = actions(&index, &doc).pop().unwrap();
        assert!(lazy.operations.is_empty() && lazy.edits.is_empty());
        host.answer_next_prompt(Some(&["yes"]));
        let result = resolve(&index, &doc, lazy).unwrap();
        assert!(result.edits.is_empty());
        let [
            FileOperation::Create {
                path,
                text,
                overwrite,
            },
        ] = result.operations.as_slice()
        else {
            panic!("one create")
        };
        assert_eq!(
            path,
            "/project/src/Domain/Http/Requests/UpdateRequestData.php"
        );
        assert!(!overwrite);
        assert!(text.contains("namespace Domain\\Http\\Requests;\r\n"));
        assert!(text.contains("fromRequest(UpdateRequest $request): self"));
        assert!(text.contains("$request->validated()"));
        assert!(text.contains("array_key_exists($key, $this->values)"));
        assert_eq!(
            host.invocation()
                .project()
                .unwrap()
                .read_file(&doc.path)
                .unwrap(),
            doc.text
        );
        assert!(host.process_runs().is_empty());
        assert_eq!(host.fetches(), 0);
    }
    #[test]
    fn registered_provider_resolves_dto_as_one_undoable_create_operation() {
        use openidea_plugin::{Position, Range, file_actions::FileActions};
        let (host, index, doc) = setup();
        let mut main = crate::Main::default();
        main.indexes.insert("/project".into(), index);
        let lazy = main
            .file_actions(&doc, Range::new(Position::new(0, 0), Position::new(0, 0)))
            .into_iter()
            .find(|action| action.title == "Create DTO from validated FormRequest…")
            .unwrap();
        host.answer_next_prompt(Some(&["yes"]));
        let resolved = main.resolve_file_action(&doc, lazy).unwrap();
        assert!(resolved.edits.is_empty());
        assert_eq!(resolved.operations.len(), 1);
        assert!(matches!(
            resolved.operations[0],
            FileOperation::Create {
                overwrite: false,
                ..
            }
        ));
        assert!(host.process_runs().is_empty());
    }

    #[test]
    fn explicit_locale_translates_prompt_and_review_without_changing_generated_source_or_project_values()
     {
        let (host, index, doc) = setup();
        host.set_setting("uiLanguage", "\"ja\"");
        assert_ne!(prompt_title(), "Create validated Request DTO?");
        let source = content("UpdateRequest", "Domain\\Http\\Requests", "\r\n");
        let project_value = "/project/{content}/😀";
        let preview = review(project_value, &doc.path, "/target/{root}.php", &source);
        assert!(preview.contains(project_value));
        assert!(preview.contains("/target/{root}.php"));
        assert!(preview.contains(&source));
        let action = actions(&index, &doc).pop().unwrap();
        assert_ne!(action.title, "Create DTO from validated FormRequest…");
        host.answer_next_prompt(Some(&["yes"]));
        let resolved = resolve(&index, &doc, action).unwrap();
        let [FileOperation::Create { text, .. }] = resolved.operations.as_slice() else {
            panic!("one create")
        };
        assert_eq!(text, &source);
    }

    #[test]
    fn cancellation_stale_snapshot_and_changed_composer_return_no_transaction() {
        let (host, index, doc) = setup();
        let action = actions(&index, &doc).pop().unwrap();
        host.answer_next_prompt(None);
        assert!(matches!(
            resolve(&index, &doc, action),
            Err(Error::Cancelled)
        ));
        let action = actions(&index, &doc).pop().unwrap();
        let mut changed = doc.clone();
        changed.text.push_str("// changed");
        assert!(resolve(&index, &changed, action).is_err());
        let action = actions(&index, &doc).pop().unwrap();
        host.add_file("composer.json", &format!("{COMPOSER} "));
        assert!(resolve(&index, &doc, action).is_err());
    }
    #[test]
    fn existing_case_equivalent_open_target_and_same_namespace_declaration_are_never_overwritten() {
        for collision in ["exact", "case", "open", "declaration"] {
            let (host, index, doc) = setup();
            let action = actions(&index, &doc).pop().unwrap();
            match collision {
                "exact" => host.add_file(
                    "src/Domain/Http/Requests/UpdateRequestData.php",
                    "<?php class Existing {}",
                ),
                "case" => host.add_file(
                    "src/Domain/Http/Requests/updaterequestdata.php",
                    "<?php class Existing {}",
                ),
                "open" => host.open_editor(
                    "/project/src/Domain/Http/Requests/UpdateRequestData.php",
                    "php",
                    "<?php // unsaved",
                    &[],
                ),
                _ => host.add_file(
                    "src/Domain/Http/Requests/Other.php",
                    "<?php namespace Domain\\Http\\Requests; class UPDATEREQUESTDATA {}",
                ),
            }
            assert!(actions(&index, &doc).is_empty(), "{collision}");
            assert!(resolve(&index, &doc, action).is_err(), "{collision}");
        }
    }
    #[test]
    fn foreign_project_targets_opaque_mappings_and_false_form_request_bases_are_withheld() {
        let (host, index, doc) = setup();
        host.add_project("/project/src/Domain/Http/Requests");
        assert!(actions(&index, &doc).is_empty());
        for composer in [
            r#"{"autoload":{"psr-4":{"Domain\\":["src/Domain/"]}}}"#,
            r#"{"autoload-dev":{"psr-4":{"Domain\\":"src/Domain/"}}}"#,
            r#"{"autoload":{"classmap":["src/"],"psr-4":{"Domain\\":"src/Domain/"}}}"#,
        ] {
            let (host, index, doc) = setup();
            host.add_file("composer.json", composer);
            assert!(actions(&index, &doc).is_empty());
        }
        let (host, index, mut doc) = setup();
        doc.text = doc.text.replace(
            "Illuminate\\Foundation\\Http\\FormRequest",
            "Vendor\\FormRequest",
        );
        host.add_file("src/Domain/Http/Requests/UpdateRequest.php", &doc.text);
        assert!(actions(&index, &doc).is_empty());
    }
    #[test]
    fn cached_custom_form_request_ancestry_enables_dto_without_changing_payload_or_psr4_target() {
        let (host, mut index, mut doc) = setup();
        let parent = "<?php namespace Domain\\Http\\Requests; class ParentRequest extends \\Illuminate\\Foundation\\Http\\FormRequest { public function rules(): array {return rulesForCurrentUser();} }";
        let path = "/project/src/Domain/Http/Requests/ParentRequest.php";
        host.add_file("src/Domain/Http/Requests/ParentRequest.php", parent);
        crate::validation_requests::cache(&mut index.request_fields, parent, path);
        doc.text = doc.text.replace(
            "Illuminate\\Foundation\\Http\\FormRequest as BaseRequest",
            "Domain\\Http\\Requests\\ParentRequest as BaseRequest",
        );
        host.add_file("src/Domain/Http/Requests/UpdateRequest.php", &doc.text);
        let action = actions(&index, &doc).pop().unwrap();
        host.answer_next_prompt(Some(&["yes"]));
        let resolved = resolve(&index, &doc, action).unwrap();
        let [
            FileOperation::Create {
                path,
                text,
                overwrite: false,
            },
        ] = resolved.operations.as_slice()
        else {
            panic!("one create")
        };
        assert_eq!(
            path,
            "/project/src/Domain/Http/Requests/UpdateRequestData.php"
        );
        assert_eq!(
            text,
            &content("UpdateRequest", "Domain\\Http\\Requests", "\r\n")
        );
        assert!(host.process_runs().is_empty());
    }
    #[test]
    fn stale_foreign_missing_or_overridden_cached_validation_provider_never_creates_dto() {
        for mode in ["stale", "missing", "foreign", "override", "http"] {
            let (host, mut index, mut doc) = setup();
            let parent = match mode {
                "override" => {
                    "<?php namespace Domain\\Http\\Requests; class ParentRequest extends \\Illuminate\\Foundation\\Http\\FormRequest {public function validated($key=null,$default=null) {return 'custom';}}"
                }
                "http" => {
                    "<?php namespace Domain\\Http\\Requests; class ParentRequest extends \\Illuminate\\Http\\Request {public function validated() {return [];}}"
                }
                _ => {
                    "<?php namespace Domain\\Http\\Requests; class ParentRequest extends \\Illuminate\\Foundation\\Http\\FormRequest {}"
                }
            };
            let path = "/project/shared/ParentRequest.php";
            host.add_file("shared/ParentRequest.php", parent);
            if mode != "missing" {
                crate::validation_requests::cache(&mut index.request_fields, parent, path);
            }
            doc.text = doc.text.replace(
                "Illuminate\\Foundation\\Http\\FormRequest as BaseRequest",
                "Domain\\Http\\Requests\\ParentRequest as BaseRequest",
            );
            host.add_file("src/Domain/Http/Requests/UpdateRequest.php", &doc.text);
            if mode == "stale" {
                let action = actions(&index, &doc).pop().unwrap();
                host.add_file("shared/ParentRequest.php", &format!("{parent} // changed"));
                assert!(resolve(&index, &doc, action).is_err());
            } else if mode == "foreign" {
                host.add_project("/project/shared");
            }
            assert!(actions(&index, &doc).is_empty(), "{mode}");
        }
    }

    #[test]
    #[ignore = "requires php executable; validates actual generated DTO payload behavior and syntax"]
    fn generated_dto_preserves_actual_php_nested_optional_null_false_and_zero_payloads() {
        use std::{
            io::Write,
            process::{Command, Stdio},
        };
        let generated = content("UpdateRequest", "Domain\\Http\\Requests", "\n");
        let script = format!(
            "{}\nclass UpdateRequest {{public function validated(): array {{return ['optional'=>null,'false'=>false,'zero'=>0,'nested'=>['x'=>['y'=>1]]];}}}}\n$dto=UpdateRequestData::fromRequest(new UpdateRequest());\nif($dto->all() !== ['optional'=>null,'false'=>false,'zero'=>0,'nested'=>['x'=>['y'=>1]]] || !$dto->has('optional') || $dto->has('absent') || $dto->get('optional','fallback')!==null || $dto->get('absent','fallback')!=='fallback' || $dto->get('false')!==false || $dto->get('zero')!==0) {{exit(1);}}\necho 'DTO payload preserved';",
            generated
        );
        for lint in [true, false] {
            let mut command = Command::new("php");
            if lint {
                command.arg("-l");
            }
            let mut child = command
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();
            child
                .stdin
                .take()
                .unwrap()
                .write_all(script.as_bytes())
                .unwrap();
            let output = child.wait_with_output().unwrap();
            assert!(
                output.status.success(),
                "{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
    #[test]
    #[ignore = "requires LARAVEL_DTO_FIXTURE with a real Laravel vendor/autoload.php"]
    fn generated_dto_preserves_real_form_request_validated_payload() {
        use std::{
            io::Write,
            process::{Command, Stdio},
        };
        let fixture = std::env::var("LARAVEL_DTO_FIXTURE").expect("Set LARAVEL_DTO_FIXTURE");
        let generated = content("UpdateRequest", "Domain\\Http\\Requests", "\n");
        let script = format!(
            "{}\nrequire getenv('LARAVEL_DTO_FIXTURE').'/vendor/autoload.php';\nclass ParentRequest extends \\Illuminate\\Foundation\\Http\\FormRequest {{}}\nclass UpdateRequest extends ParentRequest {{}}\n$translator=new \\Illuminate\\Translation\\Translator(new \\Illuminate\\Translation\\ArrayLoader(),'en');\n$factory=new \\Illuminate\\Validation\\Factory($translator);\n$request=new UpdateRequest();\n$payload=['email'=>'user@example.test','optional'=>null,'nested'=>[['name'=>'hello']],'unvalidated'=>'drop'];\n$request->setValidator($factory->make($payload,['email'=>'required|email','optional'=>'nullable','nested.*.name'=>'string']));\n$expected=$request->validated();\n$dto=UpdateRequestData::fromRequest($request);\nif($dto->all()!==$expected||$dto->has('unvalidated')||!$dto->has('optional')||$dto->get('optional','fallback')!==null) {{exit(1);}}\necho 'Real FormRequest DTO payload preserved';",
            generated
        );
        let mut child = Command::new("php")
            .env("LARAVEL_DTO_FIXTURE", fixture)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(script.as_bytes())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
