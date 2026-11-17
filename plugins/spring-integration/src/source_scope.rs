//! Current project metadata admission for cached physical-source facts.
//! This does not read source or change the shared index's discovery/I/O policy.
use openidea_plugin::host;
use spring_text::index::Index;
use std::time::{Duration,Instant};

pub(crate) struct Scope { root:String, nested:Vec<String> }
fn normalized(path:&str)->bool {
    path.starts_with('/') && spring_text::scope::scope_path(path).as_deref()==Some(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;
    #[test]
    fn metadata_scope_uses_component_boundaries_and_withholds_noncanonical_paths() {
        let host=MockHost::install();assert!(Scope::current("/project").unwrap().contains("/project/nested/A.java"));
        host.add_project("/project/nested");let scope=Scope::current("/project").unwrap();
        assert!(!scope.contains("/project/nested/A.java"));assert!(scope.contains("/project/nested-other/A.java"));
        for path in ["/project-other/A.java","/project/../A.java","/project//A.java","/project/./A.java"] {assert!(!scope.contains(path));}
        assert!(host.process_runs().is_empty() && host.requests().is_empty());
    }
    #[test]
    fn metadata_scope_refuses_unbounded_or_unknown_root_sets() {
        let host=MockHost::install();assert!(Scope::current("/unknown").is_none());
        for n in 0..64 {host.add_project(&format!("/other{n}"));}
        assert!(Scope::current("/project").is_none());
    }
}
fn within(path:&str,root:&str)->bool {path.strip_prefix(root).is_some_and(|rest|rest.is_empty() || rest.starts_with('/'))}
impl Scope {
    pub(crate) fn current(root:&str)->Option<Self> {
        if !normalized(root) {return None;}
        let projects=host::projects().ok()?;if projects.len()>64 {return None;}
        let mut roots=projects.into_iter().map(|p|p.root().ok()).collect::<Option<Vec<_>>>()?;
        if roots.iter().any(|path|!normalized(path)) || !roots.iter().any(|path|path==root) {return None;}
        roots.sort_by_key(String::len);let mut nested:Vec<String>=Vec::new();
        for path in roots.into_iter().filter(|path|path!=root && within(path,root)) {
            if !nested.iter().any(|outer|within(&path,outer)) {nested.push(path);}
        }
        Some(Self{root:root.into(),nested})
    }
    pub(crate) fn for_path(path:&str)->Option<Self> {
        let root=host::project_of(path)?.root().ok()?;let scope=Self::current(&root)?;
        scope.contains(path).then_some(scope)
    }
    pub(crate) fn refresh_for_path(&self,path:&str)->Option<Self> {
        let scope=Self::current(&self.root)?;scope.contains(path).then_some(scope)
    }
    fn contains(&self,path:&str)->bool {normalized(path) && within(path,&self.root) && !self.nested.iter().any(|root|within(path,root))}
    pub(crate) fn prune<T:Clone>(&self,index:&mut Index<T>)->bool {
        if index.files().len()>10_000 {return false;}
        let started=Instant::now();let mut foreign=Vec::new();
        for path in index.files().keys() {
            if started.elapsed()>Duration::from_millis(5) {return false;}
            if !self.contains(path) {foreign.push(path.clone());}
        }
        for path in foreign {
            if started.elapsed()>Duration::from_millis(5) {return false;}
            index.remove(&path);
        }
        true
    }
}
