# Dependency Audit

Checks the dependencies of a project against [OSV.dev](https://osv.dev), the open
vulnerability database. Opening or saving a supported file can refresh OSV evidence. Versioned live diagnostics
underline current-document dependencies using dated cached evidence, with a quick fix
that previews upgrades before returning edits to the host. Editing never triggers a network request.

| File | Versions checked |
|---|---|
| `pom.xml` | `<dependency>` entries with a literal version or a `${property}` defined in the same file |
| `build.gradle`, `build.gradle.kts` | `'group:artifact:1.2.3'` string coordinates with a literal version |
| `package.json` | the version in `package-lock.json` next to it; else an exact version (`"4.17.21"`) in `dependencies` or `devDependencies` |
| `package-lock.json` | every locked package (lockfile v2 and v3) |
| `composer.json` | the version in `composer.lock` next to it; else an exact version in `require` or `require-dev` |
| `composer.lock` | every locked package |
| `libs.versions.toml` | `[libraries]` entries as Maven dependencies: `module` or `group`+`name` with a literal `version` or a `version.ref` from `[versions]`, or `"g:a:version"`; entries without a resolvable version are skipped |
| `yarn.lock` | every locked npm package (v1 and Berry) |
| `pnpm-lock.yaml` | every locked npm package in `packages:` (lockfile v6 and v9) |
| `Cargo.lock` | every registry crate (workspace members and path or git packages are skipped) |
| `requirements.txt` | `name==version` pins (PyPI) |
| `poetry.lock` | every locked PyPI package |

A manifest with a lockfile is checked at the locked versions, so `^4.17.15` is judged by what is
installed, not by the range.

## Quick fix

⌥⏎ on an underlined dependency offers **Upgrade … to …**: the nearest fixed version above the
current one (a `^` or `~` in front stays). It is offered only for a single constraint, not for
ranges like `>=1 <2`, and only once per Maven property shared by several dependencies, with
the highest fix among them. It also works on exact versions in `libs.versions.toml` (literal, `version` or the shared `[versions]` entry) and `requirements.txt` `==` pins. Lockfile entries have no quick fix: update them with your package manager.
Listing editor actions has no side effects. Selecting Upgrade opens an exact file/range, editor-version and old/new text review; cancellation returns no edits. Resolution rechecks current project ownership, source text/version, sibling lock evidence and cached advice before returning one host-managed apply. It does not install packages.

The same supported manifest contexts offer **Open OSV advisory** and **Ignore … for package version** even when no safe upgrade exists. Open requires approval of the exact validated OSV URL. Ignore requires a reason and exact project/package/version/advisory review, then uses the same bounded project-owned records and Review scoped ignores restoration as the details view. Neither action returns text edits or requests OSV data. Lockfile editor actions remain unsupported; use the report details for those findings.

After upgrading a range in `package.json` or `composer.json`, the underline stays until the lockfile
follows: run `npm install` / `composer update`, then reopen the file.

## Command and view

**Tools › Dependency Audit: Scan Project** checks every supported file in the project and shows a
notification with the totals. The **Vulnerable Dependencies** tool window has searchable
Dependency and Advisory rows with ecosystem, installed version, reported fixed version,
severity and manifest columns. Filter by record type, ecosystem, severity or manifest.
Select a dependency to inspect every declaration/lockfile location found by the scan,
the advisory summaries, scan age and the existing ignore-setting scope. Select an advisory
for its aliases, modification time and reported fixed versions. Unknown severity or missing
advisory details are labelled explicitly rather than treated as a known fix/no-fix result.
Large reports return at most 5,000 resource rows within a bounded message; the available
row total remains visible, and shortened detail fields are marked “truncated”.

**Open** on a Dependency row goes to the first declaration; on an Advisory row it opens
that advisory on osv.dev. Selected Advisory rows also offer **Open advisory on OSV.dev**;
the backend revalidates report membership and the advisory ID before opening its fixed
OSV URL. Selected Dependency rows offer **Preview upgrade in open declaration**. Open a
listed declaration first, then choose that action. It reads the current editor buffer,
checks the selected package/ecosystem/installed version and one unique editable span,
and shows the exact file, range, editor version and old/new source version for confirmation.
Only then does it apply one undoable, version-checked edit; changes during the prompt
abort rather than retry. Review/test the change, update lockfiles with the package
manager and rescan; the report still describes the previous scan. This workflow makes
no network request, runs no package manager and writes no files directly.

Contextual upgrades support exact current declarations in the existing POM, Gradle
coordinate, version-catalog, npm/Composer and requirements parser subsets. Unknown
sources, duplicate declarations, shared version fields, ranged manifests, lockfile-only
rows and missing/unsafe/non-forward fixed versions are refused. Use the editor quick fix
for its broader range/shared-property support, or use the package manager. The reported
fixed version and approximate comparator do not establish compatibility or complete
remediation. Editor read/write permissions are requested only for this action.

Selected Advisory rows offer **Ignore this exact finding with reason**. Enter a
nonempty reason, then review the project root, ecosystem/package, exact version,
advisory ID, known aliases and reason before confirming. This suppresses only
that finding in that project, including its known ID/alias identities. It applies
to report rows, live diagnostics, editor upgrade fixes and cached commit review;
other versions, packages, ecosystems and projects remain unaffected. Changes take
effect on the next report/provider refresh; existing editor marks may need an edit
or reopen because the SDK has no diagnostic-refresh request. A surviving
advisory keeps the package visible and supplies its own upgrade recommendation.

**Review scoped ignores** is available above the report (including an empty
report) and from Tools. Select a record to see its reason/date and confirm its
restoration. The raw scan snapshot is retained when suppressing and rescanning,
so restoring reveals retained report/cache evidence without another network
request or rescan. Restoring does not refresh evidence that has otherwise been
lost. A global ignored-advisory setting can still hide the finding.

These records are plugin-owned, project-root-keyed storage: at most 200 records
and 256 KiB per project, with a 512-byte reason. They have no automatic expiry.
Unreadable, invalid, oversized or unavailable suppression storage suppresses
nothing and is not overwritten by this workflow. Review reports its error.
This does not write the global `ignoredAdvisories` setting or share native IDE
inspection profiles. Those are separate workflows. Removing a global ignore may
still require a scan if that global setting prevented facts from being recorded.

P02.2 exposes upgrade/advisory/ignore through report details and the supported editor contexts, with confirmed edit previews. It remains bounded to the parser subsets above, explicit
exact-coordinate suppression and restoration. Automatic expiry, package-manager
lock regeneration and native profile integration are not implemented. Open
supported manifests and lockfiles check their own current coordinates against
cached evidence. Directory information is source-path evidence, not a resolved
Maven/Gradle module graph.

The last successful report, including advisory facts, survives a failed/offline rescan or
separate advisory-cache eviction. A failed rescan is shown with the previous report and
does not replace it with an empty success. Scan age is distinct from query age: OSV query
answers may be reused for up to 24 hours. Older saved reports remain readable, with missing
scan timestamps marked as not recorded. If new results cannot be stored, the previous
stored report is retained and the notification explains that the new results were not kept.
Saved reports share an 8 MiB reader/writer limit: an oversized new report is rejected
before replacing last-good evidence. Advisory detail reuse/fetches are bounded to
128 KiB and must return the requested ID; missing or mismatched details keep only
the requested ID, without borrowing another advisory's aliases or fixed versions.

## Settings

Settings › Plugins › Dependency Audit › **Check on open and save** (`autoCheck`, on by default).
Off: nothing is sent until you run a scan.

**Ignored advisories** (`ignoredAdvisories`): one advisory id per line (`GHSA-…`, `CVE-…`), `#` starts a comment.
Ids are case-insensitive and match the advisory's id or any of its aliases (a `CVE-…` works for a `GHSA-…` advisory once its details are fetched). Listed ids are not reported, neither as squiggles nor in the Vulnerable Dependencies view.

## Privacy

Package names and versions, never source code, are sent to `api.osv.dev`, a free service run by
Google. Nothing else leaves your machine. Answers are kept in the plugin's storage for 24 hours
(one cache key), so reopening a file doesn't ask again. Advisory ids that look unsafe are shown
but never fetched. Turn off the setting to contact OSV only when you run a scan.

## Not checked

- Maven and Gradle dependencies without a version (managed by a BOM, a parent POM or a
  plugin such as Spring Boot's), and versions taken from variables.
- Version ranges with no lockfile, and `package-lock.json` v1.
- Catalog entries declared over several lines, `version` ranges (`strictly`, `require`), and `version.ref` keys missing from `[versions]`.
- pnpm lockfile v5 (`/name/1.2.3` keys), and `requirements.txt` lines that aren't an exact `==` pin.
- Other ecosystems (Go…).

No new dependencies: the new formats are read by hand.

See `LIMITATIONS.md` at the repository root for the API gaps behind these.

## Permissions

| Permission | Used for |
|---|---|
| `net:api.osv.dev` (optional) | Query the OSV.dev vulnerability database. Only package names and versions are sent (see Privacy). No other host is contacted |
| `project.read` (required) | Read the manifests and lockfiles in the table above |
| `storage` (required) | Cache scan results so unchanged dependencies are not queried again |
| `editor.read`, `editor.write` (optional) | Preview and apply one confirmed, versioned edit in the open source declaration |
| `events` (optional) | Re-check when a manifest is opened or saved. Without it, run the scan from the command |

```sh
cargo test
openidea-plugin build
openidea-plugin link
```

## Live and before-commit evidence

Live diagnostics do not fetch, scan files, migrate caches or publish a second manual
Problems producer. They parse current text and read bounded OSV caches. Positive
results include the query's Unix date, with an explicit stale label after 24 hours.
An unknown coordinate is quiet, which does not establish that it is safe. Ignore
settings apply immediately. Documents above 512 KiB are skipped; work is bounded.

A lock-dependent ranged manifest can reuse its last scanned facts only while its
full text is unchanged. Editing that manifest, changing a watched dependency file,
or reclaiming the plugin instance discards those facts; exact current declarations
and lockfile documents still use their own cached coordinates. Reopen/save or rescan
to warm lock-dependent facts. This deliberately avoids reading siblings while typing.
The OSV network grant is optional, so cached browsing and diagnostics remain useful
offline. Automatic refresh still follows the Check on open and save setting.

**Review cached dependency scan** is available in the host's Commit Options and
needs optional `vcs.read`. The host initially enables contributed checks; disable
this entry there if you do not want it or have not granted VCS access. The check
never fetches or reads working-tree files. It reports previous findings only for
changed dependency files, with the scan date; a missing/stale/incomplete report or
uncovered changed file asks for review. Deleted and unrelated files are skipped.
A passed check is not a certification of selected hunks: the API supplies diffs,
and the stored scan may describe different contents. Review the included changes;
the host supports Commit Anyway for acknowledged problems.

Detailed parser, build-resolution and security-analysis coverage is tracked in the
[coverage inventory](../../docs/parity-2026-11-08-refresh/dependency-audit-coverage.md).
OSV continuation pages are followed within explicit limits; incomplete responses do
not replace the previous report. Reaching the per-glob source discovery cap is now
labelled partial in the report and cached commit review.


Packaged report freshness: explicit project scan completion (including retained
last-good/failure status), successful confirmed upgrade, scoped ignore/restore,
observed saved-file cache clearing and existing opted-in open-check cache commits
now invalidate the Vulnerable Dependencies page with a model-free message as well
as native refresh. An observed editor focus that changes project identity also
clears presentation caches and invalidates. The page requests its own current
frame/project model; render never starts a scan, fetch or invalidation loop. This
is separate from immutable packaged-asset revisioning and is not a durable watcher.
Optional event grant/host delivery and final event/render races still apply.

Before historical rows are displayed or used by actions, primary manifest/lock and
all recorded declaration paths must belong to the invoking project. Ownership
checks are filesystem/network-free and deduplicated per unique evidence path;
at most512 paths and a cooperative50ms proof are accepted. Exceeding the bound,
unknown/foreign/nested project ownership or denied storage/root access returns a
replacement empty explanation model, clearing old targets/details. Individual host
calls cannot be preempted by the cooperative deadline. Retained report bytes are
not deleted: restore access, choose the appropriate narrower project or explicitly
rescan. Missing reports also replace old rows; none of these states certifies a
clean dependency graph.

Existing explicit/open/save checks and editor quick fixes read an npm/Composer
sibling lock only when it has the same project owner. Missing/foreign lock evidence
does not authorize a ranged declaration; exact current declaration validation and
versioned upgrade confirmation remain. Historical report ownership is not a fresh
build/resolver or source/lock byte-identity proof. Live cached lock-dependent facts
still require unchanged manifest text and observed file invalidation; missed lock
events remain a limitation. No effective dependency resolver, package install,
lock regeneration, new OSV/provider network path or native Security-tab integration
is added. Earlier native upgrade/ignore receipts remain dated; new packaged refresh
and owner-refusal native acceptance remains pending.

Dependency118 Gradle source admission no longer treats arbitrary quoted coordinates as editable declarations. The bounded scanner skips line/nested block comments and triple-string examples, requires a supported configuration/platform invocation, and refuses computed literal prefixes. Groovy slashy/regex forms conservatively withhold this parser's file results. The registered preview-upgrade regression previously edited a comment; it now refuses unchanged. Custom configurations/advanced Gradle expressions remain outside this source subset; no build resolution or compatibility proof is inferred.

Dependency119 POM source authority requires a single project root, unique group/artifact/version children per dependency, and one root-owned literal property for property-based versions. Duplicate property names, profile-only properties and profile shadowing do not select an arbitrary editable version span. This is source-local ambiguity handling, not effective Maven model/profile/parent resolution. Registered preview-upgrade regressions refuse these cases without mutation.

Dependency120 carries the planned source project identity through direct resource-command upgrade application and rechecks current innermost ownership immediately before its versioned editor apply. Opening a nested project between preview planning and application now refuses unchanged. This source/mock boundary regression does not claim a native modal interleaving test or atomic project-ownership CAS.

Dependency121 resource upgrade and scoped-ignore commands revalidate retained raw report identity and bounded current source ownership after confirmation. Upgrade also requires the filtered selected row to match the reviewed row; scoped ignore rechecks selected membership before its existing storage comparison. Replaced reports, newly nested source owners and changed selected evidence refuse unchanged. This is source/MockHost boundary coverage, not native modal race acceptance or atomic multi-resource CAS.

Dependency122 POM version extraction refuses non-self-closing elements without their own matching closing container. The shared XML scanner remains tolerant for other editor features; this private admission guard prevents its unfinished/mismatched recovery from establishing editable dependency spans. Registered upgrade regression covers unclosed project/dependencies and mismatched dependency closure. This is bounded container admission, not full XML namespace/schema/DTD validation or Maven model resolution.

Dependency123 POM version evidence inherits default namespace ownership from the single project root. Unnamespaced and canonical Maven roots remain supported; foreign roots, rebound dependency/version/property namespaces and prefixed/foreign ancestry do not supply editable Maven spans. Registered command tests cover foreign root/container/version shapes; canonical namespace positive source parsing remains passing. This does not add general prefix resolution or full XML/Maven model validation.

Dependency125 POM dependency-shaped plugin configuration/custom payload no longer becomes Maven edit authority. A16-ancestor source-site guard admits project dependencies/dependencyManagement and build plugin/pluginManagement dependency declarations, including profile source variants. The positive matrix checks eight structural variants; arbitrary configuration payload stays unresolved. This does not determine active profiles, resolved models or runtime plugin configuration semantics.
