# Source-navigation ownership correction

The pinned API 1.3 host uses DocumentIndexer definitions/references for automatic
Rename as well as navigation. Its indexed rename merger writes the same new name
to every gathered span. The former Flyway indexer published:

- `fw:ph:seed` over the whole `flyway.placeholders.seed` property key;
- `fw:table:user_account` over both SQL `user_account` and Java `UserAccount`;
- raw CREATE TABLE matches inside SQL comments;
- independently truncated reference collections, omitting known occurrences.

Those are not replaceable symbols. The actual exported index now returns empty
edges, clearing prior entries on reindex. The unsafe parsers are removed rather
than reused for automatic edits. A native language server's own Rename and the
separate previewed atomic migration filename action retain their own contracts.

The replacement read-only provider covers named placeholders in a single known
source configuration and literal owned migration locations. It selects only the
placeholder suffix in `.conf` and the literal bare/quoted TOML map key. SQL ranges
select the exact name inside the default `${...}` delimiter, including inside
ordinary strings; comment text is excluded. ASCII case folding matches Flyway's
case-insensitive placeholder lookup. Duplicate folded configuration keys are
ambiguous and suppressed. Neither values nor full config text are retained.

The owner cache is populated only during existing explicit/lifecycle scans. It
requires one of root `flyway.conf`, `flyway.toml`, `conf/flyway.conf` or
`conf/flyway.toml`, rejects competing source configs and Spring overlays, and
compares declared config/naming/locations to the existing loaded source config.
A bounded provider consumes cached facts/current supplied text only. Current
changed configuration invalidates the proof; current SQL replaces that file's
facts even when empty/rejected. Root/config and SQL fact freshness are independent.
No source indexes, credentials, DB schemas, JPA class names, runtime values or
closed-world usages are invented. Cached reverse locations are navigation samples,
never an automatic edit set. Size/count failures suppress results without truncation.

Original contracts inspected:

- `intellij.flyway.core/.../resolve/FlywaySqlResolveExtension.kt` resolves against
  previous Flyway files through module/project configuration, SQL PSI declarations
  and modification-aware caches. This is broader than lexical table matching.
- `intellij.spring.boot.flyway/.../SpringFlywayLocationsHintReferenceProvider.kt`
  uses framework resource-reference services and project-relative filesystem
  references for literal locations. It does not establish a database-to-Java-class
  Rename relationship.
- Pinned host `crates/ide-host/src/plugins/navigation.rs` isolates project/client
  graphs and prefers detected scopes, but fallback scopes do not prove a database,
  schema or selected Flyway configuration. Markers alone are not Rename anchors;
  definitions/references are.

Primary framework evidence: [Redgate's placeholder namespace documentation](https://documentation.red-gate.com/fd/flyway-placeholders-namespace-277579022.html)
confirms case-insensitive matching and default key substitution;
[Flyway Configuration.java](https://github.com/flyway/flyway/blob/master/flyway-core/src/main/java/org/flywaydb/core/api/configuration/Configuration.java)
documents configurable replacement, prefix and suffix. This source-navigation
subset supports default delimiters only; it does not claim to execute or validate
the effective runtime configuration.

Limits and remaining UI/native acceptance are recorded in README's navigation
section. Table/JPA associations, full SQL-dialect resolution, arbitrary environment
and Spring config ownership, complete usages and automatic graph Rename remain
pending feasible source/persistence-model work, not a blanket SDK blocker.
