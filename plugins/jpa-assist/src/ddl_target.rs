//! Where a single-entity SQL draft is saved: a File (directory and name chosen) or a project-local scratch file.
//! The scratch file is a plain file under a directory set by `jpaAssist.tableDraftScratchDirectory`; it is not the
//! IDE scratch system. The clipboard target is the JPA: Copy DDL command, and a database console is not offered.
use super::*;

pub(super) const SCRATCH_SETTING: &str = "jpaAssist.tableDraftScratchDirectory";
const DEFAULT_SCRATCH_DIRECTORY: &str = ".openidea/scratches";
const MAX_SUFFIX: u32 = 99;

pub(super) fn pick_scratch() -> Result<bool, Error> {
    let choices = [
        PickItem::new("file", "File").description("choose a directory and file name; the clipboard target is the command JPA: Copy DDL"),
        PickItem::new("scratch", "Scratch file").description("project-local file under the scratch directory setting; not the IDE scratch system"),
    ];
    let key = host::pick("Save the SQL draft to", &choices)?.ok_or(Error::Cancelled)?;
    match key.as_str() {
        "file" => Ok(false),
        "scratch" => Ok(true),
        _ => Err(Error::Stale),
    }
}

/// An absolute directory inside the project from a project-relative one.
pub(super) fn directory(root: &str, relative: &str) -> Result<String, Error> {
    let relative = relative.trim().trim_matches('/');
    let absolute = format!("{}/{relative}", root.trim_end_matches('/'));
    let checked = path(&absolute).filter(|value| !relative.is_empty() && inside(root, value));
    checked.ok_or_else(|| "Use a directory inside the project, written relative to the project root.".into())
}

pub(super) fn scratch_directory(root: &str, setting: Option<&str>) -> Result<String, Error> {
    let configured = setting.map(str::trim).filter(|value| !value.is_empty());
    directory(root, configured.unwrap_or(DEFAULT_SCRATCH_DIRECTORY))
}

pub(super) fn exists(project: &Project, target: &str) -> Result<bool, Error> {
    match project.read_file(target) {
        Ok(_) => Ok(true),
        Err(Error::NotFound(_) | Error::Invalid(_)) => Ok(false),
        Err(error) => Err(error),
    }
}

/// `<stem>.sql`, or `<stem>2.sql` and so on, in the first name that is free; the draft never replaces a file.
pub(super) fn unique_target(project: &Project, directory: &str, stem: &str) -> Result<String, Error> {
    for number in 1..=MAX_SUFFIX {
        let name = if number == 1 { stem.to_owned() } else { format!("{stem}{number}") };
        let target = format!("{directory}/{name}.sql");
        if !exists(project, &target)? {
            return Ok(target);
        }
    }
    Err("Too many scratch drafts with this name; choose another stem or clear the scratch directory.".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn directories_stay_inside_the_project_and_scratch_defaults_to_a_hidden_folder() {
        assert_eq!(directory("/project", "src/db/").unwrap(), "/project/src/db");
        assert_eq!(scratch_directory("/project", None).unwrap(), "/project/.openidea/scratches");
        assert_eq!(scratch_directory("/project", Some("  ")).unwrap(), "/project/.openidea/scratches");
        assert_eq!(scratch_directory("/project", Some("tmp/drafts")).unwrap(), "/project/tmp/drafts");
        for bad in ["", "/", "../x", "a/../../x", "a/./b", "a/*"] {
            assert!(directory("/project", bad).is_err(), "{bad}");
        }
    }
}
