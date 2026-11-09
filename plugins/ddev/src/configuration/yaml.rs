//! Source-mapped YAML 1.2, including aliases and merge keys. Bounds apply to
//! expanded aliases too, so small documents cannot expand without limit.
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use yaml_rust2::{
    Yaml,
    parser::{Event, Parser},
    scanner::{Marker, TScalarStyle},
};

pub const MAX_BYTES: usize = 256 * 1024;
const MAX_NODES: usize = 20_000;
const MAX_DEPTH: usize = 64;

#[derive(Clone, Debug)]
pub struct Node {
    pub value: Value,
    pub start: usize,
    pub end: usize,
    pub fields: BTreeMap<String, Node>,
    pub keys: BTreeMap<String, (usize, usize)>,
    pub items: Vec<Node>,
    weight: usize,
}
impl Node {
    fn scalar(value: Value, start: usize, end: usize) -> Self {
        Self {
            value,
            start,
            end,
            fields: BTreeMap::new(),
            keys: BTreeMap::new(),
            items: vec![],
            weight: 1,
        }
    }
    pub fn at(&self, path: &[String]) -> Option<&Node> {
        let Some((head, tail)) = path.split_first() else {
            return Some(self);
        };
        let child = if self.value.is_array() {
            self.items.get(head.parse::<usize>().ok()?)
        } else {
            self.fields.get(head)
        }?;
        child.at(tail)
    }
}
#[derive(Debug)]
pub struct Issue {
    pub start: usize,
    pub end: usize,
    pub message: String,
}
pub struct Tree {
    pub root: Node,
    pub duplicates: Vec<Issue>,
}
struct Reader<'a> {
    parser: Parser<std::str::Chars<'a>>,
    text: &'a str,
    byte_offsets: Vec<usize>,
    anchors: BTreeMap<usize, Node>,
    remaining: usize,
    duplicates: Vec<Issue>,
}
impl Reader<'_> {
    // yaml-rust2 scanner indices count Unicode scalar values, while editor
    // offsets are UTF-8 bytes and protocol positions are UTF-16.
    fn byte(&self, index: usize) -> usize {
        self.byte_offsets
            .get(index)
            .copied()
            .unwrap_or(self.text.len())
    }
    fn next(&mut self) -> Result<(Event, Marker), Issue> {
        self.parser.next_token().map_err(|e| Issue {
            start: self.byte(e.marker().index()),
            end: self.byte(e.marker().index()),
            message: e.info().into(),
        })
    }
    fn node(&mut self, event: Event, mark: Marker, depth: usize) -> Result<Node, Issue> {
        let start = self.byte(mark.index()).min(self.text.len());
        if depth > MAX_DEPTH || self.remaining == 0 {
            return Err(Issue {
                start,
                end: start,
                message: "YAML exceeds the configuration complexity limit.".into(),
            });
        }
        self.remaining -= 1;
        let (mut node, anchor) = match event {
            Event::Scalar(raw, style, anchor, tag) => {
                let explicit = tag
                    .as_ref()
                    .filter(|tag| tag.handle == "tag:yaml.org,2002:")
                    .map(|tag| tag.suffix.as_str());
                let value = match explicit {
                    Some("str") => Value::String(raw),
                    Some("int" | "float" | "bool" | "null") => {
                        let parsed = match explicit.unwrap() {
                            "int" => raw.parse::<i64>().ok().map(Value::from),
                            "float" => raw
                                .parse::<f64>()
                                .ok()
                                .and_then(serde_json::Number::from_f64)
                                .map(Value::Number),
                            "bool" => match raw.as_str() {
                                "true" | "True" | "TRUE" => Some(Value::Bool(true)),
                                "false" | "False" | "FALSE" => Some(Value::Bool(false)),
                                _ => None,
                            },
                            "null" => matches!(raw.as_str(), "" | "~" | "null" | "Null" | "NULL")
                                .then_some(Value::Null),
                            _ => unreachable!(),
                        };
                        parsed.ok_or_else(|| Issue {
                            start,
                            end: start,
                            message: "Invalid explicitly tagged YAML scalar.".into(),
                        })?
                    }
                    _ if style != TScalarStyle::Plain => Value::String(raw),
                    _ => match Yaml::from_str(&raw) {
                        Yaml::Null => Value::Null,
                        Yaml::Boolean(b) => Value::Bool(b),
                        Yaml::Integer(i) => Value::from(i),
                        Yaml::Real(s) => s
                            .parse::<f64>()
                            .ok()
                            .and_then(serde_json::Number::from_f64)
                            .map(Value::Number)
                            .unwrap_or(Value::String(raw)),
                        _ => Value::String(raw),
                    },
                };
                let next_index = self
                    .parser
                    .peek()
                    .map(|(_, m)| m.index())
                    .unwrap_or(self.byte_offsets.len());
                let end = scalar_end(self.text, start, style, self.byte(next_index));
                (Node::scalar(value, start, end), anchor)
            }
            Event::Alias(id) => {
                let mut alias = self.anchors.get(&id).cloned().ok_or_else(|| Issue {
                    start,
                    end: start,
                    message: "Unresolved or recursive YAML alias.".into(),
                })?;
                if alias.weight > self.remaining {
                    return Err(Issue {
                        start,
                        end: start,
                        message: "YAML alias expansion exceeds the configuration limit.".into(),
                    });
                }
                self.remaining -= alias.weight;
                alias.start = start;
                alias.end = self.text[start..]
                    .find(|c: char| c.is_whitespace() || ",]}".contains(c))
                    .map_or(self.text.len(), |n| start + n);
                return Ok(alias);
            }
            Event::SequenceStart(anchor, _) => {
                let mut node = Node::scalar(Value::Array(vec![]), start, start);
                loop {
                    let (event, mark) = self.next()?;
                    if event == Event::SequenceEnd {
                        node.end = self.byte(mark.index()).min(self.text.len());
                        break;
                    }
                    let child = self.node(event, mark, depth + 1)?;
                    node.weight += child.weight;
                    node.items.push(child);
                }
                node.value = Value::Array(node.items.iter().map(|n| n.value.clone()).collect());
                (node, anchor)
            }
            Event::MappingStart(anchor, _) => {
                let mut node = Node::scalar(Value::Object(Map::new()), start, start);
                let mut pairs = vec![];
                loop {
                    let (event, mark) = self.next()?;
                    if event == Event::MappingEnd {
                        node.end = self.byte(mark.index()).min(self.text.len());
                        break;
                    }
                    let Event::Scalar(key, style, _, _) = event else {
                        return Err(Issue {
                            start: self.byte(mark.index()),
                            end: self.byte(mark.index()),
                            message: "DDEV configuration requires scalar mapping keys.".into(),
                        });
                    };
                    let key_start = self.byte(mark.index());
                    let key_end = scalar_end(self.text, key_start, style, key_start + key.len());
                    let (event, mark) = self.next()?;
                    let child = self.node(event, mark, depth + 1)?;
                    node.weight += child.weight;
                    pairs.push((key, key_start, key_end, child));
                }
                // YAML merge sequences prefer the first mapping; explicit keys
                // always override inherited ones, independent of source order.
                for (key, _, _, merge) in &pairs {
                    if key != "<<" {
                        continue;
                    }
                    let sources: Vec<&Node> = if merge.value.is_object() {
                        vec![merge]
                    } else {
                        merge.items.iter().collect()
                    };
                    if sources.is_empty() || sources.iter().any(|s| !s.value.is_object()) {
                        return Err(Issue {
                            start: merge.start,
                            end: merge.end,
                            message: "A YAML merge must name a mapping or a list of mappings."
                                .into(),
                        });
                    }
                    for source in sources {
                        for (k, v) in &source.fields {
                            node.fields.entry(k.clone()).or_insert_with(|| v.clone());
                            if let Some(span) = source.keys.get(k) {
                                node.keys.entry(k.clone()).or_insert(*span);
                            }
                        }
                    }
                }
                let mut explicit = BTreeMap::new();
                for (key, start, end, child) in pairs {
                    if key == "<<" {
                        continue;
                    }
                    if explicit.insert(key.clone(), ()).is_some() {
                        self.duplicates.push(Issue {
                            start,
                            end,
                            message: format!("Duplicate configuration key '{key}'."),
                        });
                    }
                    node.keys.insert(key.clone(), (start, end));
                    node.fields.insert(key, child);
                }
                node.value = Value::Object(
                    node.fields
                        .iter()
                        .map(|(k, v)| (k.clone(), v.value.clone()))
                        .collect(),
                );
                (node, anchor)
            }
            _ => {
                return Err(Issue {
                    start,
                    end: start,
                    message: "Expected a YAML configuration value.".into(),
                });
            }
        };
        node.end = node.end.max(node.start).min(self.text.len());
        if anchor != 0 {
            self.anchors.insert(anchor, node.clone());
        }
        Ok(node)
    }
}

