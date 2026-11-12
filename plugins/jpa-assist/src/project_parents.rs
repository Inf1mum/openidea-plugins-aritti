//! Project repository interfaces that can serve as the parent of a new repository ("any repository in
//! your project"): generic interfaces with exactly two type parameters (entity, identifier) that extend
//! a Spring Data repository or another such project interface. Non-generic repositories cannot take an
//! entity and identifier, so they are not offered.
use super::*;
use crate::model::split_top_level;

const SCAN_LIMIT: u32 = 1001;
const SPRING_BASES: [&str; 6] = ["Repository", "CrudRepository", "ListCrudRepository", "PagingAndSortingRepository", "ListPagingAndSortingRepository", "JpaRepository"];

pub(super) struct ProjectParent {
    pub(super) name: String,
    pub(super) package: String,
}

struct Candidate {
    name: String,
    package: String,
    bases: Vec<String>,
}

fn candidates(text: &str, kotlin: bool) -> Vec<Candidate> {
    let masked = mask(text);
    let Some(package) = masked.lines().find_map(|line| line.trim().strip_prefix("package ")).map(|p| p.trim().trim_end_matches(';').trim().to_owned()) else {
        return Vec::new();
    };
    words(&masked, "interface")
        .filter_map(|at| {
            let rest = &masked[at + "interface".len()..];
            let open = rest.find('{').unwrap_or(rest.len());
            let header = rest[..open].trim();
            let name_end = header.find(|ch: char| !(ch.is_alphanumeric() || ch == '_')).unwrap_or(header.len());
            let name = &header[..name_end];
            let generics = header[name_end..].trim_start().strip_prefix('<')?;
            let mut depth = 1;
            let close = generics.char_indices().find(|(_, ch)| {
                match ch {
                    '<' => depth += 1,
                    '>' => depth -= 1,
                    _ => {}
                }
                depth == 0
            })?.0;
            if split_top_level(&generics[..close]).len() != 2 || !identifier(name) {
                return None;
            }
            let inherited = generics[close + 1..].trim_start();
            let inherited = inherited.strip_prefix(if kotlin { ":" } else { "extends" })?;
            let bases = split_top_level(inherited)
                .into_iter()
                .filter_map(|(_, part)| part.trim().split(['<', '(']).next().map(|base| base.trim().rsplit('.').next().unwrap_or("").to_owned()))
                .collect();
            Some(Candidate { name: name.to_owned(), package: package.clone(), bases })
        })
        .collect()
}

pub(super) fn discover(project: &Project, root: &str, own_path: &str) -> Result<Vec<ProjectParent>, Error> {
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut found = Vec::new();
    for extension in ["java", "kt"] {
        let files = project.find_files(&format!("**/*.{extension}"), SCAN_LIMIT)?;
        if files.len() >= SCAN_LIMIT as usize {
            return Err("Too many sources to discover project repositories.".into());
        }
        for file in files {
            if Instant::now() >= deadline {
                return Err("Project repository discovery timed out.".into());
            }
            let file = if file.starts_with('/') { file } else { format!("{}/{file}", root.trim_end_matches('/')) };
            if file == own_path || !inside(root, &file) {
                continue;
            }
            let text = project.read_file(&file)?;
            if text.len() > MAX_TEXT || !text.contains("Repository") {
                continue;
            }
            found.extend(candidates(&text, extension == "kt"));
        }
    }
    let mut known: Vec<String> = SPRING_BASES.iter().map(|name| (*name).to_owned()).collect();
    let mut accepted: Vec<ProjectParent> = Vec::new();
    loop {
        let before = accepted.len();
        for candidate in &found {
            if accepted.iter().any(|a| a.name == candidate.name && a.package == candidate.package) || !candidate.bases.iter().any(|base| known.contains(base)) {
                continue;
            }
            known.push(candidate.name.clone());
            accepted.push(ProjectParent { name: candidate.name.clone(), package: candidate.package.clone() });
        }
        if accepted.len() == before {
            break;
        }
    }
    accepted.sort_by(|a, b| (&a.package, &a.name).cmp(&(&b.package, &b.name)));
    Ok(accepted)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generic_two_parameter_repository_interfaces_are_candidates_and_chain() {
        let base = candidates("package shop;\n\n@NoRepositoryBean\npublic interface BaseRepository<T, ID extends Serializable> extends JpaRepository<T, ID> {\n}\n", false);
        assert_eq!((base[0].name.as_str(), base[0].bases.as_slice()), ("BaseRepository", ["JpaRepository".to_owned()].as_slice()));
        assert!(candidates("package shop;\ninterface OrderRepository extends JpaRepository<Order, Long> {}\n", false).is_empty());
        let kotlin = candidates("package shop\ninterface KBase<T, ID> : CrudRepository<T, ID>\n", true);
        assert_eq!(kotlin[0].name, "KBase");
    }
}
