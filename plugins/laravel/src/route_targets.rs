//! Cache-only controller string destinations with exact PHP namespace/import proof.
use crate::{Index, index::Entry, refs};
use openidea_plugin::{Location, Position, Range, providers::Document, text};
/// A proven framework controller literal owns its complete lookup result.
/// A missing exact target must not fall through to short-name graph guessing.
fn reference(index:&Index,doc:&Document,offset:usize)->Option<refs::Reference>{
    if let Some(reference)=crate::metadata::route_reference(&index.metadata,&doc.text,offset){return Some(reference);}
    let reference=refs::reference_at("php",&doc.text,offset)?;
    crate::actions::controller_binding_safe(&doc.text,&(reference.start..reference.end)).then_some(reference)
}
pub(crate) fn recognized(index:&Index,doc:&Document,at:Position)->bool{
    if doc.language!="php"{return false;}
    let Some(offset)=text::offset(&doc.text,at)else{return false;};
    reference(index,doc,offset).is_some_and(|reference|reference.closed&&matches!(reference.kind,refs::Kind::Action|refs::Kind::Controller|refs::Kind::Method))
}
fn select<'a>(
    index: &'a Index,
    doc: &Document,
    at: Position,
) -> Option<(&'a Entry, Option<&'a Entry>, bool)> {
    if index.partial || doc.language != "php" || index.route_mount_unknown.contains(&doc.path) {
        return None;
    }
    let offset = text::offset(&doc.text, at)?;
    let reference = reference(index,doc,offset)?;
    if !reference.closed
        || !matches!(
            reference.kind,
            refs::Kind::Action | refs::Kind::Controller | refs::Kind::Method
        )
    {
        return None;
    }
    let literal = doc.text.get(reference.start..reference.end)?;
    let method_requested = reference.kind == refs::Kind::Method
        || reference.kind == refs::Kind::Action
            && literal
                .rfind('@')
                .is_none_or(|at| offset > reference.start + at);
    let (class, method) = if reference.kind == refs::Kind::Method {
        (reference.class?, Some(literal.to_owned()))
    } else {
        let quote = *doc.text.as_bytes().get(reference.start.checked_sub(1)?)? as char;
        let decoded = crate::actions::controller_literal(literal, quote)?;
        if let Some((class, method)) = decoded.split_once('@') {
            (class.to_owned(), Some(method.to_owned()))
        } else {
            (
                decoded,
                (reference.kind == refs::Kind::Action).then(|| "__invoke".into()),
            )
        }
    };
    let context=index.route_mounts.get(&doc.path).cloned().unwrap_or_default();
    let effective=crate::routes::controller_at(&doc.text,&index.metadata.route_methods,&context,offset);
    let (full,method)=if let Some((full,member))=effective{
        (full,method.map(|_|member).filter(|member|!member.is_empty()))
    }else{
        let full=if reference.kind != refs::Kind::Method && class.contains('\\') {
            class.trim_start_matches('\\').to_owned()
        }else{crate::eloquent_members::source_class_name(&doc.text,&class)?};
        (full,method)
    };
    let identities = crate::eloquent_members::class_inventory(&index.eloquent_models);
    let mut matching = index.controllers.iter().filter(|(_, entry)| {
        identities
            .iter()
            .filter(|identity| {
                !identity.ambiguous
                    && !identity.is_trait
                    && identity.path == entry.path
                    && identity.full.eq_ignore_ascii_case(&full)
            })
            .count()
            == 1
    });
    let (key, entry) = matching.next()?;
    if matching.next().is_some() {
        return None;
    }
    let project = crate::project_for(&doc.path)?;
    if !crate::owns_current_path(&project, &entry.path) {
        return None;
    }
    if doc.path == entry.path {
        let name = full.rsplit('\\').next()?;
        let begin = text::offset(&doc.text, entry.at)?;
        let code = crate::package_facts::code(&doc.text)?;
        if code.get(begin..begin + name.len()) != Some(name)
            || !code[..begin].trim_end().ends_with("class")
        {
            return None;
        }
    }
    let method = method.and_then(|name| {
        let methods = index.methods.get(key)?;
        let mut matching = methods.iter().filter(|(method, entry)| {
            method.eq_ignore_ascii_case(&name) && crate::owns_current_path(&project, &entry.path)
        });
        let (_, entry) = matching.next()?;
        matching.next().is_none().then_some(entry)
    });
    Some((entry, method, method_requested))
}
fn location(entry: &Entry) -> Location {
    Location {
        path: entry.path.clone(),
        range: Range::new(
            entry.at,
            Position::new(
                entry.at.line,
                entry.at.character.saturating_add(entry.width),
            ),
        ),
    }
}
pub(crate) fn definition(index: &Index, doc: &Document, at: Position) -> Vec<Location> {
    select(index, doc, at)
        .and_then(|(class, method, requested)| {
            if requested {
                method.map(location)
            } else {
                Some(location(class))
            }
        })
        .into_iter()
        .collect()
}
pub(crate) fn type_definition(index: &Index, doc: &Document, at: Position) -> Vec<Location> {
    select(index, doc, at)
        .map(|(class, _, _)| location(class))
        .into_iter()
        .collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;
    fn fixture() -> (MockHost, Index) {
        let host = MockHost::install();
        host.add_file(
            "composer.json",
            r#"{"require":{"laravel/framework":"^13.0"},"autoload":{"psr-4":{"App\\":"app/"}}}"#,
        );
        host.add_file("artisan", "<?php");
        for folder in ["Admin", "Public"] {
            host.add_file(&format!("app/Http/Controllers/{folder}/PostController.php"),&format!("<?php\nnamespace App\\Http\\Controllers\\{folder};\nclass PostController {{\npublic function show() {{}}\npublic function __invoke() {{}}\n}}\n"));
        }
        let index = Index::build(&host.invocation().project().unwrap());
        (host, index)
    }
    fn doc(source: &str) -> Document {
        Document {
            path: "/project/routes/web.php".into(),
            language: "php".into(),
            text: source.into(),
            version: 1,
        }
    }
    #[test]
    fn exact_qualified_and_alias_targets_do_not_pick_same_short_controller_in_other_namespace() {
        let (_host, index) = fixture();
        for (source, marker) in [
            (
                "<?php Route::get('/x','App\\Http\\Controllers\\Admin\\PostController@show');",
                "@show",
            ),
            (
                "<?php use App\\Http\\Controllers\\Admin\\PostController as Posts; Route::get('/x',[Posts::class,'show']);",
                "show'",
            ),
        ] {
            let at = text::position(source, source.find(marker).unwrap() + 1);
            let d = doc(source);
            let definition = definition(&index, &d, at);
            assert_eq!(definition.len(), 1);
            assert!(definition[0].path.contains("/Admin/"));
            let types = type_definition(&index, &d, at);
            assert_eq!(types.len(), 1);
            assert!(types[0].range.start.line < definition[0].range.start.line);
        }
    }
    #[test]
    fn real_provider_mounts_select_exact_controller_namespace_and_missing_targets_never_fall_through(){
        use openidea_plugin::providers::Providers;
        let(host,_)=fixture();
        host.add_file("app/Providers/RouteServiceProvider.php",r#"<?php Route::namespace('App\Http\Controllers\Admin')->name('admin.')->group(base_path('custom/admin.php'));"#);
        let source="<?php /* 😀 */ Route::get('/x','PostController@show')->name('post');";
        host.add_file("custom/admin.php",source);let index=Index::build(&host.invocation().project().unwrap());
        let mut main=crate::Main::default();main.indexes.insert("/project".into(),index);
        let mut current=doc(source);current.path="/project/custom/admin.php".into();let at=text::position(source,source.find("@show").unwrap()+1);
        let items=main.definition(&current,at);assert_eq!(items.len(),1);assert!(items[0].path.contains("/Admin/"));
        current.text=source.replace("@show","@missing");current.version+=1;
        let at=text::position(&current.text,current.text.find("@missing").unwrap()+1);
        assert!(main.definition(&current,at).is_empty());
        let graph=crate::indexer::contextual_navigation(main.indexes.get("/project").unwrap(),&current.path,&current.text);
        assert!(graph.definitions.iter().all(|(key,_)|!key.starts_with("route#")));
        assert!(graph.markers.iter().all(|(key,_)|!key.starts_with("controller#")));
        host.add_file("app/Providers/RouteServiceProvider.php",r#"<?php Route::namespace('App\Http\Controllers\Absent')->group(base_path('custom/admin.php'));"#);
        main.indexes.insert("/project".into(),Index::build(&host.invocation().project().unwrap()));
        current.text=source.into();current.version+=1;
        assert!(main.definition(&current,text::position(source,source.find("@show").unwrap()+1)).is_empty());
    }
    #[test]
    fn unproven_short_bindings_and_foreign_route_apis_are_withheld() {
        let (host, index) = fixture();
        for source in [
            "<?php Route::get('/x','PostController@show');",
            "<?php use Vendor\\Route; Route::get('/x','App\\Http\\Controllers\\Admin\\PostController@show');",
        ] {
            assert!(
                type_definition(
                    &index,
                    &doc(source),
                    text::position(source, source.find("@show").unwrap() + 1)
                )
                .is_empty()
            );
        }
        host.add_project("/project/app/Http/Controllers/Admin");
        let source = "<?php Route::get('/x','App\\Http\\Controllers\\Admin\\PostController@show');";
        assert!(
            type_definition(
                &index,
                &doc(source),
                text::position(source, source.find("@show").unwrap() + 1)
            )
            .is_empty()
        );
    }
}
