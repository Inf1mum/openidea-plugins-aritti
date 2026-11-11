//! Bounded, ordered literal DDL facts and explicit incomplete-source diagnostics.
use crate::migrations::{Column,Event};
#[derive(Debug)]
struct Token {
    text: String,
    at: usize,
    word: bool,
    symbol: bool,
    quoted: bool,
}
fn lex(text: &str) -> Vec<Token> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at].is_ascii_whitespace() {
            at += 1;
            continue;
        }
        if bytes[at..].starts_with(b"--") {
            at = text[at..].find('\n').map_or(bytes.len(), |n| at + n);
            continue;
        }
        if bytes[at] == b'#' {at=text[at..].find('\n').map_or(bytes.len(),|n|at+n);continue;}
        if bytes[at..].starts_with(b"/*") {
            let mut depth=1usize;at+=2;
            while at<bytes.len()&&depth>0{if bytes[at..].starts_with(b"/*"){depth+=1;at+=2;}else if bytes[at..].starts_with(b"*/"){depth-=1;at+=2;}else{at+=1;}}
            continue;
        }
        if bytes[at]==b'$' {
            if let Some(end)=text[at+1..].find('$').filter(|n|*n<=64){let tag=&text[at..at+end+2];if tag[1..tag.len()-1].bytes().all(|b|b.is_ascii_alphanumeric()||b==b'_'){
                let body=at+tag.len();if let Some(close)=text[body..].find(tag){at=body+close+tag.len();continue;}else{break;}
            }}
        }
        if matches!(bytes[at], b'\'' | b'"' | b'`' | b'[') {
            let quote = bytes[at];
            let close = if quote == b'[' { b']' } else { quote };
            let start = at + 1;
            at += 1;
            let mut value = String::new();
            let mut last = at;
            let mut closed = false;
            while at < bytes.len() {
                if bytes[at] == close {
                    value.push_str(&text[last..at]);
                    if bytes.get(at + 1) == Some(&close) {
                        value.push(close as char);
                        at += 2;
                        last = at;
                        continue;
                    }
                    at += 1;
                    closed = true;
                    break;
                }
                if quote == b'\'' && bytes[at] == b'\\' {
                    at = (at + 2).min(bytes.len());
                } else {
                    at += 1;
                }
            }
            if !closed {
                break;
            }
            out.push(Token {
                text: value,
                at: start,
                word: quote != b'\'',
                symbol:false, quoted:true,
            });
            continue;
        }
        let start = at;
        if bytes[at].is_ascii_alphabetic() || bytes[at] == b'_' {
            at += 1;
            while at < bytes.len()
                && (bytes[at].is_ascii_alphanumeric() || matches!(bytes[at], b'_' | b'$'))
            {
                at += 1;
            }
            out.push(Token {
                text: text[start..at].into(),
                at: start,
                word: true,
                symbol:false, quoted:false,
            });
        } else {
            at += text[at..].chars().next().unwrap().len_utf8();
            out.push(Token {
                text: text[start..at].into(),
                at: start,
                word: false,
                symbol:true, quoted:false,
            });
        }
    }
    out
}
fn is(ts: &[Token], at: usize, name: &str) -> bool {
    ts.get(at)
        .is_some_and(|t| t.word && !t.quoted && t.text.eq_ignore_ascii_case(name))
}
/// CREATE TABLE definitions from conventional MySQL/Postgres/SQLite dumps.
/// Constraints, LIKE/AS SELECT and arbitrary ALTER expressions are withheld.
fn create_columns(text: &str) -> Vec<Column> {
    if text.len() > 2 << 20 {
        return Vec::new();
    }
    let ts = lex(text);
    let mut out = Vec::new();
    let mut at = 0;
    while at < ts.len() {
        if !is(&ts, at, "CREATE") {
            at += 1;
            continue;
        }
        let mut i = at + 1;
        if is(&ts, i, "TEMP") || is(&ts, i, "TEMPORARY") {
            i += 1;
        }
        if !is(&ts, i, "TABLE") {
            at += 1;
            continue;
        }
        i += 1;
        if is(&ts, i, "IF") && is(&ts, i + 1, "NOT") && is(&ts, i + 2, "EXISTS") {
            i += 3;
        }
        let Some(table_token) = ts.get(i).filter(|t| t.word) else {
            at += 1;
            continue;
        };
        let mut table = table_token.text.clone();
        i += 1;
        let mut table_parts=1;
        while ts.get(i).is_some_and(|t| t.text == ".") {
            table_parts+=1;if table_parts>3{break;}
            if let Some(t) = ts.get(i + 1).filter(|t| t.word) {
                table = format!("{table}.{}",t.text);
                i += 2;
            }
        }
        if !ts.get(i).is_some_and(|t| t.text == "(") {
            at = i;
            continue;
        }
        i += 1;
        let mut depth = 1;
        let mut segment = i;
        while i < ts.len() && depth > 0 {
            match (ts[i].symbol,ts[i].text.as_str()) {
                (true,"(") => depth += 1,
                (true,")") => depth -= 1,
                _ => {}
            }
            if depth == 0 || depth == 1 && ts[i].symbol && ts[i].text == "," {
                if i > segment + 1
                    && ts[segment].word
                    && ts[segment + 1].word
                    && (ts[segment].quoted || !matches!(
                        ts[segment].text.to_ascii_uppercase().as_str(),
                        "PRIMARY"
                            | "FOREIGN"
                            | "UNIQUE"
                            | "CONSTRAINT"
                            | "CHECK"
                            | "KEY"
                            | "INDEX"
                            | "EXCLUDE"
                    ))
                {
                    let name = &ts[segment];
                    out.push(Column {
                        table: table.clone(),
                        name: name.text.clone(),
                        kind: format!("SQL {}", ts[segment + 1].text),
                        offset: name.at,
                    });
                }
                segment = i + 1;
            }
            i += 1;
        }
        // Incomplete definitions cannot establish schema facts.
        if depth != 0 {
            out.retain(|c| c.table != table);
        }
        at = i;
    }
    out
}

