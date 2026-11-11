//! Explicit, version-qualified compiled Hibernate metadata + manual JDBC JPQL SELECT console.
//! Never called by providers. This is a trusted project-code adapter, not a sandbox.
use openidea_plugin::host::Command;
use openidea_plugin::views::{Body, Button, Table, View};
use openidea_plugin::{Error, Invocation, Project, host};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};
pub const VIEW: &str = "jpql-console";
const HELPER: &str = include_str!("../adapters/ReviewedJpaConsole.java");
const LOCK: &str = include_str!("../adapters/hibernate-6.6.49-runtime.json");
const LOCK7: &str = include_str!("../adapters/hibernate-7.4.15-runtime.json");
const PROFILE6: &str = "jpql-hibernate-6.6.49-v1";
const PROFILE7: &str = "jpql-hibernate-7.4.15-v1";
#[derive(Clone, Copy)]
struct RuntimeProfile {
    id: &'static str,
    version: &'static str,
    lock: &'static str,
}
fn runtime_profile(id: &str) -> Option<RuntimeProfile> {
    match id {
        PROFILE6 => Some(RuntimeProfile {
            id: PROFILE6,
            version: "6.6.49.Final",
            lock: LOCK,
        }),
        PROFILE7 => Some(RuntimeProfile {
            id: PROFILE7,
            version: "7.4.15.Final",
            lock: LOCK7,
        }),
        _ => None,
    }
}
#[derive(Clone, Copy)]
struct Dialect {
    key: &'static str,
    label: &'static str,
    class: &'static str,
}
const DIALECTS: &[Dialect] = &[
    Dialect {
        key: "h2",
        label: "H2",
        class: "org.hibernate.dialect.H2Dialect",
    },
    Dialect {
        key: "postgresql",
        label: "PostgreSQL",
        class: "org.hibernate.dialect.PostgreSQLDialect",
    },
    Dialect {
        key: "mysql",
        label: "MySQL",
        class: "org.hibernate.dialect.MySQLDialect",
    },
    Dialect {
        key: "mariadb",
        label: "MariaDB",
        class: "org.hibernate.dialect.MariaDBDialect",
    },
];
fn supported_dialects(_runtime: RuntimeProfile) -> Vec<Dialect> {
    DIALECTS.to_vec()
}
const CAP: usize = 128 << 10;
fn hash(text: &str) -> String {
    format!("{:x}", Sha256::digest(text.as_bytes()))
}
fn payload(stdout: &str) -> Result<&str, Error> {
    if stdout.len() > 2 * CAP {
        return Err(Error::Stale);
    }
    stdout
        .rsplit_once("OPENIDEA_JPQL_CONSOLE/1\n")
        .map(|(_, payload)| payload.trim())
        .filter(|payload| !payload.is_empty())
        .ok_or_else(|| "Metadata helper returned no complete protocol payload.".into())
}
fn json(text: &str) -> Result<Value, Error> {
    if text.len() > 2 * CAP {
        return Err(Error::Stale);
    }
    serde_json::from_str::<Unique>(text)
        .map(|value| value.0)
        .map_err(|_| "The metadata adapter returned incomplete or invalid JSON.".into())
}
fn relative(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 1024
        && !value.starts_with('/')
        && !value.contains(['\\', ':', '|', '*', '?', '[', ']', '{', '}'])
        && !value.chars().any(char::is_control)
        && !value
            .split('/')
            .any(|part| part.is_empty() || matches!(part, "." | ".."))
}
fn name(value: &str) -> bool {
    value.len() <= 256
        && !["java.", "javax.", "jakarta.", "org.hibernate."]
            .iter()
            .any(|prefix| value.starts_with(prefix))
        && value.split('.').all(|part| {
            part.starts_with(|ch: char| ch.is_ascii_alphabetic() || ch == '_' || ch == '$')
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"_$".contains(&byte))
        })
}
fn strings(value: Option<&Value>, max: usize, required: bool) -> Result<Vec<String>, Error> {
    let Some(value) = value else {
        return if required {
            Err(Error::Stale)
        } else {
            Ok(vec![])
        };
    };
    let array = value
        .as_array()
        .filter(|array| array.len() <= max)
        .ok_or(Error::Stale)?;
    let mut seen = BTreeSet::new();
    array
        .iter()
        .map(|value| {
            let text = value
                .as_str()
                .filter(|text| relative(text))
                .ok_or(Error::Stale)?;
            if !seen.insert(text) {
                return Err(Error::Stale);
            }
            Ok(text.into())
        })
        .collect()
}
#[derive(Clone)]
struct Class {
    name: String,
    source: String,
    root: String,
}
#[derive(Clone)]
struct Plan {
    runtime_profile: RuntimeProfile,
    dialect: Dialect,
    root: String,
    profile: String,
    source: String,
    jars: Vec<String>,
    runtime: usize,
    roots: Vec<String>,
    sources: Vec<String>,
    entities: Vec<Class>,
    converters: Vec<Class>,
    request: Request,
    driver_hashes: BTreeMap<String, String>,
}
fn classes(
    value: Option<&Value>,
    roots: &[String],
    max: usize,
    required: bool,
) -> Result<Vec<Class>, Error> {
    let Some(value) = value else {
        return if required {
            Err(Error::Stale)
        } else {
            Ok(vec![])
        };
    };
    let mut seen = BTreeSet::new();
    value
        .as_array()
        .filter(|items| items.len() <= max)
        .ok_or(Error::Stale)?
        .iter()
        .map(|entry| {
            let object = entry
                .as_object()
                .filter(|entry| entry.len() == 3)
                .ok_or(Error::Stale)?;
            let name = object
                .get("class")
                .and_then(Value::as_str)
                .filter(|value| name(value))
                .ok_or(Error::Stale)?;
            let source = object
                .get("source")
                .and_then(Value::as_str)
                .filter(|value| {
                    relative(value) && (value.ends_with(".java") || value.ends_with(".kt"))
                })
                .ok_or(Error::Stale)?;
            let root = object
                .get("classRoot")
                .and_then(Value::as_str)
                .filter(|root| roots.iter().any(|candidate| candidate == root))
                .ok_or(Error::Stale)?;
            if !seen.insert(name) {
                return Err(Error::Stale);
            }
            Ok(Class {
                name: name.into(),
                source: source.into(),
                root: root.into(),
            })
        })
        .collect()
}
fn read_plan(project: &Project, profile: &str) -> Result<Plan, Error> {
    if !relative(profile) {
        return Err("Use a project-relative Hibernate profile path.".into());
    }
    let root = project.root()?;
    if host::project_of(&format!("{root}/{profile}"))
        .and_then(|owner| owner.root().ok())
        .as_deref()
        != Some(&root)
    {
        return Err(Error::Stale);
    }
    let source = project.read_file(profile)?;
    if source.len() > 32 << 10 {
        return Err("Hibernate profile exceeds32KiB.".into());
    }
    let value = json(&source)?;
    let object = value.as_object().ok_or(Error::Stale)?;
    let runtime_profile = runtime_profile(
        value
            .get("profile")
            .and_then(Value::as_str)
            .ok_or(Error::Stale)?,
    )
    .ok_or("Choose a documented pinned Hibernate runtime profile.")?;
    if object.keys().any(|key| {
        ![
            "profile",
            "dialect",
            "runtimeJars",
            "projectJars",
            "classRoots",
            "entities",
            "converters",
            "jdbc",
            "driverJars",
            "query",
            "parameters",
            "maxRows",
        ]
        .contains(&key.as_str())
    }) {
        return Err("The Hibernate profile has unknown options.".into());
    }
    let dialects: Vec<_> = supported_dialects(runtime_profile)
        .into_iter()
        .filter(|d| ["h2", "postgresql", "mysql", "mariadb"].contains(&d.key))
        .collect();
    let dialect_key = match value.get("dialect") {
        Some(value) => value.as_str().ok_or(Error::Stale)?.to_owned(),
        None => host::pick(
            "Target SQL dialect (offline default version; no database is queried)",
            &dialects
                .iter()
                .map(|dialect| openidea_plugin::PickItem::new(dialect.key, dialect.label))
                .collect::<Vec<_>>(),
        )?
        .ok_or(Error::Cancelled)?,
    };
    let dialect = dialects
        .into_iter()
        .find(|dialect| dialect.key == dialect_key)
        .ok_or(
            "This dialect has not passed the pinned offline mapping contract for this ORM version.",
        )?;
    let mut jars = strings(value.get("runtimeJars"), 32, true)?;
    let runtime = jars.len();
    let lock = json(runtime_profile.lock)?;
    let lock = lock.as_object().ok_or(Error::Stale)?;
    let names: BTreeSet<_> = jars
        .iter()
        .filter_map(|path| path.rsplit('/').next())
        .collect();
    if jars.len() != lock.len()
        || names.len() != lock.len()
        || !lock.keys().all(|name| names.contains(name.as_str()))
    {
        return Err("runtimeJars must name every exact dependency in the selected bundled runtime lock, once each.".into());
    }
    let request = request(&value, dialect)?;
    let drivers = value
        .get("driverJars")
        .and_then(Value::as_object)
        .filter(|v| !v.is_empty() && v.len() <= 8)
        .ok_or(Error::Stale)?;
    let mut driver_hashes = BTreeMap::new();
    for (path, digest) in drivers {
        let digest = digest
            .as_str()
            .filter(|v| v.len() == 64 && v.bytes().all(|b| b.is_ascii_hexdigit()))
            .ok_or(Error::Stale)?;
        if !relative(path) {
            return Err(Error::Stale);
        }
        driver_hashes.insert(path.clone(), digest.to_lowercase());
        jars.push(path.clone());
    }
    jars.extend(strings(value.get("projectJars"), 16, false)?);
    if jars.iter().any(|jar| !jar.ends_with(".jar"))
        || jars.iter().collect::<BTreeSet<_>>().len() != jars.len()
    {
        return Err(Error::Stale);
    }
    let roots = strings(value.get("classRoots"), 4, true)?;
    if roots.is_empty() {
        return Err(Error::Stale);
    }
    let entities = classes(value.get("entities"), &roots, 32, true)?;
    if entities.is_empty() {
        return Err("Select at least one compiled annotated entity root.".into());
    }
    let converters = classes(value.get("converters"), &roots, 16, false)?;
    let mut sources: Vec<_> = entities
        .iter()
        .chain(&converters)
        .map(|entity| entity.source.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    sources.push(profile.into());
    for path in std::iter::once(profile)
        .chain(jars.iter().map(String::as_str))
        .chain(roots.iter().map(String::as_str))
        .chain(sources.iter().map(String::as_str))
    {
        let full = format!("{root}/{path}");
        if host::project_of(&full)
            .and_then(|owner| owner.root().ok())
            .as_deref()
            != Some(&root)
        {
            return Err("Every profile, classpath and declared source must belong to this project, excluding nested foreign projects.".into());
        }
        project.stat(path)?;
    }
    Ok(Plan {
        runtime_profile,
        dialect,
        root,
        profile: profile.into(),
        source,
        jars,
        runtime,
        roots,
        sources,
        entities,
        converters,
        request,
        driver_hashes,
    })
}
fn helper() -> Result<(String, String), Error> {
    let directory = host::fs::plugin_dir()?;
    let name = format!("JpaConsole_{}.java", &hash(HELPER)[..20]);
    let path = format!("{directory}/{name}");
    let entries = host::fs::read_dir(&directory)?;
    if entries.len() >= 4096 {
        return Err(Error::Stale);
    }
    if entries.iter().any(|entry| entry.name == name) {
        if host::fs::read_bytes(&path)? != HELPER.as_bytes() {
            return Err("Private metadata helper changed; no execution performed.".into());
        }
    } else {
        host::fs::write_bytes(&path, HELPER.as_bytes())?;
    }
    Ok((directory, path))
}
impl Plan {
    fn current(&self, project: &Project) -> Result<(), Error> {
        if project.root()? != self.root || project.read_file(&self.profile)? != self.source {
            return Err(Error::Stale);
        }
        for path in self
            .jars
            .iter()
            .chain(&self.roots)
            .chain(&self.sources)
            .chain(std::iter::once(&self.profile))
        {
            if host::project_of(&format!("{}/{path}", self.root))
                .and_then(|owner| owner.root().ok())
                .as_deref()
                != Some(&self.root)
            {
                return Err(Error::Stale);
            }
        }
        Ok(())
    }
    fn command(
        &self,
        directory: &str,
        helper: &str,
        mode: &str,
        proof: Option<&str>,
    ) -> Result<Command, Error> {
        let mut args = vec![
            "-Dfile.encoding=UTF-8".into(),
            "--class-path".into(),
            directory.into(),
            helper.into(),
            mode.into(),
            self.root.clone(),
            self.runtime_profile.id.into(),
            self.dialect.key.into(),
        ];
        let entities: Vec<_> = self
            .entities
            .iter()
            .map(|entity| format!("{}|{}", entity.name, entity.root))
            .collect();
        let converters: Vec<_> = self
            .converters
            .iter()
            .map(|entity| format!("{}|{}", entity.name, entity.root))
            .collect();
        for values in [
            &self.jars,
            &self.roots,
            &self.sources,
            &entities,
            &converters,
        ] {
            args.push(values.len().to_string());
            args.extend(values.iter().cloned());
        }
        if let Some(proof) = proof {
            args.push(proof.into());
        }
        if args.len() > 256 || args.iter().map(String::len).sum::<usize>() > 48 << 10 {
            return Err("Metadata invocation exceeds the bounded argument contract.".into());
        }
        let mut command = Command::new("java").args(args);
        for name in [
            "JAVA_TOOL_OPTIONS",
            "JDK_JAVA_OPTIONS",
            "_JAVA_OPTIONS",
            "JAVA_OPTS",
            "CLASSPATH",
        ] {
            command = command.env(name, "");
        }
        Ok(command)
    }
    fn inspect(&self, project: &Project, directory: &str, helper: &str) -> Result<String, Error> {
        self.current(project)?;
        let output = host::run_command(
            &self.command(directory, helper, "inspect", None)?,
            Some(project),
            None,
        )?;
        if output.status != Some(0) {
            return Err("Java metadata inspection failed; use a supported JDK and saved, owned compiled classes/runtime files. No project metadata code was requested.".into());
        }
        let proof = payload(&output.stdout)?.to_owned();
        self.validate(project, &proof)?;
        self.current(project)?;
        Ok(proof)
    }
    fn validate(&self, project: &Project, proof: &str) -> Result<(), Error> {
        let value = json(proof)?;
        if value.get("profile").and_then(Value::as_str) != Some(self.runtime_profile.id)
            || value.get("version").and_then(Value::as_str) != Some(self.runtime_profile.version)
            || value.get("dialect").and_then(Value::as_str) != Some(self.dialect.class)
            || value.get("java").and_then(Value::as_str).is_none()
        {
            return Err(Error::Stale);
        }
        let digest = value
            .get("fingerprint")
            .and_then(Value::as_str)
            .ok_or(Error::Stale)?;
        if digest.len() != 64
            || !digest.bytes().all(|byte| byte.is_ascii_hexdigit())
            || value
                .get("files")
                .and_then(Value::as_u64)
                .is_none_or(|files| files > 4096)
            || value
                .get("bytes")
                .and_then(Value::as_u64)
                .is_none_or(|bytes| bytes > 128 << 20)
        {
            return Err(Error::Stale);
        }
        let facts = value
            .get("facts")
            .and_then(Value::as_array)
            .ok_or(Error::Stale)?;
        if facts.len()
            != self.jars.len()
                + self.roots.len()
                + self.sources.len()
                + self.entities.len()
                + self.converters.len()
        {
            return Err(Error::Stale);
        }
        let lock = json(self.runtime_profile.lock)?;
        let mut index = 0;
        for (jar, fact) in self.jars.iter().zip(facts) {
            let prefix = format!("jar:{jar}:");
            let digest = fact
                .as_str()
                .and_then(|fact| fact.strip_prefix(&prefix))
                .ok_or(Error::Stale)?;
            if digest.len() != 64 || !digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                return Err(Error::Stale);
            }
            if index < self.runtime
                && lock
                    .get(jar.rsplit('/').next().ok_or(Error::Stale)?)
                    .and_then(Value::as_str)
                    != Some(digest)
            {
                return Err("Hibernate runtime artifact does not match the bundled official dependency lock.".into());
            }
            if self
                .driver_hashes
                .get(jar)
                .is_some_and(|expected| expected != digest)
            {
                return Err(
                    "JDBC driver checksum differs from the explicitly reviewed profile.".into(),
                );
            }
            index += 1;
        }
        for root in &self.roots {
            let prefix = format!("classes:{root}:");
            let digest = facts[index]
                .as_str()
                .and_then(|fact| fact.strip_prefix(&prefix))
                .ok_or(Error::Stale)?;
            if digest.len() != 64 || !digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                return Err(Error::Stale);
            }
            index += 1;
        }
        for source in &self.sources {
            let text = project.read_file(source)?;
            if text.len() > CAP
                || facts[index].as_str() != Some(&format!("source:{source}:{}", hash(&text)))
            {
                return Err("Declared source has unsaved/encoded changes or differs from the inspected disk bytes; save UTF-8 source first.".into());
            }
            index += 1;
        }
        for (label, classes) in [("entity", &self.entities), ("converter", &self.converters)] {
            for class in classes {
                if facts[index].as_str() != Some(&format!("{label}:{}|{}", class.name, class.root))
                {
                    return Err(Error::Stale);
                }
                index += 1;
            }
        }
        Ok(())
    }
}

