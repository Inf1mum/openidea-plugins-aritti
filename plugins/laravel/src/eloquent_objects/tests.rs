use crate::*;
use openidea_plugin::language_features::LanguageFeatures;
use openidea_plugin::{mock::MockHost, providers::Document, text};
fn label(p: &openidea_plugin::language_features::ParameterLabel) -> &str {
    if let openidea_plugin::language_features::ParameterLabel::Text(text) = p {
        text
    } else {
        panic!("expected text parameter label")
    }
}
fn fixture(metadata: &str) -> MockHost {
    let h = MockHost::install();
    h.add_file(
        "composer.json",
        r#"{"require":{"laravel/framework":"^13"},"autoload":{"psr-4":{"App\\":"app/"}}}"#,
    );
    h.add_file("ide.json", metadata);
    h
}
fn doc(text: &str) -> Document {
    Document {
        path: "/project/app/Demo.php".into(),
        language: "php".into(),
        text: text.into(),
        version: 1,
    }
}
fn activate(h: &MockHost) -> Main {
    let mut main = Main::default();
    main.activate(&h.context("aritti.laravel")).unwrap();crate::tests::finish_indexing(&mut main);
    main
}
#[test]
fn main_mixin_inherits_parent_interface_and_selected_trait_members_with_source_ownership() {
    let h = fixture(
        r#"{"helperCode":{"classMixins":[{"classFqn":"\\Framework\\Request","mixinFqn":"\\App\\Donor"}]}}"#,
    );
    h.add_file(
        "app/ContractBase.php",
        "<?php namespace App; interface ContractBase {public function interfaceValue(): string;}",
    );
    h.add_file(
        "app/Contract.php",
        "<?php namespace App; interface Contract extends ContractBase {}",
    );
    h.add_file("app/Inheritance/ParentDonor.php","<?php namespace App; class ParentDonor {public string $inheritedProperty;public function inherited(): string{return '';} public function renamed(): string{return '';}protected function protectedMember(): string{return '';}}");
    h.add_file(
        "app/Left.php",
        "<?php namespace App; trait Left {public function choice(): string{return '';}}",
    );
    h.add_file(
        "app/Right.php",
        "<?php namespace App; trait Right {public function choice(): string{return '';}}",
    );
    h.add_file("app/Donor.php","<?php namespace App; abstract class Donor extends ParentDonor implements Contract {use Left,Right{Left::choice insteadof Right;Right::choice as alternate;}public function typedParam(string $notProperty): string{return '';}private function hidden(): string{return '';}}");
    let mut main = activate(&h);
    let d = doc("<?php function run(\\Framework\\Request $request){$request->}");
    let at = text::position(&d.text, d.text.find("->").unwrap() + 2);
    let items = main.completion(&d, at, None);
    let labels = items.iter().map(|i| i.label.as_str()).collect::<Vec<_>>();
    for expected in [
        "inheritedProperty",
        "inherited()",
        "choice()",
        "alternate()",
        "interfaceValue()",
    ] {
        assert!(labels.contains(&expected), "missing {expected}: {labels:?}");
    }
    assert!(!labels.contains(&"protectedMember()"));
    assert!(!labels.contains(&"hidden()"));
    assert!(!labels.contains(&"notProperty"));
    let d = doc("<?php function run(\\Framework\\Request $request){$request->inherited();}");
    let at = text::position(&d.text, d.text.find("inherited").unwrap() + 2);
    let found = main.definition(&d, at);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].path, "/project/app/Inheritance/ParentDonor.php");
    h.add_project("/project/app/Inheritance");
    let d = doc("<?php function run(\\Framework\\Request $request){$request->}");
    let items = main.completion(
        &d,
        text::position(&d.text, d.text.find("->").unwrap() + 2),
        None,
    );
    assert!(!items.iter().any(|i| i.label == "inherited()"));
}
fn constructor_sources() -> Vec<(&'static str, &'static str)> {
    vec![
        ("app/Bus.php", "<?php namespace App; trait Bus {}"),
        (
            "app/Value.php",
            "<?php namespace App; class Value {public const DEFAULT_VALUE='default';}",
        ),
        (
            "app/Pending.php",
            "<?php namespace App; class Pending {public function done(): string{return '';}}",
        ),
        (
            "app/Initializes.php",
            "<?php namespace App; use App\\Value as Alias; trait Initializes {public function __construct(?Alias &$value = null, array $options = [Alias::DEFAULT_VALUE], string ...$tags){}}",
        ),
        (
            "app/Other.php",
            "<?php namespace App; trait Other {public function __construct(int $wrong){}}",
        ),
        (
            "app/Job.php",
            "<?php namespace App; class Job {use Bus,Initializes,Other {Initializes::__construct insteadof Other;Initializes::__construct as protected hiddenConstructor;}}",
        ),
        (
            "app/ChildJob.php",
            "<?php namespace App; class ChildJob extends Job {}",
        ),
        (
            "app/Boots.php",
            "<?php namespace App; trait Boots {protected function boot(int &$count = 3, string ...$labels){}}",
        ),
        (
            "app/AliasJob.php",
            "<?php namespace App; class AliasJob {use Bus,Boots{Boots::boot as public __construct;}}",
        ),
        (
            "app/PrivateJob.php",
            "<?php namespace App; class PrivateJob{use Bus,Initializes{Initializes::__construct as private;}}",
        ),
    ]
}
fn constructor_metadata() -> &'static str {
    r#"{"helperCode":{"methodsWithConstructorParameters":[{"baseFqn":"\\App\\Bus","methods":[{"name":"dispatch","returnType":"\\App\\Pending"}]}]}}"#
}
#[test]
fn main_forwarded_signatures_use_adapted_inherited_constructors_and_preserve_php_syntax() {
    let h = fixture(constructor_metadata());
    for (path, source) in constructor_sources() {
        h.add_file(path, source);
    }
    let mut main = activate(&h);
    for class in ["Job", "ChildJob"] {
        let d = doc(&format!("<?php \\App\\{class}::dispatch(null, [], 'tag');"));
        let at = text::position(&d.text, d.text.find("null").unwrap());
        let help = main
            .signature_help(&d, at)
            .unwrap_or_else(|| panic!("no signature {class}"));
        let labels = help.signatures[0]
            .parameters
            .iter()
            .map(|p| label(&p.label))
            .collect::<Vec<_>>();
        assert_eq!(
            labels,
            vec![
                "?\\App\\Value &$value = null",
                "array $options = [\\App\\Value::DEFAULT_VALUE]",
                "string ...$tags"
            ],
            "{help:?}"
        );
    }
    let d = doc("<?php \\App\\AliasJob::dispatch(3, 'tag');");
    let help = main
        .signature_help(&d, text::position(&d.text, d.text.find("3,").unwrap()))
        .expect("public constructor alias");
    assert_eq!(
        label(&help.signatures[0].parameters[0].label),
        "int &$count = 3"
    );
    assert_eq!(
        label(&help.signatures[0].parameters[1].label),
        "string ...$labels"
    );
    let d = doc("<?php \\App\\PrivateJob::dispatch(null);");
    assert!(
        main.signature_help(&d, text::position(&d.text, d.text.find("null").unwrap()))
            .is_none()
    );
    let d = doc("<?php \\App\\Job::dispatch(null)->");
    assert!(
        main.completion(&d, text::position(&d.text, d.text.len()), None)
            .iter()
            .any(|i| i.label == "done()")
    );
}
#[test]
#[ignore = "requires compatible PHP CLI; reflects fixture declarations without application bootstrap"]
fn actual_php_reflection_matches_forwarded_constructor_facts() {
    let h = fixture(constructor_metadata());
    for (path, source) in constructor_sources() {
        h.add_file(path, source);
    }
    let mut main = activate(&h);
    let root = std::env::temp_dir().join(format!("openidea-e06-reflection-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (path, source) in constructor_sources() {
        let p = root.join(path);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, source).unwrap();
    }
    let runner = root.join("reflect.php");
    std::fs::write(&runner,r#"<?php spl_autoload_register(function($class){require __DIR__.'/app/'.substr($class,4).'.php';});$result=[];foreach(['Job','ChildJob','AliasJob','PrivateJob']as$class){$r=new ReflectionClass('App\\'.$class);$c=$r->getConstructor();$result[$class]=['public'=>$c->isPublic(),'parameters'=>array_map(fn($p)=>['name'=>$p->getName(),'type'=>(string)$p->getType(),'reference'=>$p->isPassedByReference(),'variadic'=>$p->isVariadic(),'optional'=>$p->isOptional(),'default'=>$p->isDefaultValueAvailable()?$p->getDefaultValue():null],$c->getParameters())];}echo json_encode($result);"#).unwrap();
    let output = std::process::Command::new(
        std::env::var("LARAVEL_NATIVE_PHP").unwrap_or_else(|_| "php".into()),
    )
    .arg(&runner)
    .output()
    .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let facts: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    for class in ["Job", "ChildJob", "AliasJob"] {
        let d = doc(&format!("<?php \\App\\{class}::dispatch(null);"));
        let help = main
            .signature_help(&d, text::position(&d.text, d.text.find("null").unwrap()))
            .unwrap();
        let actual = facts[class]["parameters"].as_array().unwrap();
        assert_eq!(help.signatures[0].parameters.len(), actual.len());
        for (parameter_label, parameter) in help.signatures[0].parameters.iter().zip(actual) {
            assert!(
                label(&parameter_label.label)
                    .contains(&format!("${}", parameter["name"].as_str().unwrap()))
            );
            assert_eq!(
                label(&parameter_label.label).contains("&$"),
                parameter["reference"].as_bool().unwrap()
            );
            assert_eq!(
                label(&parameter_label.label).contains("...$"),
                parameter["variadic"].as_bool().unwrap()
            );
            assert_eq!(
                label(&parameter_label.label).contains(" = ")
                    || label(&parameter_label.label).contains("...$"),
                parameter["optional"].as_bool().unwrap()
            );
        }
    }
    assert_eq!(facts["Job"]["parameters"][0]["type"], "?App\\Value");
    assert_eq!(
        facts["Job"]["parameters"][1]["default"],
        serde_json::json!(["default"])
    );
    assert_eq!(facts["PrivateJob"]["public"], false);
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn main_withholds_cyclic_donor_inheritance_and_ambiguous_trait_constructors() {
    let h = fixture(
        r#"{"helperCode":{"classMixins":[{"classFqn":"\\Framework\\Request","mixinFqn":"\\App\\CycleA"}],"methodsWithConstructorParameters":[{"baseFqn":"\\App\\Bus","methods":[{"name":"dispatch","returnType":"\\App\\Pending"}]}]}}"#,
    );
    for (path, source) in constructor_sources() {
        h.add_file(path, source);
    }
    h.add_file("app/CycleA.php","<?php namespace App; class CycleA extends CycleB {public function fake(): string{return '';}}");
    h.add_file(
        "app/CycleB.php",
        "<?php namespace App; class CycleB extends CycleA {}",
    );
    h.add_file(
        "app/ConflictJob.php",
        "<?php namespace App; class ConflictJob{use Bus,Initializes,Other;}",
    );
    let mut main = activate(&h);
    let d = doc("<?php function run(\\Framework\\Request $request){$request->}");
    assert!(
        !main
            .completion(
                &d,
                text::position(&d.text, d.text.find("->").unwrap() + 2),
                None
            )
            .iter()
            .any(|item| item.label == "fake()")
    );
    let d = doc("<?php \\App\\ConflictJob::dispatch(null);");
    assert!(
        main.signature_help(&d, text::position(&d.text, d.text.find("null").unwrap()))
            .is_none()
    );
}
#[test]
fn main_forwarded_signature_maps_named_variadic_and_nested_argument_context() {
    let h = fixture(constructor_metadata());
    for (path, source) in constructor_sources() {
        h.add_file(path, source);
    }
    h.add_file("app/LiteralJob.php","<?php namespace App; class LiteralJob{use Bus;public function __construct(string $label='...',int $count=1){}}");
    let mut main = activate(&h);
    for (marked, expected) in [
        ("<?php \\App\\LiteralJob::dispatch('...', 2|);", Some(1)),
        ("<?php \\App\\Job::dispatch(tags: ['a', 'b|']);", Some(2)),
        ("<?php \\App\\Job::dispatch(options: ['a', 'b|']);", Some(1)),
        (
            "<?php \\App\\Job::dispatch(options: [], value: null, tags: 'x|');",
            Some(2),
        ),
        ("<?php \\App\\Job::dispatch(null, [], 'a', 'b|');", Some(2)),
        ("<?php \\App\\Job::dispatch((null), ['a', 'b|']);", Some(1)),
        (
            "<?php \\App\\Job::dispatch(null, fn($a, $b) => [$a, $b|]);",
            Some(1),
        ),
        ("<?php \\App\\Job::dispatch(unknown: 'x|');", Some(2)),
        ("<?php \\App\\Job::dispatch(transform('a', 'x|'));", None),
        (
            "<?php \\App\\Job::dispatch(\\App\\Other::build('x|'));",
            None,
        ),
        (
            "<?php \\App\\Job::dispatch(value: null, value: 'x|');",
            None,
        ),
    ] {
        let offset = marked.find('|').unwrap();
        let d = doc(&marked.replace('|', ""));
        let actual = main
            .signature_help(&d, text::position(&d.text, offset))
            .map(|help| help.active_parameter);
        assert_eq!(actual, expected, "{marked}");
    }
}
