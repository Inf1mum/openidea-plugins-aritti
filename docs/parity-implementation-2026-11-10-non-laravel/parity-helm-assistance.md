# Helm assistance delivery — 2026-11-10

Owned new file: `plugins/kube-explorer/src/helm_assistance.rs`. Infrastructure owner wired its Cache into metadata/index/event/provider paths and owns manifest/command integration.

## Features

- Same-directory Chart.yaml/values.schema.json/values.yaml sources are bounded, project-owned and checked fresh before providers use them. Cached Chart.yaml can still drive key suggestions while its current edit is temporarily incomplete; other-file providers refuse stale metadata/schema.
- Values YAML uses the existing strict convert::source_documents parser; aliases/tags/duplicate keys and multiple documents remain unadmitted. Cursor location is established by parsing a synthetic marker in actual YAML, including nested mapping/sequence contexts, rather than guessing hierarchy from indentation.
- Local JSON-schema core validation/completion: properties, local JSON Pointer $ref definitions, allOf, type/type unions, required, enum/const, additionalProperties, arrays/items, numeric minimum/maximum. Integer and enum numeric equality accepts mathematically integral YAML floats. Remote refs/conditional branches and unsupported constraints stay quiet; providers never fetch schemas or spawn a process.
- Quoted/unquoted scalar choices and key replacements preserve existing colons, respect UTF16 ranges, suppress sibling duplicates and avoid block-scalar false completions.
- Explicit code action adds `# yaml-language-server: $schema=./values.schema.json`, preserving CRLF and any existing schema directives. Installed YAML-language-server supplies its full schema semantics through this declared association; bounded core plugin diagnostics do not claim full draft7 implementation.
- Chart dependency key and condition-boolean path completion; name/version, duplicate effective alias/name, tags and import-values shape diagnostics. Scalar-to-string metadata conversion matches Helm decoding instead of warning on valid numeric dependency versions.
- Explicit Cache.refresh_dependency_catalog(ctx) uses approved `helm repo list --output json`, per-declared-dependency `helm search repo <alias/name> --versions --output json`, then another repo-list signature check. No automatic repo updates or network access. Chart owner/source equality is revalidated before adopting the snapshot.
- Version suggestions are repository/name/alias scoped, support quoted edits, exclude unrelated chart results, show a UTC capture date/age and expire after10minutes. Provider calls only read this cache; changed dependency identities withdraw suggestions.

## Official source and runtime verification

- https://helm.sh/docs/topics/charts/ (values.schema.json, dependencies, conditions/tags/import-values)
- https://github.com/redhat-developer/yaml-language-server/blob/main/README.md (relative modeline schemas)
- Installed official Helm **v4.2.3+g43e8b7f**.
- `/tmp/parity-helm-assistance/probe.py`: valid local-ref chart schema `helm lint --strict` passes; missing required field/minimum/additionalProperties errors fail and match core checks; duplicate aliases fail with Helm's explicit validation error.
- `/tmp/parity-helm-assistance/catalog-probe.py`: disposable localhost repository index and fully isolated HELM_REPOSITORY_CONFIG/CACHE/REGISTRY_CONFIG; repo add/list/search all exit0 and report exact1.0.0/2.0.0 versions. No real user's Helm config/cache was changed.
- Seven module regressions cover nested refs/enum completion, live validation/freshness, aliases/duplicate/remote-ref refusal, dependency scopes/conditions, explicit schema opt-in, dated version cache/no provider processes and numeric JSONschema semantics.
- Full Kube Rust suite and release Wasm build logs: `/tmp/parity-helm-assistance-full-tests.log`, `/tmp/parity-helm-assistance-build.log`.

## Limits

Local core checks are a conservative subset; full Helm merged subchart/default/condition/import semantics remain the existing explicit approved `helm lint`/`helm template` adapter. Installed YAML-server behavior in the actual desktop is not newly accepted here. Dynamic Go-template dot/variable scopes and tpl/include semantics are not invented by values-schema assistance. Registry/OCI tag enumeration is not a Helm repo-index operation and is not falsely included. No GUI, cluster mutations, installs or publishing performed.
