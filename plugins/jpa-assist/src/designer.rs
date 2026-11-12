//! Explicit current-source inspector. No compiler, XML or database claims.
use crate::{JpaAssist, attribute_info, file_actions::reverse_panel, model};
use openidea_plugin::{
    Error, Invocation, Range, TextEdit, host,
    providers::Document,
    text::{offset, position},
    views::ViewEvent,
};
use persistence_core::{
    ColumnChange, ColumnProperty, ColumnValue, Language, SourceEdit, SourceIdentity,
};
use serde::{Deserialize, Serialize};
use persistence_core::schema::{self, Danger};
use persistence_core::sql::{self, Dialect, EmitOptions, RenameDecision};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};

pub(super) const VIEW: &str = "designer";
const CHANNEL: &str = "jpa-designer";
const MAX_REQUEST: usize = 32 * 1024;

fn editor_document(ctx: &Invocation) -> Result<Document, Error> {
    let editor = ctx.editor().ok_or(Error::Stale)?;
    let current = editor.document()?;
    Ok(Document {
        path: editor.path(),
        language: editor.language(),
        text: current.text,
        version: current.version,
    })
}

#[derive(Default)]
pub(super) struct State {
    document: Option<Document>,
    root: Option<String>,
    selected: Option<String>,
    pending: Option<Prepared>,
    next: u64,
    frames: HashMap<String, (String, u64)>,
}
struct Prepared {
    token: String,
    document: Document,
    edits: Vec<TextEdit>,
    after: String,
    creation: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    channel: String,
    protocol: u32,
    frame: String,
    sequence: u64,
    root: Option<String>,
    context: Option<RequestContext>,
    operation: Operation,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RequestContext {
    path: String,
    version: u64,
    attribute: Option<String>,
}
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
enum Operation {
    Snapshot,
    Inspect,
    BindCreation,
    PrepareClass {
        draft: persistence_core::generation::ManagedClassDraft,
    },
    PrepareAttribute {
        draft: AttributeDraft,
    },
    PrepareRelationship {
        changes: Vec<crate::relationship_edit::RelationshipChange>,
    },
    Select {
        attribute: String,
    },
    Prepare {
        changes: Vec<ColumnChange>,
    },
    Apply {
        token: String,
    },
    Reset,
    PlanMigration {
        #[serde(default)]
        routes: Option<Vec<RouteDraft>>,
    },
    ReverseSources,
    ReverseInventory {
        path: String,
        dialect: Option<String>,
        schema: Option<String>,
        table: Option<String>,
    },
    GenerateMigration {
        dialect: String,
        #[serde(rename = "includeDangers")]
        include_dangers: Vec<String>,
        #[serde(rename = "renameDecisions")]
        rename_decisions: Vec<RenameDraft>,
        #[serde(default)]
        backfills: Vec<BackfillDraft>,
        description: String,
    },
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RouteDraft {
    change_type: String,
    location: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BackfillDraft {
    table: String,
    column: String,
    value: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RenameDraft {
    table: String,
    from: String,
    to: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct JoinTableDraft {
    name: String,
    join_column: String,
    inverse_column: String,
}

/// Typed form of the Add attribute page. Every free-text field is re-validated before any source is rendered.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AttributeDraft {
    name: String,
    kind: String,
    #[serde(rename = "type")]
    ty: String,
    enumerated: bool,
    not_null: bool,
    unique: bool,
    cascade: Vec<String>,
    fetch: Option<String>,
    orphan_removal: bool,
    order_by: Option<String>,
    order_column: Option<String>,
    join_column: Option<String>,
    join_column_not_null: bool,
    join_table: Option<JoinTableDraft>,
}

fn attribute_spec(draft: AttributeDraft) -> Result<crate::wizard::AttributeSpec, Error> {
    use crate::{
        model::Relation,
        wizard::{AttributeSpec, JoinTable, RelationOptions},
    };
    let relation = match draft.kind.as_str() {
        "none" | "transient" => Relation::None,
        "ManyToOne" => Relation::ManyToOne,
        "OneToMany" => Relation::OneToMany,
        "OneToOne" => Relation::OneToOne,
        "ManyToMany" => Relation::ManyToMany,
        "Embedded" => Relation::Embedded,
        "ElementCollection" => Relation::ElementCollection,
        _ => return Err("Choose a supported attribute kind.".into()),
    };
    let transient = draft.kind == "transient";
    if !crate::file_actions::identifier(&draft.name) {
        return Err("Choose a valid attribute name.".into());
    }
    let basic = matches!(relation, Relation::None | Relation::ElementCollection);
    if !crate::file_actions::identifier(&draft.ty) {
        return Err("Choose a valid type name.".into());
    }
    if draft.enumerated && !(basic && !transient && relation == Relation::None) {
        return Err("Only a basic attribute can be an enumeration.".into());
    }
    Ok(AttributeSpec {
        name: model::decapitalize(&draft.name),
        transient,
        ty: if basic { draft.ty } else { model::capitalize(&draft.ty) },
        relation,
        enumerated: draft.enumerated,
        not_null: draft.not_null,
        unique: draft.unique,
        options: RelationOptions {
            cascade: draft.cascade,
            fetch: draft.fetch,
            orphan_removal: draft.orphan_removal,
            order_by: draft.order_by,
            order_column: draft.order_column,
            join_column: draft.join_column,
            join_column_not_null: draft.join_column_not_null,
            join_table: draft.join_table.map(|t| JoinTable {
                name: t.name,
                join_column: t.join_column,
                inverse_column: t.inverse_column,
            }),
        },
        mapped_by: None,
        imports: Vec::new(),
    })
}

#[derive(Serialize)]
struct Property {
    key: ColumnProperty,
    label: &'static str,
    value: ColumnValue,
    origin: &'static str,
    editable: bool,
    detail: Option<String>,
}
struct Field {
    class: model::Class,
    start: usize,
    end: usize,
    scalar: bool,
    namespace: &'static str,
}

/// The parsed class, its masked text and the byte range of member `name`.
pub(crate) fn locate(
    doc: &Document,
    name: &str,
) -> Result<(model::Class, String, usize, usize, usize), Error> {
    let class = attribute_info::snapshot(doc).ok_or(
        "Inspect a complete source-owned persistence class with explicit annotation imports.",
    )?;
    if class.superclass.is_some() {
        return Err("Inherited mappings require a resolved project model.".into());
    }
    let masked = model::mask(&doc.text);
    let at = offset(&doc.text, class.pos).ok_or(Error::Stale)?;
    let open = at + masked[at..].find('{').ok_or(Error::Stale)?;
    if masked[at..open].trim() != class.name {
        return Err(
            "Generic or inherited persistence contexts require semantic resolution.".into(),
        );
    }
    let index = class
        .attributes
        .iter()
        .position(|a| a.name == name)
        .ok_or(Error::Stale)?;
    let member_at = offset(&doc.text, class.attributes[index].pos).ok_or(Error::Stale)?;
    let (start, end) = model::members(
        &masked,
        open + 1,
        class.body_end.ok_or(Error::Stale)?,
        class.kotlin,
        false,
    )
    .into_iter()
    .find(|(a, b)| *a <= member_at && member_at < *b)
    .ok_or(Error::Stale)?;
    Ok((class, masked, index, start, end))
}

fn field(doc: &Document, name: &str) -> Result<Field, Error> {
    let (class, masked, index, start, end) = locate(doc, name)?;
    let attribute = &class.attributes[index];
    let declaration = model::without_annotations(&masked, start, end);
    if declaration.contains(['(', '{', '}', '=', ',', '<', '>'])
        || ["static", "transient", "final"]
            .iter()
            .any(|w| model::has_word(&declaration, w))
    {
        return Err("This field shape is read-only in the source-local inspector.".into());
    }
    let namespace = if model::annotations(&masked).iter().any(|a| {
        masked[a.start + 1..a.name_end].starts_with("javax.persistence.")
            || class
                .imports
                .iter()
                .any(|i| i == &format!("javax.persistence.{}", a.name))
    }) {
        "javax.persistence"
    } else {
        "jakarta.persistence"
    };
    let ty = attribute.ty.as_str();
    let primitive = [
        "boolean", "byte", "short", "int", "long", "float", "double", "char", "byte[]",
    ]
    .contains(&ty);
    let full = if ty.contains('.') {
        Some(ty)
    } else {
        let imports: Vec<_> = class
            .imports
            .iter()
            .filter(|i| i.rsplit('.').next() == Some(ty))
            .collect();
        match imports.as_slice() {
            [one] => Some(one.as_str()),
            _ => None,
        }
    };
    let basic = primitive
        || full.is_some_and(|f| {
            f.split('.').next().is_some_and(|prefix| {
                prefix != class.name
                    && !class
                        .imports
                        .iter()
                        .any(|i| i.rsplit('.').next() == Some(prefix))
            }) && ty != class.name
                && matches!(
                    f,
                    "java.lang.String"
                        | "java.lang.Long"
                        | "java.lang.Integer"
                        | "java.lang.Short"
                        | "java.lang.Byte"
                        | "java.lang.Boolean"
                        | "java.lang.Float"
                        | "java.lang.Double"
                        | "java.lang.Character"
                        | "java.math.BigDecimal"
                        | "java.math.BigInteger"
                        | "java.util.UUID"
                        | "java.util.Date"
                        | "java.time.LocalDate"
                        | "java.time.LocalDateTime"
                        | "java.time.Instant"
                        | "java.time.LocalTime"
                        | "java.time.OffsetDateTime"
                        | "java.time.ZonedDateTime"
                )
        });
    let mapping_subset = model::annotations(&masked)
        .iter()
        .filter(|a| start <= a.start && a.start < end)
        .all(|a| {
            matches!(
                a.name,
                "Id" | "Column" | "Basic" | "Version" | "Temporal" | "Enumerated" | "Lob"
            )
        });
    let scalar =
        !class.kotlin && attribute.relation == model::Relation::None && basic && mapping_subset;
    Ok(Field {
        class,
        start,
        end,
        scalar,
        namespace,
    })
}

fn column<'a>(masked: &'a str, field: &Field) -> Result<Option<model::Annotation<'a>>, Error> {
    let annotations: Vec<_> = model::annotations(masked)
        .into_iter()
        .filter(|a| a.name == "Column" && field.start <= a.start && a.start < field.end)
        .collect();
    match annotations.len() {
        0 => Ok(None),
        1 => Ok(annotations.into_iter().next()),
        _ => Err("Duplicate column mappings cannot be edited.".into()),
    }
}

fn property_default(property: ColumnProperty) -> ColumnValue {
    use ColumnProperty::*;
    match property {
        Name => ColumnValue::Text(String::new()),
        Nullable | Insertable | Updatable => ColumnValue::Boolean(true),
        Unique => ColumnValue::Boolean(false),
        Length => ColumnValue::Number(255),
        Precision | Scale => ColumnValue::Number(0),
    }
}

fn properties(doc: &Document, name: &str) -> Result<Vec<Property>, Error> {
    let field = field(doc, name)?;
    let masked = model::mask(&doc.text);
    let column = column(&masked, &field)?;
    let mut out = Vec::new();
    for (key, label) in [
        (ColumnProperty::Name, "Column name"),
        (ColumnProperty::Nullable, "Nullable"),
        (ColumnProperty::Unique, "Unique"),
        (ColumnProperty::Insertable, "Insertable"),
        (ColumnProperty::Updatable, "Updatable"),
        (ColumnProperty::Length, "Length"),
        (ColumnProperty::Precision, "Precision"),
        (ColumnProperty::Scale, "Scale"),
    ] {
        let raw = column
            .as_ref()
            .and_then(|c| model::arg(&doc.text, &masked, c, key.argument()));
        let literal = raw.and_then(|s| match key {
            ColumnProperty::Name => s
                .strip_prefix('"')
                .and_then(|s| s.strip_suffix('"'))
                .filter(|s| !s.contains(['"', '\\']))
                .map(|s| ColumnValue::Text(s.into())),
            ColumnProperty::Nullable
            | ColumnProperty::Unique
            | ColumnProperty::Insertable
            | ColumnProperty::Updatable => match s {
                "true" => Some(ColumnValue::Boolean(true)),
                "false" => Some(ColumnValue::Boolean(false)),
                _ => None,
            },
            _ => s
                .parse::<u32>()
                .ok()
                .filter(|n| *n <= i32::MAX as u32)
                .map(ColumnValue::Number),
        });
        let attribute = field
            .class
            .attributes
            .iter()
            .find(|a| a.name == name)
            .ok_or(Error::Stale)?;
        let ty = attribute.ty.rsplit('.').next().unwrap_or(&attribute.ty);
        let type_support = match key {
            ColumnProperty::Length => matches!(ty, "String" | "byte[]"),
            ColumnProperty::Precision | ColumnProperty::Scale => ty == "BigDecimal",
            _ => true,
        };
        let resolved = raw.is_none() || literal.is_some();
        out.push(Property {
            key,
            label,
            value: literal.unwrap_or_else(|| property_default(key)),
            origin: if !resolved {
                "unresolved"
            } else if raw.is_some() {
                "explicit"
            } else {
                "annotation-default"
            },
            editable: field.scalar && type_support && resolved,
            detail: if !resolved {
                raw.map(str::to_owned)
            } else if !field.scalar {
                Some(
                    "A plain Java field with a proved scalar source type is required for editing."
                        .into(),
                )
            } else {
                None
            },
        });
    }
    Ok(out)
}

/// Replaces only selected literal argument spans. Unknown arguments, comments,
/// unrelated annotations and physical names survive exactly.
fn prepare(
    doc: &Document,
    name: &str,
    changes: &[ColumnChange],
) -> Result<(Vec<TextEdit>, String), Error> {
    if changes.is_empty() || changes.len() > 8 {
        return Err("Change one or more supported column properties.".into());
    }
    let field = field(doc, name)?;
    if !field.scalar {
        return Err("This field requires semantic type resolution before editing.".into());
    }
    let supported = properties(doc, name)?;
    let masked = model::mask(&doc.text);
    let col = column(&masked, &field)?;
    let mut seen = HashSet::new();
    let mut edits = Vec::new();
    let mut additions = Vec::new();
    for change in changes {
        let key = change.property.argument();
        if !seen.insert(key)
            || !supported
                .iter()
                .any(|p| p.key == change.property && p.editable)
        {
            return Err(
                "A property is duplicated or cannot be edited in this source context.".into(),
            );
        }
        let literal = change.literal().map_err(Error::from)?;
        if let Some((from, to)) = col
            .as_ref()
            .and_then(|c| model::value_range(&doc.text, &masked, c, key))
        {
            edits.push(SourceEdit {
                start: from,
                end: to,
                replacement: literal,
            });
        } else {
            additions.push(format!("{key} = {literal}"));
        }
    }
    if !additions.is_empty() {
        if let Some(c) = &col {
            if let Some((a, b)) = c.args {
                // Validate argument names once so duplicate/dynamic layouts cannot
                // turn a replaced first value into an effective-mapping claim.
                let mut arguments = HashSet::new();
                for (_, part) in model::split_top_level(&masked[a..b]) {
                    if part.trim().is_empty() {
                        continue;
                    }
                    let (key, _) = part
                        .split_once('=')
                        .ok_or("Unsupported column argument syntax.")?;
                    if !arguments.insert(key.trim()) {
                        return Err("Duplicate column arguments cannot be edited.".into());
                    }
                }
                let separator = if doc.text[a..b].trim().is_empty() {
                    ""
                } else {
                    ", "
                };
                edits.push(SourceEdit {
                    start: b,
                    end: b,
                    replacement: format!("{separator}{}", additions.join(", ")),
                });
            } else {
                edits.push(SourceEdit {
                    start: c.name_end,
                    end: c.name_end,
                    replacement: format!("({})", additions.join(", ")),
                });
            }
        } else {
            let newline = if doc.text.contains("\r\n") {
                "\r\n"
            } else {
                "\n"
            };
            let start = field.start + masked[field.start..field.end].len()
                - masked[field.start..field.end].trim_start().len();
            let line = doc.text[..start].rfind('\n').map_or(0, |p| p + 1);
            let indent = &doc.text[line..start];
            let separator = if indent.chars().all(|c| matches!(c, ' ' | '\t')) {
                format!("{newline}{indent}")
            } else {
                " ".into()
            };
            edits.push(SourceEdit {
                start,
                end: start,
                replacement: format!(
                    "@{}.Column({}){separator}",
                    field.namespace,
                    additions.join(", ")
                ),
            });
        }
    }
    // Reject duplicate argument names even when no new property was added.
    if let Some((a, b)) = col.as_ref().and_then(|c| c.args) {
        let mut keys = HashSet::new();
        for (_, part) in model::split_top_level(&masked[a..b]) {
            if part.trim().is_empty() {
                continue;
            }
            let (key, _) = part
                .split_once('=')
                .ok_or("Unsupported column argument syntax.")?;
            if !keys.insert(key.trim()) {
                return Err("Duplicate column arguments cannot be edited.".into());
            }
        }
    }
    let after = persistence_core::apply_edits(&doc.text, &edits).map_err(Error::from)?;
    let updated = Document {
        text: after.clone(),
        ..doc.clone()
    };
    let final_props = properties(&updated, name)?;
    let number = |key| {
        final_props
            .iter()
            .find(|p| p.key == key)
            .and_then(|p| match p.value {
                ColumnValue::Number(n) => Some(n),
                _ => None,
            })
    };
    if let (Some(p), Some(s)) = (
        number(ColumnProperty::Precision),
        number(ColumnProperty::Scale),
    ) && p > 0
        && s > p
    {
        return Err("Scale must not exceed an explicitly positive precision.".into());
    }
    Ok((
        edits
            .iter()
            .map(|e| TextEdit {
                range: Range::new(position(&doc.text, e.start), position(&doc.text, e.end)),
                text: e.replacement.clone(),
            })
            .collect(),
        after,
    ))
}

impl State {
    fn bind_creation(&mut self, ctx: &Invocation) -> Result<(), Error> {
        if self.pending.is_some() {
            return Err("Apply or reset the prepared changes first.".into());
        }
        let doc = editor_document(ctx)?;
        let root = ctx.project().ok_or(Error::Stale)?.root()?;
        if host::project_of(&doc.path)
            .and_then(|p| p.root().ok())
            .as_deref()
            != Some(root.as_str())
        {
            return Err(Error::Stale);
        }
        crate::creation::context(&doc, &root)?;
        self.document = Some(doc);
        self.root = Some(root);
        self.selected = None;
        Ok(())
    }
    fn capture(&mut self, ctx: &Invocation) -> Result<(), Error> {
        if self.pending.is_some() {
            return Err(
                "Apply or reset the prepared changes before inspecting another source.".into(),
            );
        }
        let editor = ctx
            .editor()
            .ok_or("Open a persistence source file, then choose Inspect at Caret.")?;
        let doc = editor_document(ctx)?;
        let class = attribute_info::snapshot(&doc)
            .ok_or("This source is not a supported complete persistence declaration.")?;
        let root = ctx.project().ok_or(Error::Stale)?.root()?;
        if host::project_of(&doc.path)
            .and_then(|p| p.root().ok())
            .as_deref()
            != Some(&root)
        {
            return Err(Error::Stale);
        }
        let caret = editor
            .selections()?
            .first()
            .and_then(|r| offset(&doc.text, r.start));
        let masked = model::mask(&doc.text);
        let members = offset(&doc.text, class.pos)
            .and_then(|at| masked[at..].find('{').map(|open| at + open))
            .zip(class.body_end)
            .map(|(open, close)| model::members(&masked, open + 1, close, class.kotlin, false))
            .unwrap_or_default();
        self.selected = caret
            .and_then(|caret| {
                class
                    .attributes
                    .iter()
                    .find(|a| {
                        offset(&doc.text, a.pos).is_some_and(|at| {
                            members.iter().any(|(start, end)| {
                                *start <= at && at < *end && *start <= caret && caret < *end
                            })
                        })
                    })
                    .map(|a| a.name.clone())
            })
            .or_else(|| class.attributes.first().map(|a| a.name.clone()));
        self.root = Some(root);
        self.document = Some(doc);
        Ok(())
    }
    fn model(&self, root: &str) -> Value {
        let Some(doc) = self
            .document
            .as_ref()
            .filter(|_| self.root.as_deref() == Some(root))
        else {
            return json!({"root":root,"empty":true});
        };
        let pending=self.pending.as_ref().map(|p|json!({"token":p.token,"before":p.document.text,"after":p.after,"kind":if p.creation {"creation"} else {"column"}}));
        if let Ok(creation) = crate::creation::context(doc, root) {
            return json!({"root":root,"empty":true,"source":creation.source,"creation":creation,"selected":null,"pending":pending,"canApply":true});
        }
        let Some(class) = attribute_info::snapshot(doc) else {
            return json!({"root":root,"empty":true});
        };
        let (props, note) = match self.selected.as_deref().map(|name| properties(doc, name)) {
            Some(Ok(properties)) => (properties, None),
            Some(Err(error)) => (Vec::new(), Some(error.to_string())),
            None => (
                Vec::new(),
                Some("This source declares no supported persistent attributes.".into()),
            ),
        };
        let relationship = self
            .selected
            .as_deref()
            .and_then(|name| crate::relationship_edit::properties(doc, name).ok().flatten());
        let association = self
            .selected
            .as_deref()
            .and_then(|name| crate::relationship_edit::association(doc, name).ok().flatten());
        json!({"root":root,"empty":false,"relationship":relationship,"association":association,"source":SourceIdentity {root:root.into(),path:doc.path.clone(),language:if class.kotlin {Language::Kotlin} else {Language::Java},version:doc.version},
            "className":class.name,"role":format!("{:?}",class.kind),"attributes":class.attributes.iter().map(|a|json!({"name":a.name,"type":a.ty})).collect::<Vec<_>>(),
            "selected":self.selected,"properties":props,"inspectionNote":note,"pending":pending,"creation":null,
            "canApply":true,"provenance":"Current source declaration; inherited, compiler, XML and database mappings remain unresolved."})
    }
}

/// Visibility settings of the Designer view, read again on every reply.
fn layout() -> Value {
    let flag = |key: &str, default: bool| host::setting_bool(&format!("jpaAssist.designer.{key}")).unwrap_or(default);
    json!({"palette":flag("showPalette",true),"inspector":flag("showInspector",true),"toolbar":flag("showEditorToolbar",true),"minimal":flag("minimalMode",false)})
}

const MAX_PLAN_CHANGES: usize = 500;
const MAX_PLAN_NOTES: usize = 200;
const MAX_MIGRATION_SQL: usize = 64 * 1024;
const MAX_NAME: usize = 128;
const FILE_ACTION: &str = "Generate Flyway migration from model changes…";

fn change_summary(change: &schema::Change) -> (&'static str, String, String) {
    use schema::Change::*;
    let join = |columns: &[String]| columns.join(", ");
    let (kind, table, detail) = match change {
        CreateTable { table } => (
            "create-table",
            table.name.clone(),
            format!("{} columns", table.columns.known().map_or(0, Vec::len)),
        ),
        DropTable { table } => ("drop-table", table.clone(), String::new()),
        AddColumn { table, column } => ("add-column", table.clone(), column.name.clone()),
        DropColumn { table, column } => ("drop-column", table.clone(), column.clone()),
        AlterColumnType { table, column, .. } => {
            ("alter-column-type", table.clone(), column.clone())
        }
        AlterNullability {
            table,
            column,
            nullable,
            ..
        } => (
            "alter-nullability",
            table.clone(),
            format!("{column} -> {}", if *nullable { "NULL" } else { "NOT NULL" }),
        ),
        AddForeignKey { table, foreign_key } => (
            "add-foreign-key",
            table.clone(),
            format!("({}) -> {}", join(&foreign_key.columns), foreign_key.ref_table),
        ),
        DropForeignKey { table, foreign_key } => (
            "drop-foreign-key",
            table.clone(),
            format!("({}) -> {}", join(&foreign_key.columns), foreign_key.ref_table),
        ),
        AddUnique { table, unique } => ("add-unique", table.clone(), join(&unique.columns)),
        DropUnique { table, unique } => ("drop-unique", table.clone(), join(&unique.columns)),
        AddIndex { table, index } => ("add-index", table.clone(), join(&index.columns)),
        DropIndex { table, index } => ("drop-index", table.clone(), join(&index.columns)),
        RenameCandidate { table, from, to } => {
            ("rename-candidate", table.clone(), format!("{from} -> {to}"))
        }
    };
    (kind, table, detail)
}

/// Read-only: baseline file plus a bounded scan of the current sources. Never writes.
fn migration_inputs(
    project: &openidea_plugin::Project,
    root: &str,
) -> Result<Option<(schema::SchemaSnapshot, schema::SchemaSnapshot)>, Error> {
    use crate::file_actions::migration_action as migration;
    let path = migration::target_in_root(project, root, migration::BASELINE)?;
    let Some(text) = migration::read_optional(project, &path)? else {
        return Ok(None);
    };
    let baseline = crate::schema_snapshot::snapshot_from_json(&text)
        .map_err(|e| Error::from(format!("The schema baseline is unreadable: {e}")))?;
    let model = migration::scan(project, root)?;
    Ok(Some((baseline, model)))
}

fn plan_json(project: &openidea_plugin::Project, root: &str, preview_routes: Option<Vec<RouteDraft>>) -> Result<Value, Error> {
    use crate::file_actions::{change_routing as routing, migration_action as migration};
    let Some((baseline, model)) = migration_inputs(project, root)? else {
        return Ok(json!({"kind":"plan","state":"no-baseline","fileAction":FILE_ACTION}));
    };
    let plan = schema::diff(&baseline, &model)
        .map_err(|e| Error::from(format!("The snapshots cannot be compared: {e:?}")))?;
    let mut incomplete = Vec::new();
    if !baseline.complete {
        incomplete.push("The baseline snapshot is incomplete.");
    }
    if !model.complete {
        incomplete.push("The current model scan is incomplete.");
    }
    let next = migration::target_in_root(project, root, migration::BASELINE_NEXT)?;
    let pending_baseline = migration::read_optional(project, &next)?.is_some();
    let truncated = plan.changes.len() > MAX_PLAN_CHANGES || plan.notes.len() > MAX_PLAN_NOTES;
    let targets = sql::backfill_targets(&plan, &[]);
    let levels = crate::file_actions::danger_levels::overrides().unwrap_or_default();
    let setting_routes = routing::parse_routes(host::setting("schemaChangeLocations").as_deref()).map_err(Error::from)?;
    let previewing = preview_routes.is_some();
    let routes = match preview_routes {
        Some(rows) if rows.len() <= 32 => rows
            .into_iter()
            .map(|row| routing::parse_routes(Some(&json!([{"changeType":row.change_type,"location":row.location}]).to_string())).map(|mut one| one.remove(0)))
            .collect::<Result<Vec<_>, _>>()
            .map_err(Error::from)?,
        Some(_) => return Err("Too many routes were supplied.".into()),
        None => setting_routes.clone(),
    };
    let locations = routing::assign(&plan, &routes);
    let changes: Vec<Value> = plan
        .changes
        .iter()
        .zip(locations)
        .take(MAX_PLAN_CHANGES)
        .map(|(planned, location)| {
            let (kind, table, detail) = change_summary(&planned.change);
            let mut item = json!({"kind":kind,"table":table,"detail":detail,"danger":planned.danger,"reason":planned.reason,"level":crate::file_actions::danger_levels::level_of(planned, &levels).label(),"type":crate::file_actions::danger_levels::change_type(&planned.change),"location":location.label()});
            if let schema::Change::RenameCandidate { from, to, .. } = &planned.change {
                item["from"] = json!(from);
                item["to"] = json!(to);
            }
            if let Some(target) = targets.iter().find(|t| matches!(&planned.change, schema::Change::AddColumn { table, column } if *table == t.table && column.name == t.column)
                || matches!(&planned.change, schema::Change::AlterNullability { table, column, nullable: false, .. } if *table == t.table && *column == t.column))
                && planned.danger == Danger::NeedsBackfill
                && let Some(column) = &target.column_type
                && let Some(logical) = column.sql_type.known().map(|t| t.logical)
                && sql::BackfillValue::supports(logical)
            {
                item["backfill"] = json!({"table":target.table,"column":target.column,"logical":logical,"length":column.length,"precision":column.precision,"scale":column.scale});
            }
            item
        })
        .collect();
    Ok(json!({
        "kind":"plan","state":"ready","tables":model.tables.len(),
        "baselineComplete":baseline.complete,"modelComplete":model.complete,
        "incomplete":incomplete,"pendingBaseline":pending_baseline,
        "changes":changes,"notes":plan.notes.iter().take(MAX_PLAN_NOTES).collect::<Vec<_>>(),
        "truncated":truncated,"fileAction":FILE_ACTION,
        "routes":routes.iter().map(|(kind, location)| json!({"changeType":kind,"location":location.label()})).collect::<Vec<_>>(),
        "settingRoutes":setting_routes.iter().map(|(kind, location)| json!({"changeType":kind,"location":location.label()})).collect::<Vec<_>>(),
        "previewingRoutes":previewing,"statementTypes":reverse_panel::statement_types(&plan, &routes)?,"createTableOptions":reverse_panel::create_table_options(),
    }))
}

fn bounded_name(value: &str) -> Result<(), Error> {
    if value.is_empty() || value.len() > MAX_NAME {
        return Err("A rename decision names a table or column that is empty or too long.".into());
    }
    Ok(())
}

/// Returns the exact SQL text for display only. Creating the Flyway file stays with the file action.
fn generate_json(
    project: &openidea_plugin::Project,
    root: &str,
    dialect: &str,
    include: &[String],
    renames: Vec<RenameDraft>,
    backfills: Vec<BackfillDraft>,
    description: &str,
) -> Result<Value, Error> {
    use crate::file_actions::migration_action as migration;
    let dialect = match dialect {
        "postgresql" => Dialect::PostgreSql,
        "mysql" => Dialect::MySql,
        _ => return Err("Choose PostgreSQL or MySQL.".into()),
    };
    if include.len() > 2 || renames.len() > 16 || backfills.len() > MAX_PLAN_CHANGES {
        return Err("Too many options were supplied.".into());
    }
    let mut options = EmitOptions::default();
    for key in include {
        let danger = match key.as_str() {
            "needs-backfill" => Danger::NeedsBackfill,
            "data-loss" => Danger::DataLoss,
            _ => return Err("Only NeedsBackfill and DataLoss statements can be included.".into()),
        };
        if options.allow.contains(&danger) {
            return Err("A danger was listed twice.".into());
        }
        options.allow.push(danger);
    }
    for rename in renames {
        bounded_name(&rename.table)?;
        bounded_name(&rename.from)?;
        bounded_name(&rename.to)?;
        options.renames.push(RenameDecision {
            table: rename.table,
            from: rename.from,
            to: rename.to,
        });
    }
    let description = migration::description_of(description);
    if description.is_empty() {
        return Err("Enter a description containing ASCII letters or digits.".into());
    }
    let (baseline, model) = migration_inputs(project, root)?
        .ok_or("No schema baseline exists; run 'Create schema baseline…' first.")?;
    let plan = schema::diff(&baseline, &model)
        .map_err(|e| Error::from(format!("The snapshots cannot be compared: {e:?}")))?;
    if plan.changes.is_empty() && plan.notes.is_empty() {
        return Err("The model matches the baseline; there is nothing to migrate.".into());
    }
    let targets = sql::backfill_targets(&plan, &options.renames);
    for draft in backfills {
        bounded_name(&draft.table)?;
        bounded_name(&draft.column)?;
        let column = targets
            .iter()
            .find(|t| t.table.eq_ignore_ascii_case(&draft.table) && t.column.eq_ignore_ascii_case(&draft.column))
            .and_then(|t| t.column_type.as_ref())
            .ok_or_else(|| Error::from(format!("No NeedsBackfill change with a known column type targets {}.{}.", draft.table, draft.column)))?;
        let logical = column.sql_type.known().map(|t| t.logical).ok_or(Error::Stale)?;
        let value = sql::BackfillValue::parse(&draft.value, logical)
            .and_then(|value| value.check(column).map(|()| value))
            .map_err(|why| Error::from(format!("Backfill for {}.{} refused: {why}.", draft.table, draft.column)))?;
        options.backfills.push(sql::Backfill { table: draft.table, column: draft.column, value });
    }
    let emitted = sql::emit(&plan, dialect, &options)
        .map_err(|e| Error::from(format!("SQL cannot be generated: {e:?}")))?;
    if emitted.emitted() == 0 {
        return Err("Every change is withheld or unsupported; there is no SQL to show.".into());
    }
    let text = emitted.render();
    if text.len() > MAX_MIGRATION_SQL {
        return Err("The generated SQL exceeds the 64 KiB preview limit.".into());
    }
    Ok(json!({
        "kind":"sql","sql":text,"description":description,"fileSuffix":format!("__{description}.sql"),
        "emitted":emitted.emitted(),"policySkipped":emitted.policy_skipped(),"unsupported":emitted.unsupported(),
        "fileAction":FILE_ACTION,"created":false,
        "note":"Read-only preview. The Designer cannot create files; run the file action to create the Flyway file.",
    }))
}

impl JpaAssist {
    pub(super) fn inspect_at_caret(&mut self, ctx: &Invocation) -> Result<(), Error> {
        self.designer.capture(ctx)?;
        host::reveal_view(VIEW)?;
        host::post_view_message(
            VIEW,
            &json!({"channel":CHANNEL,"invalidate":true}).to_string(),
        )
    }