#[derive(Debug)]
pub(crate) struct Statement {pub events:Vec<Event>,pub if_absent:Option<String>}
fn identifier(ts:&[Token],at:&mut usize)->Option<(String,usize)>{
    let first=ts.get(*at).filter(|t|t.word)?;let mut name=first.text.clone();let mut offset=first.at;*at+=1;
    let mut parts=1;while ts.get(*at).is_some_and(|t|t.text=="."){parts+=1;if parts>3{return None;}*at+=1;let last=ts.get(*at).filter(|t|t.word)?;name=format!("{name}.{}",last.text);offset=last.at;*at+=1;}Some((name,offset))
}
fn column_identifier(ts:&[Token],at:&mut usize)->Option<(String,usize)>{let token=ts.get(*at).filter(|t|t.word)?;*at+=1;Some((token.text.clone(),token.at))}
fn statement(text:&str,offset:usize)->Option<Statement>{
    let ts=lex(text);let mut at=0;let mut events=Vec::new();let mut if_absent=None;
    if is(&ts,at,"CREATE"){
        at+=1;if is(&ts,at,"TEMP")||is(&ts,at,"TEMPORARY"){at+=1;}if !is(&ts,at,"TABLE"){return None;}at+=1;
        let guarded=is(&ts,at,"IF")&&is(&ts,at+1,"NOT")&&is(&ts,at+2,"EXISTS");if guarded{at+=3;}
        let(table,_)=identifier(&ts,&mut at)?;if ts.get(at)?.text!="("{return None;}
        let mut depth=0;let mut complete=false;for token in &ts[at..]{if token.symbol&&token.text=="("{depth+=1;}if token.symbol&&token.text==")"{depth-=1;if depth==0{complete=true;break;}}}if !complete{return None;}
        if guarded{if_absent=Some(table.clone());}events.push(Event::Create(table));
        events.extend(create_columns(text).into_iter().map(|mut c|{c.offset+=offset;Event::Add(c)}));
    }else if is(&ts,at,"DROP")&&is(&ts,at+1,"TABLE"){
        at+=2;if is(&ts,at,"IF")&&is(&ts,at+1,"EXISTS"){at+=2;}
        loop{let(table,_)=identifier(&ts,&mut at)?;events.push(Event::DropTable(table));if ts.get(at).is_some_and(|t|t.text==","){at+=1;continue;}break;}
        if ts.get(at).is_some_and(|t|!matches!(t.text.to_ascii_uppercase().as_str(),";"|"CASCADE"|"RESTRICT")){return None;}
    }else if is(&ts,at,"RENAME")&&is(&ts,at+1,"TABLE"){
        at+=2;loop{let(from,_)=identifier(&ts,&mut at)?;if !is(&ts,at,"TO"){return None;}at+=1;let(to,_)=identifier(&ts,&mut at)?;events.push(Event::RenameTable(from,to));if ts.get(at).is_some_and(|t|t.text==","){at+=1;continue;}break;}
        if ts.get(at).is_some_and(|t|t.text!=";"){return None;}
    }else if is(&ts,at,"ALTER")&&is(&ts,at+1,"TABLE"){
        at+=2;if is(&ts,at,"ONLY"){at+=1;}let(table,_)=identifier(&ts,&mut at)?;
        let mut start=at;let mut depth=0;let mut clauses=Vec::new();for i in at..ts.len(){if ts[i].symbol&&ts[i].text=="("{depth+=1;}if ts[i].symbol&&ts[i].text==")"{depth-=1;if depth<0{return None;}}if depth==0&&ts[i].symbol&&matches!(ts[i].text.as_str(),","|";"){if start<i{clauses.push(&ts[start..i]);}start=i+1;}}
        if depth!=0{return None;}if start<ts.len(){clauses.push(&ts[start..]);}if clauses.is_empty(){return None;}
        let mut implicit=None;
        for clause in clauses{
            let inherited=implicit.filter(|_|!is(clause,0,"ADD")&&!is(clause,0,"DROP")&&!is(clause,0,"ALTER")&&!is(clause,0,"RENAME")&&!is(clause,0,"MODIFY")&&!is(clause,0,"CHANGE"));
            let mut i=if inherited.is_some(){0}else{1};
            if is(clause,0,"ADD")||is(clause,0,"MODIFY")||is(clause,0,"CHANGE")||inherited==Some("ADD"){
                implicit=Some("ADD");
                if is(clause,i,"COLUMN"){i+=1;}
                if constraint_start(clause,i){constraint(clause,i,&table,offset)?;continue;}if is(clause,i,"PERIOD"){return None;}
                if is(clause,i,"IF")&&is(clause,i+1,"NOT")&&is(clause,i+2,"EXISTS"){i+=3;}
                let(name,name_at)=column_identifier(clause,&mut i)?;
                let mut name=name;let mut name_at=name_at;
                if is(clause,0,"CHANGE"){let(to,to_at)=column_identifier(clause,&mut i)?;events.push(Event::RenameColumn{table:table.clone(),from:name,to:to.clone(),offset:offset+to_at});name=to;name_at=to_at;}
                let kind=clause.get(i).filter(|t|t.word)?.text.clone();if inherited.is_some()&&!matches!(kind.to_ascii_lowercase().as_str(),"bigint"|"binary"|"bit"|"blob"|"bool"|"boolean"|"char"|"date"|"datetime"|"datetime2"|"decimal"|"double"|"enum"|"float"|"image"|"int"|"integer"|"json"|"jsonb"|"money"|"nchar"|"ntext"|"numeric"|"nvarchar"|"real"|"serial"|"smallint"|"smallmoney"|"text"|"time"|"timestamp"|"tinyint"|"uniqueidentifier"|"uuid"|"varbinary"|"varchar"|"xml"){return None;}
                let column=Column{table:table.clone(),name,kind:format!("SQL {kind}"),offset:offset+name_at};
                events.push(if is(clause,0,"ADD")||inherited==Some("ADD"){Event::Add(column)}else{Event::Change(column)});
            }else if is(clause,0,"DROP")||inherited==Some("DROP"){
                implicit=Some("DROP");
                if constraint_start(clause,i){drop_constraint(clause)?;continue;}
                if is(clause,i,"COLUMN"){i+=1;}if is(clause,i,"IF")&&is(clause,i+1,"EXISTS"){i+=2;}let(name,_)=column_identifier(clause,&mut i)?;
                if clause.get(i).is_some_and(|t|!matches!(t.text.to_ascii_uppercase().as_str(),"CASCADE"|"RESTRICT")){return None;}
                events.push(Event::DropColumn(table.clone(),name));
            }else if is(clause,0,"RENAME"){
                implicit=None;
                if is(clause,i,"CONSTRAINT")||is(clause,i,"INDEX")||is(clause,i,"KEY"){if clause.len()!=5||!clause[2].word||!is(clause,3,"TO")||!clause[4].word{return None;}continue;}
                if is(clause,i,"COLUMN"){i+=1;let(from,_)=column_identifier(clause,&mut i)?;if !is(clause,i,"TO"){return None;}i+=1;let(to,at)=column_identifier(clause,&mut i)?;if i!=clause.len(){return None;}events.push(Event::RenameColumn{table:table.clone(),from,to,offset:offset+at});}
                else{if is(clause,i,"TO"){i+=1;}let(to,_)=identifier(clause,&mut i)?;if i!=clause.len(){return None;}events.push(Event::RenameTable(table.clone(),to));}
            }else if is(clause,0,"ALTER"){
                implicit=None;
                if is(clause,i,"COLUMN"){i+=1;}let(name,at)=column_identifier(clause,&mut i)?;
                if is(clause,i,"SET")&&(is(clause,i+1,"DEFAULT")||is(clause,i+1,"NOT"))||is(clause,i,"DROP")&&(is(clause,i+1,"DEFAULT")||is(clause,i+1,"NOT")){continue;}
                if is(clause,i,"TYPE"){i+=1;}let kind=clause.get(i).filter(|t|t.word)?.text.clone();events.push(Event::Change(Column{table:table.clone(),name,kind:format!("SQL {kind}"),offset:offset+at}));
            }else{return None;}
        }
    }else{return None;}
    Some(Statement{events,if_absent})
}
/// Ordered literal DDL. Unknown/dynamic statements never manufacture facts.
fn slices(text:&str)->Vec<(usize,usize)>{
    if text.len()>2<<20{return vec![];}let ts=lex(text);let mut result=Vec::new();let mut start=0;let mut depth=0;
    for token in &ts{if token.symbol&&token.text=="("{depth+=1;}if token.symbol&&token.text==")"{depth-=1;}let go=token.word&&!token.quoted&&token.text.eq_ignore_ascii_case("GO")&&text[..token.at].rsplit('\n').next().unwrap_or("").trim().is_empty()&&text[token.at+token.text.len()..].split('\n').next().unwrap_or("").trim().is_empty();
        if depth==0&&(token.symbol&&token.text==";"||go){let end=if go{token.at}else{token.at+1};result.push((start,end));start=token.at+token.text.len();}}
    if start<text.len(){result.push((start,text.len()));}result
}
pub(crate) fn statements(text:&str)->Vec<Statement>{slices(text).into_iter().filter_map(|(a,b)|statement(&text[a..b],a)).collect()}
#[derive(Clone)]struct Constraint{partial:bool,table:String,name:Option<String>,kind:String,columns:Vec<String>,reference:Option<String>,reference_columns:Vec<String>,offset:usize}
fn column_list(ts:&[Token],at:&mut usize)->Option<Vec<String>>{
 if !ts.get(*at).is_some_and(|t|t.symbol&&t.text=="("){return None;}*at+=1;let mut columns=Vec::new();loop{let token=ts.get(*at).filter(|t|t.word)?;columns.push(token.text.clone());*at+=1;
 // MySQL index prefix lengths and ASC/DESC do not change column identity.
 if ts.get(*at).is_some_and(|t|t.symbol&&t.text=="("){*at+=1;while ts.get(*at).is_some_and(|t|t.text!=")"){if !ts[*at].text.bytes().all(|b|b.is_ascii_digit()){return None;}*at+=1;}if ts.get(*at)?.text!=")"{return None;}*at+=1;}
 if is(ts,*at,"ASC")||is(ts,*at,"DESC"){*at+=1;}
 match ts.get(*at)?.text.as_str(){","=>{*at+=1;},")"=>{*at+=1;return Some(columns)},_=>return None}
 if columns.len()>128{return None;}
 }
}
fn pieces(ts:&[Token])->Option<Vec<&[Token]>>{let mut parts=Vec::new();let mut start=0;let mut depth=0;for(i,t)in ts.iter().enumerate(){if t.symbol&&t.text=="("{depth+=1;}if t.symbol&&t.text==")"{depth-=1;if depth<0{return None;}}if depth==0&&t.symbol&&matches!(t.text.as_str(),","|";"){if start<i{parts.push(&ts[start..i]);}start=i+1;}}if depth!=0{return None;}if start<ts.len(){parts.push(&ts[start..]);}Some(parts)}
fn constraint(ts:&[Token],mut at:usize,table:&str,offset:usize)->Option<Constraint>{
 let first=ts.get(at)?.at;let mut name=None;
 if is(ts,at,"CONSTRAINT"){at+=1;name=Some(ts.get(at).filter(|t|t.word)?.text.clone());at+=1;}
 if is(ts,at,"FULLTEXT")||is(ts,at,"SPATIAL"){at+=1;}
 let kind=if is(ts,at,"PRIMARY"){at+=1;if !is(ts,at,"KEY"){return None;}at+=1;"primary"}
 else if is(ts,at,"UNIQUE"){at+=1;if is(ts,at,"KEY")||is(ts,at,"INDEX"){at+=1;}"unique"}
 else if is(ts,at,"FOREIGN"){at+=1;if !is(ts,at,"KEY"){return None;}at+=1;"foreign"}
 else if is(ts,at,"CHECK"){at+=1;"check"}
 else if is(ts,at,"INDEX")||is(ts,at,"KEY"){at+=1;"index"}
 else if is(ts,at,"DEFAULT"){at+=1;"default"}else{return None;};
 if is(ts,at,"CLUSTERED")||is(ts,at,"NONCLUSTERED"){at+=1;}
 if is(ts,at,"NULLS"){at+=1;if is(ts,at,"NOT"){at+=1;}if !is(ts,at,"DISTINCT"){return None;}at+=1;}
 if matches!(kind,"primary"|"unique"|"foreign"|"index")&&ts.get(at).is_some_and(|t|t.word)&&!is(ts,at,"USING"){if name.is_some(){return None;}name=Some(ts[at].text.clone());at+=1;}
 if is(ts,at,"USING"){at+=2;}
 let columns=if kind=="check"{if !ts.get(at).is_some_and(|t|t.text=="("){return None;}let mut depth=0;let mut end=None;for(i,t)in ts.iter().enumerate().skip(at){if t.symbol&&t.text=="("{depth+=1;}if t.symbol&&t.text==")"{depth-=1;if depth==0{end=Some(i+1);break;}}}at=end?;vec![]}
 else if kind=="default"{let target=(at..ts.len()).find(|i|is(ts,*i,"FOR"))?;at=target+1;let column=ts.get(at).filter(|t|t.word)?.text.clone();at+=1;vec![column]}
 else{column_list(ts,&mut at)?};
 let(mut reference,mut reference_columns)=(None,Vec::new());
 if kind=="foreign"{if !is(ts,at,"REFERENCES"){return None;}at+=1;reference=Some(identifier(ts,&mut at)?.0);reference_columns=column_list(ts,&mut at)?;if columns.len()!=reference_columns.len(){return None;}}
 // Validate conventional constraint suffixes instead of treating arbitrary
 // following words as proof of a valid declaration.
 while at<ts.len(){
  if ts[at].symbol&&ts[at].text==";"{at+=1;continue;}
  if is(ts,at,"ON"){at+=1;if !is(ts,at,"DELETE")&&!is(ts,at,"UPDATE"){if kind!="foreign"&&ts.get(at).is_some_and(|t|t.word){at+=1;if ts.get(at).is_some_and(|t|t.text=="("){column_list(ts,&mut at)?;}continue;}return None;}at+=1;if is(ts,at,"CASCADE")||is(ts,at,"RESTRICT"){at+=1;}else if is(ts,at,"SET")&&(is(ts,at+1,"NULL")||is(ts,at+1,"DEFAULT"))||is(ts,at,"NO")&&is(ts,at+1,"ACTION"){at+=2;}else{return None;}}
  else if is(ts,at,"MATCH"){at+=1;if !is(ts,at,"FULL")&&!is(ts,at,"PARTIAL")&&!is(ts,at,"SIMPLE"){return None;}at+=1;}
  else if is(ts,at,"NOT")&&(is(ts,at+1,"VALID")||is(ts,at+1,"DEFERRABLE")||is(ts,at+1,"ENFORCED"))||is(ts,at,"NO")&&is(ts,at+1,"INHERIT"){at+=2;}
  else if is(ts,at,"DEFERRABLE")||is(ts,at,"ENFORCED"){at+=1;}
  else if is(ts,at,"INITIALLY")&&(is(ts,at+1,"DEFERRED")||is(ts,at+1,"IMMEDIATE")){at+=2;}
  else if is(ts,at,"USING")&&ts.get(at+1).is_some_and(|t|t.word){at+=2;}
  else if is(ts,at,"INCLUDE"){at+=1;column_list(ts,&mut at)?;}
  else if is(ts,at,"WITH"){at+=1;if !ts.get(at).is_some_and(|t|t.text=="("){return None;}let mut depth=0;let mut closed=false;while at<ts.len(){if ts[at].symbol&&ts[at].text=="("{depth+=1;}if ts[at].symbol&&ts[at].text==")"{depth-=1;if depth==0{at+=1;closed=true;break;}}at+=1;}if !closed{return None;}}
  else{return None;}
 }
 Some(Constraint{partial:false,table:table.into(),name,kind:kind.into(),columns,reference,reference_columns,offset:offset+first})
}
fn constraint_start(ts:&[Token],at:usize)->bool{["CONSTRAINT","PRIMARY","UNIQUE","FOREIGN","CHECK","KEY","INDEX","FULLTEXT","SPATIAL"].iter().any(|word|is(ts,at,word))}
fn drop_constraint(ts:&[Token])->Option<(Option<String>,Option<&'static str>)>{
 let mut at=1;if is(ts,at,"PRIMARY"){at+=1;if !is(ts,at,"KEY")||at+1!=ts.len(){return None;}return Some((None,Some("primary")));}
 if is(ts,at,"FOREIGN"){at+=1;if !is(ts,at,"KEY"){return None;}at+=1;}else if ["CONSTRAINT","INDEX","KEY","CHECK"].iter().any(|word|is(ts,at,word)){at+=1;}else{return None;}
 if is(ts,at,"IF")&&is(ts,at+1,"EXISTS"){at+=2;}let name=ts.get(at).filter(|t|t.word)?.text.clone();at+=1;if is(ts,at,"CASCADE")||is(ts,at,"RESTRICT"){at+=1;}if at!=ts.len(){return None;}Some((Some(name),None))
}
fn inline_constraints(part:&[Token],column_at:usize,table:&str,offset:usize)->Vec<Constraint>{
 let Some(column)=part.get(column_at).filter(|t|t.word)else{return vec![];};let mut output=Vec::new();
 for i in column_at+2..part.len(){
  if is(part,i,"PRIMARY")&&is(part,i+1,"KEY")||is(part,i,"UNIQUE"){output.push(Constraint{partial:false,table:table.into(),name:None,kind:if is(part,i,"PRIMARY"){"primary"}else{"unique"}.into(),columns:vec![column.text.clone()],reference:None,reference_columns:vec![],offset:offset+column.at});}
  if is(part,i,"REFERENCES"){let mut at=i+1;let Some((reference,_))=identifier(part,&mut at)else{continue;};let Some(reference_columns)=column_list(part,&mut at)else{continue;};if reference_columns.len()!=1{continue;}let name=if i>=2&&is(part,i-2,"CONSTRAINT"){Some(part[i-1].text.clone())}else{None};output.push(Constraint{partial:false,table:table.into(),name,kind:"foreign".into(),columns:vec![column.text.clone()],reference:Some(reference),reference_columns,offset:offset+column.at});}
 }output
}
/// Catalog literal constraints through ordered table and constraint evolution.
/// Expressions/row data are never exported; CHECK records only declaration identity.
pub(crate) fn constraints(text:&str)->Vec<serde_json::Value>{
 let mut catalog=Vec::<Constraint>::new();let mut tables=std::collections::BTreeSet::new();
 for(start,end)in slices(text){let raw=&text[start..end];let ts=lex(raw);let mut at;
  if is(&ts,0,"CREATE")&&(is(&ts,1,"INDEX")||is(&ts,1,"UNIQUE")&&is(&ts,2,"INDEX")){
   let unique=is(&ts,1,"UNIQUE");at=if unique{3}else{2};if is(&ts,at,"IF")&&is(&ts,at+1,"NOT")&&is(&ts,at+2,"EXISTS"){at+=3;}let Some((name,_))=identifier(&ts,&mut at)else{continue;};if !is(&ts,at,"ON"){continue;}at+=1;let Some((table,_))=identifier(&ts,&mut at)else{continue;};if is(&ts,at,"USING"){at+=2;}let Some(columns)=column_list(&ts,&mut at)else{continue;};catalog.retain(|c|c.table!=table||c.name.as_deref()!=Some(&name));catalog.push(Constraint{partial:ts[at..].iter().any(|t|t.word&&!t.quoted&&t.text.eq_ignore_ascii_case("WHERE")),table,name:Some(name),kind:if unique{"unique"}else{"index"}.into(),columns,reference:None,reference_columns:vec![],offset:start+ts[0].at});continue;
  }
  if is(&ts,0,"DROP")&&is(&ts,1,"INDEX"){at=2;if is(&ts,at,"IF")&&is(&ts,at+1,"EXISTS"){at+=2;}let Some(name)=ts.get(at).filter(|t|t.word).map(|t|&t.text)else{continue;};catalog.retain(|c|c.name.as_ref()!=Some(name));continue;}
  let Some(parsed)=statement(raw,start)else{continue;};
  if parsed.if_absent.as_ref().is_some_and(|table|tables.contains(table)){continue;}
  for event in &parsed.events{match event{Event::Create(table)=>{tables.insert(table.clone());catalog.retain(|c|&c.table!=table);},Event::DropTable(table)=>{tables.remove(table);catalog.retain(|c|&c.table!=table&&c.reference.as_ref()!=Some(table));},Event::RenameTable(from,to)=>{if tables.remove(from){tables.insert(to.clone());}for c in &mut catalog{if &c.table==from{c.table=to.clone();}if c.reference.as_ref()==Some(from){c.reference=Some(to.clone());}}},Event::DropColumn(table,name)=>catalog.retain(|c|!(&c.table==table&&c.columns.contains(name)||c.reference.as_ref()==Some(table)&&c.reference_columns.contains(name))),Event::RenameColumn{table,from,to,..}=>for c in &mut catalog{if &c.table==table{for name in &mut c.columns{if name==from{*name=to.clone();}}}if c.reference.as_ref()==Some(table){for name in &mut c.reference_columns{if name==from{*name=to.clone();}}}},_=>{}}}
  if is(&ts,0,"CREATE"){at=1;if is(&ts,at,"TEMP")||is(&ts,at,"TEMPORARY"){at+=1;}at+=1;if is(&ts,at,"IF"){at+=3;}let Some((table,_))=identifier(&ts,&mut at)else{continue;};if ts.get(at).is_none_or(|t|t.text!="("){continue;}let body_start=at+1;let mut depth=1;at+=1;while at<ts.len(){if ts[at].symbol&&ts[at].text=="("{depth+=1;}if ts[at].symbol&&ts[at].text==")"{depth-=1;if depth==0{break;}}at+=1;}let Some(parts)=pieces(&ts[body_start..at])else{continue;};for part in parts{if let Some(value)=constraint(part,0,&table,start){catalog.push(value);}else{catalog.extend(inline_constraints(part,0,&table,start));}}}
  else if is(&ts,0,"ALTER"){at=2;if is(&ts,at,"ONLY"){at+=1;}let Some((table,_))=identifier(&ts,&mut at)else{continue;};let Some(parts)=pieces(&ts[at..])else{continue;};for part in parts{
   if is(part,0,"ADD"){if let Some(value)=constraint(part,1,&table,start){catalog.retain(|c|c.table!=table||value.name.is_none()||c.name!=value.name);catalog.push(value);}else{let column_at=if is(part,1,"COLUMN"){2}else{1};catalog.extend(inline_constraints(part,column_at,&table,start));}}
   else if is(part,0,"DROP"){if let Some((name,kind))=drop_constraint(part){catalog.retain(|c|c.table!=table||!name.as_ref().is_some_and(|name|c.name.as_ref()==Some(name))&&!kind.is_some_and(|kind|c.kind==kind));}}
   else if is(part,0,"RENAME")&&(is(part,1,"CONSTRAINT")||is(part,1,"INDEX")||is(part,1,"KEY")){if let(Some(from),Some(to))=(part.get(2),part.get(4)){if is(part,3,"TO"){for c in &mut catalog{if c.table==table&&c.name.as_ref()==Some(&from.text){c.name=Some(to.text.clone());}}}}}
  }}
 }
 catalog.into_iter().map(|c|serde_json::json!({"table":c.table,"partial":c.partial,"name":c.name,"kind":c.kind,"columns":c.columns,"referenceTable":c.reference,"referenceColumns":c.reference_columns,"offset":c.offset})).collect()
}
fn lexical_issue(text:&str)->Option<(usize,String)>{
 let b=text.as_bytes();let mut at=0;let mut parens=Vec::new();
 while at<b.len(){let start=at;
  if text[at..].starts_with("--")||b[at]==b'#'{at+=text[at..].find('\n').unwrap_or(b.len()-at);continue;}
  if text[at..].starts_with("/*"){let mut depth=1;at+=2;while at<b.len()&&depth>0{if text[at..].starts_with("/*"){depth+=1;at+=2;}else if text[at..].starts_with("*/"){depth-=1;at+=2;}else{at+=text[at..].chars().next()?.len_utf8();}}if depth!=0{return Some((start,"SQL block comment is unterminated; schema inventory is incomplete.".into()));}continue;}
  if b[at]==b'$'{if let Some(end)=text[at+1..].find('$').filter(|n|*n<=64){let tag=&text[at..at+end+2];if tag[1..tag.len()-1].bytes().all(|b|b.is_ascii_alphanumeric()||b==b'_'){at+=tag.len();let Some(close)=text[at..].find(tag)else{return Some((start,"SQL dollar-quoted body is unterminated; schema inventory is incomplete.".into()));};at+=close+tag.len();continue;}}}
  if matches!(b[at],b'\''|b'"'|b'`'|b'['){let quote=b[at];let close=if quote==b'['{b']'}else{quote};let mut closed=false;at+=1;while at<b.len(){if quote==b'\''&&b[at]==b'\\'{at=(at+2).min(b.len());continue;}if b[at]==close{if b.get(at+1)==Some(&close){at+=2;continue;}at+=1;closed=true;break;}at+=1;}if !closed{return Some((start,"SQL quoted literal/identifier is unterminated; schema inventory is incomplete.".into()));}continue;}
  if b[at]==b'('{parens.push(at);}else if b[at]==b')'&&parens.pop().is_none(){return Some((at,"SQL closing parenthesis is unmatched; schema inventory is incomplete.".into()));}
  at+=text[at..].chars().next()?.len_utf8();
 }
 parens.first().map(|at|(*at,"SQL table/constraint definition is unterminated; schema facts were withheld.".into()))
}