pub(crate) struct Unique(pub(crate) Value);
impl<'de> serde::Deserialize<'de> for Unique {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = Unique;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("JSON with unique object keys")
            }
            fn visit_map<M: serde::de::MapAccess<'de>>(self, mut m: M) -> Result<Unique, M::Error> {
                let mut out = serde_json::Map::new();
                while let Some((k, v)) = m.next_entry::<String, Unique>()? {
                    if out.insert(k, v.0).is_some() {
                        return Err(serde::de::Error::custom("duplicate JSON key"));
                    }
                }
                Ok(Unique(Value::Object(out)))
            }
            fn visit_seq<S: serde::de::SeqAccess<'de>>(self, mut s: S) -> Result<Unique, S::Error> {
                let mut out = Vec::new();
                while let Some(v) = s.next_element::<Unique>()? {
                    out.push(v.0);
                }
                Ok(Unique(Value::Array(out)))
            }
            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Unique, E> {
                Ok(Unique(Value::String(v.into())))
            }
            fn visit_string<E: serde::de::Error>(self, v: String) -> Result<Unique, E> {
                Ok(Unique(Value::String(v)))
            }
            fn visit_bool<E: serde::de::Error>(self, v: bool) -> Result<Unique, E> {
                Ok(Unique(Value::Bool(v)))
            }
            fn visit_unit<E: serde::de::Error>(self) -> Result<Unique, E> {
                Ok(Unique(Value::Null))
            }
            fn visit_i64<E: serde::de::Error>(self, v: i64) -> Result<Unique, E> {
                Ok(Unique(Value::Number(v.into())))
            }
            fn visit_u64<E: serde::de::Error>(self, v: u64) -> Result<Unique, E> {
                Ok(Unique(Value::Number(v.into())))
            }
            fn visit_f64<E: serde::de::Error>(self, v: f64) -> Result<Unique, E> {
                Ok(Unique(Value::Number(
                    serde_json::Number::from_f64(v).ok_or_else(|| E::custom("nonfinite number"))?,
                )))
            }
        }
        d.deserialize_any(Visitor)
    }
}

