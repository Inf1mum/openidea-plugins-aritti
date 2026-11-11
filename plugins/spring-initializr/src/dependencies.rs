//! The Dependencies section: the server's catalogue as a tree by category,
//! with the chosen ids kept in the form's `dependencies` field.

use crate::{Launcher, START};
use openidea_plugin::views::{Body, Button, TreeItem, View, ViewEvent};
use openidea_plugin::{Error, host};
use serde_json::Value;
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

const SELECTED: &str = "selected";
const CATEGORY_PREFIX: &str = "cat:";
const DEPENDENCY_PREFIX: &str = "dep:";
const CHOSEN_PREFIX: &str = "sel:";

pub struct Dependency {
    pub id: String,
    pub name: String,
    pub description: String,
    pub range: Option<String>,
}

pub struct Category {
    pub name: String,
    pub dependencies: Vec<Dependency>,
}

pub struct Catalogue {
    pub default_boot: String,
    pub categories: Vec<Category>,
}

impl Catalogue {
    pub fn from_metadata(metadata: &Value) -> Catalogue {
        let text = |value: &Value| value.as_str().unwrap_or_default().to_owned();
        let categories = metadata["dependencies"]["values"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|group| Category {
                name: text(&group["name"]),
                dependencies: group["values"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|d| {
                        let id = d["id"].as_str()?;
                        Some(Dependency {
                            id: id.to_owned(),
                            name: d["name"].as_str().unwrap_or(id).to_owned(),
                            description: text(&d["description"]),
                            range: crate::compatibility::declared_range(d).map(str::to_owned),
                        })
                    })
                    .collect(),
            })
            .collect();

        Catalogue { default_boot: text(&metadata["bootVersion"]["default"]), categories }
    }

    fn find(&self, id: &str) -> Option<&Dependency> {
        self.categories.iter().flat_map(|c| &c.dependencies).find(|d| d.id == id)
    }
}

#[derive(Default)]
pub struct State {
    catalogue: Option<Result<Catalogue, String>>,
    search: Option<String>,
    expansion: BTreeMap<String, bool>,
}

fn version_parts(version: &str) -> Option<(Vec<u64>, (u8, u64))> {
    let (numbers, qualifier) = version.split_once('-').unwrap_or((version, ""));
    if !numbers.split('.').all(|p|!p.is_empty()&&p.bytes().all(|b|b.is_ascii_digit())) {return None;}
    let parts:Vec<u64>=numbers.split('.').map(str::parse).collect::<Result<_,_>>().ok()?;
    let (rank,number)=if qualifier.is_empty() {(3,0)}else if qualifier=="SNAPSHOT" {(2,0)}else{
        let (rank,n)=if let Some(n)=qualifier.strip_prefix("RC") {(1,n)}else if let Some(n)=qualifier.strip_prefix('M') {(0,n)}else{return None;};
        if n.is_empty()||!n.bytes().all(|b|b.is_ascii_digit()) {return None;}(rank,n.parse::<u64>().ok()?)
    };

    Some((parts, (rank, number)))
}

pub fn compare_versions(a: &str, b: &str) -> Option<Ordering> {
    let (mut a_parts, a_qualifier) = version_parts(a)?;
    let (mut b_parts, b_qualifier) = version_parts(b)?;
    let length = a_parts.len().max(b_parts.len());
    a_parts.resize(length, 0);
    b_parts.resize(length, 0);

    Some(a_parts.cmp(&b_parts).then(a_qualifier.cmp(&b_qualifier)))
}

/// Whether `version` is inside `range`; true when either cannot be parsed.
pub fn in_range(version: &str, range: &str) -> bool {
    let range = range.trim();
    let bounded = range.starts_with(['[', '(']) && range.ends_with([']', ')']);
    let (low, low_inclusive, high, high_inclusive) = if bounded {
        let Some((low, high)) = range[1..range.len() - 1].split_once(',') else { return true };
        (low.trim(), range.starts_with('['), high.trim(), range.ends_with(']'))
    } else {
        (range, true, "", false)
    };

    let above_low = low.is_empty()
        || match compare_versions(version, low) {
            Some(Ordering::Greater) => true,
            Some(Ordering::Equal) => low_inclusive,
            Some(Ordering::Less) => false,
            None => true,
        };
    let below_high = high.is_empty()
        || match compare_versions(version, high) {
            Some(Ordering::Less) => true,
            Some(Ordering::Equal) => high_inclusive,
            Some(Ordering::Greater) => false,
            None => true,
        };

    above_low && below_high
}

fn split_ids(value: &str) -> Vec<String> {
    value.split([',', ' ']).filter(|d| !d.is_empty()).map(str::to_owned).collect()
}

