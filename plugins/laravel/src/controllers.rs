//! Controller classes of `app/Http/Controllers` and their public methods.

pub struct Controller {
    /// Relative to `App\Http\Controllers`: `Auth\LoginController`.
    pub class: String,
    /// Offset of the class name.
    pub class_offset: usize,
    /// Public instance methods and the offset of each name.
    pub methods: Vec<(String, usize)>,
}

/// The class a controller file declares, from its text with comments blanked.
pub fn parse(relative: &str, text: &str) -> Option<Controller> {
    let class=relative.strip_suffix(".php")?.replace('/',"\\");
    let short=class.rsplit('\\').next()?;
    let(full,class_offset,methods)=crate::eloquent_members::controller_declaration(text)?;
    if full.rsplit('\\').next()!=Some(short){return None;}
    Some(Controller{class,class_offset,methods})
}

/// Every method a controller file declares, whatever its visibility, or
/// `None` when the class may have more (a trait, `__call`, a parent other
/// than `Controller`).
pub fn declared_methods(masked: &str) -> Option<Vec<String>> {
    let class_at = masked.match_indices("class ").find_map(|(at, _)| {
        let line_start = masked[..at].rfind('\n').map_or(0, |i| i + 1);
        masked[line_start..at].split_whitespace().all(|word| matches!(word, "final" | "abstract" | "readonly")).then_some(at)
    })?;
    let open = masked[class_at..].find('{').map(|i| class_at + i)?;

    let header: Vec<&str> = masked[class_at..open].split_whitespace().collect();
    if let Some(at) = header.iter().position(|word| *word == "extends") {
        let parent = header.get(at + 1)?.trim_start_matches('\\');
        if parent.rsplit('\\').next() != Some("Controller") {
            return None;
        }
    }

    let body = &masked[open..];
    let has_trait = body.lines().skip(1).map(str::trim).any(|line| line.starts_with("use ") && !line.contains('(') && line.ends_with(';'));
    if has_trait || body.contains("__call") {
        return None;
    }

    let names = body
        .match_indices("function ")
        .map(|(at, _)| body[at + "function ".len()..].chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect::<String>())
        .filter(|name| !name.is_empty())
        .collect();
    Some(names)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_public_methods_only() {
        let text = "<?php\nnamespace App\\Http\\Controllers\\Auth;\nfinal class LoginController extends Controller\n{\n    public function login() { $f = function () {}; }\n    protected function guard() {}\n    private static function x() {}\n    function show() {}\n    public function __construct() {}\n}\n";
        let controller = parse("Auth/LoginController.php", text).unwrap();
        assert_eq!(controller.class, "Auth\\LoginController");
        let methods: Vec<_> = controller.methods.iter().map(|(m, _)| m.as_str()).collect();
        assert_eq!(methods, ["login", "show"]);
        assert_eq!(&text[controller.class_offset..controller.class_offset + 15], "LoginController");
    }

    #[test]
    fn ignores_files_that_declare_no_such_class() {
        assert!(parse("A.php", "<?php\ntrait A {}\n").is_none());
    }

    #[test]
    fn declared_methods_are_unknown_for_classes_that_may_inherit_more() {
        let plain = "<?php\nclass A extends Controller\n{\n    public function index() {}\n    protected function p() {}\n}\n";
        assert_eq!(declared_methods(plain).unwrap(), ["index", "p"]);
        assert!(declared_methods("<?php\nclass A extends Base\n{\n}\n").is_none());
        assert!(declared_methods("<?php\nclass A extends Controller\n{\n    use Foo;\n}\n").is_none());
        assert!(declared_methods("<?php\nclass A\n{\n    public function __call($m, $a) {}\n}\n").is_none());
    }
}

#[cfg(test)]mod invoke_case_tests{
    #[test]fn invoke_identity_is_case_insensitive_and_source_range_stays_exact(){let source="<?php\nnamespace App\\Http\\Controllers;\nclass Run {\npublic function __INVOKE() {}\n}\n";let controller=super::parse("Run.php",source).unwrap();assert_eq!(controller.methods[0].0,"__invoke");assert_eq!(&source[controller.methods[0].1..controller.methods[0].1+8],"__INVOKE");}
}

#[cfg(test)]
mod layout_regressions{
    #[test]
    fn ordinary_inline_class_and_method_headers_keep_exact_source_offsets_and_visibility(){
        let source="<?php namespace App\\Http\\Controllers\\Admin; final class PostController { public function SHOW(){} protected function hidden(){} public static function util(){} public function __INVOKE(){} public function __construct(){} public function outer(){ function nested(){} } }";
        let controller=super::parse("Admin/PostController.php",source).unwrap();
        assert_eq!(controller.class,"Admin\\PostController");assert_eq!(controller.class_offset,source.find("PostController {").unwrap());
        assert_eq!(controller.methods.iter().map(|(name,_)|name.as_str()).collect::<Vec<_>>(),["SHOW","__invoke","outer"]);
        for(name,at)in controller.methods{assert!(source[at..].starts_with(if name=="__invoke"{"__INVOKE"}else{&name}));}
        assert!(super::parse("Other.php",source).is_none());
    }
}
