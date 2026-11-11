//! Positive physical-source WebFlux Gateway DSL facts, never server endpoints.
//! Intentionally accepts only a complete, direct literal return chain.
use openidea_plugin::Range;
use spring_text::{
    java::{self, JavaFile, Tok},
    range,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GatewayRoute {
    pub id: String,
    pub uri: String,
    pub paths: Vec<String>,
    pub methods: Vec<String>,
    /// Exact contents of the literal route ID, excluding quotes.
    pub range: Range,
    pub uri_range: Range,
    pub path_ranges: Vec<Range>,
    pub method_ranges: Vec<Range>,
}

fn owned(file: &JavaFile, written: &str, qualified: &str) -> bool {
    if written == qualified {
        return !file.types.iter().any(|t| t.name == "org")
            && !file
                .imports
                .iter()
                .any(|i| i.path.rsplit('.').next() == Some("org"));
    }
    let short = qualified.rsplit('.').next().unwrap();
    written == short
        && !file.types.iter().any(|t| t.name == short)
        && file
            .imports
            .iter()
            .filter(|i| !i.is_static && i.path.rsplit('.').next() == Some(short))
            .map(|i| i.path.as_str())
            .collect::<Vec<_>>()
            == [qualified]
}

struct Chain<'a> {
    toks: &'a [Tok<'a>],
    i: usize,
    text: &'a str,
}
impl Chain<'_> {
    fn take(&mut self, s: &str) -> Option<()> {
        (self.toks.get(self.i)?.s == s).then(|| {
            self.i += 1;
        })
    }
    fn ident(&mut self) -> Option<&str> {
        let t = self.toks.get(self.i)?;
        if !t.is_ident() || t.string().is_some() {
            return None;
        }
        self.i += 1;
        Some(t.s)
    }
    fn literal(&mut self) -> Option<(String, Range)> {
        let t = self.toks.get(self.i)?;
        // Java preprocessing/escaped names and text blocks need a richer source map.
        if !t.s.starts_with('"')
            || t.s.starts_with("\"\"\"")
            || !t.s.ends_with('"')
            || t.s.contains('\\')
        {
            return None;
        }
        let s = t.string()?;
        if s.value.is_empty() || s.value.contains(['\n', '\r']) || s.value.contains("${") {
            return None;
        }
        self.i += 1;
        Some((s.value, range(self.text, s.at, s.end)))
    }
    fn literals(&mut self) -> Option<Vec<(String, Range)>> {
        self.take("(")?;
        let mut values = vec![self.literal()?];
        while self.toks.get(self.i).is_some_and(|t| t.s == ",") {
            self.i += 1;
            values.push(self.literal()?);
        }
        self.take(")")?;
        Some(values)
    }
    fn route(&mut self) -> Option<GatewayRoute> {
        self.take(".")?;
        self.take("route")?;
        self.take("(")?;
        let (id, at) = self.literal()?;
        self.take(",")?;
        let lambda = self.ident()?.to_owned();
        self.take("-")?;
        self.take(">")?;
        self.take(&lambda)?;
        let mut paths = Vec::new();
        let mut methods = Vec::new();
        let mut first = true;
        let mut or_kind: Option<String> = None;
        let mut joined_by_or = false;
        let mut used_and = false;
        loop {
            self.take(".")?;
            let name = self.ident()?.to_owned();
            match name.as_str() {
                "path" | "method" => {
                    let values = self.literals()?;
                    let target = if name == "path" {
                        &mut paths
                    } else {
                        &mut methods
                    };
                    // Repeated AND groups cannot be flattened; a pure `.or()` chain of one
                    // predicate kind is exactly the union of its values.
                    if !target.is_empty() && !(joined_by_or && or_kind.as_deref() == Some(name.as_str()) && !used_and) {
                        return None;
                    }
                    if joined_by_or && or_kind.as_deref() != Some(name.as_str()) {
                        return None;
                    }
                    if name == "method"
                        && values
                            .iter()
                            .any(|(v, _)| !v.bytes().all(|b| b.is_ascii_uppercase()))
                    {
                        return None;
                    }
                    target.extend(values);
                    joined_by_or = false;
                    first = false;
                }
                "uri" if !first && !joined_by_or => {
                    self.take("(")?;
                    let (uri, uri_range) = self.literal()?;
                    self.take(")")?;
                    self.take(")")?;
                    return Some(GatewayRoute {
                        id,
                        uri,
                        range: at,
                        uri_range,
                        paths: paths.iter().map(|(s, _)| s.clone()).collect(),
                        methods: methods.iter().map(|(s, _)| s.clone()).collect(),
                        path_ranges: paths.iter().map(|(_, r)| *r).collect(),
                        method_ranges: methods.iter().map(|(_, r)| *r).collect(),
                    });
                }
                _ => return None,
            }
            // BooleanSpec requires explicit AND to return to PredicateSpec.
            if self.toks.get(self.i + 1).is_some_and(|t| t.s == "and") {
                if or_kind.is_some() {
                    return None;
                }
                used_and = true;
                self.take(".")?;
                self.take("and")?;
                self.take("(")?;
                self.take(")")?;
                joined_by_or = false;
            } else if self.toks.get(self.i + 1).is_some_and(|t| t.s == "or") {
                if used_and || (or_kind.is_some() && or_kind.as_deref() != Some(name.as_str())) {
                    return None;
                }
                or_kind = Some(name.clone());
                joined_by_or = true;
                self.take(".")?;
                self.take("or")?;
                self.take("(")?;
                self.take(")")?;
            } else if !self.toks.get(self.i + 1).is_some_and(|t| t.s == "uri") {
                return None;
            }
        }
    }
}

