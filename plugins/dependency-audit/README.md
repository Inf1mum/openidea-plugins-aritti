# Dependency Audit

Checks the dependencies of a project against [OSV.dev](https://osv.dev), the open
vulnerability database. Opening or saving a supported file can refresh OSV
evidence. Versioned live diagnostics underline current-document dependencies using
dated cached evidence, with a quick fix that previews upgrades before returning
edits to the host. Editing never triggers a network request.
MIT; see `LICENSE`.

| File | Versions checked |
|---|---|
| `pom.xml` | `<dependency>` entries with a literal version or a `${property}` defined in the same file |
| `build.gradle`, `build.gradle.kts` | `'group:artifact:1.2.3'` string coordinates with a literal version |
| `package.json` | the version in `package-lock.json` next to it; else an exact version (`"4.17.21"`) in `dependencies` or `devDependencies` |
| `package-lock.json` | every locked package (lockfile v1, v2 and v3; v1 traversal is bounded to 32 nested levels and 10,000 entries) |
| `composer.json` | the version in `composer.lock` next to it; else an exact version in `require` or `require-dev` |
| `composer.lock` | every locked package |
| `libs.versions.toml` | `[libraries]` entries as Maven dependencies: `module` or `group`+`name` with a literal `version` or a `version.ref` from `[versions]`, or `"g:a:version"`; entries without a resolvable version are skipped |
| `yarn.lock` | every locked npm package (v1 and Berry) |
| `pnpm-lock.yaml` | every locked npm package in `packages:` (lockfile v6 and v9) |
| `Cargo.lock` | every registry crate (workspace members and path or git packages are skipped) |
| `requirements.txt` | `name==version` pins (PyPI) |
| `poetry.lock` | every locked PyPI package |
| `Pipfile.lock` | exact `==` versions from declared verified canonical PyPI indexes in default/develop groups; private/ambiguous/Git/path records skipped |
| `uv.lock` | resolved packages from the canonical PyPI registry; editable, Git, URL and private-registry packages are skipped |

A manifest with a lockfile is checked at the locked versions, so `^4.17.15` is
judged by what is installed, not by the range.

## Quick fix

⌥⏎ on an underlined dependency offers **Upgrade … to …**: the nearest fixed
version above the current one (a `^` or `~` in front stays). It is offered only for
a single constraint, not for ranges like `>=1 <2`, and only once per Maven property
shared by several dependencies, with the highest fix among them. It also works on
exact versions in `libs.versions.toml` (literal, `version` or the shared
`[versions]` entry) and `requirements.txt` `==` pins. Lockfile entries have no
quick fix: update them with your package manager.

Listing editor actions has no side effects. Selecting Upgrade opens an exact
file/range, editor-version and old/new text review; cancellation returns no edits.
Resolution rechecks current project ownership, source text/version, sibling lock
evidence and cached advice before returning one host-managed apply. It does not
install packages.

The same supported manifest contexts offer **Open OSV advisory** and **Ignore … for
package version** even when no safe upgrade exists. Open requires approval of the
exact validated OSV URL. Ignore requires a reason and exact
project/package/version/advisory review, then uses the same bounded project-owned
records and Review scoped ignores restoration as the details view. Neither action
returns text edits or requests OSV data. Lockfile editor actions remain
unsupported; use the report details for those findings.

After upgrading a range in `package.json` or `composer.json`, the underline stays
until the lockfile follows: run `npm install` / `composer update`, then reopen the
file.

### Source admission

Edit actions only run on unambiguous current declarations:

- Gradle coordinates require a supported configuration/platform invocation; line/nested block comments, triple-string examples, computed literal prefixes and Groovy slashy/regex forms are skipped or withhold results. Custom configurations/advanced Gradle expressions are outside this subset; no build resolution or compatibility proof is inferred.
- POM versions require a single project root, unique group/artifact/version children per dependency, and one root-owned literal property for property-based versions. Duplicate property names, profile-only properties, profile shadowing, non-self-closing elements without their own matching closing container, foreign/rebound namespaces and dependency-shaped plugin configuration payloads do not select an editable version span. This is source-local ambiguity handling, not effective Maven model/profile/parent resolution.
- Upgrade application carries the source project identity through direct resource commands and rechecks current innermost ownership immediately before its versioned editor apply; opening a nested project between preview and application refuses unchanged. Resource upgrade and scoped-ignore commands revalidate retained raw report identity and bounded current source ownership after confirmation, and the filtered selected row must match the reviewed row. Replaced reports, newly nested source owners and changed selected evidence refuse unchanged.

## Command and view

**Tools › Dependency Audit: Scan Project** checks every supported file in the
project and shows a notification with the totals. The **Vulnerable Dependencies**
tool window has searchable Dependency and Advisory rows with ecosystem, installed
version, reported fixed version, severity and manifest columns. Filter by record
type, ecosystem, severity or manifest. Select a dependency to inspect every
declaration/lockfile location found by the scan, the advisory summaries, scan age
and the existing ignore-setting scope. Select an advisory for its aliases,
modification time and reported fixed versions. Unknown severity or missing
advisory details are labelled explicitly rather than treated as a known fix/no-fix
result. Large reports return at most 5,000 resource rows within a bounded message;
the available row total remains visible, and shortened detail fields are marked
"truncated".

**Open** on a Dependency row goes to the first declaration; on an Advisory row it
opens that advisory on osv.dev. Selected Advisory rows also offer **Open advisory
on OSV.dev**; the backend revalidates report membership and the advisory ID before
opening its fixed OSV URL. Dependency and Advisory rows offer **Ignore this exact
finding with reason** — a dependency with multiple advisories opens a picker so the
action targets one exact finding. Enter a nonempty reason, then review the project
root, ecosystem/package, exact version, advisory ID, known aliases and reason
before confirming. This suppresses only that finding in that project, including
its known ID/alias identities. It applies to report rows, live diagnostics, editor
upgrade fixes and cached commit review; other versions, packages, ecosystems and
projects remain unaffected. Changes take effect on the next report/provider
refresh; existing editor marks may need an edit or reopen because the SDK has no
diagnostic-refresh request. A surviving advisory keeps the package visible and
supplies its own upgrade recommendation.

Selected Dependency rows offer **Preview upgrade in open declaration**. Open a
listed declaration first, then choose that action. It reads the current editor
buffer, checks the selected package/ecosystem/installed version and one unique
editable span, and shows the exact file, range, editor version and old/new source
version for confirmation. Only then does it apply one undoable, version-checked
edit; changes during the prompt abort rather than retry. Review/test the change,
update lockfiles with the package manager and rescan; the report still describes
the previous scan. This workflow makes no network request, runs no package manager
and writes no files directly. Contextual upgrades support exact current
declarations in the existing POM, Gradle coordinate, version-catalog, npm/Composer
and requirements parser subsets. Unknown sources, duplicate declarations, shared
version fields, ranged manifests, lockfile-only rows and
missing/unsafe/non-forward fixed versions are refused — use the editor quick fix
for its broader range/shared-property support, or use the package manager. The
reported fixed version and approximate comparator do not establish compatibility
or complete remediation. Editor read/write permissions are requested only for
this action.

**Review scoped ignores** is available above the report (including an empty
report) and from Tools. Select a record to see its reason/date and confirm its
restoration. The raw scan snapshot is retained when suppressing and rescanning, so
restoring reveals retained report/cache evidence without another network request
or rescan; restoring does not refresh evidence that has otherwise been lost. A
global ignored-advisory setting can still hide the finding. These records are
plugin-owned, project-root-keyed storage: at most 200 records and 256 KiB per
project, with a 512-byte reason, and no automatic expiry. Unreadable, invalid,
oversized or unavailable suppression storage suppresses nothing and is not
overwritten by this workflow; Review reports its error. This does not write the
global `ignoredAdvisories` setting or share native IDE inspection profiles.
Removing a global ignore may still require a scan if that global setting
prevented facts from being recorded.

## Reports and evidence

The last successful report, including advisory facts, survives a failed/offline
rescan or separate advisory-cache eviction; a failed rescan is shown with the
previous report, not replaced by an empty success. Scan age is distinct from query
age: OSV query answers may be reused for up to 24 hours. Older saved reports remain
readable, with missing scan timestamps marked as not recorded. If new results
cannot be stored, the previous stored report is retained and the notification
explains that the new results were not kept. Saved reports share an 8 MiB
reader/writer limit: an oversized new report is rejected before replacing last-good
evidence. Advisory detail reuse/fetches are bounded to 128 KiB and must return the
requested ID; missing or mismatched details keep only the requested ID, without
borrowing another advisory's aliases or fixed versions. OSV continuation pages are
followed within explicit limits; incomplete responses do not replace the previous
report, and reaching the per-glob source discovery cap is labelled partial in the
report and cached commit review.

Explicit scan completion (including retained last-good/failure status), a
successful confirmed upgrade, scoped ignore/restore, observed saved-file cache
clearing, opted-in open-check cache commits and an observed editor focus that
changes project identity invalidate the Vulnerable Dependencies page. Render never
starts a scan, fetch or invalidation loop. Before historical rows are displayed or
used by actions, the primary manifest/lock and all recorded declaration paths must
belong to the invoking project. Ownership checks are filesystem/network-free and
deduplicated per unique evidence path; at most 512 paths and a cooperative 50 ms
proof are accepted. Exceeding the bound, unknown/foreign/nested project ownership
or denied storage/root access returns a replacement empty explanation model,
clearing old targets/details; retained report bytes are not deleted. Restore
access, choose the appropriate narrower project or explicitly rescan. None of
these states certifies a clean dependency graph. Explicit/open/save checks and
editor quick fixes read an npm/Composer sibling lock only when it has the same
project owner; missing/foreign lock evidence does not authorize a ranged
declaration. Directory information is source-path evidence, not a resolved
Maven/Gradle module graph.

## Live diagnostics

Live diagnostics do not fetch, scan files, migrate caches or publish a second
manual Problems producer. They parse current text and read bounded OSV caches.
Positive results include the query's Unix date, with an explicit stale label after
24 hours. An unknown coordinate is quiet, which does not establish that it is
safe. Ignore settings apply immediately. Documents above 512 KiB are skipped; work
is bounded. A lock-dependent ranged manifest can reuse its last scanned facts only
while its full text is unchanged. Editing that manifest, changing a watched
dependency file, or reclaiming the plugin instance discards those facts; exact
current declarations and lockfile documents still use their own cached
coordinates. Reopen/save or rescan to warm lock-dependent facts — the plugin
deliberately avoids reading siblings while typing. The OSV network grant is
optional, so cached browsing and diagnostics remain useful offline. Automatic
refresh still follows the Check on open and save setting.

## Commit check

**Review cached dependency scan** is available in the host's Commit Options and
needs optional `vcs.read`. The host initially enables contributed checks; disable
this entry there if you do not want it or have not granted VCS access. The check
never fetches or reads working-tree files. It reports previous findings only for
changed dependency files, with the scan date; a missing/stale/incomplete report or
uncovered changed file asks for review. Deleted and unrelated files are skipped. A
passed check is not a certification of selected hunks: the API supplies diffs, and
the stored scan may describe different contents. Review the included changes; the
host supports Commit Anyway for acknowledged problems.

## Inspect npm lockfile dependencies

**Dependency Audit: Inspect npm Lockfile Graph** opens a native read-only tree
for the current `package-lock.json` v2/v3 buffer. Each package lists its declared
normal, optional and peer edges (plus root dev dependencies), resolved through
nearest physical `node_modules` entries. Activating a resolved package/edge goes
to that lockfile entry. Linked workspaces and missing targets are labelled
unresolved. This is lockfile evidence, not a proof of active platform/peer/optional
resolution or an installed runtime graph. Input is bounded to 512 KiB, 2,000
package records and 8,000 edges; source edits/project switches withhold stale
navigation. No package manager or network request runs for this inspection.

## Resolve effective Maven or Gradle dependencies

**Dependency Audit: Resolve Effective Build Dependencies…** offers explicitly
reviewed local profiles for **Maven 3.9.16** (Dependency Plugin 3.8.1 JSON tree
export) and **Gradle 9.9.1** (owned init-script export of a selected resolvable
configuration, `runtimeClasspath` by default). Independently install/approve those
exact tool versions. Optional permissions are `process:mvn`, `process:gradle` and
`project.write`. No wrapper or installer is run automatically.

The approved build executes project plugins and may contact configured
repositories; review precedes even the version probe. Managed processes write fresh
observations under `.openidea/dependency-audit-resolve-*`. The native Dependency
Resolution view provides Poll and Cancel, then **Audit Resolved Dependencies** after
a successful complete export. Only that explicit audit queries OSV; editor
providers remain cache-only. Failed, foreign, truncated or stale-source exports
retain the prior report. Resolution does not install, upgrade or edit dependency
declarations.

Maven captures the reactor's parent/property/BOM-effective and transitive versions,
excluding internal reactor coordinates. Gradle captures external module versions
for each module's requested configuration, excludes internal project components
and labels unavailable configurations as skipped. Module provenance stays in the
report; source navigation is to the associated build file/root POM, not a fabricated
exact declaration span. Observations are dated; this is not a vulnerable-call
model or proof that all configurations/platforms have been inspected. Reviews bound
source snapshots to 500 files/4 MiB, exports to 4 MiB and 10,000 coordinates.
Broader tool versions, remote/Windows paths and build inputs beyond captured
project files require separate qualification. Generated resolver artifacts remain
for inspection.

## Optional vulnerable-call analysis

**Dependency Audit: Analyze Vulnerable Calls (OSV-Scanner)…** runs a reviewed,
locally approved **OSV-Scanner 2.7.1** process for Go or experimental Rust. Install
the official CLI and the selected language toolchain yourself; the plugin neither
installs tools nor runs project wrappers. The review explains execution, dependency
downloads and external query risks before even probing the version. Go uses
govulncheck; Rust compiles dependencies and may execute build scripts. Java,
Kotlin, Python and other languages remain **Unknown** in this adapter.

The native Call Analysis view polls/cancels the owned process and retains separate
dated, source-bound call evidence. It never changes the existing vulnerability
report. Each advisory is Called, Uncalled or Unknown only from its own explicit
scanner analysis record; missing facts and alias-only records stay Unknown.
Uncalled is not a safety guarantee. A scan with no explicit advisory call facts
keeps previous evidence and reports Unknown. Invalid, failed, stale, oversized or
foreign output retains previous evidence. Source capture is bounded to 500
files/512 KiB and persisted evidence to 512 KiB; configured external
dependencies/toolchain inputs are not fully captured. Dirty project buffers
prevent launch/import; changed captured sources visibly mark older evidence stale.

On Darwin arm64 the official 2.7.1 binary with Go produced both Called and
Uncalled results on a fixture; for Rust it rejects Mach-O output with
`bad magic number` (exit 127), so the Rust profile remains experimental and no
successful Darwin Rust analysis is claimed — partial JSON from that failure is
never accepted. The separate **Rust source-built Mach-O/dSYM adapter** choice
requires the optional `process:osv-scanner-macho` grant and the exact
`2.7.1-openidea-macho.1` executable: a separately named source build with an owned
patch to pinned OSV-Scanner, not the unchanged official binary. Reproducible
build and qualification instructions are retained under
`adapters/osv-scanner-macho/`. Both profiles use the same saved-source checks and
refuse changed sources or missing/failed call evidence. This public evidence does
not establish equivalence to proprietary intelligence or coverage of all
call-analysis languages.

## Optional sbt effective resolution

**Dependency Audit: Resolve Effective sbt Dependencies…** supports the approved
official **sbt 2.0.12** runner/runtime with Java 17+ and a root `build.sbt` plus
exact `project/build.properties` version. Review precedes the runner probe or build
execution. The managed `--server --batch` session loads the executable Scala build
and its plugins, may contact configured repositories and writes ordinary sbt/build
caches. No project wrapper runs and no build definition is edited. Global
settings/JVM options and arbitrary external build inputs are outside the bounded
captured source set.

The owned temporary session command evaluates `update` for every owned project
reference. Its versioned export includes non-evicted effective module coordinates
across reported configurations, exact cross-versioned internal project identities,
and a complete module/record footer. Internal project modules are excluded from OSV
queries. External project base directories, incomplete resolution, duplicate/foreign
records, missing artifacts and exceeded bounds refuse import. All reported
configurations are inspected, including sbt's internal/tool configurations; this
does not claim that every coordinate executes in the application.

The native sbt Resolution view polls/cancels the owned process. After successful
completion, **Audit Resolved Dependencies** separately queries OSV using the
existing report pipeline. Dirty buffers or changed captured build sources prevent
acceptance; failed resolution/queries preserve the previous report. Root
`build.sbt` navigation is honest module/configuration provenance, not an exact
dependency declaration span. Other sbt versions and platforms require separate
qualification; sbt 1.x is not admitted by this profile.

## Known-malicious records

OSV's exact `MAL-<year>-<number>` namespace is surfaced as known-malicious package
report evidence from OpenSSF via OSV.dev, with the report/query dates. Unknown
advisory namespaces do not imply maliciousness or safety. Withdrawn records are
historical. Matching a package/version does not establish execution; no automatic
upgrade is suggested for known-malicious records (including retained older
reports). Advisory review and dependency removal/replacement remain explicit user
actions. This public dataset does not claim complete malware coverage or
equivalence to proprietary intelligence.

## Settings

- Settings › Plugins › Dependency Audit › **Check on open and save** (`autoCheck`, on by default). Off: nothing is sent until you run a scan.
- **Ignored advisories** (`ignoredAdvisories`): one advisory id per line (`GHSA-…`, `CVE-…`), `#` starts a comment. Ids are case-insensitive and match the advisory's id or any of its aliases (a `CVE-…` works for a `GHSA-…` advisory once its details are fetched). Listed ids are not reported, neither as squiggles nor in the Vulnerable Dependencies view.

## Privacy

Package names and versions, never source code, are sent to `api.osv.dev`, a free
service run by Google. Nothing else leaves your machine. Answers are kept in the
plugin's storage for 24 hours (one cache key), so reopening a file doesn't ask
again. Advisory ids that look unsafe are shown but never fetched. Turn off the
setting to contact OSV only when you run a scan.

## Permissions

| Permission | Used for |
|---|---|
| `net:api.osv.dev` (optional) | Query the OSV.dev vulnerability database. Only package names and versions are sent (see Privacy). No other host is contacted |
| `project.read` (required) | Read the manifests and lockfiles in the table above |
| `storage` (required) | Cache scan results so unchanged dependencies are not queried again |
| `editor.read`, `editor.write` (optional) | Preview and apply one confirmed, versioned edit in the open source declaration |
| `events` (optional) | Re-check when a manifest is opened or saved. Without it, run the scan from the command |
| `vcs.read` (optional) | The Review cached dependency scan commit check |
| `process:mvn`, `process:gradle`, `process:sbt` (optional) | Effective dependency resolution |
| `process:osv-scanner`, `process:osv-scanner-macho` (optional) | Vulnerable-call analysis |
| `project.write` (optional) | Effective dependency resolution observations |

## Not checked

- Maven and Gradle dependencies without a version (managed by a BOM, a parent POM or a plugin such as Spring Boot's), and versions taken from variables.
- Version ranges with no lockfile, and `package-lock.json` v1.
- Catalog entries declared over several lines, `version` ranges (`strictly`, `require`), and `version.ref` keys missing from `[versions]`.
- pnpm lockfile v5 (`/name/1.2.3` keys), and `requirements.txt` lines that aren't an exact `==` pin.
- Other ecosystems (Go…).
- Automatic ignore expiry, package-manager lock regeneration and native inspection-profile integration.
- No effective dependency resolver, package install or lock regeneration; new parsers are read by hand.

## Development

```sh
cargo test
openidea-plugin build
openidea-plugin link
```