/// One diagnostic per unresolved DDL statement; conventional dump data/setup
/// statements are ignored. Successful columns do not imply a complete schema.
pub(crate) fn issues(text:&str)->Vec<(usize,String)>{
 if text.len()>2<<20{return vec![(0,"SQL source exceeds the 2 MiB schema parsing bound; inventory is incomplete.".into())];}
 let mut issues=Vec::new();for(start,end)in slices(text){let ts=lex(&text[start..end]);if ts.is_empty(){continue;}let ddl=["CREATE","ALTER","DROP","RENAME","DO","EXEC","EXECUTE","CALL"].iter().any(|name|is(&ts,0,name));if !ddl{continue;}
  let index=is(&ts,0,"CREATE")&&(is(&ts,1,"INDEX")||is(&ts,1,"UNIQUE")&&is(&ts,2,"INDEX"))||is(&ts,0,"DROP")&&is(&ts,1,"INDEX");
  if index&&is(&ts,0,"CREATE")&&constraints(&text[start..end]).is_empty(){issues.push((start+ts[0].at,"SQL index declaration is incomplete or expression-based; index column facts were withheld.".into()));continue;}
  if !index&&statement(&text[start..end],start).is_none(){issues.push((start+ts[0].at,"SQL statement cannot establish complete literal table/column/constraint evolution; dynamic, procedural or malformed DDL was withheld.".into()));continue;}
  if is(&ts,0,"CREATE")&&!index{let mut at=1;if is(&ts,at,"TEMP")||is(&ts,at,"TEMPORARY"){at+=1;}at+=1;if is(&ts,at,"IF"){at+=3;}if let Some((table,_))=identifier(&ts,&mut at){if ts.get(at).is_some_and(|t|t.text=="("){let mut finish=at+1;let mut depth=1;while finish<ts.len(){if ts[finish].symbol&&ts[finish].text=="("{depth+=1;}if ts[finish].symbol&&ts[finish].text==")"{depth-=1;if depth==0{break;}}finish+=1;}if let Some(parts)=pieces(&ts[at+1..finish]){for part in parts{if constraint_start(part,0)&&constraint(part,0,&table,start).is_none(){issues.push((start+part[0].at,"SQL constraint declaration is incomplete or unsupported; constraint facts were withheld.".into()));}}}}}}
  // A constraint declaration is not considered modeled merely because the
  // column parser can safely ignore it.
  if is(&ts,0,"ALTER"){let mut at=2;if is(&ts,at,"ONLY"){at+=1;}let table=identifier(&ts,&mut at).map(|v|v.0).unwrap_or_default();if let Some(parts)=pieces(&ts[at..]){for part in parts{if is(part,0,"ADD")&&constraint_start(part,1)&&constraint(part,1,&table,start).is_none(){issues.push((start+part[0].at,"SQL constraint declaration is incomplete or unsupported; constraint facts were withheld.".into()));}}}}
 }if let Some(issue)=lexical_issue(text){let span=slices(text).into_iter().find(|(a,b)|*a<=issue.0&&issue.0<*b);if !span.is_some_and(|(a,b)|issues.iter().any(|(offset,_)|a<=*offset&&*offset<b)){issues.push(issue);}}issues.truncate(128);issues
}

