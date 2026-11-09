//! Navigation relationships are not rename anchors. Keep transformed names,
//! declaration-wide ranges and filename targets in a plugin-owned graph.
use openidea_plugin::indexing::Edges;
use openidea_plugin::providers::LineMarker;
use openidea_plugin::{Location, Position, Range};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::time::{Duration, Instant};

const MAX_ENTRIES: usize = 32_768;
const MAX_ALIASES: usize = 4096;
const MAX_TARGETS: usize = 256;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Graph {
    pub definitions: Vec<(String, Range)>,
    pub references: Vec<(String, Range)>,
    pub markers: Vec<(String, Range)>,
    pub aliases: Vec<(String, String)>,
}
impl From<Edges> for Graph {
    fn from(edges: Edges) -> Self {
        let pairs = |anchors: Vec<openidea_plugin::indexing::Anchor>| anchors.into_iter().map(|a| (a.key, a.range)).collect();
        Self {
            definitions: pairs(edges.definitions),
            references: pairs(edges.references),
            markers: pairs(edges.markers),
            aliases: edges.aliases.into_iter().map(|a| (a.prefix, a.target)).collect(),
        }
    }
}

/// Build only from one project's cached files. This resolver never returns edits
/// or publishes data to the host's Find Usages/Rename graph.
pub struct Resolver {
    definitions: BTreeMap<String, Vec<Location>>,
    aliases: Vec<(String, String)>,
}
impl Resolver {
    pub fn new<'a>(files: impl IntoIterator<Item = (&'a str, &'a Graph)>) -> Self {
        let mut result = Self { definitions: BTreeMap::new(), aliases: Vec::new() };
        let mut entries = 0;
        for (path, graph) in files {
            for (key, range) in &graph.definitions {
                if entries == MAX_ENTRIES {
                    break;
                }
                result.definitions.entry(key.clone()).or_default().push(Location { path: path.into(), range: *range });
                entries += 1;
            }
            result.aliases.extend(graph.aliases.iter().take(MAX_ALIASES - result.aliases.len()).cloned());
        }
        result
    }

    fn candidates(&self, key: &str) -> Vec<(String, Location)> {
        let mut queue = VecDeque::from([(key.to_owned(), 0)]);
        let mut visited = BTreeSet::new();
        let mut locations = BTreeSet::new();
        let mut result = Vec::new();
        while let Some((key, depth)) = queue.pop_front() {
            if visited.len() >= MAX_TARGETS || !visited.insert(key.clone()) {
                continue;
            }
            for location in self.definitions.get(&key).into_iter().flatten() {
                if result.len() == MAX_TARGETS {
                    return result;
                }
                if locations.insert((key.clone(), location.path.clone(), location.range.start, location.range.end)) {
                    result.push((key.clone(), location.clone()));
                }
            }
            if depth == 16 {
                continue;
            }
            for (prefix, target) in &self.aliases {
                if let Some(rest) = key.strip_prefix(prefix) {
                    if queue.len() < MAX_TARGETS {
                        queue.push_back((format!("{target}{rest}"), depth + 1));
                    }
                }
            }
        }
        result
    }

    pub fn targets(&self, key: &str) -> Vec<Location> {
        let mut seen = BTreeSet::new();
        self.candidates(key).into_iter().map(|(_, l)| l).filter(|l| seen.insert((l.path.clone(), l.range.start, l.range.end))).collect()
    }

    /// Refresh only candidate target files. Project reads include unsaved editor
    /// text; a shared budget/cache bounds work across the entire provider call.
    pub fn checked<F: FnMut(&str) -> Option<Graph>>(&self, read: F) -> Checked<'_, F> {
        Checked { resolver: self, read, files: BTreeMap::new(), deadline: Instant::now() + Duration::from_millis(10) }
    }

    pub fn definition(&self, source: &Graph, at: Position) -> Vec<Location> {
        let mut result = Vec::new();
        for (key, range) in &source.references {
            if range.start <= at && at <= range.end {
                result.extend(self.targets(key));
            }
        }
        result.truncate(MAX_TARGETS);
        result
    }

    pub fn markers(&self, source: &Graph) -> Vec<LineMarker> {
        let mut seen = BTreeSet::new();
        let mut result = Vec::new();
        for (key, range) in &source.markers {
            for target in self.targets(key) {
                if result.len() == MAX_TARGETS {
                    return result;
                }
                if seen.insert((range.start.line, target.path.clone(), target.range.start, target.range.end)) {
                    result.push(LineMarker {
                        line: range.start.line,
                        icon: Some("navigate".into()),
                        tooltip: format!("Related {key}"),
                        target: Some(target),
                    });
                }
            }
        }
        result
    }
}