#[derive(Clone)]
struct Request {
    url: String,
    username: String,
    password: String,
    driver: String,
    query: String,
    parameters: Vec<(String, String, String)>,
    max_rows: u32,
    /// Reviewed UPDATE/DELETE text; `query` then holds only its equivalent count SELECT.
    mutation: Option<String>,
}
const MUTATION_SETTING: &str = "jpaAssist.jpqlMutationPreview";
/// Pure analysis of a single JPQL UPDATE/DELETE: returns the equivalent `select count(*)` query.
fn mutation_count_query(statement: &str) -> Result<String, Error> {
    let refuse = |why: &str| Err(Error::from(format!("Mutation statement refused: {why}.")));
    let text = statement.trim();
    let lower = text.to_ascii_lowercase();
    if text.contains([';', '\0']) || ["--", "/*", "*/", "//"].iter().any(|c| text.contains(c)) {
        return refuse("multiple statements or comments");
    }
    if text.chars().any(|c| c.is_control() && !c.is_whitespace()) {
        return refuse("control characters");
    }
    if lower.contains("function(") || lower.contains("sql(") || lower.contains("native") {
        return refuse("native SQL or provider functions");
    }
    let original: Vec<&str> = text.split_whitespace().collect();
    let lowered: Vec<String> = original.iter().map(|w| w.to_ascii_lowercase()).collect();
    let words: Vec<&str> = lowered.iter().map(String::as_str).collect();
    let ident = |w: &str| {
        !w.is_empty()
            && w.bytes().enumerate().all(|(i, b)| {
                b == b'_' || b == b'.' || b.is_ascii_alphabetic() || (i > 0 && b.is_ascii_digit())
            })
    };
    if words.last() == Some(&"where") {
        return refuse("empty WHERE clause");
    }
    let where_at = lower.find(" where ").map(|i| i + 7);
    let tail = where_at.map(|i| text[i..].trim()).filter(|t| !t.is_empty());
    if where_at.is_some() && tail.is_none() {
        return refuse("empty WHERE clause");
    }
    let (entity_at, alias_at) = match words.as_slice() {
        ["update", "versioned", ..] => return refuse("versioned update"),
        ["delete", "from", _, rest @ ..] | ["delete", _, rest @ ..] => {
            let entity_at = if words[1] == "from" { 2 } else { 1 };
            let after = entity_at + 1;
            let alias_at = match rest.first() {
                None | Some(&"where") => None,
                Some(&"as") => Some(after + 1),
                Some(_) => Some(after),
            };
            (entity_at, alias_at)
        }
        ["update", _, rest @ ..] => {
            if !lower.contains(" set ") {
                return refuse("UPDATE without SET");
            }
            let alias_at = match rest.first() {
                Some(&"set") | None => None,
                Some(&"as") => Some(3),
                Some(_) => Some(2),
            };
            (1, alias_at)
        }
        _ => return refuse("only one UPDATE or DELETE JPQL statement"),
    };
    let entity = original[entity_at];
    let alias = alias_at.map(|i| original.get(i).copied().unwrap_or(""));
    if !ident(entity) || alias.is_some_and(|a| !ident(a) || a.contains('.') || a.eq_ignore_ascii_case("set")) {
        return refuse("unsupported entity or alias syntax");
    }
    let from = match alias {
        Some(a) => format!("{entity} {a}"),
        None => entity.to_owned(),
    };
    Ok(match tail {
        Some(t) => format!("select count(*) from {from} where {t}"),
        None => format!("select count(*) from {from}"),
    })
}
/// The datasource is provably local embedded H2 only; tcp/ssl/zip/remote forms are refused.
fn offline_h2(dialect: Dialect, url: &str) -> bool {
    let Some(rest) = url.strip_prefix("jdbc:h2:") else {
        return false;
    };
    dialect.key == "h2"
        && !rest.is_empty()
        && !["tcp:", "ssl:", "zip:", "http", "//"].iter().any(|p| rest.starts_with(p))
}
fn bounded(value: Option<&Value>, max: usize) -> Result<String, Error> {
    value
        .and_then(Value::as_str)
        .filter(|v| v.len() <= max && !v.contains('\0'))
        .map(str::to_owned)
        .ok_or(Error::Stale)
}
fn request(value: &Value, dialect: Dialect) -> Result<Request, Error> {
    let jdbc = value
        .get("jdbc")
        .and_then(Value::as_object)
        .filter(|v| v.len() == 3)
        .ok_or(Error::Stale)?;
    let url = bounded(jdbc.get("url"), 2048)?;
    let (prefix, driver) = match dialect.key {
        "h2" => ("jdbc:h2:", "org.h2.Driver"),
        "postgresql" => ("jdbc:postgresql://", "org.postgresql.Driver"),
        "mysql" => ("jdbc:mysql://", "com.mysql.cj.jdbc.Driver"),
        "mariadb" => ("jdbc:mariadb://", "org.mariadb.jdbc.Driver"),
        _ => return Err(Error::Stale),
    };
    if !url.starts_with(prefix)
        || url.contains([';', '?', '@', '#', '\\'])
        || url.chars().any(char::is_control)
    {
        return Err("Use a bare explicit JDBC target URL without URL credentials, properties, or init scripts; credentials have separate fields.".into());
    }
    let mut query = bounded(value.get("query"), 8192)?;
    let mut mutation = None;
    let head = query.trim_start().to_ascii_lowercase();
    if head.starts_with("update ") || head.starts_with("delete ") {
        if !host::setting_bool(MUTATION_SETTING).unwrap_or(false) {
            return Err(format!("JPQL UPDATE/DELETE needs the opt-in setting {MUTATION_SETTING}; preview only, never executed.").into());
        }
        if !offline_h2(dialect, &url) {
            return Err("Mutation preview is limited to a local embedded H2 target; remote datasources are refused.".into());
        }
        let count = mutation_count_query(&query)?;
        mutation = Some(query.trim().to_owned());
        query = count;
    }
    if !query
        .trim_start()
        .to_ascii_lowercase()
        .starts_with("select ")
        || query.contains(';')
    {
        return Err(
            "This console admits one explicit SELECT scalar-projection JPQL query only.".into(),
        );
    }
    let mut parameters = Vec::new();
    let mut names = BTreeSet::new();
    if let Some(values) = value.get("parameters") {
        for p in values
            .as_array()
            .filter(|v| v.len() <= 16)
            .ok_or(Error::Stale)?
        {
            if p.as_object().is_none_or(|v| v.len() != 3) {
                return Err(Error::Stale);
            }
            let name = bounded(p.get("name"), 64)?;
            if name.is_empty()
                || !name.bytes().enumerate().all(|(i, b)| {
                    b == b'_' || b.is_ascii_alphabetic() || (i > 0 && b.is_ascii_digit())
                })
                || !names.insert(name.clone())
            {
                return Err(Error::Stale);
            }
            let kind = bounded(p.get("type"), 16)?;
            if ![
                "string",
                "long",
                "decimal",
                "boolean",
                "date",
                "timestamp",
                "uuid",
            ]
            .contains(&kind.as_str())
            {
                return Err(
                    "Unsupported typed parameter; null/list/entity parameters are not inferred."
                        .into(),
                );
            }
            parameters.push((name, kind, bounded(p.get("value"), 2048)?));
        }
    }
    if mutation.is_some() {
        let lower = mutation.as_deref().unwrap_or("").to_ascii_lowercase();
        let tail = lower.find(" where ").map_or("", |i| &lower[i..]);
        parameters.retain(|(n, _, _)| tail.contains(&format!(":{}", n.to_ascii_lowercase())));
    }
    let max_rows = value
        .get("maxRows")
        .map(|v| {
            v.as_u64()
                .filter(|n| (1..=100).contains(n))
                .ok_or(Error::Stale)
        })
        .transpose()?
        .unwrap_or(25) as u32;
    Ok(Request {
        url,
        username: bounded(jdbc.get("username"), 1024)?,
        password: bounded(jdbc.get("password"), 4096)?,
        driver: driver.into(),
        query,
        parameters,
        max_rows,
        mutation,
    })
}
fn input_line(text: &str) -> String {
    let mut result = String::with_capacity(text.len() * 2 + 1);
    for b in text.bytes() {
        use std::fmt::Write;
        let _ = write!(result, "{b:02x}");
    }
    result.push('\n');
    result
}
impl Request {
    fn input(&self, proof: &str) -> Result<String, Error> {
        let max = self.max_rows.to_string();
        let mut result = String::new();
        for text in [
            proof,
            &self.url,
            &self.username,
            &self.password,
            &self.query,
            &self.driver,
            &max,
        ] {
            result.push_str(&input_line(text));
        }
        for (name, kind, value) in &self.parameters {
            for text in [name, kind, value] {
                result.push_str(&input_line(text));
            }
        }
        if result.len() > 64 << 10 {
            return Err("Query request exceeds64KiB stdin budget.".into());
        }
        Ok(result)
    }
    fn review(&self) -> String {
        if let Some(statement) = &self.mutation {
            return format!(
                "MUTATION DRY-RUN (the statement below is NOT executed; commit is unavailable: the pinned adapter is read-only and always rolls back)\nOffline H2 target: {}\nStatement:\n{}\nParameter values (WHERE only):\n{}\nRow-count preview runs this SELECT:\n{}",
                self.url,
                ui_kit::prompts::readable_source(statement),
                self.parameters
                    .iter()
                    .map(|(n, t, v)| format!("{n}: {t} = {}", ui_kit::prompts::readable_source(v)))
                    .collect::<Vec<_>>()
                    .join("\n"),
                ui_kit::prompts::readable_source(&self.query)
            );
        }
        format!(
            "Exact manual JDBC target: {}\nDriver: {}\nCredentials: separate profile fields, hidden from this view/arguments\nIntegration qualified with H2 2.2.224; other supplied driver versions/servers are unverified.\nSELECT (max{}rows;15sec driver query timeout):\n{}\nTyped parameters:\n{}",
            self.url,
            self.driver,
            self.max_rows,
            ui_kit::prompts::readable_source(&self.query),
            self.parameters
                .iter()
                .map(|(n, t, v)| format!("{n}: {t} = {}", ui_kit::prompts::readable_source(v)))
                .collect::<Vec<_>>()
                .join("\n")
        )
    }
}
struct Session {
    plan: Plan,
    proof: String,
    directory: String,
    helper: String,
    child: Option<host::Child>,
    stdout: String,
    over: bool,
    status: String,
    rows: Vec<Vec<String>>,
    started: Instant,
}
#[derive(Default)]
pub struct Console {
    session: Option<Session>,
}
impl Console {
    pub fn run(&mut self, id: &str, ctx: &Invocation) -> Result<(), Error> {
        let project = ctx.project().ok_or("Open the owning project first.")?;
        let root = project.root()?;
        match id {
            "jpql-console-run" => {
                if self.session.as_ref().is_some_and(|s| s.child.is_some()) {
                    return Err(
                        "A query is running; inspect/cancel its owned process first.".into(),
                    );
                }
                let path = host::input(
                    "Saved explicit JDBC/compiled-model/SELECT profile (project-relative)",
                    Some(".openidea/jpql-console.json"),
                )?
                .ok_or(Error::Cancelled)?;
                let plan = read_plan(&project, &path)?;
                let (directory, helper) = helper()?;
                let proof = plan.inspect(&project, &directory, &helper)?;
                let review = format!(
                    "{}\n\nProfile: {}\nRuntime: {}\nHelper SHA256: {}\nExact inspected source/classpath fingerprints:\n{}\n\nThis connects to the specified external JDBC target. Only SELECT scalar queries are admitted; read-only/manual-flush settings and rollback are requested. Database functions, trusted project converters/types/contributors and driver code can have arbitrary side effects. Read-only is a driver hint, not a security boundary; use a database account with read-only privileges. No app main, Spring or persistence-unit bootstrap. Compilation source hashes do not prove bytecode equivalence. Raw child logs are suppressed because drivers may print credentials. Results may contain sensitive data. Driver query timeouts may be advisory; the local60sec deadline is evaluated when Status is polled. This child belongs to the plugin instance, not a durable job. Cancel kills this instance-owned process; it does not prove server cancellation/rollback. Review SELECT/params/target and trusted artifacts before connecting.",
                    plan.request.review(),
                    plan.runtime_profile.id,
                    plan.runtime_profile.version,
                    hash(HELPER),
                    proof
                );
                if !ui_kit::prompts::confirm_exact("Run reviewed JDBC JPQL SELECT", &review)? {
                    return Err(Error::Cancelled);
                }
                if ctx.project().and_then(|p| p.root().ok()).as_deref() != Some(&root)
                    || plan.inspect(&project, &directory, &helper)? != proof
                {
                    return Err(Error::Stale);
                }
                let stdin = plan.request.input(&proof)?;
                let child = host::start(
                    &plan.command(&directory, &helper, "query", None)?,
                    Some(&project),
                )?;
                if child
                    .write(&stdin)
                    .and_then(|_| child.close_stdin())
                    .is_err()
                {
                    let _ = child.kill();
                    return Err(
                        "Failed to deliver reviewed query input; owned child terminated.".into(),
                    );
                }
                self.session = Some(Session {
                    plan,
                    proof,
                    directory,
                    helper,
                    child: Some(child),
                    stdout: String::new(),
                    over: false,
                    status:
                        "Running; Refresh Status polls bounded output. Raw driver logs suppressed."
                            .into(),
                    rows: vec![],
                    started: Instant::now(),
                });
                host::reveal_view(VIEW)?;
            }
            "jpql-console-status" => self.poll(&project)?,
            "jpql-console-cancel" => {
                let session = self
                    .session
                    .as_mut()
                    .filter(|s| s.plan.root == root)
                    .ok_or(Error::Stale)?;
                let child = session.child.as_ref().ok_or("No owned query is running.")?;
                child.kill()?;
                session.child = None;
                session.stdout.clear();
                session.rows.clear();
                session.status =
                    "Owned query process cancelled; server outcome/rollback not proved.".into();
            }
            _ => return Err(Error::Stale),
        }
        host::refresh_view(VIEW);
        Ok(())
    }
    fn poll(&mut self, project: &Project) -> Result<(), Error> {
        let root = project.root()?;
        let session = self
            .session
            .as_mut()
            .filter(|s| s.plan.root == root)
            .ok_or(Error::Stale)?;
        let Some(child) = session.child.as_ref() else {
            return Ok(());
        };
        if session
            .plan
            .current(project)
            .and_then(|_| session.plan.validate(project, &session.proof))
            .is_err()
        {
            let _ = child.kill();
            session.child = None;
            session.stdout.clear();
            session.rows.clear();
            session.status =
                "Profile/source ownership changed; child cancelled, result discarded.".into();
            return Err(Error::Stale);
        }
        if session.started.elapsed() > Duration::from_secs(60) {
            let _ = child.kill();
            session.child = None;
            session.stdout.clear();
            session.status =
                "Local60sec deadline exceeded; process terminated. Server outcome unknown.".into();
            return Ok(());
        }
        let chunk = match child.poll(16 << 10) {
            Ok(c) => c,
            Err(_) => {
                session.child = None;
                session.stdout.clear();
                session.status = "Process status unavailable; result discarded.".into();
                return Ok(());
            }
        };
        if session.stdout.len() + chunk.stdout.len() > 2 * CAP {
            session.over = true;
            session.stdout.clear();
        } else if !session.over {
            session.stdout.push_str(&chunk.stdout);
        }
        if let Some(status) = chunk.status {
            if !chunk.stdout.is_empty() || !chunk.stderr.is_empty() {
                session.status = "Exited; Refresh Status to drain bounded result.".into();
                return Ok(());
            }
            session.child = None;
            if status != 0 || session.over {
                session.stdout.clear();
                session.status =
                    "Query failed or exceeded output budget; raw driver/provider logs suppressed."
                        .into();
                return Ok(());
            }
            if session
                .plan
                .inspect(project, &session.directory, &session.helper)
                .as_deref()
                != Ok(session.proof.as_str())
            {
                session.stdout.clear();
                session.status = "Source/classpath drift; result discarded.".into();
                return Err(Error::Stale);
            }
            let captured = std::mem::take(&mut session.stdout);
            session.status = "Result protocol validation incomplete; no rows accepted.".into();
            let parsed = json(payload(&captured)?)?;
            if parsed.get("proof").and_then(Value::as_str) != Some(&session.proof) {
                return Err(Error::Stale);
            }
            let result = parsed.get("result").ok_or(Error::Stale)?;
            let rows = decode_rows(result, session.plan.request.max_rows)?;
            session.rows = rows;
            session.status = format!(
                "Finished SELECT; rollback requested. {}{}rows; raw logs suppressed.",
                session.rows.len(),
                if result.get("truncated").and_then(Value::as_bool) == Some(true) {
                    "+ (truncated) "
                } else {
                    " "
                }
            );
        }
        Ok(())
    }
    pub fn view(&self, root: &str) -> View {
        let Some(s) = self.session.as_ref().filter(|s| s.plan.root == root) else {
            return View::message(
                "Run a reviewed manual JDBC JPQL SELECT for this project; compiled model and explicit credentials/profile required.",
            ).button(Button::new("jpql-console-run","Run SELECT…").icon("run").enabled(!self.session.as_ref().is_some_and(|s|s.child.is_some())));
        };
        if s.started.elapsed() > Duration::from_secs(15 * 60)
            || host::project_of(root).is_none_or(|project| {
                s.plan
                    .current(&project)
                    .and_then(|_| s.plan.validate(&project, &s.proof))
                    .is_err()
            })
        {
            return View::message(
                "Source/profile ownership changed or results expired; run a fresh reviewed query.",
            )
            .button(
                Button::new("jpql-console-cancel", "Cancel Owned Query").enabled(s.child.is_some()),
            )
            .button(
                Button::new("jpql-console-run", "Run SELECT…")
                    .icon("run")
                    .enabled(s.child.is_none()),
            );
        }
        let columns = s
            .rows
            .first()
            .map(|r| (1..=r.len()).map(|i| format!("Projection{i}")).collect())
            .unwrap_or_default();
        View::new(Body::Table(Table {
            columns,
            total_rows: s.rows.len() as u32,
            first_row: 0,
            rows: s.rows.clone(),
            editable: false,
        }))
        .status(&format!(
            "{} · {} · {}",
            s.status, s.plan.runtime_profile.id, s.plan.request.url
        ))
        .button(
            Button::new("jpql-console-run", "Run SELECT…")
                .icon("run")
                .enabled(s.child.is_none()),
        )
        .button(Button::new("jpql-console-status", "Refresh Status").enabled(s.child.is_some()))
        .button(Button::new("jpql-console-cancel", "Cancel Owned Query").enabled(s.child.is_some()))
    }
    pub fn deactivate(&mut self) {
        if let Some(s) = &mut self.session {
            if let Some(child) = s.child.take() {
                let _ = child.kill();
            }
            s.stdout.clear();
            s.rows.clear();
        }
    }
}
fn decode_rows(value: &Value, max: u32) -> Result<Vec<Vec<String>>, Error> {
    if value.get("rollbackRequested").and_then(Value::as_bool) != Some(true)
        || value.get("truncated").and_then(Value::as_bool).is_none()
    {
        return Err(Error::Stale);
    }
    let mut count = None;
    let mut rows = vec![];
    for row in value
        .get("rows")
        .and_then(Value::as_array)
        .filter(|v| v.len() <= max as usize)
        .ok_or(Error::Stale)?
    {
        let cells = row
            .as_array()
            .filter(|v| !v.is_empty() && v.len() <= 32)
            .ok_or(Error::Stale)?;
        if count.is_some_and(|n| n != cells.len()) {
            return Err(Error::Stale);
        }
        count = Some(cells.len());
        let mut result = vec![];
        for cell in cells {
            let kind = bounded(cell.get("type"), 128)?;
            let text = if kind == "null" && cell.get("value") == Some(&Value::Null) {
                "NULL".into()
            } else {
                let mut text = bounded(cell.get("value"), 8192)?;
                if cell.get("truncated").and_then(Value::as_bool) == Some(true) {
                    text.push_str("… [cell truncated]")
                }
                format!("{text} ({kind})")
            };
            result.push(text);
        }
        rows.push(result);
    }
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;
    fn fixture(id: &str) -> (MockHost, Value, String) {
        let host = MockHost::install().with_capabilities(&["project.read", "process:java"]);
        let runtime = runtime_profile(id).unwrap();
        let lock = json(runtime.lock).unwrap();
        let jars: Vec<_> = lock
            .as_object()
            .unwrap()
            .keys()
            .map(|name| format!("runtime/{name}"))
            .collect();
        let profile = serde_json::json!({"profile":id,"dialect":"h2","runtimeJars":jars,"driverJars":{"runtime/jdbc.jar":"d".repeat(64)},"classRoots":["classes"],"entities":[{"class":"example.Customer","source":"src/Customer.java","classRoot":"classes"}],"jdbc":{"url":"jdbc:h2:/disposable/fixture","username":"sa","password":"private-password"},"query":"select c.name from Customer c where c.id=:id","parameters":[{"name":"id","type":"long","value":"1"}],"maxRows":2});
        host.add_file("profile.json", &profile.to_string());
        host.add_file(
            "src/Customer.java",
            "package example; @Entity class Customer {}",
        );
        host.add_dir("classes");
        host.add_file("classes/example/Customer.class", "mock bytecode");
        for jar in &jars {
            host.add_file(jar, "mock runtime artifact")
        }
        host.add_file("runtime/jdbc.jar", "mock driver");
        let plan = read_plan(&host.invocation().project().unwrap(), "profile.json").unwrap();
        let mut facts: Vec<_> = jars
            .iter()
            .map(|jar| {
                format!(
                    "jar:{jar}:{}",
                    lock[jar.rsplit('/').next().unwrap()].as_str().unwrap()
                )
            })
            .collect();
        facts.push(format!("jar:runtime/jdbc.jar:{}", "d".repeat(64)));
        facts.push(format!("classes:classes:{}", "a".repeat(64)));
        for source in &plan.sources {
            facts.push(format!(
                "source:{source}:{}",
                hash(
                    &host
                        .invocation()
                        .project()
                        .unwrap()
                        .read_file(source)
                        .unwrap()
                )
            ))
        }
        facts.push("entity:example.Customer|classes".into());
        let proof=serde_json::json!({"profile":id,"version":runtime.version,"dialect":"org.hibernate.dialect.H2Dialect","java":"26.0.1","fingerprint":"b".repeat(64),"facts":facts,"files":22,"bytes":3000}).to_string();
        let output = proof.clone();
        host.on_process("java", move |args, _| {
            assert_eq!(args[4], "inspect");
            Ok(openidea_plugin::ProcessOutput {
                status: Some(0),
                stdout: format!("OPENIDEA_JPQL_CONSOLE/1\n{output}"),
                stderr: String::new(),
            })
        });
        (host, profile, proof)
    }
    fn start(console: &mut Console, host: &MockHost) {
        host.answer_next_prompt(Some(&["profile.json"]));
        host.answer_next_prompt(Some(&[]));
        console.run("jpql-console-run", &host.invocation()).unwrap();
    }
    #[test]
    fn console_requires_approval_and_never_places_credentials_or_query_in_argv() {
        for id in [PROFILE6, PROFILE7] {
            let (host, _, proof) = fixture(id);
            let mut console = Console::default();
            host.answer_next_prompt(Some(&["profile.json"]));
            host.answer_next_prompt(None);
            assert!(console.run("jpql-console-run", &host.invocation()).is_err());
            assert!(console.session.is_none());
            assert_eq!(host.process_runs().len(), 1);
            start(&mut console, &host);
            let session = console.session.as_ref().unwrap();
            assert_eq!(session.proof, proof);
            let command = session
                .plan
                .command(&session.directory, &session.helper, "query", None)
                .unwrap();
            assert!(!format!("{:?}", command.args).contains("private-password"));
            assert!(!command.args.iter().any(|arg| arg.contains("select c.name")));
            assert!(command.env.iter().all(|(_, v)| v.is_empty()));
            assert!(!format!("{:?}", console.view("/project")).contains("private-password"));
            assert_eq!(host.revealed_views(), [VIEW]);
            console.deactivate();
        }
    }
    #[test]
    fn managed_query_busy_foreign_cancel_and_deactivation_are_instance_owned() {
        let (host, _, _) = fixture(PROFILE6);
        let mut console = Console::default();
        start(&mut console, &host);
        assert!(console.run("jpql-console-run", &host.invocation()).is_err());
        host.add_project("/foreign");
        host.set_project_root(Some("/foreign"));
        assert!(
            console
                .run("jpql-console-cancel", &host.invocation())
                .is_err()
        );
        assert!(matches!(console.view("/foreign").body, Body::Message(_)));
        host.set_project_root(Some("/project"));
        console
            .run("jpql-console-cancel", &host.invocation())
            .unwrap();
        assert!(console.session.as_ref().unwrap().child.is_none());
        assert!(
            console
                .session
                .as_ref()
                .unwrap()
                .status
                .contains("not proved")
        );
        start(&mut console, &host);
        console.deactivate();
        assert!(console.session.as_ref().unwrap().child.is_none());
    }
    #[test]
    fn source_or_profile_drift_discards_cached_rows_and_cancels_owned_child() {
        for source in ["profile.json", "src/Customer.java"] {
            let (host, _, _) = fixture(PROFILE6);
            let mut console = Console::default();
            start(&mut console, &host);
            console.session.as_mut().unwrap().rows = vec![vec!["private-result".into()]];
            host.add_file(source, "changed");
            assert!(!format!("{:?}", console.view("/project")).contains("private-result"));
            assert!(
                console
                    .run("jpql-console-status", &host.invocation())
                    .is_err()
            );
            let session = console.session.as_ref().unwrap();
            assert!(session.child.is_none());
            assert!(session.rows.is_empty());
        }
    }
    #[test]
    fn profiles_refuse_mutations_url_options_duplicate_keys_and_unpinned_drivers() {
        let (host, original, proof) = fixture(PROFILE6);
        let project = host.invocation().project().unwrap();
        for (key, value) in [
            ("query", serde_json::json!("delete from Customer")),
            ("maxRows", serde_json::json!(101)),
            ("dialect", serde_json::json!("oracle")),
        ] {
            let mut changed = original.clone();
            changed[key] = value;
            host.add_file("profile.json", &changed.to_string());
            assert!(read_plan(&project, "profile.json").is_err());
        }
        for url in [
            "jdbc:h2:/db;INIT=RUNSCRIPT FROM 'evil'",
            "jdbc:h2:/db?password=secret",
            "jdbc:postgresql://user:pass@host/db",
        ] {
            let mut changed = original.clone();
            changed["jdbc"]["url"] = url.into();
            host.add_file("profile.json", &changed.to_string());
            assert!(read_plan(&project, "profile.json").is_err());
        }
        assert!(json("{\"query\":1,\"query\":2}").is_err());
        host.add_file("profile.json", &original.to_string());
        let plan = read_plan(&project, "profile.json").unwrap();
        let mut parsed = json(&proof).unwrap();
        parsed["facts"][plan.runtime] = format!("jar:runtime/jdbc.jar:{}", "e".repeat(64)).into();
        assert!(plan.validate(&project, &parsed.to_string()).is_err());
    }
    #[test]
    fn mutation_analysis_builds_count_and_refuses_unsafe_statements() {
        assert_eq!(
            mutation_count_query("delete from Customer c where c.id = :id").unwrap(),
            "select count(*) from Customer c where c.id = :id"
        );
        assert_eq!(
            mutation_count_query("update Customer as c set c.name = :n where c.id = :id").unwrap(),
            "select count(*) from Customer c where c.id = :id"
        );
        assert_eq!(
            mutation_count_query("delete from Customer").unwrap(),
            "select count(*) from Customer"
        );
        for bad in [
            "delete from Customer; delete from Order",
            "delete from Customer -- x",
            "delete from Customer /* x */",
            "update versioned Customer set name = 'a'",
            "update Customer c where c.id = 1",
            "delete from Customer where function('sql', x) = 1",
            "select c from Customer c",
            "delete from Customer where",
        ] {
            assert!(mutation_count_query(bad).is_err(), "{bad}");
        }
    }
    #[test]
    fn mutation_preview_requires_setting_and_offline_h2_and_never_executes() {
        let (host, original, _) = fixture(PROFILE6);
        let project = host.invocation().project().unwrap();
        let mut changed = original.clone();
        changed["query"] = "update Customer c set c.name = :n where c.id = :id".into();
        changed["parameters"] = serde_json::json!([
            {"name":"n","type":"string","value":"x"},
            {"name":"id","type":"long","value":"1"}
        ]);
        host.add_file("profile.json", &changed.to_string());
        assert!(read_plan(&project, "profile.json").is_err());
        host.set_setting(MUTATION_SETTING, "true");
        let plan = read_plan(&project, "profile.json").unwrap();
        assert_eq!(plan.request.query, "select count(*) from Customer c where c.id = :id");
        assert_eq!(plan.request.parameters.len(), 1);
        let review = plan.request.review();
        assert!(review.contains("NOT executed") && review.contains("jdbc:h2:/disposable/fixture"));
        assert!(review.contains("c.name = :n"));
        for url in ["jdbc:h2:tcp://host/db", "jdbc:h2:ssl://host/db"] {
            changed["jdbc"]["url"] = url.into();
            host.add_file("profile.json", &changed.to_string());
            assert!(read_plan(&project, "profile.json").is_err());
        }
        changed["jdbc"]["url"] = "jdbc:postgresql://host/db".into();
        changed["dialect"] = "postgresql".into();
        host.add_file("profile.json", &changed.to_string());
        assert!(read_plan(&project, "profile.json").is_err());
    }
    #[test]
    fn typed_result_decoder_enforces_bounds_and_stdin_keeps_exact_reviewed_bytes() {
        let (host, _, _) = fixture(PROFILE6);
        let plan = read_plan(&host.invocation().project().unwrap(), "profile.json").unwrap();
        let input = plan.request.input("exact-proof").unwrap();
        assert!(!input.contains("private-password"));
        assert!(input.contains(&input_line("private-password")));
        let row = serde_json::json!({"rows":[[{"type":"java.lang.String","value":"Alice","truncated":false},{"type":"null","value":null}]],"rollbackRequested":true,"truncated":false});
        assert_eq!(
            decode_rows(&row, 2).unwrap()[0],
            ["Alice (java.lang.String)", "NULL"]
        );
        assert!(decode_rows(&row, 0).is_err());
        let mut invalid = row.clone();
        invalid["rollbackRequested"] = false.into();
        assert!(decode_rows(&invalid, 2).is_err());
        invalid = row.clone();
        invalid["rows"][0][0]["value"] = "x".repeat(8193).into();
        assert!(decode_rows(&invalid, 2).is_err());
    }
    #[test]
    fn classpath_proof_change_after_review_never_starts_a_query() {
        use std::cell::Cell;
        use std::rc::Rc;
        let (host, _, proof) = fixture(PROFILE6);
        let counter = Rc::new(Cell::new(0));
        let calls = counter.clone();
        host.on_process("java", move |_, _| {
            let mut parsed = json(&proof).unwrap();
            if calls.get() > 0 {
                parsed["fingerprint"] = "c".repeat(64).into();
            }
            calls.set(calls.get() + 1);
            Ok(openidea_plugin::ProcessOutput {
                status: Some(0),
                stdout: format!("OPENIDEA_JPQL_CONSOLE/1\n{parsed}"),
                stderr: String::new(),
            })
        });
        let mut console = Console::default();
        host.answer_next_prompt(Some(&["profile.json"]));
        host.answer_next_prompt(Some(&[]));
        assert!(console.run("jpql-console-run", &host.invocation()).is_err());
        assert!(console.session.is_none());
        assert_eq!(counter.get(), 2);
    }
    #[test]
    fn completed_native_results_require_drained_matching_protocol_and_fresh_proof() {
        for drift in [false, true] {
            let (host, _, proof) = fixture(PROFILE6);
            let original = proof.clone();
            let counter = std::rc::Rc::new(std::cell::Cell::new(0));
            let calls = counter.clone();
            host.on_process("java", move |_, _| {
                let mut parsed = json(&original).unwrap();
                if drift && calls.get() >= 2 {
                    parsed["fingerprint"] = "c".repeat(64).into();
                }
                calls.set(calls.get() + 1);
                Ok(openidea_plugin::ProcessOutput {
                    status: Some(0),
                    stdout: format!("OPENIDEA_JPQL_CONSOLE/1\n{parsed}"),
                    stderr: String::new(),
                })
            });
            let mut console = Console::default();
            start(&mut console, &host);
            let session = console.session.as_mut().unwrap();
            let _ = session.child.as_ref().unwrap().wait(1).unwrap();
            session.stdout = format!(
                "sensitive prelude not rendered\nOPENIDEA_JPQL_CONSOLE/1\n{}",
                serde_json::json!({"proof":proof,"result":{"rows":[[{"type":"java.lang.String","value":"Alice","truncated":false}]],"truncated":false,"rollbackRequested":true}})
            );
            let result = console.run("jpql-console-status", &host.invocation());
            if drift {
                assert!(result.is_err());
                assert!(console.session.as_ref().unwrap().rows.is_empty());
            } else {
                result.unwrap();
                assert_eq!(
                    console.session.as_ref().unwrap().rows[0],
                    ["Alice (java.lang.String)"]
                );
                assert!(matches!(console.view("/project").body, Body::Table(_)));
            }
            assert!(console.session.as_ref().unwrap().child.is_none());
            assert!(!format!("{:?}", console.view("/project")).contains("sensitive prelude"));
        }
    }
}