impl Launcher {
    fn chosen_ids(&mut self) -> Vec<String> {
        split_ids(&self.get("dependencies"))
    }

    fn toggle_dependency(&mut self, id: &str) {
        let mut ids = self.chosen_ids();
        match ids.iter().position(|chosen| chosen == id) {
            Some(at) => {
                ids.remove(at);
            }
            None => ids.push(id.to_owned()),
        }
        self.set("dependencies", &ids.join(","));
    }

    pub(crate) fn dependencies_badge(&mut self) -> String {
        format!("({})", self.chosen_ids().len())
    }

    pub(crate) fn ensure_catalogue(&mut self) {
        if self.dependencies.catalogue.is_some() {
            return;
        }

        self.load_metadata(false);
        self.dependencies.catalogue = Some(match &self.metadata {
            Some(metadata) => Ok(Catalogue::from_metadata(metadata)),
            None => Err(self.catalog_status.clone().unwrap_or_else(|| "No saved catalogue. Refresh to load the server choices.".into())),
        });
    }

    fn catalogue_message(&mut self) -> Option<String> {
        if self.get("server").trim_end_matches('/') != START {
            return Some(format!("The dependency list is only available for {START}. Type the ids in the Project section."));
        }

        self.ensure_catalogue();
        match &self.dependencies.catalogue {
            Some(Err(message)) => Some(message.clone()),
            _ => None,
        }
    }

    pub(crate) fn dependencies_tree(&mut self) -> (View, String) {
        let chosen = self.chosen_ids();
        let message = self.catalogue_message();
        let boot = Some(self.get("bootVersion")).filter(|v| !v.is_empty());
        let catalogue = match (&message, &self.dependencies.catalogue) {
            (None, Some(Ok(catalogue))) => Some(catalogue),
            _ => None,
        };
        let boot = boot.or_else(|| catalogue.map(|c| c.default_boot.clone()).filter(|v| !v.is_empty()));
        let search = self.dependencies.search.clone();
        let needle = search.as_ref().map(|q| q.to_lowercase());
        let compatible = |d: &Dependency| match (&boot, &d.range) {
            (Some(boot), Some(range)) => crate::compatibility::allows(boot,range)!=Some(false),
            _ => true,
        };
        let unverified=|d:&Dependency|d.range.as_ref().is_some_and(|r|boot.as_ref().is_none_or(|b|crate::compatibility::allows(b,r).is_none()));
        let matches =
            |d: &Dependency| needle.as_ref().is_none_or(|n| d.id.to_lowercase().contains(n) || d.name.to_lowercase().contains(n) || d.description.to_lowercase().contains(n));

        let mut items = vec![TreeItem::new(SELECTED, &format!("Selected ({})", chosen.len())).icon("check")];
        for id in &chosen {
            let known = catalogue.and_then(|c| c.find(id));
            let label = known.map_or(id.as_str(), |d| d.name.as_str());
            let detail = match (known, &boot) {
                (Some(d), Some(boot)) if !compatible(d) => format!("(not compatible with {boot})"),
                (Some(d), _) if unverified(d)=>format!("{} (compatibility range unverified)",d.description),
                (Some(d), _) => d.description.clone(),
                (None, _) => String::new(),
            };
            let mut item = TreeItem::new(&format!("{CHOSEN_PREFIX}{id}"), label).parent(SELECTED);
            if !detail.is_empty() {
                item = item.detail(&detail);
            }
            items.push(item);
        }

        let mut found = 0;
        if let Some(message) = &message {
            items.push(TreeItem::new("message", message).icon("info"));
        }
        for category in catalogue.into_iter().flat_map(|c| &c.categories) {
            let shown: Vec<&Dependency> = category.dependencies.iter().filter(|d| compatible(d) && matches(d)).collect();
            if shown.is_empty() {
                continue;
            }

            found += shown.len();
            let key = format!("{CATEGORY_PREFIX}{}", category.name);
            items.push(TreeItem::new(&key, &category.name).icon("folder"));
            for d in shown {
                let mut item = TreeItem::new(&format!("{DEPENDENCY_PREFIX}{}", d.id), &d.name).parent(&key);
                if unverified(d) {item=item.detail(&format!("{} (compatibility range unverified)",d.description));}
                else if !d.description.is_empty() {
                    item = item.detail(&d.description);
                }
                if chosen.contains(&d.id) {
                    item = item.icon("check");
                }
                items.push(item);
            }
        }

        self.apply_expansion(&mut items, search.is_some());
        let status = match &search {
            Some(query) => format!("{found} matches for \"{query}\""),
            None => format!("{} selected. Double-click a dependency to add or remove it.", chosen.len()),
        };

        let body = if ui_kit::web::is_rendering() {
            Body::Other { kind:"tree".into(), data:serde_json::json!({"items":items.iter().map(|item| {
                let dependency=item.key.strip_prefix(DEPENDENCY_PREFIX).or_else(||item.key.strip_prefix(CHOSEN_PREFIX));
                let selected=dependency.is_some_and(|id|chosen.iter().any(|chosen|chosen == id));
                serde_json::json!({"key":item.key,"parent":item.parent,"label":item.label,"icon":item.icon,"detail":item.detail,
                    "expandable":item.expandable,"expanded":item.expanded,
                    "activationLabel":dependency.map(|_|if selected {"Deselect"} else {"Select"}),
                    "activationIcon":dependency.map(|_|if selected {"remove"} else {"add"})})
            }).collect::<Vec<_>>()}).to_string() }
        } else { Body::Tree(items) };
        let mut view = View::new(body).button(Button::new("search", "Search…").icon("search"));
        if search.is_some() {
            view = view.button(Button::new("clearSearch", "Clear search").icon("close"));
        }
        let view = view
            .button(Button::new("clearAll", "Clear all").icon("trash").enabled(!chosen.is_empty()))
            .button(Button::new("refreshCatalogue", "Refresh").icon("refresh"));

        let status = if self.catalog_status.as_ref().is_some_and(|status| status.starts_with("Catalogue refresh failed")) {
            format!("{status} {}", self.catalog_status.as_deref().unwrap_or_default())
        } else {
            status
        };
        let status=if catalogue.is_some_and(|c|c.categories.iter().flat_map(|c|&c.dependencies).any(unverified)) {format!("{status} Some declared compatibility ranges are unverified; these entries remain visible without a compatibility assertion.")}else{status};
        (view, status)
    }

