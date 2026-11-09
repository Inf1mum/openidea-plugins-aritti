# Third-party software

`web/vendor/` bundles **Swagger UI** (`swagger-ui-dist` 5.34.1), Apache License 2.0, Copyright 2020-2021 SmartBear Software Inc. —
https://github.com/swagger-api/swagger-ui. Its licence, notice and the licences of the libraries it
bundles are in `web/vendor/LICENSE`, `web/vendor/NOTICE` and `web/vendor/swagger-ui-bundle.js.LICENSE.txt`.

`web/vendor/` also bundles **Redoc** (`redoc` 2.5.4, `redoc.standalone.js`), MIT License, Copyright (c) 2015-present Rebilly, Inc. and (c) 2021-present Redocly Inc. —
https://github.com/Redocly/redoc. Its licence is in `web/vendor/redoc-LICENSE` and the licences of the libraries it bundles are in
`web/vendor/redoc.standalone.js.LICENSE.txt`. It is loaded only when the preview's Redoc button is used.

Source-backed YAML pointer navigation uses `yaml-rust2` 0.13 under its MIT license
option, Copyright (c) 2015 Chen Yuheng and Copyright (c) 2023 Ethiraric. The exact
MIT notice is included in `LICENSE-yaml-rust2`. Dependency source is not vendored;
anchors, aliases and tags are refused before loading this supported subset.
Its newly introduced dependencies are hashlink 0.12.2 (MIT option; derived in part
from linked-hash-map, Copyright 2015 The Rust Project Developers) and
arraydeque 0.5.1 (MIT, Copyright 2018 Andy Lok). Their exact notices are included in
`LICENSE-hashlink` and `LICENSE-arraydeque`.
