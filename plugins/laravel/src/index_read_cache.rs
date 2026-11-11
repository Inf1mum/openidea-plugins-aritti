//! One bounded source snapshot per path during a single index build. Universal
//! declarations are parsed only by the caller of `Fresh`; later phases clone
//! the same text without repeating reads or universal parser side effects.
use std::collections::BTreeMap;
#[derive(Default)]
pub(super) struct SourceCache {sources:BTreeMap<String,Option<String>>,bytes:usize,exhausted:bool}
pub(super) enum Read {Fresh(String),Cached(String),Missing,Exhausted}
impl SourceCache {
 pub(super) fn sources(&self)->&BTreeMap<String,Option<String>>{&self.sources}

 pub(super) fn read(&mut self,path:&str,load:impl FnOnce()->Option<String>)->Read {
  if let Some(text)=self.sources.get(path){return text.clone().map(Read::Cached).unwrap_or(Read::Missing);}
  if self.exhausted{return Read::Exhausted;}
  // Account for stored paths as well as UTF-8 source bytes; node overhead is
  // additionally bounded by MAX_FILES. The cache is discarded after the build.
  const MAX_BYTES:usize=8*1024*1024;
  if self.sources.len()>=super::MAX_FILES as usize || self.bytes.saturating_add(path.len())>MAX_BYTES {self.exhausted=true;return Read::Exhausted;}
  let text=load();let bytes=path.len()+text.as_ref().map_or(0,String::len);
  if self.bytes.saturating_add(bytes)>MAX_BYTES{self.exhausted=true;return Read::Exhausted;}
  self.bytes+=bytes;self.sources.insert(path.into(),text.clone());text.map(Read::Fresh).unwrap_or(Read::Missing)
 }
}
#[cfg(test)]
mod tests{
 use super::*;
 #[test]fn one_snapshot_per_build_preserves_parse_side_effect_contract_and_missing_files(){
  let mut cache=SourceCache::default();let mut loads=0;
  assert!(matches!(cache.read("app/A.php",||{loads+=1;Some("first".into())}),Read::Fresh(text)if text=="first"));
  assert!(matches!(cache.read("app/A.php",||{loads+=1;Some("changed".into())}),Read::Cached(text)if text=="first"));assert_eq!(loads,1);
  assert!(matches!(cache.read("missing",||{loads+=1;None}),Read::Missing));assert!(matches!(cache.read("missing",||{loads+=1;Some("later".into())}),Read::Missing));assert_eq!(loads,2);
  let mut next=SourceCache::default();assert!(matches!(next.read("app/A.php",||Some("changed".into())),Read::Fresh(text)if text=="changed"));
 }
 #[test]fn bounded_cache_withholds_new_sources_but_retains_its_owned_snapshot(){
  let mut cache=SourceCache::default();assert!(matches!(cache.read("small",||Some("source".into())),Read::Fresh(_)));
  assert!(matches!(cache.read("large",||Some("x".repeat(8*1024*1024))),Read::Exhausted));
  assert!(matches!(cache.read("new",||panic!("must not load after exhaustion")),Read::Exhausted));assert!(matches!(cache.read("small",||panic!("must reuse snapshot")),Read::Cached(_)));
 }
}
