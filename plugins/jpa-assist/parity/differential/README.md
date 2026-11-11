# Differential evidence kit

Purpose: compare what our JPA Assist produces with what the original JPA Buddy (`com.haulmont.jpab` 263.7307.112) produces on the same inputs. A human runs the original in a licensed IDE; this kit defines what to run, what to capture and how to compare. It produces evidence for individual scenarios. It never proves parity by itself.

## Rules

- Observe behavior only. Never decompile, disassemble or read the original's classes, resources or templates, and never copy implementation, strings, icons or templates from it.
- Record only observable outputs: files the action creates or edits, dialog text and defaults as shown on screen, diagnostics the editor displays, messages verbatim.
- Never commit the original plugin binary, its jar, or an archive of it. Captures contain only text produced from our fixture projects plus screenshots of the UI.
- Captures from the original are third-party output derived from our own fixture inputs. Keep them under `captures/` locally or attach them to the receipt; do not paste UI strings into source code or templates.

## Install the reference plugin

1. Use a separate IDE profile so nothing leaks into the daily one: start the IDE with its own config/system directories (`-Didea.config.path`, `-Didea.system.path`, `-Didea.plugins.path`, or a fresh Toolbox installation).
2. Install JPA Buddy version `263.7307.112` (marketplace id 15075) from a locally downloaded archive via Settings > Plugins > Install Plugin from Disk. Check the SHA-256 of the archive equals `reference.archiveSha256` in `plugins/jpa-assist/parity/requirements.json` before installing (`shasum -a 256 <archive>`). Stop if it differs.
3. Do not install OpenIDEA JPA Assist in this profile. For the "ours" side use OpenIDEA with this plugin and no JPA Buddy.
4. Fixtures live in `plugins/jpa-assist/parity/fixtures/<name>/` (boot-jpa-java-jakarta, boot-jpa-kotlin-jakarta, legacy-javax-hibernate5, envers-flyway-liquibase, mapstruct-lombok-dto, multimodule-gradle). Copy a fixture to a fresh directory per run so the two sides start from identical content. Each fixture has a `README.md` (layout) and a `GOLDEN.md` (expected diagnostics: rule, file, line); some scenarios need a file swapped in first (persistence variants, pom variants, the Liquibase overlay): the scenario's `swap` list gives the exact command, and `plugins/jpa-assist/parity/ACCEPTANCE.md` (Swap recipes, Scratch files) repeats them.

## Run one scenario

1. Open `scenarios/DF-NNN-*.json`. It names the fixture, the input file (`inputFile`, a path that exists inside the fixture; `inputFiles` lists further ones, a path starting with `fixtures/` belongs to another fixture; `inputNote` says where the closest existing file stands in for a file the tester must create, or what is missing), `swap` steps (`from` is an existing fixture file, `step` the exact command to run in the copy), our action, the closest documented original action (`originalActionDocumented: false` means the name is not documented; find the entry point in the IDE menus and record where it was), the inputs to give and what to capture.
2. Fill a copy of `CAPTURE_TEMPLATE.md` while running. Do both sides with identical inputs.
3. After each side, copy every created or edited file (keep the relative path inside the project) to:
   - `captures/DF-NNN/ours/` and `captures/DF-NNN/theirs/` (one subdirectory per input variant, same names on both sides, e.g. `variant-uuid-sequence/`).
   - Diagnostics as `diagnostics.json`: a list of `{file, line, column, severity, message, fixes}`.
   - Screenshots as `DF-NNN_<side>_<step>.png`, e.g. `DF-003_theirs_01-dialog-defaults.png`.
4. Compare each variant:

```sh
python3 scripts/compare-jpa-output.py --ours captures/DF-001/ours/variant-a --theirs captures/DF-001/theirs/variant-a --scenario plugins/jpa-assist/parity/differential/scenarios/DF-001-entity-id-types.json
```

   Exit 0 means identical after the scenario's normalizers. Exit 1 lists files only on one side and a unified diff per differing file, classified `cosmetic` (equal under a stricter whitespace/order-insensitive pass) or `semantic`. Add `--member-order` or `--sql-statement-order` only when that order is known not to matter, and say so in the receipt.
5. Behavior that is not an output (prompts, defaults, refusals, undo) goes into the capture sheet, not the comparator.

## Fixture references are checked

`python3 scripts/check-jpa-receipts.py --check-fixtures` (also part of the default run) fails when a scenario's `fixture` directory, `inputFile`, `inputFiles`, `goldenRef` or `swap[].from` does not exist under `plugins/jpa-assist/parity/fixtures/`. Keep every such field a plain existing path (no globs, no prose); put prose in `inputNote`. Diagnostics scenarios cite the fixture's `GOLDEN.md` rows. The 14 rule codes GOLDEN.md lists as legacy-only (never emitted by the live provider) cannot be observed in the IDE and are out of scope for DF-021 and DF-022.

## Naming

- Scenario files: `DF-NNN-slug.json`. Capture sheet: `captures/DF-NNN/sheet.md`.
- File captures keep project-relative paths. Never rename files to make them match.

## Attach to receipts

Receipts are defined by `plugins/jpa-assist/parity/receipts/receipt.schema.json` (owned by the checklist agent; do not edit it from here). For each scenario, create the receipt that schema describes, naming the scenario id, the ledger rows from the scenario file, the comparator output (`--json` form), the filled capture sheet and screenshot names. A receipt records what was observed; closing a ledger row remains a separate reviewed decision.

## Open items per scenario

Scenarios list the original action name as recalled from public documentation. Correct them from the installed menus. Names marked `unknown` are not documented.