pub fn routes(path: &str, text: &str) -> Vec<GatewayRoute> {
    if !path.ends_with(".java") || text.contains("\\u") || text.len() > 256 * 1024 {
        return Vec::new();
    }
    let file = java::parse(text);
    let toks = java::tokens(text);
    // Type parameters may shadow explicitly imported Spring types. Without a
    // lexical type-parameter resolver, withhold the file rather than infer it.
    if file.types.iter().any(|ty| {
        toks.iter()
            .filter(|t| t.at >= ty.at)
            .take_while(|t| t.s != "{")
            .any(|t| t.s == "<")
    }) {
        return Vec::new();
    }
    let mut out = Vec::new();
    for ty in &file.types {
        for m in ty.methods() {
            let Some(bean) = m.annotation("Bean") else {
                continue;
            };
            if !owned(
                &file,
                &text[bean.at + 1..bean.name_at + bean.name.len()],
                "org.springframework.context.annotation.Bean",
            ) || !m.ty.as_ref().is_some_and(|t| {
                owned(
                    &file,
                    &t.text,
                    "org.springframework.cloud.gateway.route.RouteLocator",
                )
            }) || m.params.len() != 1
            {
                continue;
            }
            let p = &m.params[0];
            if !owned(
                &file,
                &p.ty.text,
                "org.springframework.cloud.gateway.route.builder.RouteLocatorBuilder",
            ) {
                continue;
            }
            let Some((from, to)) = m.body else { continue };
            if toks
                .iter()
                .any(|t| t.at >= m.start && t.at < from && t.s == "<")
            {
                continue;
            }
            let body: Vec<_> = toks
                .iter()
                .copied()
                .filter(|t| t.at >= from && t.end() <= to)
                .collect();
            let mut chain = Chain {
                toks: &body,
                i: 0,
                text,
            };
            let parse = (|| -> Option<Vec<GatewayRoute>> {
                chain.take("{")?;
                chain.take("return")?;
                chain.take(&p.name)?;
                chain.take(".")?;
                chain.take("routes")?;
                chain.take("(")?;
                chain.take(")")?;
                let mut routes = Vec::new();
                while chain.toks.get(chain.i + 1).is_some_and(|t| t.s == "route") {
                    routes.push(chain.route()?);
                }
                chain.take(".")?;
                chain.take("build")?;
                chain.take("(")?;
                chain.take(")")?;
                chain.take(";")?;
                chain.take("}")?;
                (chain.i == body.len()).then_some(routes)
            })();
            if let Some(routes) = parse {
                out.extend(routes);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    const HEAD: &str = "import org.springframework.context.annotation.Bean;\nimport org.springframework.cloud.gateway.route.RouteLocator;\nimport org.springframework.cloud.gateway.route.builder.RouteLocatorBuilder;\nclass Gateway { @Bean RouteLocator routes(RouteLocatorBuilder builder) { return ";
    fn source(chain: &str) -> String {
        format!("{HEAD}{chain}; }} }}")
    }
    #[test]
    fn literal_routes_keep_own_predicates_uri_and_exact_ranges() {
        let text = source(
            "builder.routes().route(\"users\", r -> r.path(\"/users/**\", \"/people/**\").and().method(\"GET\", \"HEAD\").uri(\"lb://users\")).route(\"write\", p -> p.method(\"POST\").and().path(\"/write\").uri(\"https://example.org\")).build()",
        );
        let found = routes("Gateway.java", &text);
        assert_eq!(found.len(), 2);
        assert_eq!(
            (
                &found[0].id,
                &found[0].uri,
                &found[0].paths,
                &found[0].methods
            ),
            (
                &"users".into(),
                &"lb://users".into(),
                &vec!["/users/**".into(), "/people/**".into()],
                &vec!["GET".into(), "HEAD".into()]
            )
        );
        assert_eq!(found[1].paths, ["/write"]);
        assert_eq!(found[1].methods, ["POST"]);
        let slice = |r: Range| {
            &text[openidea_plugin::text::offset(&text, r.start).unwrap()
                ..openidea_plugin::text::offset(&text, r.end).unwrap()]
        };
        assert_eq!(slice(found[0].range), "users");
        assert_eq!(slice(found[0].uri_range), "lb://users");
        assert_eq!(slice(found[1].path_ranges[0]), "/write");
        assert_eq!(slice(found[0].method_ranges[1]), "HEAD");
    }
    #[test]
    fn pure_or_chains_of_one_predicate_kind_are_the_union_of_their_values() {
        let text = source(
            "builder.routes().route(\"a\", r -> r.path(\"/a\").or().path(\"/b\", \"/c\").uri(\"lb://a\")).route(\"m\", r -> r.method(\"GET\").or().method(\"POST\").uri(\"lb://m\")).build()",
        );
        let found = routes("Gateway.java", &text);
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].paths, ["/a", "/b", "/c"]);
        assert!(found[0].methods.is_empty());
        assert_eq!(found[0].path_ranges.len(), 3);
        assert_eq!(found[1].methods, ["GET", "POST"]);
    }
    #[test]
    fn dynamic_foreign_shadowed_or_nested_chains_are_not_inferred() {
        let valid = "builder.routes().route(\"x\", r -> r.path(\"/x\").uri(\"lb://x\")).build()";
        for bad in [
            valid.replace("\"x\", r", "ID, r"),
            valid.replace("\"lb://x\"", "uri"),
            valid.replace("r.path", "other.path"),
            valid.replace(".uri(\"lb://x\")", ".or().method(\"GET\").uri(\"lb://x\")"),
            valid.replace(".uri(\"lb://x\")", ".and().path(\"/y\").uri(\"lb://x\")"),
            valid.replace(".uri(\"lb://x\")", ".or().path(\"/y\").and().path(\"/z\").uri(\"lb://x\")"),
            valid.replace("r.path(\"/x\")", "r.method(\"GET\").and().path(\"/x\").or().path(\"/y\")"),
            valid.replace("r.path(\"/x\")", "helper(r -> r.path(\"/x\"))"),
            valid.replace("\"/x\"", "\"/x\" + SUFFIX"),
            valid.replace("\"/x\"", "\"/\\u0078\""),
            valid.replace(
                ".build()",
                ".build(); other.route(\"fake\", r -> r.path(\"/fake\").uri(\"lb://fake\"))",
            ),
        ] {
            assert!(routes("Gateway.java", &source(&bad)).is_empty(), "{bad}");
        }
        for text in [
            source(valid).replace(
                "org.springframework.cloud.gateway.route.builder.RouteLocatorBuilder",
                "foreign.RouteLocatorBuilder",
            ),
            source(valid).replace(
                "org.springframework.context.annotation.Bean",
                "foreign.Bean",
            ),
            format!("{} class RouteLocatorBuilder {{}}", source(valid)),
            source(valid).replace("return builder", "builder = other; return builder"),
            source(valid).replace("class Gateway", "class Gateway<RouteLocatorBuilder>"),
            source(valid).replace(
                "@Bean RouteLocator",
                "@Bean <RouteLocatorBuilder> RouteLocator",
            ),
        ] {
            assert!(routes("Gateway.java", &text).is_empty());
        }
        assert!(routes("Gateway.kt", &source(valid)).is_empty());
    }
    #[test]
    fn comments_and_other_methods_cannot_contribute_route_facts() {
        let text = source(
            "builder.routes().route(\"one\", r -> r.path(\"/one\") /* .uri(\"fake\") */ .uri(\"lb://one\")).build()",
        );
        let found = routes("Gateway.java", &text);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].uri, "lb://one");
        assert!(routes("Gateway.java", &text.replace("@Bean ", "")).is_empty());
    }
}
