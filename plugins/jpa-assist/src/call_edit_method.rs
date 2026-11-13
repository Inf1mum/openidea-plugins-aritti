//! Caret on a call of an existing repository method (`customerRepository.findByTitle(..)`): open the reviewed query method
//! editor on that method without leaving the calling file. The repository is located with a few direct reads from the import
//! (or the package) of the declared type, never by a project search, so the code-action request stays fast; resolving then runs
//! `query_edit_action` on the repository document, which is the only document edited.
use super::*;
use super::call_assist;
use crate::queries;

pub(super) const TITLE: &str = "Edit repository method from this call…";
const MAX_PACKAGES: usize = 4;

struct Target {
    method: String,
    repository: Document,
}

/// Packages the type may live in: its explicit import, else the own package and the wildcard imports.
fn candidate_packages(doc: &Document, ty: &str) -> Vec<String> {
    let mut found = Vec::new();
    for line in mask(&doc.text).lines() {
        let Some(import) = line.trim().strip_prefix("import ") else {
            continue;
        };
        let import = import.trim().trim_end_matches(';').trim();
        if let Some(package) = import.strip_suffix(&format!(".{ty}")) {
            return vec![package.to_owned()];
        }
        if let Some(package) = import.strip_suffix(".*") {
            found.push(package.to_owned());
        }
    }
    found.extend(package(doc));
    found.truncate(MAX_PACKAGES);
    found
}

fn source_root(doc: &Document) -> Option<String> {
    let own = package(doc)?;
    let directory = doc.path.rsplit_once('/')?.0;
    let suffix = format!("/{}", own.replace('.', "/"));
    directory.strip_suffix(suffix.as_str()).map(str::to_owned)
}

fn target(doc: &Document, selection: Range, project: &Project) -> Option<Target> {
    if !call_assist::spring_component(doc) || doc.text.len() > MAX_TEXT {
        return None;
    }
    let (method, repository_type) = call_assist::call_method(doc, selection)?;
    let root = source_root(doc)?;
    for package in candidate_packages(doc, &repository_type) {
        for extension in ["java", "kt"] {
            let file = format!("{root}/{}/{repository_type}.{extension}", package.replace('.', "/"));
            let Ok(text) = project.read_file(&file) else {
                continue;
            };
            if text.len() > MAX_TEXT || path(&file).as_deref() != Some(file.as_str()) {
                continue;
            }
            let kotlin = extension == "kt";
            let declared = queries::parse_repository(&text, kotlin)?;
            if !declared.methods.iter().any(|existing| existing.name == method && !existing.declared_query && queries::split_subject(&method).is_some()) {
                return None;
            }
            let repository = Document { path: file, language: if kotlin { "kotlin" } else { "java" }.into(), version: 0, text };
            return query_edit_action::eligible(&repository).then_some(Target { method, repository });
        }
    }
    None
}

pub(super) fn eligible(doc: &Document, selection: Range) -> bool {
    host::project_of(&doc.path).is_some_and(|project| target(doc, selection, &project).is_some())
}

