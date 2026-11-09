//! Current project membership of cached facts, without source discovery or reads.
use super::*;
use std::collections::BTreeSet;

pub(super) trait Facts {
    fn iter(&self) -> impl Iterator<Item = (&str, &Info)>;
    fn fresh(&self) -> bool;
}
impl Facts for Index<Info> {
    fn iter(&self) -> impl Iterator<Item = (&str, &Info)> { Index::iter(self) }
    fn fresh(&self) -> bool { Index::fresh(self) }
}
fn path_valid(path: &str) -> bool {
    (path.starts_with('/') || path.as_bytes().get(1)==Some(&b':') && path.as_bytes().get(2)==Some(&b'/') && path.as_bytes()[0].is_ascii_alphabetic())
        && path.len()<=4096 && !path.contains('\\') && !path.chars().any(char::is_control)
        && !path.split('/').skip(usize::from(path.starts_with('/'))).any(|part|matches!(part,""|"."|".."))
}
pub(super) fn roots() -> Option<Vec<String>> {
    let projects=host::projects().ok()?;
    if projects.len()>64 {return None;}
    let mut roots=projects.into_iter().map(|project| {
        let root=project.root().ok()?;
        path_valid(&root).then_some(root)
    }).collect::<Option<Vec<_>>>()?;
    roots.sort(); roots.dedup(); Some(roots)
}
fn owner<'a>(roots:&'a [String],path:&str)->Option<&'a str> {
    if !path_valid(path) {return None;}
    roots.iter().filter(|root|path.strip_prefix(root.as_str()).is_some_and(|tail|tail.starts_with('/')))
        .max_by_key(|root|root.len()).map(String::as_str)
}
pub(super) struct Scope {roots:Vec<String>,root:Option<String>}
impl Scope {
    pub fn new(roots:Option<Vec<String>>,path:&str)->Self {
        let roots=roots.unwrap_or_default();
        let root=owner(&roots,path).map(str::to_owned);
        Self{roots,root}
    }
    pub fn known(&self)->bool {self.root.is_some()}
}
pub(super) struct Scoped<'a> {index:&'a Index<Info>,allowed:BTreeSet<&'a str>,complete:bool}
impl<'a> Scoped<'a> {
    pub fn new(index:&'a Index<Info>,scope:&Scope)->Self {
        let mut allowed=BTreeSet::new();
        let mut complete=scope.known();
        for (path,_) in index.iter() {
            match owner(&scope.roots,path) {
                Some(owner) if Some(owner)==scope.root.as_deref()=>{allowed.insert(path);}
                Some(_)=>{} // a known other owner is excluded, not merged into this model
                None=>complete=false,
            }
        }
        Self{index,allowed,complete}
    }
}
impl Facts for Scoped<'_> {
    fn iter(&self)->impl Iterator<Item=(&str,&Info)> {self.index.iter().filter(|(path,_)|self.allowed.contains(*path))}
    fn fresh(&self)->bool {self.complete && self.index.fresh()}
}
