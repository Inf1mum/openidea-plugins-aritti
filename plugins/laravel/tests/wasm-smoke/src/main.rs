#![allow(unused_variables)]
use ide_core::CancellationToken;
use ide_extension::Capability;
use ide_plugin::*;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    io::Read,
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::{
        Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};
use std::{
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};
struct Live {
    child: Child,
    stdout: Arc<Mutex<Vec<u8>>>,
    stderr: Arc<Mutex<Vec<u8>>>,
    out_done: Arc<AtomicBool>,
    err_done: Arc<AtomicBool>,
}
struct ProbeHost {
    root: PathBuf,
    docker: Option<String>,
    reads: AtomicUsize,
    storage: Mutex<BTreeMap<String, String>>,
    messages: Mutex<Vec<Value>>,
    notifications: Mutex<Vec<(String, String)>>,
    jobs: Mutex<BTreeMap<u64, Live>>,
    terminals: Mutex<Vec<Vec<String>>>,
}
fn error(e: impl std::fmt::Display) -> HostError {
    HostError::Failed(e.to_string())
}
fn capture(mut reader: impl Read + Send + 'static) -> (Arc<Mutex<Vec<u8>>>, Arc<AtomicBool>) {
    let bytes = Arc::new(Mutex::new(Vec::new()));
    let done = Arc::new(AtomicBool::new(false));
    let (target, finished) = (bytes.clone(), done.clone());
    std::thread::spawn(move || {
        let mut buf = [0; 4096];
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => target.lock().unwrap().extend_from_slice(&buf[..n]),
            }
        }
        finished.store(true, Ordering::SeqCst);
    });
    (bytes, done)
}
fn take(bytes: &Mutex<Vec<u8>>, max: usize) -> String {
    let mut bytes = bytes.lock().unwrap();
    let end = max.min(bytes.len());
    String::from_utf8_lossy(&bytes.drain(..end).collect::<Vec<_>>()).into_owned()
}

