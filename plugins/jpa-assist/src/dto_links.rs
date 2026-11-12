//! DTO Declaration conventions: a DTO is linked to its entity by its type name (`dtoNameRegexp`) or by
//! its Javadoc (`dtoCommentLinkRegexp`), both needing an `(?<entity>...)` group; empty disables.
//! Linked types are offered by the "reuse an existing DTO" step of the creation flows, found by a
//! bounded scan of the project sources when the action runs (never in a provider).
use super::*;
use dto_convert_action::Candidate;
use regex_lite::Regex;

const SCAN_LIMIT: u32 = 1001;

fn pattern(setting: &str, anchored: bool) -> Result<Option<Regex>, Error> {
    let value = host::setting_string(setting).unwrap_or_default();
    if value.is_empty() {
        return Ok(None);
    }
    let source = if anchored { format!("^(?:{value})$") } else { value };
    let regex = Regex::new(&source).map_err(|error| Error::from(format!("{setting} is not a valid regular expression: {error}")))?;
    if !regex.capture_names().flatten().any(|name| name == "entity") {
        return Err(format!("{setting} needs an (?<entity>...) group.").into());
    }
    Ok(Some(regex))
}

fn leading_comment(text: &str) -> Option<&str> {
    let declaration = ["class ", "record ", "interface "].iter().filter_map(|keyword| text.find(keyword)).min()?;
    let start = text[..declaration].rfind("/**")?;
    let end = start + text[start..].find("*/")?;
    (end < declaration).then(|| &text[start..end])
}

/// DTOs of the entity at `own` that the DTO Declaration settings link to it.
pub(super) fn linked(project: &Project, root: &str, own: &str, kotlin: bool) -> Result<Vec<Candidate>, Error> {
    let (by_name, by_comment) = (pattern("dtoNameRegexp", true)?, pattern("dtoCommentLinkRegexp", false)?);
    let Some(entity) = parse_class(&project.read_file(own)?, own) else {
        return Ok(Vec::new());
    };
    let qualified = entity.package.as_deref().map_or_else(|| entity.name.clone(), |package| format!("{package}.{}", entity.name));
    let extension = if kotlin { "kt" } else { "java" };
    let files = project.find_files(&format!("**/*.{extension}"), SCAN_LIMIT)?;
    if files.len() >= SCAN_LIMIT as usize {
        return Err("The project has too many sources to discover linked DTOs.".into());
    }
    let deadline = Instant::now() + Duration::from_secs(3);
    let (mut found, mut mapped, mut scanned) = (Vec::new(), Vec::new(), Vec::new());
    for file in files {
        if Instant::now() >= deadline {
            return Err("Linked DTO discovery timed out.".into());
        }
        let file = if file.starts_with('/') { file } else { format!("{}/{file}", root.trim_end_matches('/')) };
        scanned.push(file.clone());
        if file == own || path(&file).as_deref() != Some(file.as_str()) || !inside(root, &file) {
            continue;
        }
        let stem = file.rsplit('/').next().and_then(|name| name.rsplit_once('.')).map_or("", |(stem, _)| stem);
        let named = by_name.as_ref().and_then(|regex| regex.captures(stem)).and_then(|caps| caps.name("entity").map(|m| m.as_str() == entity.name)).unwrap_or(false);
        let text = project.read_file(&file)?;
        if !kotlin {
            for name in mapstruct_links::dto_names(&text, &qualified) {
                if !mapped.contains(&name) {
                    mapped.push(name);
                }
            }
        }
        let commented = by_comment.as_ref().zip(leading_comment(&text)).is_some_and(|(regex, comment)| {
            regex.captures_iter(comment).filter_map(|caps| caps.name("entity").map(|m| m.as_str().trim().to_owned())).any(|value| value == qualified || value == entity.name)
        });
        if !named && !commented {
            continue;
        }
        let Some((name, _)) = dto_convert_action::declared_fields(&text, &file) else {
            continue;
        };
        found.push(Candidate { path: file, name, badge: if named { "linked by class name" } else { "linked by comment" } });
    }
    for name in mapped.iter().take(SCAN_LIMIT as usize) {
        let Some(file) = mapstruct_links::locate(&scanned, name) else { continue };
        if file == own || found.iter().any(|known: &Candidate| known.path == file) {
            continue;
        }
        let text = project.read_file(&file)?;
        if ["@Entity", "@MappedSuperclass", "@Embeddable"].iter().any(|marker| text.contains(marker)) {
            continue;
        }
        if let Some((name, _)) = dto_convert_action::declared_fields(&text, &file) {
            found.push(Candidate { path: file, name, badge: "linked by mapper method" });
        }
    }
    found.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;

    const ENTITY_PATH: &str = "/project/src/main/java/shop/Customer.java";
    const ENTITY: &str = "package shop;\nimport jakarta.persistence.*;\n@Entity\npublic class Customer {\n @Id Long id;\n String name;\n}\n";

    fn host(settings: &[(&str, &str)]) -> (MockHost, Project) {
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        host.add_file(ENTITY_PATH, ENTITY);
        host.add_file("/project/src/main/java/shop/web/CustomerView.java", "package shop.web;\npublic record CustomerView(Long id) {}\n");
        host.add_file("/project/src/main/java/shop/web/Other.java", "package shop.web;\n/**\n * Maps {@link shop.Customer}.\n */\npublic class Other {\n private Long id;\n}\n");
        host.add_file("/project/src/main/java/shop/web/Unrelated.java", "package shop.web;\npublic record Unrelated(Long id) {}\n");
        for (key, value) in settings {
            host.set_setting(key, value);
        }
        let project = host.invocation().project().unwrap();
        (host, project)
    }

    fn names(project: &Project) -> Vec<(String, &'static str)> {
        linked(project, "/project", ENTITY_PATH, false).unwrap().into_iter().map(|c| (c.name, c.badge)).collect()
    }

    #[test]
    fn empty_settings_disable_linking_and_regexps_link_by_name_or_comment() {
        let (_host, project) = host(&[]);
        assert!(names(&project).is_empty());
        let (_host, project) = host(&[("dtoNameRegexp", "\"(?<entity>.*)View\"")]);
        assert_eq!(names(&project), vec![("CustomerView".to_owned(), "linked by class name")]);
        let (_host, project) = host(&[("dtoCommentLinkRegexp", "\"\\\\{@link (?<entity>[\\\\w.]+)\\\\}\"")]);
        assert_eq!(names(&project), vec![("Other".to_owned(), "linked by comment")]);
    }

    #[test]
    fn a_regexp_without_the_entity_group_or_invalid_is_refused() {
        let (_host, project) = host(&[("dtoNameRegexp", "\"(.*)View\"")]);
        assert!(linked(&project, "/project", ENTITY_PATH, false).is_err());
        let (_host, project) = host(&[("dtoCommentLinkRegexp", "\"(?<entity>\"")]);
        assert!(linked(&project, "/project", ENTITY_PATH, false).is_err());
    }
}
