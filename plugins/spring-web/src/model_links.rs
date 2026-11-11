//! Positive source-declared MVC model names. No absence/type/runtime claims.
use openidea_plugin::Range;
#[cfg(test)]
use openidea_plugin::text;
use spring_text::{parse_source, range, source_calls, xml};
#[derive(Clone)]
pub struct Fact {
    pub view: String,
    pub name: String,
    pub range: Range,
}
pub fn facts(path: &str, source: &str) -> Option<Vec<Fact>> {
    let file = parse_source(path, source);
    let views = crate::source_view_literals(path, source, &file);
    let calls = source_calls(path, source);
    let mut out = Vec::new();
    for class in file.types.iter().filter(|c| {
        c.has(&["Controller"]) && !c.has(&["RestController"]) && file.from_spring("Controller")
    }) {
        let class_views = views
            .iter()
            .filter(|v| class.at <= v.at && v.at < class.end)
            .collect::<Vec<_>>();
        for method in class.methods() {
            if let Some(a) = method
                .annotation("ModelAttribute")
                .filter(|_| file.from_spring("ModelAttribute"))
            {
                if let Some(value) = a
                    .strings(&["name", "value"])
                    .first()
                    .filter(|s| !s.value.is_empty() && !s.value.contains(['$', '#']))
                {
                    for view in &class_views {
                        out.push(Fact {
                            view: crate::view_name(&view.value).into(),
                            name: value.value.clone(),
                            range: range(source, value.at, value.end),
                        });
                    }
                }
            }
            let Some((from, to)) = method.body else {
                continue;
            };
            if method.ty.as_ref().is_some_and(|ty| {
                ty.name == "ModelAndView"
                    || spring_text::is_kotlin(path)
                        && ty.name == "Unit"
                        && ty.at == method.at
                        && source.as_bytes().get(from) != Some(&b'{')
            }) && file.from_spring("ModelAndView")
            {
                let tokens = spring_text::java::tokens(source);
                for call in calls.iter().filter(|c| {
                    from <= c.at
                        && c.at < to
                        && c.name == "ModelAndView"
                        && (c.is_new || spring_text::is_kotlin(path))
                }) {
                    let before = tokens.partition_point(|token| token.at < call.at);
                    let direct = if spring_text::is_kotlin(path) {
                        call.at == from || before >= 1 && tokens[before - 1].s == "return"
                    } else if call.is_new {
                        before >= 2
                            && tokens[before - 1].s == "new"
                            && tokens[before - 2].s == "return"
                    } else {
                        before >= 1 && tokens[before - 1].s == "return"
                    };
                    if !direct {
                        continue;
                    }
                    let Some(view) = call
                        .args
                        .first()
                        .and_then(|arg| arg.string.as_ref())
                        .filter(|s| !s.value.contains(['$', '#']))
                    else {
                        continue;
                    };
                    let mut attributes = Vec::new();
                    if call.args.len() == 3 {
                        if let Some(name) = call.args[1].string.as_ref() {
                            attributes.push(name);
                        }
                    }
                    if call.args.len() == 1 {
                        let mut end = call.end;
                        loop {
                            let Some(next) = calls.iter().find(|next| {
                                next.at >= end
                                    && next.at < to
                                    && next.name == "addObject"
                                    && next.args.len() == 2
                                    && source
                                        .get(end..next.at)
                                        .is_some_and(|gap| gap.trim() == ".")
                            }) else {
                                break;
                            };
                            if let Some(name) = next.args[0].string.as_ref() {
                                attributes.push(name);
                            }
                            end = next.end;
                        }
                    }
                    for name in attributes
                        .into_iter()
                        .filter(|s| !s.value.is_empty() && !s.value.contains(['$', '#']))
                    {
                        out.push(Fact {
                            view: crate::view_name(&view.value).into(),
                            name: name.value.clone(),
                            range: range(source, name.at, name.end),
                        });
                    }
                }
            }
            let receivers = method
                .params
                .iter()
                .filter(|p| {
                    matches!(p.ty.name.as_str(), "Model" | "ModelMap")
                        && file.from_spring(&p.ty.name)
                })
                .map(|p| p.name.as_str())
                .collect::<Vec<_>>();
            let method_views = class_views
                .iter()
                .filter(|v| from <= v.at && v.at < to)
                .collect::<Vec<_>>();
            for call in calls.iter().filter(|c| {
                from <= c.at
                    && c.at < to
                    && c.name == "addAttribute"
                    && c.receiver
                        .as_deref()
                        .is_some_and(|r| receivers.contains(&r))
            }) {
                let Some(value) = call
                    .args
                    .first()
                    .and_then(|a| a.string.as_ref())
                    .filter(|s| !s.value.is_empty() && !s.value.contains(['$', '#']))
                else {
                    continue;
                };
                for view in &method_views {
                    out.push(Fact {
                        view: crate::view_name(&view.value).into(),
                        name: value.value.clone(),
                        range: range(source, value.at, value.end),
                    });
                }
            }
        }
    }
    for class in file.types.iter().filter(|c| {
        c.annotation("ControllerAdvice")
            .is_some_and(|a| a.args.is_empty())
            && file.from_spring("ControllerAdvice")
    }) {
        for a in class
            .methods()
            .filter_map(|m| m.annotation("ModelAttribute"))
            .filter(|_| file.from_spring("ModelAttribute"))
        {
            if let Some(value) = a
                .strings(&["name", "value"])
                .first()
                .filter(|s| !s.value.is_empty() && !s.value.contains(['$', '#']))
            {
                out.push(Fact {
                    view: ALL_VIEWS.into(),
                    name: value.value.clone(),
                    range: range(source, value.at, value.end),
                });
            }
        }
    }
    (!out.is_empty()).then_some(out)
}
/// View marker for unrestricted `@ControllerAdvice` attributes (every view).
pub const ALL_VIEWS: &str = "*";
pub fn reference(source: &str, at: usize) -> Option<(String, Range)> {
    let all = xml::elements(source);
    for (i, e) in all.iter().enumerate() {
        for attr in &e.attrs {
            if !attr.holds(at)
                || !(attr.name.starts_with("th:") || attr.name.starts_with("data-th-"))
            {
                continue;
            }
            let raw = attr.value.trim();
            let inner = raw.strip_prefix("${")?;
            let inner = match inner.strip_suffix('}') {
                Some(inner) => inner,
                None if !inner.contains('}') => inner,
                None => return None,
            };
            let root = inner
                .split(|c: char| !(c.is_alphanumeric() || c == '_'))
                .next()?;
            if root.is_empty()
                || !root.starts_with(|c: char| c.is_alphabetic() || c == '_')
                || ["true", "false", "null", "new", "T"].contains(&root)
            {
                return None;
            }
            let start = attr.value_at + attr.value.len() - attr.value.trim_start().len() + 2;
            if !(start <= at && at <= start + root.len()) {
                return None;
            }
            let mut ancestor = Some(i);
            while let Some(n) = ancestor {
                for a in &all[n].attrs {
                    if matches!(a.name.as_str(), "th:each" | "data-th-each")
                        && a.value
                            .split(':')
                            .next()
                            .is_some_and(|vars| vars.split(',').any(|v| v.trim() == root))
                    {
                        return None;
                    }
                    if matches!(a.name.as_str(), "th:with" | "data-th-with")
                        && a.value
                            .split(',')
                            .any(|part| part.split('=').next().is_some_and(|v| v.trim() == root))
                    {
                        return None;
                    }
                }
                ancestor = all[n].parent;
            }
            return Some((root.into(), range(source, start, start + root.len())));
        }
    }
    None
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn model_names_are_scoped_to_proved_handler_views_and_known_model_receivers() {
        let s = "@Controller class C { @GetMapping(\"/one\") String one(Model model){model.addAttribute(\"account\",user); return \"users/show\";} @GetMapping(\"/two\") String two(){other.addAttribute(\"wrong\",user);return \"users/list\";} }";
        let found = facts("/p/C.java", s).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(
            (&*found[0].view, &*found[0].name),
            ("users/show", "account")
        );
        assert_eq!(
            &s[text::offset(s, found[0].range.start).unwrap()
                ..text::offset(s, found[0].range.end).unwrap()],
            "account"
        );
    }
    #[test]
    fn returned_model_and_view_constructor_and_direct_add_object_chain_keep_exact_names() {
        let source = "@Controller class C { @GetMapping(\"/one\") ModelAndView one(){return new ModelAndView(\"users/show\",\"account\",user);} @GetMapping(\"/two\") ModelAndView two(){return new ModelAndView(\"users/list\").addObject(\"rows\",rows).addObject(\"count\",count);} }";
        let found = facts("/p/C.java", source).unwrap();
        assert_eq!(
            found
                .iter()
                .map(|f| (f.view.as_str(), f.name.as_str()))
                .collect::<Vec<_>>(),
            [
                ("users/show", "account"),
                ("users/list", "rows"),
                ("users/list", "count")
            ]
        );
        for fact in found {
            assert_eq!(
                &source[text::offset(source, fact.range.start).unwrap()
                    ..text::offset(source, fact.range.end).unwrap()],
                fact.name
            );
        }
    }
    #[test]
    fn kotlin_returned_view_models_and_inferred_model_and_view_keep_owned_names() {
        let source = "@Controller\nclass Page {\n@GetMapping(\"/one\")\nfun one(model:Model):String { model.addAttribute(\"account\",user); return \"users/show\" }\n@GetMapping(\"/two\")\nfun two() = ModelAndView(\"users/list\",\"rows\",rows)\n}";
        let found = facts("/p/Page.kt", source).unwrap();
        assert_eq!(
            found
                .iter()
                .map(|f| (f.view.as_str(), f.name.as_str()))
                .collect::<Vec<_>>(),
            [("users/show", "account"), ("users/list", "rows")]
        );
    }
    #[test]
    fn unrestricted_controller_advice_model_attributes_apply_to_every_view() {
        let source = "@ControllerAdvice class A { @ModelAttribute(\"user\") User u(){return null;} @ModelAttribute User skipped(){return null;} }";
        let found = facts("/p/A.java", source).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!((&*found[0].view, &*found[0].name), (ALL_VIEWS, "user"));
        let scoped = "@ControllerAdvice(basePackages=\"x\") class A { @ModelAttribute(\"user\") User u(){return null;} }";
        assert!(facts("/p/A.java", scoped).is_none());
        let foreign = "import other.ControllerAdvice; @ControllerAdvice class A { @ModelAttribute(\"user\") User u(){return null;} }";
        assert!(facts("/p/A.java", foreign).is_none());
    }
    #[test]
    fn discarded_or_foreign_model_and_view_objects_do_not_publish_names() {
        let source = "@Controller class C { @GetMapping(\"/one\") String one(){new ModelAndView(\"wrong\",\"account\",user);return \"actual\";} }";
        assert!(facts("/p/C.java", source).is_none());
        let source = "import other.ModelAndView; @Controller class C { @GetMapping(\"/one\") ModelAndView one(){return new ModelAndView(\"wrong\",\"account\",user);} }";
        assert!(facts("/p/C.java", source).is_none());
    }
    #[test]
    fn template_locals_and_member_names_do_not_become_global_model_references() {
        let s = "<div th:each='account : ${accounts}'><p th:text='${account.name}'/></div><p th:text='${account.name}'/>";
        let first = s.find("${account.name}").unwrap() + 3;
        assert!(reference(s, first).is_none());
        let last = s.rfind("${account.name}").unwrap() + 3;
        assert_eq!(
            reference(s, last).map(|(name, _)| name).as_deref(),
            Some("account")
        );
        assert!(reference(s, last + 8).is_none());
    }
}
