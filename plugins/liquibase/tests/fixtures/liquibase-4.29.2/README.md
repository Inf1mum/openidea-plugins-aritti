# Official Liquibase OSS 4.29.2 runtime fixtures

Captured 2026-11-10 using the official distribution from
https://github.com/liquibase/liquibase/releases/tag/v4.29.2 with Java26.0.1 on
macOS arm64. Core JAR SHA1 matched the independently retrieved Maven Central
`org/liquibase/liquibase-core/4.29.2/liquibase-core-4.29.2.jar.sha1`.

Reference disposable H2 database: `CUSTOMER(ID INT PRIMARY KEY, NAME VARCHAR(40))`
and index `CUSTOMER_NAME(NAME)`. Target disposable database: `LEGACY(ID INT)`.
Both were created under `/tmp/parity-liquibase-runtime/fixture`; no external or
user database was contacted.

Run `snapshot --url=jdbc:h2:file:<fixture>/<reference|target> --snapshot-format=json`;
then retain only database metadata shortName/productName/majorVersion/minorVersion/
productVersion, exactly as plugin staging does. The checked-in JSON files are
these scrubbed snapshots. The CLI's own H2 bookkeeping objects are retained.

With empty defaults, fixture search path and log-level off:

```text
diff --reference-url=offline:h2?snapshot=reference.json --url=offline:h2?snapshot=target.json --diff-types=tables,columns,indexes,views
diff-changelog --reference-url=offline:h2?snapshot=reference.json --url=offline:h2?snapshot=target.json --diff-types=tables,columns,indexes,views --changelog-file=<vacant-output>.xml --author=parity
```

`diff.txt` is exact stdout; `generated.xml` is exact generated UTF-8 source.
The runtime run also tested the command-specific environment bindings used by
the plugin; diff output was byte-identical and generated source contained the
expected create-table/index/drop-table changes. Those bindings reject inherited
key collisions in the API1.4 host; this is not a general environment sandbox.
No full desktop/process-transport/platform parity is claimed.

SHA256 of acquired runtime inputs:

- `liquibase-4.29.2.tar.gz`: `1d017a206a95ab3076a52f660679c2d80b9f2174942cb0715e35d53931e20ee9`
- `cli/internal/lib/liquibase-core.jar`: `e69b5ec55891bef987f2e3f83d199a210f3ef2a7fcd6f6c255a2327693739d40`
- `cli/internal/lib/h2.jar`: `b9d8f19358ada82a4f6eb5b174c6cfe320a375b5a9cb5a4fe456d623e6e55497`
- `cli/LICENSE.txt`: `4a2a405a698f0f9669f5ff6014a31e6b13343aa30a2b235289a7b34f33b67007`
