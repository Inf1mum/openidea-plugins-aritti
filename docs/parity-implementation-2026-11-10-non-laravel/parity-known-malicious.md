Implemented known-malicious package/version evidence in Dependency Audit.

Data contract: exact active MAL-YYYY-number advisory IDs from existing OSV query results are classified as OpenSSF MAL reports via OSV.dev. Other namespaces remain maliciousness-unknown; withdrawn records are historical. Reporter source strings from malicious-packages-origins are retained as bounded text. Resource dependency/advisory rows include evidence class, source reporters, existing report scan/query freshness and advisory modified dates. Diagnostics identify package evidence and explicitly deny an execution inference. Known-malicious findings receive error diagnostics and no automatic upgrade; historical saved reports have recommendations suppressed before actions/render.

Official live metadata qualification (2026-11-11): api.osv.dev querybatch PyPI0-8@0.0.0.1 returned MAL-2025-2925; official advisory detail fetched into unchanged tests/fixtures/known-malicious-osv.json. SHA256/query/date response receipt /tmp/parity-known-malicious-receipt.json. Upstream attribution/license retained beside fixture (Apache2). No package archive downloaded or executed.

Tests: actual advisory reporter/classification, exact namespace/malformed/withdrawn refusal, no fix, explicit package-not-execution diagnostic, and old stored report recommendation suppression with dated provenance/cache-only render. Full package passed148 tests after scanner integration (scanner runtime qualification in progress; see separate report).

This is supported public known-malicious evidence, not full malware coverage, typosquatting heuristics or original proprietary service equivalence. Removed/ignored findings remain governed by existing explicit ignore actions.