pub fn parse(text: &str) -> Result<Tree, Issue> {
    if text.len() > MAX_BYTES {
        return Err(Issue {
            start: 0,
            end: 0,
            message: "Configuration exceeds 256 KiB; assistance is limited to smaller files."
                .into(),
        });
    }
    let mut r = Reader {
        parser: Parser::new_from_str(text),
        text,
        byte_offsets: text
            .char_indices()
            .map(|(i, _)| i)
            .chain(std::iter::once(text.len()))
            .collect(),
        anchors: BTreeMap::new(),
        remaining: MAX_NODES,
        duplicates: vec![],
    };
    let (event, _) = r.next()?;
    debug_assert_eq!(event, Event::StreamStart);
    let (event, mark) = r.next()?;
    if event == Event::StreamEnd {
        return Ok(Tree {
            root: Node::scalar(Value::Object(Map::new()), 0, 0),
            duplicates: vec![],
        });
    }
    if event != Event::DocumentStart {
        return Err(Issue {
            start: r.byte(mark.index()),
            end: r.byte(mark.index()),
            message: "Expected a YAML document.".into(),
        });
    }
    let (event, mark) = r.next()?;
    let root = r.node(event, mark, 0)?;
    let _ = r.next()?;
    let (event, mark) = r.next()?;
    if event != Event::StreamEnd {
        return Err(Issue {
            start: r.byte(mark.index()),
            end: r.byte(mark.index()),
            message: "DDEV configuration must contain one YAML document.".into(),
        });
    }
    Ok(Tree {
        root,
        duplicates: r.duplicates,
    })
}