impl HostApi for ProbeHost {
    fn notify(
        &self,
        client: Option<&str>,
        level: NotificationLevel,
        title: String,
        body: String,
        actions: Vec<ide_plugin::NotificationAction>,
    ) -> Result<(), HostError> {
        self.notifications.lock().unwrap().push((title, body));
        Ok(())
    }
    fn status_item(&self, id: &str, item: Option<StatusItem>) {}
    fn projects(&self) -> Vec<std::path::PathBuf> {
        vec![self.root.clone()]
    }
    fn buffer_text(&self, path: &Path) -> Option<String> {
        None
    }
    fn document(&self, path: &Path) -> Option<(u64, String)> {
        None
    }
    fn apply_edits(
        &self,
        client: Option<&str>,
        path: &Path,
        version: u64,
        edits: Vec<ide_core::TextEdit>,
    ) -> Result<u64, HostError> {
        panic!("Unexpected host callback: apply_edits")
    }
    fn storage_get(&self, key: &str) -> Result<Option<String>, HostError> {
        Ok(self.storage.lock().unwrap().get(key).cloned())
    }
    fn storage_set(&self, key: &str, value: Option<String>) -> Result<(), HostError> {
        if let Some(value) = value {
            self.storage.lock().unwrap().insert(key.into(), value);
        } else {
            self.storage.lock().unwrap().remove(key);
        }
        Ok(())
    }
    fn storage_keys(&self, prefix: &str) -> Result<Vec<String>, HostError> {
        Ok(self
            .storage
            .lock()
            .unwrap()
            .keys()
            .filter(|k| k.starts_with(prefix))
            .cloned()
            .collect())
    }
    fn vcs_status(&self, root: &Path) -> Result<Option<RepositoryStatus>, HostError> {
        panic!("Unexpected host callback: vcs_status")
    }
    fn vcs_fetch(&self, root: &Path, cancel: &CancellationToken) -> Result<(), HostError> {
        panic!("Unexpected host callback: vcs_fetch")
    }
    fn find_files(&self, root: &Path, glob: &str, limit: usize) -> Result<Vec<String>, HostError> {
        let paths = if glob.starts_with("app/Models/") {
            vec![
                "app/Models/WorkspaceRecord.php",
                "app/Models/WorkspacePurchase.php",
                "app/Models/WorkspaceCustomer.php",
            ]
        } else if glob.starts_with("app/Traits/") {
            vec!["app/Traits/WorkspaceTrait.php"]
        } else if glob.starts_with("app/Casts/") {
            vec!["app/Casts/WorkspaceAddressCast.php"]
        } else if glob.starts_with("app/ValueObjects/") {
            vec!["app/ValueObjects/WorkspaceAddress.php"]
        } else if glob.starts_with("routes/") {
            vec!["routes/web.php"]
        } else if glob.starts_with("app/Http/Controllers/") {
            vec!["app/Http/Controllers/WorkspaceRuntimeController.php"]
        } else if glob.starts_with("config/") {
            vec!["config/workspace.php"]
        } else if glob.starts_with("app/Enums/") {
            vec!["app/Enums/WorkspaceStatus.php"]
        } else if glob.starts_with("database/migrations/") {
            vec!["database/migrations/999_workspace.php"]
        } else {
            vec![]
        };
        Ok(paths
            .into_iter()
            .filter(|path| root.join(path).is_file())
            .take(limit)
            .map(|path| root.join(path).to_string_lossy().into_owned())
            .collect())
    }
    fn publish_diagnostics(
        &self,
        path: &Path,
        items: Vec<ide_core::Diagnostic>,
    ) -> Result<(), HostError> {
        Ok(())
    }
    fn fetch(
        &self,
        request: HttpRequest,
        cancel: &CancellationToken,
    ) -> Result<HttpResponse, HostError> {
        panic!("Unexpected host callback: fetch")
    }
    fn secret_is_set(&self, name: &str) -> bool {
        panic!("Unexpected host callback: secret_is_set")
    }
    fn run_process(
        &self,
        cmd: ProcessCommand,
        cwd: Option<&Path>,
        stdin: Option<String>,
        cancel: &CancellationToken,
        timeout: Duration,
    ) -> Result<ProcessOutput, HostError> {
        panic!("Unexpected host callback: run_process")
    }
    fn subscribe(&self, kind: EventKind) {}
    fn subscribe_paths(&self, kind: &str, globs: Vec<String>) -> Result<(), HostError> {
        Ok(())
    }
    fn unsubscribe(&self, kind: &str) {}
    fn start_process(
        &self,
        cmd: ProcessCommand,
        cwd: Option<&Path>,
        client: Option<&str>,
    ) -> Result<u64, HostError> {
        assert_eq!(cwd, Some(self.root.as_path()));
        if cmd.program=="docker" {
            let container=self.docker.as_ref().expect("Docker testing requires an explicit disposable container");
            assert_eq!(&cmd.args[..2],["--host","unix:///var/run/docker.sock"]);
            assert!(cmd.args.contains(container));
            assert!(matches!(cmd.args[2].as_str(),"inspect"|"exec"));
        }else{assert_eq!(cmd.program,"php");}
        let mut child = Command::new(&cmd.program)
            .args(&cmd.args)
            .current_dir(&self.root)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(error)?;
        let (stdout, out_done) = capture(child.stdout.take().unwrap());
        let (stderr, err_done) = capture(child.stderr.take().unwrap());
        let mut jobs = self.jobs.lock().unwrap();
        let id = jobs.keys().next_back().copied().unwrap_or(0) + 1;
        jobs.insert(
            id,
            Live {
                child,
                stdout,
                stderr,
                out_done,
                err_done,
            },
        );
        Ok(id)
    }
    fn poll_process(&self, id: u64, max: usize) -> Result<ProcessChunk, HostError> {
        let mut jobs = self.jobs.lock().unwrap();
        let live = jobs.get_mut(&id).unwrap();
        let status = live
            .child
            .try_wait()
            .map_err(error)?
            .filter(|_| {
                live.out_done.load(Ordering::SeqCst) && live.err_done.load(Ordering::SeqCst)
            })
            .map(|s| s.code().unwrap_or(-1));
        Ok(ProcessChunk {
            stdout: take(&live.stdout, max),
            stderr: take(&live.stderr, max),
            status,
        })
    }
    fn write_process(
        &self,
        id: u64,
        text: &str,
        cancel: &CancellationToken,
        timeout: Duration,
    ) -> Result<(), HostError> {
        panic!("Unexpected host callback: write_process")
    }
    fn close_process_stdin(&self, id: u64) -> Result<(), HostError> {
        Ok(())
    }
    fn kill_process(&self, id: u64) -> Result<(), HostError> {
        let mut jobs = self.jobs.lock().unwrap();
        let live = jobs.get_mut(&id).unwrap();
        if live.child.try_wait().map_err(error)?.is_none() {
            live.child.kill().map_err(error)?;
        }
        Ok(())
    }
    fn release_process(&self, id: u64) {
        if let Some(mut live) = self.jobs.lock().unwrap().remove(&id) {
            let _ = live.child.kill();
            let _ = live.child.wait();
        }
    }
    fn wait_process(
        &self,
        id: u64,
        max: usize,
        timeout: Duration,
        cancel: &CancellationToken,
    ) -> Result<ProcessChunk, HostError> {
        let started = Instant::now();
        loop {
            let done = {
                let mut jobs = self.jobs.lock().unwrap();
                let live = jobs.get_mut(&id).unwrap();
                live.child.try_wait().map_err(error)?.is_some()
                    && live.out_done.load(Ordering::SeqCst)
                    && live.err_done.load(Ordering::SeqCst)
            };
            if done || started.elapsed() >= timeout {
                return self.poll_process(id, max);
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    fn task_begin(
        &self,
        client: Option<&str>,
        title: String,
        cancellable: bool,
        cancel: CancellationToken,
    ) -> u64 {
        1
    }
    fn task_report(&self, id: u64, done: u32, total: Option<u32>, message: Option<String>) {}
    fn task_cancelled(&self, id: u64) -> bool {
        false
    }
    fn task_end(&self, id: u64) {}
    fn refresh_view(&self, view: &str) {}
    fn post_view_message(&self, view: &str, message: String) -> Result<(), HostError> {
        self.messages
            .lock()
            .unwrap()
            .push(serde_json::from_str(&message).map_err(error)?);
        Ok(())
    }
    fn open_terminal(
        &self,
        client: Option<&str>,
        title: String,
        program: &str,
        args: Vec<String>,
        cwd: Option<&Path>,
    ) -> Result<(), HostError> {
        panic!("Unexpected host callback: open_terminal")
    }
    fn open_location(
        &self,
        client: Option<&str>,
        path: &Path,
        line: u32,
        column: u32,
    ) -> Result<(), HostError> {
        panic!("Unexpected host callback: open_location")
    }
    fn open_log_tab(
        &self,
        client: &str,
        key: String,
        title: String,
        log: ServiceLog,
    ) -> Result<(), HostError> {
        panic!("Unexpected host callback: open_log_tab")
    }
    fn locale(&self) -> String {
        "en".into()
    }
    fn setting(&self, root: Option<&Path>, key: &str) -> Option<String> {
        let id=self.docker.as_ref()?;
        match key {"artisanDockerContainer"=>Some(json!(id).to_string()),"artisanDockerWorkdir"=>Some(json!("/workspace").to_string()),_=>None}
    }
    fn read_text_file(&self, real: &Path, max: usize) -> Result<String, HostError> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        let value = std::fs::read_to_string(real).map_err(error)?;
        if value.len() > max {
            return Err(error("oversize"));
        }
        Ok(value)
    }
    fn write_text_file(&self, real: &Path, text: &str) -> Result<(), HostError> {
        panic!("Unexpected host callback: write_text_file")
    }
    fn write_bytes_file(&self, real: &Path, bytes: &[u8]) -> Result<(), HostError> {
        panic!("Unexpected host callback: write_bytes_file")
    }
    fn ask(
        &self,
        _client: Option<&str>,
        prompt: Prompt,
        _cancel: &CancellationToken,
    ) -> Result<Option<Vec<String>>, HostError> {
        assert!(matches!(prompt, Prompt::Confirm { .. }));
        Ok(Some(vec!["ok".into()]))
    }
    fn open_terminal_session(
        &self,
        _client: &str,
        key: String,
        _title: String,
        cmd: ProcessCommand,
        _cancel: &CancellationToken,
        _timeout: Duration,
    ) -> Result<String, HostError> {
        self.terminals.lock().unwrap().push(cmd.args);
        Ok(key)
    }
}
fn request(
    runtime: &mut Runtime,
    host: &ProbeHost,
    invocation: &Invocation,
    operation: Value,
) -> Value {
    let mut value = json!({"channel":"ui-kit","sequence":1,"frameId":"0123456789abcdef0123456789abcdef","project":host.root.to_str().unwrap(),"targetView":"laravel-artisan"});
    value
        .as_object_mut()
        .unwrap()
        .extend(operation.as_object().unwrap().clone());
    let start = host.messages.lock().unwrap().len();
    runtime
        .view_event(
            "laravel",
            ViewEvent::Message {
                text: value.to_string(),
            },
            invocation.clone(),
        )
        .unwrap();
    let messages = host.messages.lock().unwrap();
    if let Some(reply) = messages[start..]
        .iter()
        .find(|m| m["channel"] == "ui-kit-view-message")
    {
        return serde_json::from_str(reply["message"].as_str().unwrap()).unwrap();
    }
    messages.last().unwrap().clone()
}
fn opaque(runtime: &mut Runtime, host: &ProbeHost, invocation: &Invocation, value: Value) -> Value {
    request(
        runtime,
        host,
        invocation,
        json!({"op":"opaque-message","message":value.to_string()}),
    )
}
fn settled(runtime: &mut Runtime, host: &ProbeHost, invocation: &Invocation, id: &str) -> Value {
    let started = Instant::now();
    loop {
        let reply = opaque(runtime, host, invocation, json!({"op":"poll","id":id}));
        assert!(reply.get("error").is_none(), "{reply}");
        if reply["job"]["running"] == false {
            return reply;
        }
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "command did not settle"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}
fn main() {
    let args: Vec<_> = std::env::args().collect();
    assert_eq!(
        args.len(),
        3,
        "usage: laravel-artisan-wasm-smoke <plugin.wasm> <temporary Laravel app>"
    );
    let root = std::fs::canonicalize(&args[2]).unwrap();
    assert!(
        root.file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("laravel-artisan-acceptance-"),
        "only a disposable acceptance app is allowed"
    );
    assert!(root.join("vendor/laravel/framework").is_dir());
    let console = root.join("routes/console.php");
    let mut source = std::fs::read_to_string(&console).unwrap();
    if !source.contains("workspace:probe") {
        source.push_str(include_str!("../probe.php.inc"));
    }
    if !source.contains("workspace:catalog-") {
        source.push_str(r"
for ($i = 0; $i < 180; $i++) { \Illuminate\Support\Facades\Artisan::command('workspace:catalog-'.$i, function () {})->purpose(str_repeat('Catalog fixture ', 200)); }
");
    }
    std::fs::write(&console, source).unwrap();
    let web = root.join("routes/web.php");
    let mut web_source = std::fs::read_to_string(&web).unwrap();
    if !web_source.contains("workspace/runtime-first") {
        web_source.push_str(r"
foreach (['workspace/runtime-first'=>\App\Http\Controllers\WorkspaceRuntimeController::class,'workspace/runtime-second'=>\App\Http\Controllers\WorkspaceRuntimeController::class] as $uri=>$controller) { \Illuminate\Support\Facades\Route::get($uri,[$controller,'index']); \Illuminate\Support\Facades\Route::post($uri,[$controller,'store']); \Illuminate\Support\Facades\Route::delete($uri.'/{id}',[$controller,'destroy']); }
");
        std::fs::write(&web, web_source).unwrap();
    }
    std::fs::create_dir_all(root.join("app/Http/Controllers")).unwrap();
    std::fs::create_dir_all(root.join("app/Enums")).unwrap();
    for dir in ["app/Traits","app/Casts","app/ValueObjects"] {std::fs::create_dir_all(root.join(dir)).unwrap();}
    for (name, source) in [
        (
            "app/Http/Controllers/WorkspaceRuntimeController.php",
            r#"<?php
namespace App\Http\Controllers;
class WorkspaceRuntimeController { public function index(){return 'index';} public function store(){return 'store';} public function destroy(){return 'destroy';} }
"#,
        ),
        (
            "app/Models/WorkspaceRecord.php",
            r#"<?php namespace App\Models; use Illuminate\Database\Eloquent\Model; abstract class WorkspaceRecord extends Model {protected $table='workspace_purchases'; protected $casts=['status'=>\App\Enums\WorkspaceStatus::class]; public function customer(){return $this->belongsTo(WorkspaceCustomer::class);}public function scopeActive($q){return $q;}}"#,
        ),
        (
            "app/Enums/WorkspaceStatus.php",
            r#"<?php namespace App\Enums; enum WorkspaceStatus:string {case Paid='paid'; public function label():string{return 'Paid';}}"#,
        ),
        (
            "app/Models/WorkspacePurchase.php",
            r#"<?php namespace App\Models; class WorkspacePurchase extends WorkspaceRecord {use \App\Traits\WorkspaceTrait; protected function casts(): array {return ['address'=>\App\Casts\WorkspaceAddressCast::class];} public function recipient(): WorkspaceCustomer {return new WorkspaceCustomer;}}"#,
        ),
        (
            "app/Models/WorkspaceCustomer.php",
            r#"<?php namespace App\Models; use Illuminate\Database\Eloquent\Model; class WorkspaceCustomer extends Model {protected $table='workspace_customers';}"#,
        ),
        ("app/Traits/WorkspaceTrait.php",r#"<?php namespace App\Traits; trait WorkspaceTrait {public function scopeFeatured($query){return $query;}}"#),
        ("app/Casts/WorkspaceAddressCast.php",r#"<?php namespace App\Casts; use Illuminate\Contracts\Database\Eloquent\CastsAttributes; use App\ValueObjects\WorkspaceAddress; class WorkspaceAddressCast implements CastsAttributes {public function get($m,$k,$v,$a): WorkspaceAddress{return new WorkspaceAddress;} public function set($m,$k,$v,$a): string{return '';}}"#),
        ("app/ValueObjects/WorkspaceAddress.php",r#"<?php namespace App\ValueObjects; class WorkspaceAddress {public string $city; public function normalized(): self{return $this;}}"#),
        (
            "config/workspace.php",
            r#"<?php return ['rows'=>[['name'=>env('WORKSPACE_NAME', 'first')],['name'=>'second']]];"#,
        ),
        (
            "database/migrations/999_workspace.php",
            r#"<?php Schema::create('workspace_purchases',function($table){$table->id();$table->string('reference');});Schema::create('workspace_customers',function($table){$table->id();$table->string('email');});"#,
        ),
    ] {
        std::fs::write(root.join(name), source).unwrap();
    }
    let host = Arc::new(ProbeHost {
        root: root.clone(),
        docker: std::env::var("LARAVEL_ACCEPTANCE_DOCKER_ID").ok().map(|id|{assert!(id.len()==64&&id.bytes().all(|c|c.is_ascii_hexdigit()));id}),
        reads: AtomicUsize::new(0),
        storage: Default::default(),
        messages: Default::default(),
        notifications: Default::default(),
        jobs: Default::default(),
        terminals: Default::default(),
    });
    let engine = PluginEngine::new().unwrap();
    let component = engine.compile(Path::new(&args[1])).unwrap();
    let mut runtime = Runtime::instantiate(
        &engine,
        &component,
        RuntimeSetup {
            plugin_id: "aritti.laravel".into(),
            version: "0.2.0".into(),
            granted: vec![
                Capability::ProjectRead,
                Capability::Storage,
                Capability::Events,
                Capability::Process("php".into()),
                Capability::Process("docker".into()),
            ],
            host: host.clone(),
            limits: Limits::default(),
        },
    )
    .unwrap();
    runtime.activate().unwrap();
    for (expression, wanted) in [
        ("Order::active()->first()->ref", "reference"),
        ("Order::first()->customer->em", "email"),
        ("Order::get()->", "first()"),
        ("Order::first()->status->va", "value"),
        ("Order::featured()->first()->ref", "reference"),
        ("Order::first()->recipient()->em", "email"),
        ("Order::first()->address->normalized()->ci", "city"),
    ] {
        let source = format!("<?php use App\\Models\\WorkspacePurchase as Order; {expression}");
        let at = ide_core::Pos {
            line: 0,
            col: source.encode_utf16().count() as u32,
        };
        let document = ProviderDocument {
            path: root.join("app/Http/Controllers/WorkspaceProbe.php"),
            language: "php".into(),
            version: 1,
            text: source,
        };
        let before = host.reads.load(Ordering::SeqCst);
        let started = Instant::now();
        let items = runtime
            .provide_completion(&document, at, None, Duration::from_millis(50))
            .unwrap();
        assert!(
            items.iter().any(|item| item.label == wanted),
            "missing {wanted}: {items:?}"
        );
        assert_eq!(
            host.reads.load(Ordering::SeqCst),
            before,
            "completion must use cached facts"
        );
        println!(
            "PASS compiled Eloquent completion: {expression} → {wanted}, {:?}",
            started.elapsed()
        );
    }

    let invocation = Invocation {
        client: Some("acceptance".into()),
        workspace_root: Some(root),
        ..Default::default()
    };
    request(&mut runtime, &host, &invocation, json!({"op":"select-view","target":"laravel-generators","targetView":"laravel"}));
    let generator_preview = request(
        &mut runtime,
        &host,
        &invocation,
        json!({"op":"opaque-message","targetView":"laravel-generators","message":json!({
            "op":"preview","draft":{
                "name":"WorkspaceGeneratedAcceptance","table":"workspace_generated_acceptance",
                "migrationStamp":"2026_11_10_120000","fields":[{"name":"title","kind":"string","nullable":true}],
                "related":["migration","factory"]
            }
        }).to_string()}),
    );
    assert!(generator_preview.get("error").is_none(), "compiled generator preview: {generator_preview}");
    let generated=generator_preview["files"].as_array().expect("compiled generator files");
    assert_eq!(generated.len(),3,"model, migration and factory preview");
    assert!(generated.iter().any(|file|file["path"]=="app/Models/WorkspaceGeneratedAcceptance.php" && file["text"].as_str().unwrap().contains("'title'")));
    assert!(generated.iter().any(|file|file["path"].as_str().unwrap().starts_with("database/migrations/") && file["text"].as_str().unwrap().contains("->nullable()")));
    assert!(generated.iter().all(|file|!host.root.join(file["path"].as_str().unwrap()).exists()),"preview must not create files");
    assert!(host.jobs.lock().unwrap().is_empty(),"generator preview must not start PHP");
    println!("PASS compiled model wizard preview: typed model/migration/factory files, nullable field, no writes or PHP execution");
    let selected_preview=request(&mut runtime,&host,&invocation,json!({"op":"opaque-message","targetView":"laravel-generators","message":json!({
        "op":"preview","draft":{"name":"WorkspaceSelectedAcceptance","table":"workspace_selected_acceptance","migrationStamp":"2026_11_10_120001",
        "fields":[{"name":"title","kind":"string"}],"related":["factory","resource"],"factoryFields":[],"resourceFields":["title"],"resourceRelations":[]}
    }).to_string()}));
    assert!(selected_preview.get("error").is_none(),"compiled output selection: {selected_preview}");
    let selected_files=selected_preview["files"].as_array().expect("selected preview files");
    let factory=selected_files.iter().find(|file|file["path"].as_str().unwrap().ends_with("Factory.php")).expect("selected factory")["text"].as_str().unwrap();
    let resource=selected_files.iter().find(|file|file["path"].as_str().unwrap().ends_with("Resource.php")).expect("selected resource")["text"].as_str().unwrap();
    assert!(!factory.contains("'title' =>"),"empty factory selection stays empty");
    assert!(resource.contains("'title' => $this->title")&&!resource.contains("'id' =>"),"resource selection omits the primary key");
    assert!(selected_files.iter().all(|file|!host.root.join(file["path"].as_str().unwrap()).exists()),"selected preview never writes");
    assert!(host.jobs.lock().unwrap().is_empty(),"selected preview never starts PHP");
    println!("PASS compiled per-output wizard selection: empty factory, selected resource field, omitted primary key, no writes or PHP");

    request(&mut runtime, &host, &invocation, json!({"op":"select-view","target":"laravel","targetView":"laravel-generators"}));
    let registered = request(
        &mut runtime,
        &host,
        &invocation,
        json!({"op":"event","targetView":"laravel","event":{"kind":"button","key":"routes-runtime-refresh"}}),
    );
    let runtime_rows = registered["model"]["body"]["items"]
        .as_array()
        .expect("runtime route rows");
    let expanded: Vec<_> = runtime_rows
        .iter()
        .filter(|row| {
            row["cells"][1]
                .as_str()
                .unwrap_or("")
                .starts_with("/workspace/runtime-")
        })
        .collect();
    assert_eq!(
        expanded.len(),
        6,
        "runtime routes must expand the two map entries into six routes"
    );
    assert!(runtime_rows.iter().all(|row| row["cells"][1] != "/?"));
    assert!(
        registered["model"]["status"]
            .as_str()
            .unwrap()
            .contains("Artisan snapshot")
    );
    println!(
        "PASS compiled runtime route:list JSON: expanded map routes, real URI/controller/method, no placeholder rows"
    );
    let config = request(
        &mut runtime,
        &host,
        &invocation,
        json!({"op":"select-view","target":"laravel-config","targetView":"laravel"}),
    );
    let rows = config["model"]["body"]["items"]
        .as_array()
        .expect("compiled Config inventory");
    let row = rows
        .iter()
        .find(|row| row["key"] == "config:workspace.rows.0.name")
        .expect("nested list key");
    assert_eq!(row["cells"][1], "env('WORKSPACE_NAME', 'first')");
    assert_eq!(row["cells"][2], "config/workspace.php");
    println!("PASS compiled Config inventory: nested numeric key, source expression and file");
    request(
        &mut runtime,
        &host,
        &invocation,
        json!({"op":"select-view","target":"laravel-artisan","targetView":"laravel-config"}),
    );
    let initial = opaque(&mut runtime, &host, &invocation, json!({"op":"state"}));
    assert_eq!(initial["commands"], json!([]));
    assert!(host.jobs.lock().unwrap().is_empty());
    let discovered = opaque(&mut runtime, &host, &invocation, json!({"op":"refresh"}));
    assert!(discovered.get("error").is_none(), "{discovered}");
    assert!(discovered["commands"].as_array().unwrap().len() > 100);
    assert!(
        discovered["commands"].to_string().len() > 512 * 1024,
        "fixture must exceed the old catalog limit"
    );
    println!(
        "PASS compiled large Artisan catalog: {} commands / {} normalized bytes",
        discovered["commands"].as_array().unwrap().len(),
        discovered["commands"].to_string().len()
    );
    let token = discovered["token"].clone();
    let values = json!({"name":"hello café; $(not executed)","tags":["one two","three"],"--label":"value with spaces","--flag":true});
    let preview = opaque(
        &mut runtime,
        &host,
        &invocation,
        json!({"op":"preview","command":"workspace:probe","token":token,"values":values,"revision":1}),
    );
    assert!(preview.get("error").is_none(), "{preview}");
    for (id, extra, status) in [
        ("acceptance-request-0001", json!({}), 0),
        ("acceptance-request-0002", json!({"--fail":true}), 7),
        ("acceptance-request-0003", json!({"--bytes":"100000"}), 0),
    ] {
        let mut input = values.clone();
        input
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        let result = opaque(
            &mut runtime,
            &host,
            &invocation,
            json!({"op":"run","id":id,"command":"workspace:probe","token":token,"values":input}),
        );
        assert!(result.get("error").is_none(), "{result}");
        let replay = opaque(
            &mut runtime,
            &host,
            &invocation,
            json!({"op":"run","id":id,"command":"workspace:probe","token":token,"values":input}),
        );
        assert!(replay["error"].is_string());
        let result = settled(&mut runtime, &host, &invocation, id);
        assert_eq!(result["job"]["status"], status, "{result}");
        if id.ends_with("3") {
            assert_eq!(result["job"]["truncated"], true);
            assert!(result["job"]["stdout"].as_str().unwrap().len() <= 65536);
        } else {
            assert!(
                result["job"]["stdout"]
                    .as_str()
                    .unwrap()
                    .contains("hello café; $(not executed)")
            );
        }
        if status == 7 {
            let count = || {
                host.notifications
                    .lock()
                    .unwrap()
                    .iter()
                    .filter(|(title, body)| {
                        title == "Artisan" && body.contains("exited with status 7")
                    })
                    .count()
            };
            assert_eq!(count(), 1, "failed command must notify once");
            settled(&mut runtime, &host, &invocation, id);
            assert_eq!(
                count(),
                1,
                "repeated polling must not duplicate the notification"
            );
        }
        println!("PASS real Laravel: {id}, exit {status}");
    }
    let id = "acceptance-request-0004";
    let mut wait = values.clone();
    wait["--wait"] = "10".into();
    let started = opaque(
        &mut runtime,
        &host,
        &invocation,
        json!({"op":"run","id":id,"command":"workspace:probe","token":token,"values":wait}),
    );
    assert!(started.get("error").is_none(), "{started}");
    let cancelled = opaque(
        &mut runtime,
        &host,
        &invocation,
        json!({"op":"cancel","id":id}),
    );
    assert!(cancelled.get("error").is_none());
    let final_result = settled(&mut runtime, &host, &invocation, id);
    assert_eq!(final_result["job"]["cancelled"], true);
    assert_ne!(final_result["job"]["status"], 0);
    let terminal = opaque(
        &mut runtime,
        &host,
        &invocation,
        json!({"op":"run","id":"acceptance-terminal-0001","command":"workspace:probe","token":token,"values":values,"terminal":true}),
    );
    assert_eq!(terminal["terminal"], true, "{terminal}");
    assert_eq!(host.terminals.lock().unwrap().len(), 1);
    assert!(
        host.jobs.lock().unwrap().is_empty(),
        "all process handles released"
    );
    let mut denied = Runtime::instantiate(
        &engine,
        &component,
        RuntimeSetup {
            plugin_id: "aritti.laravel".into(),
            version: "0.2.0".into(),
            granted: vec![
                Capability::ProjectRead,
                Capability::Storage,
                Capability::Events,
            ],
            host: host.clone(),
            limits: Limits::default(),
        },
    )
    .unwrap();
    denied.activate().unwrap();
    let refused = opaque(
        &mut denied,
        &host,
        &invocation,
        json!({"op":"run","id":"acceptance-denied-0001","command":"workspace:probe","token":token,"values":values}),
    );
    assert!(
        refused["error"].as_str().unwrap().contains("process:php"),
        "{refused}"
    );
    assert!(host.jobs.lock().unwrap().is_empty());
    let artisan = host.root.join("artisan");
    let original = std::fs::read_to_string(&artisan).unwrap();
    std::fs::write(&artisan, format!("{original}\n// stale acceptance probe\n")).unwrap();
    let stale = opaque(
        &mut runtime,
        &host,
        &invocation,
        json!({"op":"run","id":"acceptance-stale-0001","command":"workspace:probe","token":token,"values":values}),
    );
    std::fs::write(&artisan, original).unwrap();
    assert!(stale["error"].is_string());
    assert!(host.jobs.lock().unwrap().is_empty());
    for section in ["about","events","schedule","queues"] {
        let result=runtime.mcp_call("runtime_refresh",&json!({"section":section,"runner":"php"}).to_string(),invocation.clone()).unwrap();
        let result:Value=serde_json::from_str(&result).unwrap();
        assert!(result["snapshots"].as_array().unwrap().iter().any(|snapshot|snapshot["section"]==section));
    }
    let rendered=request(&mut runtime,&host,&invocation,json!({"op":"select-view","target":"laravel-runtime","targetView":"laravel-artisan"}));
    assert_eq!(rendered["model"]["body"]["columns"],json!(["Section","Resource","Details"]));
    let toolbar = rendered["model"]["toolbar"].as_array().expect("runtime toolbar");
    let toolbar_keys: std::collections::BTreeSet<_> = toolbar.iter()
        .map(|action| action["key"].as_str().expect("runtime action key"))
        .collect();
    assert_eq!(toolbar_keys.len(), toolbar.len(), "runtime actions must have unique keys");
    for required in ["runtime-database-list", "runtime-about", "runtime-events", "runtime-schedule",
        "runtime-queues", "runtime-queue-status", "runtime-workers", "runtime-logs", "runtime-runner"] {
        assert!(toolbar_keys.contains(required), "missing runtime action: {required}");
    }
    assert!(rendered["model"]["status"].as_str().unwrap().contains("4 captured sections"));
    request(&mut runtime,&host,&invocation,json!({"op":"select-view","target":"laravel-artisan","targetView":"laravel-runtime"}));
    let inventory=runtime.mcp_call("inventory",r#"{"resource":"config","query":"workspace.rows.0","limit":10}"#,invocation.clone()).unwrap();
    assert!(inventory.contains("workspace.rows.0.name"));assert!(!inventory.contains("WORKSPACE_NAME"));
    let commands:Value=serde_json::from_str(&runtime.mcp_call("artisan_commands","{}",invocation.clone()).unwrap()).unwrap();
    let agent=runtime.mcp_call("artisan_execute",&json!({"runner":"php","command":"workspace:probe","token":commands["token"],"requestId":"acceptance-agent-0001","values":{"name":"Agent value"}}).to_string(),invocation.clone()).unwrap();
    let agent:Value=serde_json::from_str(&agent).unwrap();assert_eq!(agent["job"]["running"],true);
    loop {
        let result:Value=serde_json::from_str(&runtime.mcp_call("artisan_job",r#"{"id":"acceptance-agent-0001"}"#,invocation.clone()).unwrap()).unwrap();
        if result["job"]["running"]==false {assert_eq!(result["job"]["status"],0);assert!(result["job"]["stdout"].as_str().unwrap().contains("Agent value"));break;}
        std::thread::sleep(Duration::from_millis(10));
    }
    println!("PASS compiled MCP: owned paginated inventories, four real runtime JSON snapshots, validated Artisan execution and polling");
    if host.docker.is_some() {
        opaque(&mut runtime,&host,&invocation,json!({"op":"runner","runner":"docker"}));
        let discovered=opaque(&mut runtime,&host,&invocation,json!({"op":"refresh"}));
        assert!(discovered.get("error").is_none(),"{discovered}");let token=discovered["token"].clone();
        assert_eq!(discovered["runner"],"docker");assert!(discovered["commands"].as_array().unwrap().len()>100);
        let id="acceptance-docker-0001";
        let started=opaque(&mut runtime,&host,&invocation,json!({"op":"run","id":id,"command":"workspace:probe","token":token,"values":{"name":"Docker A B; $HOME"}}));
        assert!(started.get("error").is_none(),"{started}");
        let done=settled(&mut runtime,&host,&invocation,id);assert_eq!(done["job"]["status"],0,"{done}");assert_eq!(done["job"]["runner"],"docker");assert!(done["job"]["stdout"].as_str().unwrap().contains("Docker A B; $HOME"));
        let terminal=opaque(&mut runtime,&host,&invocation,json!({"op":"run","id":"acceptance-docker-term","command":"workspace:probe","token":token,"values":{"name":"Terminal"},"terminal":true}));assert_eq!(terminal["terminal"],true,"{terminal}");assert!(host.terminals.lock().unwrap().last().unwrap().contains(&"-it".to_owned()));
        opaque(&mut denied,&host,&invocation,json!({"op":"runner","runner":"docker"}));
        let refused=opaque(&mut denied,&host,&invocation,json!({"op":"run","id":"acceptance-docker-denied","command":"workspace:probe","token":token,"values":{"name":"Denied"}}));
        assert!(refused["error"].as_str().unwrap().contains("process:docker"),"{refused}");
        let about=runtime.mcp_call("runtime_refresh",r#"{"runner":"docker","section":"about"}"#,invocation.clone()).unwrap();assert!(about.contains("php_version"));
        println!("PASS compiled Docker: exact bind inspection, separate metadata, captured execution/argv/output, interactive terminal dispatch and runtime snapshot in disposable container");
    }
    println!(
        "PASS compiled Wasm + real PHP: discovery, ordered form argv, output, nonzero exit, truncation/drain, cancellation, replay guard, history, terminal dispatch, denied process permission and stale metadata"
    );
}