pub(super) fn resolve(pending: &Pending, doc: &Document, project: &Project) -> Result<FileAction, Error> {
    current(pending, doc)?;
    let Target { method, repository } = target(doc, pending.selection, project).ok_or(Error::Stale)?;
    let repository_pending = Pending { root: pending.root.clone(), doc: repository.clone(), operation: Operation::QueryEdit, selection: Range::default() };
    let action = query_edit_action::resolve_method(&repository_pending, &repository, project, Some(&method))?;
    current(pending, doc)?;
    Ok(action)
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::{mock::MockHost, text::position};

    const REPOSITORY: &str = "package shop.domain;\n\nimport java.util.List;\nimport org.springframework.data.jpa.repository.JpaRepository;\n\npublic interface CustomerRepository extends JpaRepository<Customer, Long> {\n    List<Customer> findByTitle(String title);\n\n    List<Customer> findAllByOrderByTitleAsc();\n}\n";
    const ENTITY: &str = "package shop.domain;\n\nimport jakarta.persistence.Entity;\nimport jakarta.persistence.Id;\n\n@Entity\npublic class Customer {\n    @Id\n    private Long id;\n\n    private String title;\n\n    private Boolean active;\n}\n";
    const SERVICE: &str = "package shop.service;\n\nimport org.springframework.stereotype.Service;\nimport shop.domain.CustomerRepository;\n\n@Service\npublic class CustomerService {\n    private final CustomerRepository customerRepository;\n\n    CustomerService(CustomerRepository customerRepository) {\n        this.customerRepository = customerRepository;\n    }\n\n    void run() {\n        customerRepository.findByTitle(\"a\");\n        customerRepository.findByMissing(\"a\");\n    }\n}\n";
    const PATH: &str = "/project/src/main/java/shop/service/CustomerService.java";

    fn setup(service: &str) -> (MockHost, JpaAssist, Document) {
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        host.add_file("src/main/java/shop/domain/Customer.java", ENTITY);
        host.add_file("src/main/java/shop/domain/CustomerRepository.java", REPOSITORY);
        host.add_file("src/main/java/shop/service/CustomerService.java", service);
        host.open_editor(PATH, "java", service, &[]);
        (host, JpaAssist::default(), Document { path: PATH.into(), language: "java".into(), version: 1, text: service.into() })
    }

    fn on(doc: &Document, word: &str) -> Range {
        let start = doc.text.find(word).unwrap();
        Range::new(position(&doc.text, start), position(&doc.text, start + 2))
    }

    fn offered(plugin: &mut JpaAssist, doc: &Document, word: &str) -> Option<FileAction> {
        plugin.file_actions(doc, on(doc, word)).into_iter().find(|action| action.title == TITLE)
    }

    #[test]
    fn offered_only_on_a_call_whose_method_the_repository_declares() {
        let (_host, mut plugin, doc) = setup(SERVICE);
        assert!(offered(&mut plugin, &doc, "findByTitle").is_some());
        assert!(offered(&mut plugin, &doc, "findByMissing").is_none());
        assert!(offered(&mut plugin, &doc, "customerRepository;").is_none());
        let plain = SERVICE.replace("@Service\n", "");
        let (_host, mut plugin, doc) = setup(&plain);
        assert!(offered(&mut plugin, &doc, "findByTitle").is_none());
    }

    #[test]
    fn not_offered_when_the_repository_is_not_a_direct_read_away() {
        let moved = SERVICE.replace("shop.domain.CustomerRepository", "shop.elsewhere.CustomerRepository");
        let (_host, mut plugin, doc) = setup(&moved);
        assert!(offered(&mut plugin, &doc, "findByTitle").is_none());
    }

    #[test]
    fn the_editor_runs_on_the_called_method_and_edits_only_the_repository() {
        let (host, mut plugin, doc) = setup(SERVICE);
        let pending = offered(&mut plugin, &doc, "findByTitle").unwrap();
        assert!(pending.operations.is_empty() && pending.edits.is_empty());
        for answer in [&["0"] as &[&str], &["active"], &["true"], &[], &["keep"], &["and"], &["~keep"], &[""], &["list"], &[]] {
            host.answer_next_prompt(Some(answer));
        }
        let result = plugin.resolve_file_action(&doc, pending).unwrap();
        let [(edited, edits)] = result.edits.as_slice() else { panic!("only the repository is edited") };
        assert_eq!(edited, "/project/src/main/java/shop/domain/CustomerRepository.java");
        let after = patched(REPOSITORY, edits).unwrap();
        assert!(after.contains("findByTitleAndActiveTrue(String title)"), "{after}");
        assert!(after.contains("findAllByOrderByTitleAsc"), "{after}");
        assert_eq!(host.editor_text(), SERVICE);
    }

    #[test]
    fn a_buffer_edit_after_the_request_makes_the_action_stale() {
        let (host, mut plugin, doc) = setup(SERVICE);
        let pending = offered(&mut plugin, &doc, "findByTitle").unwrap();
        host.open_editor(PATH, "java", &format!("{SERVICE}\n"), &[]);
        assert!(plugin.resolve_file_action(&doc, pending).is_err());
    }
}