#[cfg(test)]fn parse(text:&str)->Vec<Column>{let mut columns=Vec::<Column>::new();for statement in statements(text){if statement.if_absent.as_ref().is_some_and(|t|columns.iter().any(|c|&c.table==t)){continue;}for event in statement.events{match event{Event::Create(table)|Event::DropTable(table)=>columns.retain(|c|c.table!=table),Event::Add(c)=>{if !columns.iter().any(|old|old.table==c.table&&old.name==c.name){columns.push(c)}},Event::Change(c)=>{if let Some(old)=columns.iter_mut().find(|old|old.table==c.table&&old.name==c.name){*old=c;}},Event::DropColumn(table,name)=>columns.retain(|c|c.table!=table||c.name!=name),Event::RenameTable(from,to)=>for c in &mut columns{if c.table==from{c.table=to.clone();}},Event::RenameColumn{table,from,to,offset}=>for c in &mut columns{if c.table==table&&c.name==from{c.name=to.clone();c.offset=offset;}}}}}columns}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dialects_constraints_defaults_and_qualified_names() {
        let sql = "-- CREATE TABLE fake (x text);\nCREATE TABLE `posts` (`id` bigint PRIMARY KEY, `title` varchar(255) DEFAULT 'a,b)', CONSTRAINT `posts_unique` UNIQUE (`title`)); CREATE TABLE public.\"users\" (\"email\" TEXT, [active] INTEGER);";
        let columns = parse(sql);
        assert_eq!(
            columns
                .iter()
                .map(|c| (c.table.as_str(), c.name.as_str()))
                .collect::<Vec<_>>(),
            [
                ("posts", "id"),
                ("posts", "title"),
                ("public.users", "email"),
                ("public.users", "active")
            ]
        );
        for c in columns {
            assert_eq!(&sql[c.offset..c.offset + c.name.len()], c.name);
        }
    }
    #[test]
    fn strings_and_incomplete_statements_are_not_definitions() {
        assert!(
            parse("SELECT 'CREATE TABLE fake (x text)'; CREATE TABLE broken (x text").is_empty()
        );
    }
    #[test]fn ordered_alter_rename_drop_and_mysql_change_keep_owned_offsets(){
        let sql="CREATE TABLE posts (id bigint, title varchar(255), obsolete text); ALTER TABLE posts RENAME COLUMN title TO headline; ALTER TABLE posts DROP COLUMN obsolete, ADD COLUMN published boolean; ALTER TABLE posts CHANGE COLUMN headline summary text; ALTER TABLE posts RENAME TO entries;";
        let columns=parse(sql);assert_eq!(columns.iter().map(|c|(c.table.as_str(),c.name.as_str(),c.kind.as_str())).collect::<Vec<_>>(),[("entries","id","SQL bigint"),("entries","summary","SQL text"),("entries","published","SQL boolean")]);
        let summary=columns.iter().find(|c|c.name=="summary").unwrap();assert_eq!(&sql[summary.offset..summary.offset+7],"summary");
    }
    #[test]fn opaque_dollar_bodies_nested_comments_and_punctuation_strings_never_make_ddl(){
        let sql="DO $$ BEGIN PERFORM ';'; CREATE TABLE fake (id integer); END $$; /* outer /* inner */ CREATE TABLE comment_fake (x int); */ CREATE TABLE real (value text DEFAULT ';', close text DEFAULT ')');";
        let columns=parse(sql);assert_eq!(columns.iter().map(|c|c.table.as_str()).collect::<Vec<_>>(),["real","real"]);
    }
    #[test]fn guarded_create_and_sqlserver_batches_preserve_existing_columns(){
        let sql="CREATE TABLE [items] ([id] bigint);\nGO\nCREATE TABLE IF NOT EXISTS items (wrong text);\nALTER TABLE items ADD [name] nvarchar(255);\nGO\nALTER TABLE items ALTER COLUMN name nvarchar(512) NOT NULL; DROP TABLE IF EXISTS gone;";
        let columns=parse(sql);assert_eq!(columns.iter().map(|c|c.name.as_str()).collect::<Vec<_>>(),["id","name"]);
    }
    #[test]fn unsupported_or_unbalanced_alter_is_withheld_as_one_statement(){
        let sql="CREATE TABLE users (id integer); ALTER TABLE users ADD safe text, DYNAMIC BAD; ALTER TABLE users ADD broken varchar(255;";
        assert_eq!(parse(sql).iter().map(|c|c.name.as_str()).collect::<Vec<_>>(),["id"]);
    }

    #[test]fn sqlserver_column_batches_and_constraint_evolution_are_ordered(){
        let sql="CREATE TABLE owners (id bigint PRIMARY KEY); CREATE TABLE posts (id bigint, owner_id bigint, title text, CONSTRAINT pk_posts PRIMARY KEY(id), CONSTRAINT fk_owner FOREIGN KEY(owner_id) REFERENCES owners(id), UNIQUE(title)); ALTER TABLE posts ADD enabled bit, rank integer; ALTER TABLE posts ADD CONSTRAINT uq_title UNIQUE(title); ALTER TABLE posts RENAME CONSTRAINT uq_title TO uq_headline; ALTER TABLE posts RENAME COLUMN title TO headline; ALTER TABLE posts DROP CONSTRAINT fk_owner; ALTER TABLE posts RENAME TO entries;";
        let columns=parse(sql);assert!(columns.iter().any(|c|c.table=="entries"&&c.name=="enabled"));assert!(columns.iter().any(|c|c.table=="entries"&&c.name=="rank"));let facts=constraints(sql);assert!(facts.iter().any(|c|c["table"]=="entries"&&c["name"]=="uq_headline"&&c["columns"]==serde_json::json!(["headline"])));assert!(!facts.iter().any(|c|c["name"]=="fk_owner"));assert!(issues(sql).is_empty(),"{:?}",issues(sql));
    }
    #[test]fn check_index_and_foreign_metadata_never_export_expression_or_row_data(){
        let sql="CREATE TABLE posts (id bigint, owner_id bigint, CONSTRAINT checked CHECK (id > 0), CONSTRAINT owner_fk FOREIGN KEY(owner_id) REFERENCES owners(id) ON DELETE CASCADE); CREATE UNIQUE INDEX uq_posts ON posts(id);";let facts=constraints(sql);assert!(facts.iter().any(|c|c["kind"]=="check"&&c["name"]=="checked"));assert!(facts.iter().any(|c|c["referenceTable"]=="owners"&&c["referenceColumns"]==serde_json::json!(["id"])));assert!(facts.iter().any(|c|c["kind"]=="unique"&&c["name"]=="uq_posts"));assert!(!serde_json::json!(facts).to_string().contains("id > 0"));
    }
    #[test]fn incomplete_and_opaque_ddl_has_anchored_diagnostics_but_dump_data_does_not(){
        let sql="INSERT INTO posts VALUES (1); ALTER TABLE posts ADD valid text, DYNAMIC BAD; DO $$ BEGIN EXECUTE 'CREATE TABLE dynamic (id int)'; END $$; CREATE TABLE broken (id int";let reported=issues(sql);assert_eq!(reported.len(),3,"{reported:?}");assert!(reported.iter().all(|(at,_)|*at<sql.len()));assert!(reported.iter().all(|(_,text)|text.contains("withheld")));assert!(issues("INSERT INTO posts VALUES (1); SET FOREIGN_KEY_CHECKS=0;").is_empty());
    }

    #[test]fn schema_qualified_tables_never_conflate_and_guarded_create_does_not_add_fake_constraints(){let sql="CREATE TABLE public.users(id bigint PRIMARY KEY); CREATE TABLE tenant.users(id text); CREATE TABLE blank(id int); CREATE TABLE IF NOT EXISTS blank(id int, UNIQUE(id));";let values=parse(sql);assert!(values.iter().any(|c|c.table=="public.users"&&c.kind=="SQL bigint"));assert!(values.iter().any(|c|c.table=="tenant.users"&&c.kind=="SQL text"));let facts=constraints(sql);assert!(!facts.iter().any(|c|c["table"]=="blank"));assert_eq!(facts[0]["table"],"public.users");}
    #[test]fn driver_constraint_variants_and_incomplete_suffixes_are_explicit(){
      let sql="CREATE TABLE [items] ([id] int, [active] bit, CONSTRAINT [pk_items] PRIMARY KEY CLUSTERED ([id]) WITH (IGNORE_DUP_KEY = OFF) ON [PRIMARY]); ALTER TABLE items ADD CONSTRAINT df_active DEFAULT (1) FOR active; ALTER TABLE items ADD FULLTEXT INDEX text_search(id); ALTER TABLE items DROP CONSTRAINT df_active;";let facts=constraints(sql);assert!(facts.iter().any(|c|c["name"]=="pk_items"&&c["kind"]=="primary"));assert!(facts.iter().any(|c|c["name"]=="text_search"&&c["kind"]=="index"));assert!(!facts.iter().any(|c|c["name"]=="df_active"));assert!(issues(sql).is_empty(),"{:?}",issues(sql));
      assert!(!issues("ALTER TABLE items DROP CONSTRAINT;").is_empty());assert!(!issues("ALTER TABLE items ADD CONSTRAINT bad UNIQUE(id) GARBAGE;").is_empty());assert!(!issues("CREATE INDEX expr_index ON users(lower(email));").is_empty());assert!(!issues("CREATE TABLE good(id int); /* missing end").is_empty());
    }
    #[test]
    #[ignore="requires Python stdlib sqlite3; executes authored DDL in an in-memory SQLite database only"]
    fn real_sqlite_ddl_matches_ordered_column_and_foreign_constraint_inventory(){
      let sql="CREATE TABLE owners(id INTEGER PRIMARY KEY); CREATE TABLE posts(id INTEGER PRIMARY KEY, owner_id INTEGER REFERENCES owners(id), title TEXT, CONSTRAINT title_unique UNIQUE(title), CHECK(length(title)>0)); ALTER TABLE posts ADD enabled INTEGER; ALTER TABLE posts RENAME COLUMN title TO headline; ALTER TABLE posts DROP COLUMN enabled; ALTER TABLE posts RENAME TO entries;";
      let output=std::process::Command::new("python3").args(["-c","import sqlite3,json,sys; db=sqlite3.connect(':memory:'); db.executescript(sys.argv[1]); print(json.dumps({'columns':[r[1] for r in db.execute('PRAGMA table_info(entries)')],'foreignTables':[r[2] for r in db.execute('PRAGMA foreign_key_list(entries)')]}))",sql]).output().unwrap();assert!(output.status.success(),"{}",String::from_utf8_lossy(&output.stderr));let actual:serde_json::Value=serde_json::from_slice(&output.stdout).unwrap();let expected:Vec<_>=parse(sql).into_iter().filter(|c|c.table=="entries").map(|c|c.name).collect();assert_eq!(actual["columns"],serde_json::json!(expected));assert_eq!(actual["foreignTables"],serde_json::json!(["owners"]));let facts=constraints(sql);assert!(facts.iter().any(|c|c["table"]=="entries"&&c["kind"]=="foreign"&&c["referenceTable"]=="owners"));assert!(facts.iter().any(|c|c["table"]=="entries"&&c["kind"]=="unique"&&c["columns"]==serde_json::json!(["headline"])));assert!(issues(sql).is_empty(),"{:?}",issues(sql));
    }

}