pub struct Checked<'a, F> {
    resolver: &'a Resolver,
    read: F,
    files: BTreeMap<String, Option<Graph>>,
    deadline: Instant,
}
impl<F: FnMut(&str) -> Option<Graph>> Checked<'_, F> {
    pub fn targets(&mut self, key: &str) -> Vec<Location> {
        let mut seen = BTreeSet::new();
        let mut result = Vec::new();
        for (resolved_key, old) in self.resolver.candidates(key) {
            if !self.files.contains_key(&old.path) {
                if Instant::now() >= self.deadline {
                    continue;
                }
                self.files.insert(old.path.clone(), (self.read)(&old.path));
            }
            if let Some(Some(graph)) = self.files.get(&old.path) {
                for (key, range) in &graph.definitions {
                    if key == &resolved_key && seen.insert((old.path.clone(), range.start, range.end)) {
                        result.push(Location { path: old.path.clone(), range: *range });
                        if result.len() == MAX_TARGETS {
                            return result;
                        }
                    }
                }
            }
        }
        result
    }

    pub fn definition(&mut self, source: &Graph, at: Position) -> Vec<Location> {
        let mut result = Vec::new();
        for (key, range) in &source.references {
            if range.start <= at && at <= range.end {
                result.extend(self.targets(key));
            }
        }
        result.truncate(MAX_TARGETS);
        result
    }

    pub fn markers(&mut self, source: &Graph) -> Vec<LineMarker> {
        let mut result = Vec::new();
        let mut seen = BTreeSet::new();
        for (key, range) in &source.markers {
            for target in self.targets(key) {
                if seen.insert((range.start.line, target.path.clone(), target.range.start, target.range.end)) {
                    result.push(LineMarker {
                        line: range.start.line,
                        icon: Some("navigate".into()),
                        tooltip: format!("Related {key}"),
                        target: Some(target),
                    });
                    if result.len() == MAX_TARGETS {
                        return result;
                    }
                }
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn graph(key: &str) -> Graph {
        Graph { definitions: vec![(key.into(), Range::new(Position::new(1, 0), Position::new(1, 11)))], ..Graph::default() }
    }
    #[test]
    fn transformed_relationships_and_empty_ranges_are_navigation_only() {
        let factory = graph("factory#Post");
        let resource = Graph { definitions: vec![("resource#app.xml".into(), Range::default())], ..Graph::default() };
        let resolver = Resolver::new([("/p/PostFactory.php", &factory), ("/p/app.xml", &resource)]);
        assert_eq!(resolver.targets("factory#Post")[0].path, "/p/PostFactory.php");
        assert_eq!(resolver.targets("resource#app.xml")[0].range, Range::default());
        assert!(Resolver::new(std::iter::empty()).targets("factory#Post").is_empty());
    }
    #[test]
    fn alias_cycles_and_duplicate_targets_are_bounded() {
        let mut g = graph("type:Child#bean");
        g.aliases = vec![("type:Parent#".into(), "type:Child#".into()), ("type:Child#".into(), "type:Parent#".into())];
        let resolver = Resolver::new([("/p/Child.java", &g)]);
        assert_eq!(resolver.targets("type:Parent#bean").len(), 1);
        let source = Graph {
            markers: vec![("type:Parent#bean".into(), Range::default()), ("type:Child#bean".into(), Range::default())],
            ..Graph::default()
        };
        assert_eq!(resolver.markers(&source).len(), 1);
    }
    #[test]
    fn candidate_targets_are_revalidated_and_moved_ranges_are_refreshed() {
        let old = graph("factory#Post");
        let resolver = Resolver::new([("/p/PostFactory.php", &old)]);
        let moved =
            Graph { definitions: vec![("factory#Post".into(), Range::new(Position::new(7, 0), Position::new(7, 11)))], ..Graph::default() };
        let mut reads = 0;
        let mut checked = resolver.checked(|_| {
            reads += 1;
            Some(moved.clone())
        });
        assert_eq!(checked.targets("factory#Post")[0].range.start.line, 7);
        assert_eq!(checked.targets("factory#Post")[0].range.start.line, 7);
        drop(checked);
        assert_eq!(reads, 1);
        assert!(resolver.checked(|_| Some(Graph::default())).targets("factory#Post").is_empty());
        assert!(resolver.checked(|_| None).targets("factory#Post").is_empty());
    }
    #[test]
    fn ambiguous_modules_return_all_choices_instead_of_picking_one() {
        let g = graph("event:Ready#listener");
        let resolver = Resolver::new([("/p/a/L.java", &g), ("/p/b/L.java", &g)]);
        assert_eq!(resolver.checked(|_| Some(g.clone())).targets("event:Ready#listener").len(), 2);
    }
}