    fn apply_expansion(&self, items: &mut [TreeItem], searching: bool) {
        let parents: BTreeSet<String> = items.iter().filter_map(|i| i.parent.clone()).collect();
        for item in items.iter_mut() {
            item.expandable = parents.contains(&item.key);
            item.expanded =
                item.expandable && self.dependencies.expansion.get(&item.key).copied().unwrap_or(item.key == SELECTED || searching);
        }
    }

    /// True when the event belonged to the Dependencies section.
    pub(crate) fn handle_dependencies(&mut self, event: &ViewEvent) -> Result<bool, Error> {
        match event {
            ViewEvent::Expand(key) => {
                self.dependencies.expansion.insert(key.clone(), true);
            }
            ViewEvent::Collapse(key) => {
                self.dependencies.expansion.insert(key.clone(), false);
            }
            ViewEvent::Activate(key) => {
                let Some(id) = key.strip_prefix(DEPENDENCY_PREFIX).or_else(|| key.strip_prefix(CHOSEN_PREFIX)) else {
                    return Ok(false);
                };
                self.toggle_dependency(id);
            }
            ViewEvent::Button(b) if b == "search" => match host::input("Search dependencies", self.dependencies.search.as_deref()) {
                Ok(Some(query)) => self.dependencies.search = Some(query.trim().to_owned()).filter(|q| !q.is_empty()),
                Ok(None) | Err(Error::Invalid(_)) => {}
                Err(error) => return Err(error),
            },
            ViewEvent::Button(b) if b == "clearSearch" => self.dependencies.search = None,
            ViewEvent::Button(b) if b == "clearAll" => match host::confirm("Clear all dependencies", "Remove every chosen dependency?") {
                Ok(true) => self.set("dependencies", ""),
                Ok(false) | Err(Error::Invalid(_)) => {}
                Err(error) => return Err(error),
            },
            ViewEvent::Button(b) if b == "refreshCatalogue" => {
                self.load_metadata(true);
                self.dependencies.catalogue = self.metadata.as_ref().map(|metadata| Ok(Catalogue::from_metadata(metadata)));
            }
            _ => return Ok(false),
        }

        Ok(true)
    }
}

