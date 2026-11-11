//! Ephemeral syntax snapshots shared only within one bounded index build. Exact
//! text comparison follows the hash: no borrowed text, unsafe code or cross-call
//! source assumptions are retained. All exposed token text reborrows this call.
use super::{Kind,Source,Token};
use std::{cell::RefCell,collections::{BTreeMap,VecDeque},rc::Rc};
const MAX_ENTRIES:usize=8;
const MAX_BYTES:usize=8*1024*1024;
#[derive(Clone)]struct OwnedToken{start:usize,end:usize,kind:Kind}
struct Syntax{tokens:Vec<OwnedToken>,pairs:Rc<BTreeMap<usize,usize>>,paths:Rc<Vec<Vec<usize>>>}
struct Entry{hash:u64,text:String,syntax:Option<Syntax>,bytes:usize}
#[derive(Default)]struct Cache{depth:u32,entries:VecDeque<Entry>,bytes:usize}
thread_local!{static CACHE:RefCell<Cache>=RefCell::new(Cache::default());}
pub(crate) struct Scope;
pub(crate) fn enter()->Scope{CACHE.with(|slot|{let mut cache=slot.borrow_mut();if cache.depth==0{cache.entries.clear();cache.bytes=0;}cache.depth=cache.depth.saturating_add(1);});Scope}
impl Drop for Scope{fn drop(&mut self){CACHE.with(|slot|{let mut cache=slot.borrow_mut();cache.depth=cache.depth.saturating_sub(1);if cache.depth==0{cache.entries.clear();cache.bytes=0;}});}}
pub(super) enum Lookup<'a>{Inactive,Miss(u64),Hit(Option<Source<'a>>)}
pub(super) fn lookup(text:&str)->Lookup<'_>{CACHE.with(|slot|{let mut cache=slot.borrow_mut();if cache.depth==0{return Lookup::Inactive;}
 let hash=text.bytes().fold(0xcbf29ce484222325u64,|hash,byte|(hash^u64::from(byte)).wrapping_mul(0x100000001b3));
 let Some(at)=cache.entries.iter().position(|entry|entry.hash==hash&&entry.text==text)else{return Lookup::Miss(hash);};
 let entry=cache.entries.remove(at).unwrap();let result=entry.syntax.as_ref().map(|syntax|Source{text,tokens:syntax.tokens.iter().map(|token|Token{value:&text[token.start..token.end],start:token.start,end:token.end,kind:token.kind}).collect(),pairs:syntax.pairs.clone(),paths:syntax.paths.clone()});cache.entries.push_front(entry);Lookup::Hit(result)
})}
pub(super) fn remember(hash:u64,text:&str,source:Option<&Source<'_>>){CACHE.with(|slot|{let mut cache=slot.borrow_mut();if cache.depth==0{return;}
 let derived=source.map_or(0,|source|source.tokens.len()*std::mem::size_of::<OwnedToken>()+source.pairs.len()*64+source.paths.len()*std::mem::size_of::<Vec<usize>>()+source.paths.iter().map(|path|path.len()*std::mem::size_of::<usize>()).sum::<usize>());
 let bytes=text.len().saturating_add(derived).saturating_add(std::mem::size_of::<Entry>());if bytes>MAX_BYTES{return;}
 while cache.entries.len()>=MAX_ENTRIES||cache.bytes.saturating_add(bytes)>MAX_BYTES{let Some(old)=cache.entries.pop_back()else{break;};cache.bytes-=old.bytes;}
 let syntax=source.map(|source|Syntax{tokens:source.tokens.iter().map(|token|OwnedToken{start:token.start,end:token.end,kind:token.kind}).collect(),pairs:source.pairs.clone(),paths:source.paths.clone()});cache.entries.push_front(Entry{hash,text:text.into(),syntax,bytes});cache.bytes+=bytes;
});}
#[cfg(test)]mod tests{use super::*;
 #[test]fn shares_immutable_syntax_but_reborrows_current_text_and_clears_at_scope_exit(){
  let first="<?php namespace App; class A {public function run(): void{}}".to_owned();let second=first.clone();let guard=enter();let a=Source::new(&first).unwrap();let b=Source::new(&second).unwrap();assert!(Rc::ptr_eq(&a.paths,&b.paths));assert!(Rc::ptr_eq(&a.pairs,&b.pairs));assert_eq!(b.tokens[0].value.as_ptr(),second.as_ptr());assert_ne!(a.tokens[0].value.as_ptr(),b.tokens[0].value.as_ptr());drop(guard);let c=Source::new(&second).unwrap();assert!(!Rc::ptr_eq(&a.paths,&c.paths));CACHE.with(|cache|{assert_eq!(cache.borrow().bytes,0);assert!(cache.borrow().entries.is_empty());});
 }
 #[test]fn modified_source_and_mru_eviction_cannot_reuse_stale_tokens(){let _guard=enter();let a=Source::new("<?php class A{}").unwrap();let changed=Source::new("<?php class B{}").unwrap();assert!(!Rc::ptr_eq(&a.paths,&changed.paths));assert!(changed.tokens.iter().any(|token|token.value=="B"));for i in 0..9{Source::new(&format!("<?php class C{i}{{}}")).unwrap();}let next=Source::new("<?php class A{}").unwrap();assert!(!Rc::ptr_eq(&a.paths,&next.paths));CACHE.with(|cache|{let cache=cache.borrow();assert!(cache.entries.len()<=MAX_ENTRIES);assert!(cache.bytes<=MAX_BYTES);});}
}