pub fn scalar_end(text: &str, start: usize, style: TScalarStyle, next: usize) -> usize {
    if start >= text.len() {
        return text.len();
    }
    if matches!(style, TScalarStyle::Literal | TScalarStyle::Folded) {
        return next.min(text.len());
    }
    let quote = match style {
        TScalarStyle::SingleQuoted => Some('\''),
        TScalarStyle::DoubleQuoted => Some('"'),
        _ => None,
    };
    if let Some(quote) = quote {
        let mut chars = text[start..].char_indices().peekable();
        let _ = chars.next();
        while let Some((i, c)) = chars.next() {
            if c == '\\' && quote == '"' {
                let _ = chars.next();
                continue;
            }
            if c == quote {
                if quote == '\'' && chars.peek().is_some_and(|(_, c)| *c == '\'') {
                    let _ = chars.next();
                    continue;
                }
                return start + i + c.len_utf8();
            }
        }
        return text.len();
    }
    let mut previous = ' ';
    for (i, c) in text[start..].char_indices() {
        if c == '\n'
            || c == '\r'
            || ",]}".contains(c)
            || c == ':'
                && text[start + i + 1..]
                    .chars()
                    .next()
                    .is_none_or(|c| c.is_whitespace())
            || c == '#' && previous.is_whitespace()
        {
            return start + text[start..start + i].trim_end().len();
        }
        previous = c;
    }
    text.len()
}
