//! Mapper Declaration conventions: names of generated mapping methods and the naming strategy for
//! collection mapping methods (settings `mapperToDtoPattern`, `mapperToEntityPattern`,
//! `mapperUpdatePattern`, `mapperCollectionNaming`). A pattern is the method name itself.
use super::*;

#[derive(Clone, Copy, PartialEq, Debug)]
enum Strategy {
    Plural,
    Same,
    CollectionType,
}

pub(super) struct Names {
    pub(super) to_dto: String,
    pub(super) to_entity: String,
    pub(super) update: String,
    strategy: Strategy,
}

fn pattern(setting: &str, default: &str) -> Result<String, Error> {
    let value = host::setting_string(setting).map(|v| v.trim().to_owned()).filter(|v| !v.is_empty()).unwrap_or_else(|| default.to_owned());
    if !identifier(&value) {
        return Err(format!("{setting} must be a valid Java method name, got `{value}`.").into());
    }
    Ok(value)
}

fn plural(word: &str) -> String {
    let consonant_y = word.strip_suffix('y').and_then(|stem| stem.chars().last()).is_some_and(|c| c.is_ascii_alphabetic() && !"aeiouAEIOU".contains(c));
    if consonant_y {
        return format!("{}ies", &word[..word.len() - 1]);
    }
    if ["s", "x", "z", "ch", "sh"].iter().any(|end| word.ends_with(end)) {
        return format!("{word}es");
    }
    format!("{word}s")
}

impl Names {
    #[cfg(test)]
    pub(super) fn standard() -> Self {
        Self { to_dto: "toDto".into(), to_entity: "toEntity".into(), update: "updateEntity".into(), strategy: Strategy::Plural }
    }

    pub(super) fn load() -> Result<Self, Error> {
        let strategy = match host::setting_string("mapperCollectionNaming").as_deref() {
            None | Some("plural") => Strategy::Plural,
            Some("same") => Strategy::Same,
            Some("collection-type") => Strategy::CollectionType,
            Some(other) => return Err(format!("Unknown mapperCollectionNaming `{other}`; use plural, same or collection-type.").into()),
        };
        let names = Self {
            to_dto: pattern("mapperToDtoPattern", "toDto")?,
            to_entity: pattern("mapperToEntityPattern", "toEntity")?,
            update: pattern("mapperUpdatePattern", "updateEntity")?,
            strategy,
        };
        if names.to_dto == names.to_entity || names.to_dto == names.update || names.to_entity == names.update {
            return Err("The mapper method name patterns must differ from each other.".into());
        }
        Ok(names)
    }

    /// Name of the collection variant of a mapping method.
    pub(super) fn collection(&self, single: &str) -> String {
        match self.strategy {
            Strategy::Plural => plural(single),
            Strategy::Same => single.to_owned(),
            Strategy::CollectionType => format!("{single}List"),
        }
    }

    pub(super) fn collection_param(&self, single: &str) -> String {
        match self.strategy {
            Strategy::Plural => plural(single),
            _ => single.to_owned(),
        }
    }

    /// `(to_dto, to_entity)` collection signatures for Java or Kotlin, indented as interface members.
    pub(super) fn collection_methods(&self, kotlin: bool, entity: &str, dto: &str, to_dto: bool, to_entity: bool) -> Vec<String> {
        let mut out = Vec::new();
        if to_dto {
            let (name, param) = (self.collection(&self.to_dto), self.collection_param("entity"));
            out.push(if kotlin {
                format!("    fun {name}({param}: List<{entity}>): List<{dto}>")
            } else {
                format!("    List<{dto}> {name}(List<{entity}> {param});")
            });
        }
        if to_entity {
            let (name, param) = (self.collection(&self.to_entity), self.collection_param("dto"));
            out.push(if kotlin {
                format!("    fun {name}({param}: List<{dto}>): List<{entity}>")
            } else {
                format!("    List<{entity}> {name}(List<{dto}> {param});")
            });
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;

    fn names(settings: &[(&str, &str)]) -> Result<Names, Error> {
        let host = MockHost::install();
        for (key, value) in settings {
            host.set_setting(key, value);
        }
        Names::load()
    }

    #[test]
    fn defaults_keep_the_existing_names_and_plural_collection_names() {
        let names = names(&[]).unwrap();
        assert_eq!((names.to_dto.as_str(), names.to_entity.as_str(), names.update.as_str()), ("toDto", "toEntity", "updateEntity"));
        assert_eq!(names.collection("toDto"), "toDtos");
        assert_eq!(names.collection("toEntity"), "toEntities");
    }

    #[test]
    fn strategies_change_collection_names_and_patterns_are_validated() {
        assert_eq!(names(&[("mapperCollectionNaming", "\"same\"")]).unwrap().collection("toDto"), "toDto");
        assert_eq!(names(&[("mapperCollectionNaming", "\"collection-type\"")]).unwrap().collection("toDto"), "toDtoList");
        assert!(names(&[("mapperCollectionNaming", "\"bogus\"")]).is_err());
        assert!(names(&[("mapperToDtoPattern", "\"to Dto\"")]).is_err());
        assert!(names(&[("mapperToDtoPattern", "\"same\""), ("mapperToEntityPattern", "\"same\"")]).is_err());
    }

    #[test]
    fn collection_signatures_follow_the_names_in_java_and_kotlin() {
        let names = names(&[("mapperToDtoPattern", "\"mapToModel\"")]).unwrap();
        assert_eq!(names.collection_methods(false, "Customer", "CustomerDto", true, true), vec!["    List<CustomerDto> mapToModels(List<Customer> entities);", "    List<Customer> toEntities(List<CustomerDto> dtos);"]);
        assert_eq!(names.collection_methods(true, "Customer", "CustomerDto", true, false), vec!["    fun mapToModels(entities: List<Customer>): List<CustomerDto>"]);
    }
}