#[cfg(test)]
mod strict_range_tests {
    use super::*;
    use openidea_plugin::{mock::MockHost,views::Body};
    use serde_json::json;
    #[test]
    fn current_client_version_range_filters_and_blocks_generation_for_excluded_selection() {
        let host = MockHost::install().with_capabilities(&["storage"]);
        let mut launcher = Launcher::default();
        let _ = launcher.changed();
        launcher.metadata = Some(json!({"bootVersion":{"default":"4.1.2","values":[{"id":"4.1.2"}]},"dependencies":{"values":[{"name":"Tools","values":[
            {"id":"modern","name":"Modern","versionRange":"[4.0.0,4.3.0-M1)"},
            {"id":"legacy","name":"Legacy","versionRange":"[3.0.0,4.0.0)"}
        ]}]}}));
        launcher.set("dependencies", "legacy");
        let (view, _) = launcher.dependencies_tree();
        let Body::Tree(rows) = view.body else { panic!("tree") };
        assert!(rows.iter().any(|row| row.key == "dep:modern"));
        assert!(!rows.iter().any(|row| row.key == "dep:legacy"));
        assert!(rows.iter().any(|row| row.key == "sel:legacy" && row.detail.as_deref().unwrap_or_default().contains("not compatible")));
        assert!(launcher.compatibility_problem().unwrap().contains("legacy"));
        assert!(host.requests().is_empty());
    }
    #[test]
    fn catalogue_search_matches_dependency_ids_not_just_display_text() {
        let host = MockHost::install().with_capabilities(&["storage"]);
        let mut launcher = Launcher::default();
        let _ = launcher.changed();
        launcher.metadata = Some(json!({"bootVersion":{"default":"3.5.0"},"dependencies":{"values":[{"name":"Web","values":[
            {"id":"data-jpa","name":"Persistence","description":"Relational storage"},
            {"id":"web","name":"Spring Web","description":"MVC"}
        ]}]}}));
        launcher.dependencies.search = Some("DATA-JPA".into());
        let (view, status) = launcher.dependencies_tree();
        let Body::Tree(rows) = view.body else { panic!("tree") };
        assert!(rows.iter().any(|row| row.key == "dep:data-jpa"));
        assert!(!rows.iter().any(|row| row.key == "dep:web"));
        assert!(status.contains("1 matches"));
        assert!(host.requests().is_empty());
    }
    #[test]
    fn version_parse_never_silently_keeps_numeric_prefix_or_overflowed_qualifier() {
        for version in ["3.5.bad","3.18446744073709551616.0","3.5.0-RC18446744073709551616","3.5.0-M","3.5.0-custom","+3.5.0"] {assert_eq!(compare_versions(version,"3.5.0"),None,"{version}");assert_eq!(crate::compatibility::allows(version,"[3.0.0,4.0.0)"),None);}
        assert_eq!(compare_versions("3.5.0-RC2","3.5.0-M9"),Some(Ordering::Greater));assert_eq!(compare_versions("3.5","3.5.0"),Some(Ordering::Equal));
    }
    #[test]
    fn contradictory_or_overflowed_ranges_are_unknown_not_compatibility_proof() {
        for range in ["[4.0.0,3.0.0)","(3.5.0,3.5.0]","[3.18446744073709551616.0,4.0.0)","[3.5.0-RC18446744073709551616,4.0.0)"] {assert_eq!(crate::compatibility::allows("3.5.0",range),None,"{range}");}
        assert_eq!(crate::compatibility::allows("3.5.0","[3.5.0,3.5.0]"),Some(true));
    }
    #[test]
    fn unknown_ranges_stay_visible_labelled_and_do_not_fetch_or_assert_incompatibility() {
        let host=MockHost::install().with_capabilities(&["storage"]);let mut l=Launcher::default();let _=l.changed();l.metadata=Some(json!({"bootVersion":{"default":"3.5.0","values":[{"id":"3.5.0"}]},"dependencies":{"values":[{"name":"Test","values":[{"id":"unknown","name":"Unknown","compatibilityRange":"[4.0.0,3.0.0)"},{"id":"old","name":"Old","compatibilityRange":"[1.0.0,2.0.0)"}]}]}}));l.set("dependencies","unknown");
        let (view,status)=l.dependencies_tree();let Body::Tree(rows)=view.body else {panic!()};assert!(rows.iter().any(|r|r.key=="dep:unknown"&&r.detail.as_deref().is_some_and(|d|d.contains("unverified"))));assert!(rows.iter().any(|r|r.key=="sel:unknown"&&r.detail.as_deref().is_some_and(|d|d.contains("unverified"))));assert!(!rows.iter().any(|r|r.key=="dep:old"));assert!(status.contains("unverified"));assert!(host.requests().is_empty());assert!(l.compatibility_problem().is_none());
        let host=host.with_capabilities(&["storage","net:start.spring.io"]);let previous=l.metadata.clone();let mut bad=previous.clone().unwrap();bad["dependencies"]["values"][0]["values"][0]["compatibilityRange"]=json!(false);
        host.on_fetch(move |_|Ok(openidea_plugin::HttpResponse{status:200,headers:vec![],body:bad.to_string().into_bytes()}));l.load_metadata(true);assert_eq!(l.metadata,previous,"malformed range type cannot become an unrestricted compatibility fact");assert!(l.catalog_status.as_deref().unwrap().contains("failed"));
    }
}