    pub(super) fn designer_event(
        &mut self,
        event: ViewEvent,
        ctx: &Invocation,
    ) -> Result<(), Error> {
        let ViewEvent::Message(text) = event else {
            return Ok(());
        };
        if text.len() > MAX_REQUEST {
            return Err(Error::Invalid(
                "Designer request exceeds its size limit.".into(),
            ));
        }
        let request: Request = serde_json::from_str(&text)
            .map_err(|_| Error::Invalid("Malformed designer request.".into()))?;
        if request.channel != CHANNEL
            || request.protocol != persistence_core::PROTOCOL_VERSION
            || request.sequence == 0
            || request.frame.len() != 32
            || !request.frame.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err(Error::Invalid("Invalid designer protocol/frame.".into()));
        }
        let Some(project) = ctx.project() else {
            return host::post_view_message(VIEW, &json!({"channel":CHANNEL,"frame":request.frame,"sequence":request.sequence,"layout":layout(),"model":{"empty":true,"root":null}}).to_string());
        };
        let root = project.root()?;
        if let Some((owner, sequence)) = self.designer.frames.get(&request.frame) {
            if owner != &root {
                self.designer.frames.remove(&request.frame);
                return host::post_view_message(VIEW, &json!({"channel":CHANNEL,"frame":request.frame,"sequence":request.sequence,"reset":true}).to_string());
            }
            if request.sequence <= *sequence {
                return Err(Error::Stale);
            }
        } else {
            if !matches!(request.operation, Operation::Snapshot) || request.root.is_some() {
                return host::post_view_message(VIEW, &json!({"channel":CHANNEL,"frame":request.frame,"sequence":request.sequence,"reset":true}).to_string());
            }
            // Reloaded panels must not permanently exhaust the bounded registry.
            // Evicted clients receive a reset before they can mutate anything.
            if self.designer.frames.len() >= 8
                && let Some(retired) = self.designer.frames.keys().min().cloned()
            {
                self.designer.frames.remove(&retired);
            }
        }
        if !matches!(request.operation, Operation::Snapshot)
            && request.root.as_deref() != Some(root.as_str())
        {
            return Err(Error::Stale);
        }
        self.designer
            .frames
            .insert(request.frame.clone(), (root.clone(), request.sequence));
        let mut migration: Option<Value> = None;
        let mut reverse: Option<Value> = None;
        let result = (|| -> Result<(), Error> {
            if matches!(
                request.operation,
                Operation::Prepare { .. }
                    | Operation::PrepareClass { .. }
                    | Operation::PrepareAttribute { .. }
                    | Operation::PrepareRelationship { .. }
                    | Operation::Apply { .. }
                    | Operation::Select { .. }
                    | Operation::Reset
            ) {
                let document = self.designer.document.as_ref().ok_or(Error::Stale)?;
                let context = request.context.as_ref().ok_or(Error::Stale)?;
                if context.path != document.path
                    || context.version != document.version
                    || context.attribute != self.designer.selected
                {
                    return Err(Error::Stale);
                }
            }
            match request.operation {
                Operation::Snapshot => {}
                Operation::Inspect => self.designer.capture(ctx)?,
                Operation::BindCreation => self.designer.bind_creation(ctx)?,
                Operation::PrepareClass { draft } => {
                    if self.designer.root.as_deref() != Some(root.as_str()) {
                        return Err(Error::Stale);
                    }
                    let doc = self.designer.document.as_ref().ok_or(Error::Stale)?;
                    if editor_document(ctx)? != *doc
                        || host::project_of(&doc.path)
                            .and_then(|p| p.root().ok())
                            .as_deref()
                            != Some(root.as_str())
                    {
                        return Err(Error::Stale);
                    }
                    let after = crate::creation::prepare(doc, &root, &project, &draft)?;
                    self.designer.next = self.designer.next.checked_add(1).ok_or(Error::Stale)?;
                    self.designer.pending = Some(Prepared {
                        token: format!("{}:{}:{}", request.frame, doc.version, self.designer.next),
                        document: doc.clone(),
                        edits: vec![TextEdit {
                            range: Range::new(
                                position(&doc.text, 0),
                                position(&doc.text, doc.text.len()),
                            ),
                            text: after.clone(),
                        }],
                        after,
                        creation: true,
                    });
                }
                Operation::PrepareAttribute { draft } => {
                    if self.designer.root.as_deref() != Some(root.as_str()) {
                        return Err(Error::Stale);
                    }
                    let doc = self.designer.document.as_ref().ok_or(Error::Stale)?;
                    if editor_document(ctx)? != *doc
                        || host::project_of(&doc.path)
                            .and_then(|p| p.root().ok())
                            .as_deref()
                            != Some(root.as_str())
                    {
                        return Err(Error::Stale);
                    }
                    if attribute_info::snapshot(doc).is_none() {
                        return Err("Inspect a complete source-owned persistence class before adding an attribute.".into());
                    }
                    let edits = crate::attribute_text_edits(doc, &attribute_spec(draft)?)?;
                    let after = crate::file_actions::checked_after(doc, &edits)?;
                    self.designer.next = self.designer.next.checked_add(1).ok_or(Error::Stale)?;
                    self.designer.pending = Some(Prepared {
                        token: format!("{}:{}:{}", request.frame, doc.version, self.designer.next),
                        document: doc.clone(),
                        edits,
                        after,
                        creation: false,
                    });
                }
                Operation::PrepareRelationship { changes } => {
                    if self.designer.root.as_deref() != Some(root.as_str()) {
                        return Err(Error::Stale);
                    }
                    let doc = self.designer.document.as_ref().ok_or(Error::Stale)?;
                    if editor_document(ctx)? != *doc
                        || host::project_of(&doc.path)
                            .and_then(|p| p.root().ok())
                            .as_deref()
                            != Some(root.as_str())
                    {
                        return Err(Error::Stale);
                    }
                    let name = self.designer.selected.as_deref().ok_or(Error::Stale)?;
                    let (edits, after) = crate::relationship_edit::prepare(doc, name, &changes)?;
                    self.designer.next = self.designer.next.checked_add(1).ok_or(Error::Stale)?;
                    self.designer.pending = Some(Prepared {
                        token: format!("{}:{}:{}", request.frame, doc.version, self.designer.next),
                        document: doc.clone(),
                        edits,
                        after,
                        creation: false,
                    });
                }
                Operation::Select { attribute } => {
                    if self.designer.pending.is_some() {
                        return Err(
                            "Apply or reset the prepared changes before changing attributes."
                                .into(),
                        );
                    }
                    if self.designer.root.as_deref() != Some(root.as_str()) {
                        return Err(Error::Stale);
                    }
                    let doc = self.designer.document.as_ref().ok_or(Error::Stale)?;
                    let class = attribute_info::snapshot(doc).ok_or(Error::Stale)?;
                    if !class.attributes.iter().any(|a| a.name == attribute) {
                        return Err(Error::Stale);
                    }
                    self.designer.selected = Some(attribute);
                }
                Operation::Prepare { changes } => {
                    if self.designer.root.as_deref() != Some(root.as_str()) {
                        return Err(Error::Stale);
                    }
                    let doc = self.designer.document.as_ref().ok_or(Error::Stale)?;
                    let current = editor_document(ctx)?;
                    if current != *doc
                        || host::project_of(&doc.path)
                            .and_then(|p| p.root().ok())
                            .as_deref()
                            != Some(root.as_str())
                    {
                        return Err(Error::Stale);
                    }
                    let name = self.designer.selected.as_deref().ok_or(Error::Stale)?;
                    let (edits, after) = prepare(doc, name, &changes)?;
                    self.designer.next = self.designer.next.checked_add(1).ok_or(Error::Stale)?;
                    self.designer.pending = Some(Prepared {
                        token: format!("{}:{}:{}", request.frame, doc.version, self.designer.next),
                        document: doc.clone(),
                        edits,
                        after,
                        creation: false,
                    });
                }
                Operation::Apply { token } => {
                    if self.designer.root.as_deref() != Some(root.as_str()) {
                        return Err(Error::Stale);
                    }
                    let pending = self.designer.pending.as_ref().ok_or(Error::Stale)?;
                    if pending.token != token {
                        return Err(Error::Stale);
                    }
                    let editor = ctx.editor().ok_or(Error::Stale)?;
                    if editor_document(ctx)? != pending.document
                        || host::project_of(&pending.document.path)
                            .and_then(|p| p.root().ok())
                            .as_deref()
                            != Some(root.as_str())
                    {
                        return Err(Error::Stale);
                    }
                    if pending.creation {
                        let target = crate::creation::context(&pending.document, &root)?;
                        crate::file_actions::no_collision(
                            &project,
                            &root,
                            pending
                                .document
                                .path
                                .rsplit_once('/')
                                .ok_or(Error::Stale)?
                                .0,
                            &target.name,
                            Some(&pending.document.path),
                        )?;
                    }
                    let version = editor.apply_edits(pending.document.version, &pending.edits)?;
                    let doc = Document {
                        text: pending.after.clone(),
                        version,
                        ..pending.document.clone()
                    };
                    if pending.creation {
                        self.designer.selected = attribute_info::snapshot(&doc)
                            .and_then(|c| c.attributes.first().map(|a| a.name.clone()));
                    }
                    self.designer.pending = None;
                    self.designer.document = Some(doc);
                    self.scanned = false;
                    host::refresh_view("structure");
                }
                Operation::Reset => {
                    if self.designer.root.as_deref() != Some(root.as_str()) {
                        return Err(Error::Stale);
                    }
                    self.designer.pending = None;
                }
                Operation::PlanMigration { routes } => migration = Some(plan_json(&project, &root, routes)?),
                Operation::ReverseSources => reverse = Some(reverse_panel::sources(&project, &root)?),
                Operation::ReverseInventory { path, dialect, schema, table } => {
                    reverse = Some(reverse_panel::inventory(&project, &root, &path, dialect.as_deref(), schema.as_deref(), table.as_deref())?)
                }
                Operation::GenerateMigration {
                    dialect,
                    include_dangers,
                    rename_decisions,
                    backfills,
                    description,
                } => {
                    migration = Some(generate_json(
                        &project,
                        &root,
                        &dialect,
                        &include_dangers,
                        rename_decisions,
                        backfills,
                        &description,
                    )?)
                }
            }
            Ok(())
        })();
        if let Err(error) = &result {
            host::notify(
                openidea_plugin::NotificationLevel::Warning,
                "Persistence Designer",
                &error.to_string(),
            );
        }
        host::post_view_message(VIEW, &json!({"channel":CHANNEL,"frame":request.frame,"sequence":request.sequence,"ok":result.is_ok(),"layout":layout(),"model":self.designer.model(&root),"migration":migration,"reverse":reverse}).to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;
    const PATH: &str = "/project/src/main/java/shop/Person.java";
    const FRAME: &str = "0123456789abcdef0123456789abcdef";
    const SOURCE: &str = "package shop;\nimport jakarta.persistence.Entity;\nimport jakarta.persistence.Id;\nimport jakarta.persistence.Column;\nimport java.lang.String;\n@Entity\nclass Person {\n    @Id long id;\n    // 😀 keep this comment\n    @Column(name = \"display_name\", nullable = true, columnDefinition = \"varchar(40)\", length = 40)\n    private String name;\n}\n";
    fn doc(text: &str) -> Document {
        Document {
            path: PATH.into(),
            text: text.into(),
            language: "java".into(),
            version: 1,
        }
    }
    fn change(property: ColumnProperty, value: ColumnValue) -> ColumnChange {
        ColumnChange { property, value }
    }
    fn request(
        plugin: &mut JpaAssist,
        host: &MockHost,
        seq: u64,
        op: Value,
    ) -> Result<Value, Error> {
        let context = plugin.designer.document.as_ref().map(
            |d| json!({"path":d.path,"version":d.version,"attribute":plugin.designer.selected}),
        );
        let text = json!({"channel":CHANNEL,"protocol":1,"frame":FRAME,"sequence":seq,"root":if seq==1 {Value::Null} else {json!("/project")},"context":context,"operation":op}).to_string();
        plugin.designer_event(ViewEvent::Message(text), &host.invocation())?;
        let (_, reply) = host.view_messages().last().cloned().ok_or(Error::Stale)?;
        serde_json::from_str(&reply).map_err(|_| Error::Stale)
    }
    fn setup() -> (MockHost, JpaAssist) {
        let host =
            MockHost::install().with_capabilities(&["project.read", "editor.read", "editor.write"]);
        host.add_file("src/main/java/shop/Person.java", SOURCE);
        host.open_editor(PATH, "java", SOURCE, &[(10, 20, 10, 20)]);
        (host, JpaAssist::default())
    }
    #[test]
    fn literal_changes_preserve_physical_names_other_arguments_comments_and_unicode() {
        let (_host, _) = setup();
        let (edits, after) = prepare(
            &doc(SOURCE),
            "name",
            &[
                change(ColumnProperty::Nullable, ColumnValue::Boolean(false)),
                change(ColumnProperty::Unique, ColumnValue::Boolean(true)),
            ],
        )
        .unwrap();
        assert_eq!(edits.len(), 2);
        assert_eq!(
            after,
            SOURCE
                .replace("nullable = true", "nullable = false")
                .replace("length = 40)", "length = 40, unique = true)")
        );
        assert!(
            after.contains("// 😀 keep this comment") && after.contains("name = \"display_name\"")
        );
    }
    #[test]
    fn new_column_uses_owned_namespace_and_preserves_crlf() {
        let (_host, _) = setup();
        let source = SOURCE.replace("    @Column(name = \"display_name\", nullable = true, columnDefinition = \"varchar(40)\", length = 40)\n", "").replace("jakarta.persistence", "javax.persistence").replace('\n', "\r\n");
        let (_, after) = prepare(
            &doc(&source),
            "name",
            &[change(ColumnProperty::Length, ColumnValue::Number(120))],
        )
        .unwrap();
        assert!(
            after.contains(
                "    @javax.persistence.Column(length = 120)\r\n    private String name;"
            )
        );
        assert!(!after.replace("\r\n", "").contains('\n'));
    }
    #[test]
    fn dynamic_arguments_are_locked_but_unrelated_literals_remain_editable() {
        let (_host, _) = setup();
        let source = SOURCE.replace("length = 40", "length = Config.SIZE");
        let props = properties(&doc(&source), "name").unwrap();
        assert!(
            props.iter().any(|p| p.key == ColumnProperty::Length
                && !p.editable
                && p.origin == "unresolved")
        );
        assert!(
            prepare(
                &doc(&source),
                "name",
                &[change(ColumnProperty::Length, ColumnValue::Number(5))]
            )
            .is_err()
        );
        let (_, after) = prepare(
            &doc(&source),
            "name",
            &[change(ColumnProperty::Unique, ColumnValue::Boolean(true))],
        )
        .unwrap();
        assert!(after.contains("length = Config.SIZE, unique = true"));
    }
    #[test]
    fn duplicated_arguments_foreign_annotations_inherited_types_and_wrong_values_refuse() {
        let (_host, _) = setup();
        for source in [
            SOURCE.replace("nullable = true", "nullable = true, nullable = false"),
            SOURCE.replace("jakarta.persistence.Column", "vendor.Column"),
            SOURCE.replace("class Person {", "class Person extends Base {"),
            SOURCE.replace("import java.lang.String;", "import vendor.String;"),
        ] {
            assert!(
                prepare(
                    &doc(&source),
                    "name",
                    &[change(
                        ColumnProperty::Nullable,
                        ColumnValue::Boolean(false)
                    )]
                )
                .is_err(),
                "{source}"
            );
        }
        assert!(
            prepare(
                &doc(SOURCE),
                "name",
                &[change(ColumnProperty::Length, ColumnValue::Boolean(true))]
            )
            .is_err()
        );
        assert!(
            prepare(
                &doc(SOURCE),
                "name",
                &[change(
                    ColumnProperty::Name,
                    ColumnValue::Text("x\"), @Id".into())
                )]
            )
            .is_err()
        );
    }
    #[test]
    fn nested_column_metadata_and_type_prefix_shadows_do_not_borrow_field_ownership() {
        let (_host, _) = setup();
        for source in [
            SOURCE.replace("@Column(name", "@jakarta.persistence.AttributeOverride(name = \"other\", column = @Column(name").replace("length = 40)", "length = 40))"),
            SOURCE.replace("class Person {", "class Person implements Other {"),
            SOURCE.replace("private String name", "private java.lang.String name").replace("import java.lang.String;", "import other.java;"),
            SOURCE.replace("private String name", "@jakarta.persistence.Transient private String name"),
        ] {
            assert!(prepare(&doc(&source), "name", &[change(ColumnProperty::Nullable, ColumnValue::Boolean(false))]).is_err(), "{source}");
        }
    }
    #[test]
    fn public_bridge_prepares_without_mutation_then_applies_exactly_once() {
        let (host, mut plugin) = setup();
        request(&mut plugin, &host, 1, json!({"kind":"snapshot"})).unwrap();
        let inspected = request(&mut plugin, &host, 2, json!({"kind":"inspect"})).unwrap();
        assert_eq!(inspected["model"]["selected"], "name");
        let prepared = request(
            &mut plugin,
            &host,
            3,
            json!({"kind":"prepare","changes":[{"property":"nullable","value":false}]}),
        )
        .unwrap();
        assert_eq!(host.editor_text(), SOURCE);
        let token = prepared["model"]["pending"]["token"]
            .as_str()
            .unwrap()
            .to_owned();
        let result = request(&mut plugin, &host, 4, json!({"kind":"apply","token":token})).unwrap();
        assert_eq!(result["ok"], true);
        assert_eq!(
            host.editor_text(),
            SOURCE.replace("nullable = true", "nullable = false")
        );
        assert!(result["model"]["pending"].is_null());
        assert_eq!(
            request(&mut plugin, &host, 5, json!({"kind":"apply","token":token})).unwrap()["ok"],
            false
        );
    }
    #[test]
    fn replies_carry_the_visibility_settings_of_the_layout() {
        let (host, mut plugin) = setup();
        let reply = request(&mut plugin, &host, 1, json!({"kind":"snapshot"})).unwrap();
        assert_eq!(reply["layout"], json!({"palette":true,"inspector":true,"toolbar":true,"minimal":false}));
        for key in ["showPalette", "showInspector", "showEditorToolbar"] {
            host.set_setting(&format!("jpaAssist.designer.{key}"), "false");
        }
        host.set_setting("jpaAssist.designer.minimalMode", "true");
        let reply = request(&mut plugin, &host, 2, json!({"kind":"inspect"})).unwrap();
        assert_eq!(reply["layout"], json!({"palette":false,"inspector":false,"toolbar":false,"minimal":true}));
    }
    fn attribute_draft(kind: &str, name: &str, ty: &str) -> Value {
        json!({"name":name,"kind":kind,"type":ty,"enumerated":false,"notNull":false,"unique":false,
            "cascade":[],"fetch":null,"orphanRemoval":false,"orderBy":null,"orderColumn":null,
            "joinColumn":null,"joinColumnNotNull":false,"joinTable":null})
    }
    #[test]
    fn attribute_drafts_preview_then_apply_one_versioned_edit() {
        let (host, mut plugin) = setup();
        request(&mut plugin, &host, 1, json!({"kind":"snapshot"})).unwrap();
        request(&mut plugin, &host, 2, json!({"kind":"inspect"})).unwrap();
        let mut draft = attribute_draft("OneToMany", "lines", "Line");
        draft["cascade"] = json!(["PERSIST", "REMOVE"]);
        draft["orphanRemoval"] = json!(true);
        draft["orderBy"] = json!("id DESC");
        let reply = request(&mut plugin, &host, 3, json!({"kind":"prepare-attribute","draft":draft})).unwrap();
        assert_eq!(reply["ok"], true, "{reply}");
        assert_eq!(host.editor_text(), SOURCE);
        let after = reply["model"]["pending"]["after"].as_str().unwrap().to_owned();
        assert!(after.contains("@OneToMany(cascade = {CascadeType.PERSIST, CascadeType.REMOVE}, orphanRemoval = true)"), "{after}");
        assert!(after.contains("@OrderBy(\"id DESC\")") && after.contains("private List<Line> lines = new ArrayList<>();"), "{after}");
        let token = reply["model"]["pending"]["token"].as_str().unwrap().to_owned();
        assert_eq!(request(&mut plugin, &host, 4, json!({"kind":"apply","token":token})).unwrap()["ok"], true);
        assert_eq!(host.editor_text(), after);
    }
    #[test]
    fn attribute_drafts_refuse_invalid_names_options_duplicates_and_unknown_fields() {
        let (host, mut plugin) = setup();
        request(&mut plugin, &host, 1, json!({"kind":"snapshot"})).unwrap();
        request(&mut plugin, &host, 2, json!({"kind":"inspect"})).unwrap();
        let mut orphan = attribute_draft("ManyToOne", "owner", "Person");
        orphan["orphanRemoval"] = json!(true);
        let mut injected = attribute_draft("none", "nick", "String");
        injected["type"] = json!("String; } @X");
        let mut unknown = attribute_draft("none", "nick", "String");
        unknown["extra"] = json!(1);
        for (seq, draft) in [
            attribute_draft("none", "name", "String"),
            attribute_draft("none", "class", "String"),
            attribute_draft("bogus", "nick", "String"),
            orphan,
            injected,
            unknown,
        ]
        .into_iter()
        .enumerate()
        {
            let reply = request(&mut plugin, &host, 3 + seq as u64, json!({"kind":"prepare-attribute","draft":draft}));
            assert!(reply.map_or(true, |r| r["ok"] == false), "draft {seq}");
            assert!(plugin.designer.pending.is_none());
            assert_eq!(host.editor_text(), SOURCE);
        }
    }
    #[test]
    fn existing_relationships_expose_properties_and_apply_one_versioned_edit() {
        let source = "package shop;\nimport jakarta.persistence.Entity;\nimport jakarta.persistence.Id;\nimport jakarta.persistence.OneToMany;\nimport java.util.List;\n@Entity\nclass Person {\n    @Id long id;\n    @OneToMany(mappedBy = \"owner\")\n    private List<Pet> pets;\n}\n";
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read", "editor.write"]);
        host.add_file("src/main/java/shop/Person.java", source);
        host.open_editor(PATH, "java", source, &[(8, 20, 8, 20)]);
        let mut plugin = JpaAssist::default();
        request(&mut plugin, &host, 1, json!({"kind":"snapshot"})).unwrap();
        let inspected = request(&mut plugin, &host, 2, json!({"kind":"inspect"})).unwrap();
        assert_eq!(inspected["model"]["selected"], "pets");
        let keys: Vec<_> = inspected["model"]["relationship"].as_array().unwrap().iter().map(|p| p["key"].as_str().unwrap().to_owned()).collect();
        assert_eq!(keys, ["fetch", "orphanRemoval", "cascade", "orderBy"]);
        let association = &inspected["model"]["association"];
        assert_eq!(association["cardinality"], "OneToMany");
        assert_eq!(association["owningSide"], false);
        assert_eq!(association["mappedBy"], "owner");
        let reply = request(&mut plugin, &host, 3, json!({"kind":"prepare-relationship","changes":[{"property":"orphanRemoval","value":true},{"property":"fetch","value":"EAGER"}]})).unwrap();
        assert_eq!(reply["ok"], true, "{reply}");
        assert_eq!(host.editor_text(), source);
        let after = reply["model"]["pending"]["after"].as_str().unwrap().to_owned();
        assert!(after.contains("@OneToMany(mappedBy = \"owner\", orphanRemoval = true, fetch = FetchType.EAGER)"), "{after}");
        let token = reply["model"]["pending"]["token"].as_str().unwrap().to_owned();
        assert_eq!(request(&mut plugin, &host, 4, json!({"kind":"apply","token":token})).unwrap()["ok"], true);
        assert_eq!(host.editor_text(), after);
        let refused = request(&mut plugin, &host, 5, json!({"kind":"prepare-relationship","changes":[{"property":"optional","value":false}]})).unwrap();
        assert_eq!(refused["ok"], false);
    }
    #[test]
    fn join_and_ordering_changes_flow_through_the_relationship_bridge() {
        let source = "package shop;\nimport jakarta.persistence.Entity;\nimport jakarta.persistence.Id;\nimport jakarta.persistence.ManyToMany;\nimport jakarta.persistence.ManyToOne;\nimport java.util.List;\n@Entity\nclass Person {\n    @Id long id;\n    @ManyToOne\n    private Pet pet;\n    @ManyToMany\n    private List<Tag> tags;\n}\n";
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read", "editor.write"]);
        host.add_file("src/main/java/shop/Person.java", source);
        host.open_editor(PATH, "java", source, &[(9, 20, 9, 20)]);
        let mut plugin = JpaAssist::default();
        request(&mut plugin, &host, 1, json!({"kind":"snapshot"})).unwrap();
        let inspected = request(&mut plugin, &host, 2, json!({"kind":"inspect"})).unwrap();
        assert_eq!(inspected["model"]["selected"], "pet");
        let join = inspected["model"]["relationship"].as_array().unwrap().iter().find(|p| p["key"] == "joinColumn").unwrap().clone();
        assert_eq!(join["value"], json!({"name": "", "nullable": true}));
        let reply = request(&mut plugin, &host, 3, json!({"kind":"prepare-relationship","changes":[{"property":"joinColumn","name":"pet_id","nullable":false}]})).unwrap();
        assert_eq!(reply["ok"], true, "{reply}");
        assert_eq!(host.editor_text(), source);
        let after = reply["model"]["pending"]["after"].as_str().unwrap().to_owned();
        assert!(after.contains("@ManyToOne\n    @JoinColumn(name = \"pet_id\", nullable = false)\n    private Pet pet;") && after.contains("import jakarta.persistence.JoinColumn;"), "{after}");
        let token = reply["model"]["pending"]["token"].as_str().unwrap().to_owned();
        assert_eq!(request(&mut plugin, &host, 4, json!({"kind":"apply","token":token})).unwrap()["ok"], true);
        assert_eq!(host.editor_text(), after);
        let refused = request(&mut plugin, &host, 5, json!({"kind":"prepare-relationship","changes":[{"property":"joinTable","name":"t","joinColumn":"a","inverseColumn":"b"}]})).unwrap();
        assert_eq!(refused["ok"], false);
        let malformed = request(&mut plugin, &host, 6, json!({"kind":"prepare-relationship","changes":[{"property":"joinColumn","name":"x"}]}));
        assert!(malformed.map_or(true, |r| r["ok"] == false));
    }
    #[test]
    fn dirty_source_after_preview_is_not_overwritten_and_reset_never_mutates() {
        let (host, mut plugin) = setup();
        request(&mut plugin, &host, 1, json!({"kind":"snapshot"})).unwrap();
        request(&mut plugin, &host, 2, json!({"kind":"inspect"})).unwrap();
        let reply = request(
            &mut plugin,
            &host,
            3,
            json!({"kind":"prepare","changes":[{"property":"unique","value":true}]}),
        )
        .unwrap();
        host.type_in_editor("// unsaved");
        let token = reply["model"]["pending"]["token"].clone();
        let refused =
            request(&mut plugin, &host, 4, json!({"kind":"apply","token":token})).unwrap();
        assert_eq!(refused["ok"], false);
        assert_eq!(host.editor_text(), format!("{SOURCE}// unsaved"));
        request(&mut plugin, &host, 5, json!({"kind":"reset"})).unwrap();
        assert_eq!(host.editor_text(), format!("{SOURCE}// unsaved"));
    }
    #[test]
    fn stale_displayed_attribute_context_cannot_redirect_a_draft() {
        let (host, mut plugin) = setup();
        request(&mut plugin, &host, 1, json!({"kind":"snapshot"})).unwrap();
        request(&mut plugin, &host, 2, json!({"kind":"inspect"})).unwrap();
        plugin.designer.selected = Some("id".into());
        let text =
            json!({"channel":CHANNEL,"protocol":1,"frame":FRAME,"sequence":3,"root":"/project",
            "context":{"path":PATH,"version":1,"attribute":"name"},
            "operation":{"kind":"prepare","changes":[{"property":"nullable","value":false}]}})
            .to_string();
        plugin
            .designer_event(ViewEvent::Message(text), &host.invocation())
            .unwrap();
        let reply: Value = serde_json::from_str(&host.view_messages().last().unwrap().1).unwrap();
        assert_eq!(reply["ok"], false);
        assert!(plugin.designer.pending.is_none());
        assert_eq!(host.editor_text(), SOURCE);
    }
    #[test]
    fn inspecting_an_annotation_selects_its_field_and_denied_writes_preserve_preview() {
        let host = MockHost::install().with_capabilities(&["project.read", "editor.read"]);
        host.add_file("src/main/java/shop/Person.java", SOURCE);
        host.open_editor(PATH, "java", SOURCE, &[(9, 8, 9, 8)]);
        let mut plugin = JpaAssist::default();
        request(&mut plugin, &host, 1, json!({"kind":"snapshot"})).unwrap();
        let inspected = request(&mut plugin, &host, 2, json!({"kind":"inspect"})).unwrap();
        assert_eq!(inspected["model"]["selected"], "name");
        let reply = request(
            &mut plugin,
            &host,
            3,
            json!({"kind":"prepare","changes":[{"property":"unique","value":true}]}),
        )
        .unwrap();
        let token = reply["model"]["pending"]["token"].clone();
        let result = request(&mut plugin, &host, 4, json!({"kind":"apply","token":token})).unwrap();
        assert_eq!(result["ok"], false);
        assert!(plugin.designer.pending.is_some());
        assert_eq!(host.editor_text(), SOURCE);
    }
    #[test]
    fn create_page_previews_then_applies_to_the_bound_empty_editor() {
        let (host, mut plugin) = setup();
        host.open_editor(PATH, "java", "", &[]);
        request(&mut plugin, &host, 1, json!({"kind":"snapshot"})).unwrap();
        let bound = request(&mut plugin, &host, 2, json!({"kind":"bind-creation"})).unwrap();
        assert_eq!(bound["model"]["creation"]["name"], "Person");
        let draft = json!({"name":"Person","role":"entity","namespace":"jakarta","tableName":"people","versioned":false,
            "identifier":{"name":"key","column":"person_id","idType":"integer","strategy":"sequence","sequence":{"generator":"person_ids","sequenceName":"SEQ_PERSON","initialValue":5,"allocationSize":10},"table":null}});
        let prepared = request(
            &mut plugin,
            &host,
            3,
            json!({"kind":"prepare-class","draft":draft}),
        )
        .unwrap();
        assert_eq!(prepared["ok"], true);
        assert_eq!(host.editor_text(), "");
        assert!(
            prepared["model"]["pending"]["after"]
                .as_str()
                .unwrap()
                .contains("allocationSize = 10")
        );
        let token = prepared["model"]["pending"]["token"].clone();
        let applied =
            request(&mut plugin, &host, 4, json!({"kind":"apply","token":token})).unwrap();
        assert_eq!(applied["ok"], true);
        assert!(host.editor_text().contains("java.lang.Integer key"));
        assert_eq!(applied["model"]["selected"], "key");
    }
    #[test]
    fn class_previews_do_not_overwrite_new_typing_or_new_colliding_types() {
        for collide in [false, true] {
            let (host, mut plugin) = setup();
            host.open_editor(PATH, "java", "", &[]);
            request(&mut plugin, &host, 1, json!({"kind":"snapshot"})).unwrap();
            request(&mut plugin, &host, 2, json!({"kind":"bind-creation"})).unwrap();
            let draft = json!({"name":"Person","role":"embeddable","namespace":"javax","tableName":"","identifier":null,"versioned":false});
            let prepared = request(
                &mut plugin,
                &host,
                3,
                json!({"kind":"prepare-class","draft":draft}),
            )
            .unwrap();
            assert_eq!(prepared["ok"], true);
            if collide {
                host.add_file("src/main/java/shop/Person.kt", "package shop\nclass Person");
            } else {
                host.type_in_editor("// keep unsaved text");
            }
            let token = prepared["model"]["pending"]["token"].clone();
            let result =
                request(&mut plugin, &host, 4, json!({"kind":"apply","token":token})).unwrap();
            assert_eq!(result["ok"], false);
            assert_eq!(
                host.editor_text(),
                if collide { "" } else { "// keep unsaved text" }
            );
        }
    }
    #[test]
    fn malformed_generator_choices_and_creation_in_nonempty_source_never_prepare() {
        let (host, mut plugin) = setup();
        request(&mut plugin, &host, 1, json!({"kind":"snapshot"})).unwrap();
        assert_eq!(
            request(&mut plugin, &host, 2, json!({"kind":"bind-creation"})).unwrap()["ok"],
            false
        );
        assert_eq!(host.editor_text(), SOURCE);
        host.open_editor(PATH, "java", "", &[]);
        request(&mut plugin, &host, 3, json!({"kind":"bind-creation"})).unwrap();
        let draft = json!({"name":"Person","role":"entity","namespace":"javax","tableName":"people","versioned":false,
            "identifier":{"name":"id","column":"id","idType":"string","strategy":"uuid","sequence":null,"table":null}});
        assert_eq!(
            request(
                &mut plugin,
                &host,
                4,
                json!({"kind":"prepare-class","draft":draft})
            )
            .unwrap()["ok"],
            false
        );
        assert!(plugin.designer.pending.is_none());
        assert_eq!(host.editor_text(), "");
    }
    #[test]
    fn replay_and_project_switch_do_not_reuse_authority() {
        let (host, mut plugin) = setup();
        request(&mut plugin, &host, 1, json!({"kind":"snapshot"})).unwrap();
        request(&mut plugin, &host, 2, json!({"kind":"inspect"})).unwrap();
        assert!(request(&mut plugin, &host, 2, json!({"kind":"reset"})).is_err());
        host.set_project_root(Some("/other"));
        let reply = request(&mut plugin, &host, 3, json!({"kind":"snapshot"})).unwrap();
        assert_eq!(reply["reset"], true);
        assert_eq!(host.editor_text(), SOURCE);
    }
    const CUSTOMER_PATH: &str = "/project/src/main/java/shop/Customer.java";
    const CUSTOMER: &str = "package shop;\nimport jakarta.persistence.Entity;\nimport jakarta.persistence.Id;\n@Entity\npublic class Customer {\n    @Id Long id;\n    String name;\n}\n";
    const CUSTOMER_RENAMED: &str = "package shop;\nimport jakarta.persistence.Entity;\nimport jakarta.persistence.Id;\n@Entity\npublic class Customer {\n    @Id Long id;\n    String fullName;\n}\n";
    const CUSTOMER_PLUS: &str = "package shop;\nimport jakarta.persistence.Column;\nimport jakarta.persistence.Entity;\nimport jakarta.persistence.Id;\n@Entity\npublic class Customer {\n    @Id Long id;\n    String name;\n    @Column(length = 100) String email;\n}\n";

    fn migration_host(baseline: Option<&str>, current: &str) -> (MockHost, JpaAssist) {
        let host = MockHost::install().with_capabilities(&["project.read"]);
        host.add_file("src/main/java/shop/Customer.java", current);
        if let Some(source) = baseline {
            let doc = Document {
                path: CUSTOMER_PATH.into(),
                language: "java".into(),
                text: source.into(),
                version: 0,
            };
            let snapshot = crate::schema_snapshot::snapshot_from_entities(&[doc]);
            let json = crate::schema_snapshot::snapshot_json(&snapshot).unwrap();
            host.add_file("db/jpa-schema-baseline.json", &format!("{json}\n"));
        }
        let mut plugin = JpaAssist::default();
        request(&mut plugin, &host, 1, json!({"kind":"snapshot"})).unwrap();
        (host, plugin)
    }
    fn generate_op(dialect: &str, include: &[&str], renames: Value) -> Value {
        json!({"kind":"generate-migration","dialect":dialect,"includeDangers":include,"renameDecisions":renames,"description":"Add customer email!"})
    }

    #[test]
    fn plan_migration_returns_the_change_plan_as_json_without_mutation() {
        let (host, mut plugin) = migration_host(Some(CUSTOMER), CUSTOMER_PLUS);
        let reply = request(&mut plugin, &host, 2, json!({"kind":"plan-migration"})).unwrap();
        assert_eq!(reply["ok"], true);
        let plan = &reply["migration"];
        assert_eq!(plan["kind"], "plan");
        assert_eq!(plan["state"], "ready");
        assert_eq!(plan["truncated"], false);
        assert_eq!(plan["pendingBaseline"], false);
        let change = &plan["changes"][0];
        assert_eq!(change["kind"], "add-column");
        assert_eq!(change["table"], "customer");
        assert_eq!(change["detail"], "email");
        assert_eq!(change["danger"], "safe");
        assert!(change["reason"].as_str().is_some_and(|r| !r.is_empty()));
        assert!(plan["notes"].is_array() && plan["incomplete"].is_array());
        assert!(plugin.designer.pending.is_none() && plugin.designer.document.is_none());
        let project = host.invocation().project().unwrap();
        assert!(!project.exists("/project/db/jpa-schema-baseline.next.json").unwrap());
        assert!(!project.exists("/project/src/main/resources/db/migration").unwrap());
    }

    #[test]
    fn plan_migration_without_a_baseline_is_an_explanatory_state_not_an_error() {
        let (host, mut plugin) = migration_host(None, CUSTOMER);
        let reply = request(&mut plugin, &host, 2, json!({"kind":"plan-migration"})).unwrap();
        assert_eq!(reply["ok"], true);
        assert_eq!(reply["migration"]["state"], "no-baseline");
        assert!(reply["migration"]["changes"].is_null());
    }

    #[test]
    fn rename_candidates_are_reported_and_emitted_only_with_an_explicit_decision() {
        let (host, mut plugin) = migration_host(Some(CUSTOMER), CUSTOMER_RENAMED);
        let plan = request(&mut plugin, &host, 2, json!({"kind":"plan-migration"})).unwrap();
        let candidate = plan["migration"]["changes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["kind"] == "rename-candidate")
            .unwrap();
        assert_eq!(candidate["danger"], "ambiguous");
        assert_eq!((candidate["from"].as_str(), candidate["to"].as_str()), (Some("name"), Some("full_name")));
        let without = request(&mut plugin, &host, 3, generate_op("postgresql", &["data-loss", "needs-backfill"], json!([]))).unwrap();
        let executable = |sql: &str| sql.lines().filter(|l| !l.starts_with("--")).collect::<Vec<_>>().join("\n");
        assert!(!executable(without["migration"]["sql"].as_str().unwrap()).contains("RENAME"));
        let decision = json!([{"table":"customer","from":"name","to":"full_name"}]);
        let with = request(&mut plugin, &host, 4, generate_op("postgresql", &[], decision)).unwrap();
        assert_eq!(with["ok"], true);
        assert!(executable(with["migration"]["sql"].as_str().unwrap()).contains("RENAME COLUMN"));
        let unknown = json!([{"table":"customer","from":"x","to":"y"}]);
        let refused = request(&mut plugin, &host, 5, generate_op("postgresql", &[], unknown)).unwrap();
        assert_eq!(refused["ok"], false);
        assert!(refused["migration"].is_null());
    }

    #[test]
    fn generate_migration_returns_exact_sql_read_only_and_points_at_the_file_action() {
        let (host, mut plugin) = migration_host(Some(CUSTOMER), CUSTOMER_PLUS);
        let reply = request(&mut plugin, &host, 2, generate_op("mysql", &[], json!([]))).unwrap();
        let sql = &reply["migration"];
        assert_eq!(reply["ok"], true);
        assert_eq!(sql["kind"], "sql");
        assert_eq!(sql["created"], false);
        assert_eq!(sql["fileSuffix"], "__Add_customer_email.sql");
        assert_eq!(sql["fileAction"], "Generate Flyway migration from model changes…");
        let text = sql["sql"].as_str().unwrap();
        assert!(text.contains("ALTER TABLE") && text.contains("email"), "{text}");
        assert!(plugin.designer.pending.is_none());
        let project = host.invocation().project().unwrap();
        assert!(!project.exists("/project/src/main/resources/db/migration").unwrap());
        assert!(!project.exists("/project/db/jpa-schema-baseline.next.json").unwrap());
    }

    #[test]
    fn route_previews_move_a_change_type_without_writing_anything() {
        let (host, mut plugin) = migration_host(Some(CUSTOMER), CUSTOMER_PLUS);
        let base = request(&mut plugin, &host, 2, json!({"kind":"plan-migration"})).unwrap();
        assert_eq!(base["migration"]["changes"][0]["location"], "primary");
        assert_eq!(base["migration"]["changes"][0]["type"], "addColumn");
        assert_eq!(base["migration"]["previewingRoutes"], false);
        let routes = json!([{"changeType":"addColumn","location":"ignored"}]);
        let moved = request(&mut plugin, &host, 3, json!({"kind":"plan-migration","routes":routes})).unwrap();
        assert_eq!(moved["migration"]["changes"][0]["location"], "ignored");
        assert_eq!(moved["migration"]["previewingRoutes"], true);
        assert_eq!(moved["migration"]["settingRoutes"], json!([]));
        let types = moved["migration"]["statementTypes"].as_array().unwrap();
        assert_eq!(types.len(), 14);
        let add = types.iter().find(|t| t["type"] == "addColumn").unwrap();
        assert_eq!((add["count"].clone(), add["location"].clone(), add["levelOverride"].clone()), (json!(1), json!("ignored"), Value::Null));
        assert_eq!(moved["migration"]["createTableOptions"][1]["setting"], "jpaAssist.maxIdentifierLength");
        let restored = request(&mut plugin, &host, 4, json!({"kind":"plan-migration","routes":[]})).unwrap();
        assert_eq!(restored["migration"]["changes"][0]["location"], "primary");
        let unknown = json!([{"changeType":"nope","location":"ignored"}]);
        assert_eq!(request(&mut plugin, &host, 5, json!({"kind":"plan-migration","routes":unknown})).unwrap()["ok"], false);
        let place = json!([{"changeType":"addColumn","location":"elsewhere"}]);
        assert_eq!(request(&mut plugin, &host, 6, json!({"kind":"plan-migration","routes":place})).unwrap()["ok"], false);
        assert!(!host.invocation().project().unwrap().exists("/project/src/main/resources/db").unwrap());
    }

    const SHOP_SQL: &str = "CREATE SCHEMA shop;\nCREATE TABLE shop.customer (id BIGINT PRIMARY KEY, name VARCHAR(40), email VARCHAR(100));\nCREATE TABLE shop.orders (id BIGINT PRIMARY KEY, customer_id BIGINT REFERENCES shop.customer(id), total NUMERIC(10,2));\nCREATE VIEW shop.order_totals AS SELECT id, total FROM shop.orders;\n";

    #[test]
    fn reverse_inventory_groups_relations_labels_unmapped_columns_and_lists_references() {
        let (host, mut plugin) = migration_host(Some(CUSTOMER), CUSTOMER);
        host.add_file("db/schema.sql", SHOP_SQL);
        let sources = request(&mut plugin, &host, 2, json!({"kind":"reverse-sources"})).unwrap();
        assert_eq!(sources["reverse"]["sqlFiles"][0]["label"], "db/schema.sql");
        assert_eq!(sources["reverse"]["generateAction"], "Generate entities from SQL file…");
        assert_eq!(sources["reverse"]["attributesAction"], "Add attributes from database columns…");
        let op = json!({"kind":"reverse-inventory","path":"/project/db/schema.sql","dialect":null,"schema":null,"table":"shop.orders"});
        let inventory = request(&mut plugin, &host, 3, op).unwrap();
        assert_eq!(inventory["ok"], true);
        let reverse = &inventory["reverse"];
        assert_eq!(reverse["dialect"], "postgresql");
        assert_eq!(reverse["schemas"], json!([{"value":"shop","label":"shop"}]));
        let group = |name: &str| reverse["relations"].as_array().unwrap().iter().find(|r| r["name"] == name).unwrap()["group"].clone();
        assert_eq!(group("shop.customer"), "mapped");
        assert_eq!(group("shop.orders"), "table");
        assert_eq!(group("shop.order_totals"), "view");
        let customer = reverse["relations"].as_array().unwrap().iter().find(|r| r["name"] == "shop.customer").unwrap();
        assert_eq!(customer["unmapped"], 1, "email has no attribute yet");
        let detail = &reverse["selected"];
        assert_eq!(detail["references"][0]["refTable"], "shop.customer");
        assert_eq!(detail["references"][0]["refMapped"], true);
        assert_eq!(detail["references"][0]["columnPresent"], true);
        assert!(detail["columns"].as_array().unwrap().iter().any(|c| c["name"] == "total" && c["java"] == "BigDecimal"));
        assert!(reverse["typeMappings"].as_array().unwrap().iter().any(|m| m["java"] == "Long"));
        let mapped = request(&mut plugin, &host, 4, json!({"kind":"reverse-inventory","path":"/project/db/schema.sql","dialect":"mysql","schema":"shop","table":"shop.customer"})).unwrap();
        assert_eq!(mapped["ok"], true);
        let columns = mapped["reverse"]["selected"]["columns"].as_array().unwrap();
        assert_eq!(columns.iter().filter(|c| c["mapped"] == true).count(), 2, "id and name are mapped, email is not");
        assert_eq!(mapped["reverse"]["dialectDetected"], false);
    }

    #[test]
    fn reverse_inventory_refuses_files_outside_the_project_and_non_sql_paths() {
        let (host, mut plugin) = migration_host(Some(CUSTOMER), CUSTOMER);
        host.add_file("db/schema.sql", SHOP_SQL);
        for (seq, path) in [(2, "/etc/passwd"), (3, "/project/src/main/java/shop/Customer.java"), (4, "/project/../etc/x.sql"), (5, "db/schema.sql")] {
            let reply = request(&mut plugin, &host, seq, json!({"kind":"reverse-inventory","path":path,"dialect":null,"schema":null,"table":null})).unwrap();
            assert_eq!(reply["ok"], false, "{path}");
            assert!(reply["reverse"].is_null());
        }
        let bad_dialect = request(&mut plugin, &host, 6, json!({"kind":"reverse-inventory","path":"/project/db/schema.sql","dialect":"oracle","schema":null,"table":null})).unwrap();
        assert_eq!(bad_dialect["ok"], false);
    }

    const CUSTOMER_REQUIRED: &str = "package shop;\nimport jakarta.persistence.Column;\nimport jakarta.persistence.Entity;\nimport jakarta.persistence.Id;\n@Entity\npublic class Customer {\n    @Id Long id;\n    String name;\n    @Column(nullable = false, length = 5) String email;\n}\n";

    #[test]
    fn needs_backfill_rows_carry_the_column_type_and_generate_validates_values() {
        let (host, mut plugin) = migration_host(Some(CUSTOMER), CUSTOMER_REQUIRED);
        let plan = request(&mut plugin, &host, 2, json!({"kind":"plan-migration"})).unwrap();
        let row = &plan["migration"]["changes"][0];
        assert_eq!(row["danger"], "needs-backfill");
        assert_eq!(row["backfill"], json!({"table":"customer","column":"email","logical":"varchar","length":5,"precision":null,"scale":null}));

        let op = |backfills: Value, include: &[&str]| json!({"kind":"generate-migration","dialect":"postgresql","includeDangers":include,"renameDecisions":[],"backfills":backfills,"description":"x"});
        let ok = request(&mut plugin, &host, 3, op(json!([{"table":"customer","column":"email","value":"n/a"}]), &["needs-backfill"])).unwrap();
        assert_eq!(ok["ok"], true);
        let sql = ok["migration"]["sql"].as_str().unwrap();
        assert!(sql.contains("UPDATE \"customer\" SET \"email\" = 'n/a' WHERE \"email\" IS NULL;") && sql.contains("SET NOT NULL;"), "{sql}");

        let mut seq = 3;
        for (backfills, include) in [
            (json!([{"table":"customer","column":"email","value":"far too long"}]), vec!["needs-backfill"]),
            (json!([{"table":"customer","column":"email","value":"a\nb"}]), vec!["needs-backfill"]),
            (json!([{"table":"customer","column":"nope","value":"x"}]), vec!["needs-backfill"]),
            (json!([{"table":"customer","column":"email","value":"x"}]), vec![]),
        ] {
            seq += 1;
            assert_eq!(request(&mut plugin, &host, seq, op(backfills, &include)).unwrap()["ok"], false);
        }
    }

    #[test]
    fn migration_operations_validate_options_and_bounds() {
        let (host, mut plugin) = migration_host(Some(CUSTOMER), CUSTOMER_PLUS);
        let mut seq = 1;
        let mut call = |plugin: &mut JpaAssist, op: Value| {
            seq += 1;
            request(plugin, &host, seq, op)
        };
        for op in [
            generate_op("oracle", &[], json!([])),
            generate_op("mysql", &["rewrite"], json!([])),
            generate_op("mysql", &["data-loss", "data-loss"], json!([])),
            generate_op("mysql", &[], json!([{"table":"","from":"a","to":"b"}])),
            generate_op("mysql", &[], json!([{"table":"t","from":"a","to":"b".repeat(129)}])),
            json!({"kind":"generate-migration","dialect":"mysql","includeDangers":[],"renameDecisions":[],"description":"!!!"}),
        ] {
            assert_eq!(call(&mut plugin, op).unwrap()["ok"], false);
        }
        let many: Vec<Value> = (0..17).map(|n| json!({"table":"t","from":format!("a{n}"),"to":format!("b{n}")})).collect();
        assert_eq!(call(&mut plugin, generate_op("mysql", &[], json!(many))).unwrap()["ok"], false);
        let unknown = json!({"kind":"generate-migration","dialect":"mysql","includeDangers":[],"renameDecisions":[],"description":"x","path":"/etc"});
        assert!(call(&mut plugin, unknown).is_err());
    }

    #[test]
    fn migration_requests_reject_replayed_foreign_and_unbound_frames() {
        let (host, mut plugin) = migration_host(Some(CUSTOMER), CUSTOMER_PLUS);
        request(&mut plugin, &host, 2, json!({"kind":"plan-migration"})).unwrap();
        assert!(matches!(request(&mut plugin, &host, 2, json!({"kind":"plan-migration"})), Err(Error::Stale)));
        let foreign = json!({"channel":CHANNEL,"protocol":1,"frame":"f".repeat(32),"sequence":1,"root":null,"context":null,"operation":{"kind":"plan-migration"}}).to_string();
        plugin.designer_event(ViewEvent::Message(foreign), &host.invocation()).unwrap();
        let (_, reply) = host.view_messages().last().cloned().unwrap();
        let reply: Value = serde_json::from_str(&reply).unwrap();
        assert_eq!(reply["reset"], true);
        assert!(reply.get("migration").is_none());
        let wrong_root = json!({"channel":CHANNEL,"protocol":1,"frame":FRAME,"sequence":9,"root":"/elsewhere","context":null,"operation":{"kind":"plan-migration"}}).to_string();
        assert!(matches!(plugin.designer_event(ViewEvent::Message(wrong_root), &host.invocation()), Err(Error::Stale)));
    }

    #[test]
    fn oversized_plans_are_truncated_and_oversized_sql_is_refused() {
        let host = MockHost::install().with_capabilities(&["project.read"]);
        let entity = |name: &str, extra: usize| {
            let fields: String = (0..extra).map(|n| format!("    String longColumnNameForPreviewLimit{n};\n")).collect();
            format!("package shop;\nimport jakarta.persistence.Entity;\nimport jakarta.persistence.Id;\n@Entity\npublic class {name} {{\n    @Id Long id;\n{fields}}}\n")
        };
        let snapshot = |extra: usize| {
            let docs: Vec<Document> = ["A", "B", "C"]
                .iter()
                .map(|n| Document { path: format!("/project/src/main/java/shop/{n}.java"), language: "java".into(), text: entity(n, extra), version: 0 })
                .collect();
            crate::schema_snapshot::snapshot_json(&crate::schema_snapshot::snapshot_from_entities(&docs)).unwrap()
        };
        for n in ["A", "B", "C"] {
            host.add_file(&format!("src/main/java/shop/{n}.java"), &entity(n, 300));
        }
        host.add_file("db/jpa-schema-baseline.json", &format!("{}\n", snapshot(0)));
        let mut plugin = JpaAssist::default();
        request(&mut plugin, &host, 1, json!({"kind":"snapshot"})).unwrap();
        let plan = request(&mut plugin, &host, 2, json!({"kind":"plan-migration"})).unwrap();
        assert_eq!(plan["migration"]["truncated"], true);
        assert_eq!(plan["migration"]["changes"].as_array().unwrap().len(), MAX_PLAN_CHANGES);
        let sql = request(&mut plugin, &host, 3, generate_op("postgresql", &[], json!([]))).unwrap();
        assert_eq!(sql["ok"], false);
        assert!(sql["migration"].is_null());
    }
}
