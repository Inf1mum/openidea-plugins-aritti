# JDBC URL rules: evidence

Implemented in `helper/Helper.java` (`checkUrl`, `ALLOWED_KEYS`, `DENIED_KEYS`, `UNVERIFIED_VENDORS`, `*_URL` patterns). Tests: `crates/persistence-core/tests/jdbc_helper_java.rs` (`url_policy_table`, opt-in `JPA_JDBC_JAVA=1`).

Policy: a key outside the vendor's allowlist is always `UrlRejected` (URL query/semicolon tokens, including valueless ones, percent-decoded, plus `conn.props`). `DENIED_KEYS` is checked first for every vendor and only sharpens the message. Host-embedded properties, user info (`user:pw@`) and per-host `address=(...)` forms are rejected by the URL shape. Vendors in `UNVERIFIED_VENDORS` answer `UrlUnsupported` before any shape check. All pages retrieved 2026-11-12.

Status: PostgreSQL verified; MySQL verified; MariaDB verified; SQL Server verified; H2 partially verified (kept enabled, see below); Oracle unverified (disabled); Db2 unverified (disabled).

## PostgreSQL (verified)

| Rule | Docs URL | retrievedOn | Paraphrase |
|---|---|---|---|
| Shape `jdbc:postgresql:db`, `//`, `//host[:port][,host2[:port2]]/[db]` | https://jdbc.postgresql.org/documentation/use/ | 2026-11-12 | Six documented forms; comma-separated host:port pairs give fail-over; reserved characters must be percent-encoded. |
| Rejected: `sslfactory`, `sslfactoryarg`, `sslhostnameverifier`, `sslpasswordcallback`, `socketFactory`, `socketFactoryArg`, `connectExecutor(Arg)`, `authenticationPluginClassName`, `classLoaderStrategy` | same | 2026-11-12 | Each names a class the driver instantiates. |
| Rejected: `sslkey`, `sslcert`, `sslpassword`, `loggerFile`, `loggerLevel` | same | 2026-11-12 | Client key/cert paths and key password; logger options are documented as ignored. `sslrootcert` (trusted root path) stays allowed because verify-full needs it. |
| Rejected: `gsslib`, `jaasApplicationName`, `jaasLogin`, `kerberosServerName`, `sspiServiceClass`, `options`, `replication`, `service`, `passfile` | same | 2026-11-12 | Authentication stack selection, server startup settings, replication mode; `service` and `passfile` are not in the page, so they are rejected as unknown. |
| Allowed: `user`, `ssl`, `sslmode`, `sslrootcert`, `ApplicationName`, `currentSchema`, `connectTimeout`, `loginTimeout`, `socketTimeout`, `tcpKeepAlive`, `readOnly`, `targetServerType`, `loadBalanceHosts`, `hostRecheckSeconds`, `sslResponseTimeout`, `channelBinding` | same | 2026-11-12 | Documented tuning and TLS-verification options; none loads a class, runs a script or opens a different transport. |

## MySQL Connector/J (verified)

| Rule | Docs URL | retrievedOn | Paraphrase |
|---|---|---|---|
| Schemes `jdbc:mysql:`, `jdbc:mysql+srv:`, optional `loadbalance:` / `replication:` | https://dev.mysql.com/doc/connector-j/en/connector-j-reference-jdbc-url-format.html | 2026-11-12 | Documented protocols (`mysqlx` is not JDBC). Host list is comma-separated, optional database, properties after `?` joined by `&`. Host-specific `address=(...)` / `(host=..,key=..)` forms and user info carry arbitrary properties, so the shape refuses them. |
| Rejected: `allowLoadLocalInfile`, `allowLoadLocalInfileInPath`, `allowUrlInLocalInfile`, `serverRSAPublicKeyFile`, `trustCertificateKeyStoreUrl`, `clientCertificateKeyStoreUrl`, key/trust manager, key store and SSL context providers | https://dev.mysql.com/doc/connector-j/en/connector-j-connp-props-security.html | 2026-11-12 | Local file or URL reads, key store locations, provider class names. |
| Rejected: `authenticationPlugins`, `disabledAuthenticationPlugins`, `defaultAuthenticationPlugin`, `ociConfigFile`, WebAuthn and OpenID callback handlers | https://dev.mysql.com/doc/connector-j/en/connector-j-connp-props-authentication.html | 2026-11-12 | Class names and a config file path. |
| Rejected: `logger`, `profilerEventHandler`, `autoGenerateTestcaseScript` | https://dev.mysql.com/doc/connector-j/en/connector-j-connp-props-debugging-profiling.html | 2026-11-12 | Two take class names; the page describes the script option as writing to stderr. |
| Rejected: `socketFactory`, `socksProxyHost`, `dnsSrv`, `localSocketAddress`, `useCompression` | https://dev.mysql.com/doc/connector-j/en/connector-j-connp-props-networking.html | 2026-11-12 | Class name, proxy and DNS redirection of the connection. |
| Rejected: interceptors, `propertiesTransform`, `clientInfoProvider`, cache factories, `useConfigs`, `sessionVariables`, `allowMultiQueries`, `autoDeserialize` | https://dev.mysql.com/doc/connector-j/en/connector-j-reference-configuration-properties.html | 2026-11-12 | Summary tables list these as class-valued or behaviour-changing. The page shows only names (descriptions live on the sub-pages above), so those items rest on name plus sub-page context; they are blocked by the allowlist regardless. |
| Allowed: `user`, `sslMode`, `allowPublicKeyRetrieval`, `tlsVersions`, `tlsCiphersuites` | security and authentication pages above | 2026-11-12 | TLS level and protocol selection, key retrieval from the server. |
| Allowed: `connectTimeout`, `socketTimeout` | networking page | 2026-11-12 | Millisecond timeouts (the helper overrides them). |
| Allowed: `connectionTimeZone`, `serverTimezone` | https://dev.mysql.com/doc/connector-j/en/connector-j-connp-props-datetime-types-processing.html | 2026-11-12 | Time zone conversion; the SQL-running variant `forceConnectionTimeZoneToSession` is not allowed. |
| Allowed: `nullDatabaseMeansCurrent`, `useInformationSchema`, `getProceduresReturnsFunctions` | https://dev.mysql.com/doc/connector-j/en/connector-j-connp-props-metadata.html | 2026-11-12 | Metadata behaviour only. |

Note: the sub-pages are version-labelled inconsistently by the site (6.3.x text for 8/9 features); the property semantics used are the ones the pages state.

## MariaDB Connector/J (verified)

| Rule | Docs URL | retrievedOn | Paraphrase |
|---|---|---|---|
| Shape `jdbc:mariadb:[mode:]//host[:port][,host...]/[db][?k=v&...]`, modes `replication`, `loadbalance`, `sequential`, `load-balance-read` | https://mariadb.com/docs/connectors/mariadb-connector-j/about-mariadb-connector-j | 2026-11-12 | Documented syntax; hosts may also be `address=(...)` groups with `localSocket` or `pipe`, which the shape refuses. `jdbc:mysql:` is accepted by this driver only with `permitMysqlScheme`, which is rejected. |
| Rejected: `socketFactory`, `credentialType`, `tlsSocketType`, `localSocket`, `pipe`, `localSocketAddress`, `allowLocalInfile`, `serverSslCert`, `keyStore`, `trustStore`, `serverRsaPublicKeyFile`, `initSql`, `sessionVariables`, `permitMysqlScheme` | https://mariadb.com/docs/connectors/mariadb-connector-j/about-mariadb-connector-j/connection-parameters | 2026-11-12 | Class or plugin selection, file paths, other transports, SQL run at connect. |
| Allowed: `user`, `sslMode`, `connectTimeout`, `allowPublicKeyRetrieval` | connection-parameters page and about page | 2026-11-12 | TLS level, timeout, key retrieval. `socketTimeout` is not on the fetched page, so it is not allowed (the helper forces it). |

## SQL Server, mssql-jdbc (verified)

| Rule | Docs URL | retrievedOn | Paraphrase |
|---|---|---|---|
| Shape `jdbc:sqlserver://[serverName[\instance][:port]][;property=value...]` | https://learn.microsoft.com/en-us/sql/connect/jdbc/building-the-connection-url | 2026-11-12 | Server part is optional (name can come from `serverName`), `;` is the only property delimiter, duplicates not allowed, braces escape special characters. IPv6 literals are not supported in the URL host. Page advises against building URLs from user input. |
| Rejected: `authenticationScheme`, `authentication`, `integratedSecurity`, `accessToken`, `accessTokenCallbackClass`, `socketFactoryClass`, `trustManagerClass`, `trustManagerConstructorArg`, `keyStoreLocation`, `keyStoreAuthentication`, `keyStoreSecret`, `clientCertificate`, `clientKey`, `trustStore`, `jaasConfigurationName`, `useDefaultJaasConfig`, `failoverPartner`, `gsscredential`, `fipsProvider`, `enclaveAttestationUrl`, `password` | https://learn.microsoft.com/en-us/sql/connect/jdbc/setting-the-connection-properties | 2026-11-12 | Native library loading (`NativeAuthentication` loads an auth DLL), class names, key and trust store paths, JAAS, client cert/key files, alternative hosts. |
| Allowed: `user`, `databaseName`/`database`, `serverName`, `portNumber`, `instanceName`, `encrypt`, `trustServerCertificate`, `hostNameInCertificate`, `applicationIntent`, `loginTimeout`, `applicationName` | same | 2026-11-12 | Plain addressing, TLS flags and a label. Every other documented property (about 90) is refused by the allowlist, including `sslProtocol`, `multiSubnetFailover`, `useDefaultGSSCredential`, `msiClientId`. |

## H2 (partially verified, kept enabled)

| Rule | Docs URL | retrievedOn | Paraphrase |
|---|---|---|---|
| Shape `jdbc:h2:file:<path>`, `jdbc:h2:tcp://...`, `jdbc:h2:/abs`, `jdbc:h2:~/...`; no `;` settings | https://h2database.com/html/features.html | 2026-11-12 | Embedded, tcp, ssl, zip, mem forms are documented; settings follow `;`. `mem`, `zip`, `ssl` and relative paths are not accepted. `INIT`, `RUNSCRIPT`, `TRACE_LEVEL_*`, `AUTO_SERVER` are rejected. |
| Forced `ACCESS_MODE_DATA=r`, `IFEXISTS=TRUE` for file databases | same | 2026-11-12 | Read-only open, no create. |
| Unsettled | same | 2026-11-12 | The page does not say whether opening an untrusted database file runs stored trigger or alias code (alias source is recompiled on reopen) and does not discuss untrusted files. H2 stays enabled because it is the helper's test vehicle and projects commonly use local H2 files; the mandatory connect confirmation (D8) is the guard. Needs a user decision if untrusted repositories must be covered. |

## Oracle (unverified, disabled)

| Rule | Docs URL | retrievedOn | Paraphrase |
|---|---|---|---|
| Shape `jdbc:oracle:thin:@//host[:port]/service` only | https://docs.oracle.com/en/database/oracle/oracle-database/23/jjdbc/data-sources-and-URLs.html | 2026-11-12 | Easy Connect Plus is self-contained. The page documents no `host:port:SID` form, so the earlier SID form was dropped. Alias, descriptor, `tcps://?wallet_location=`, `?TNS_ADMIN=`, `ldap` forms read files or directories and are refused by the shape. User/password in the URL is refused. |
| Property keys denied (dotted `oracle.net.*`, `tns_admin`, `wallet_location`, `javax.net.ssl.*`, `oracle.jdbc.config`) | https://docs.oracle.com/en/database/oracle/oracle-database/23/jajdb/oracle/jdbc/OracleConnection.html | 2026-11-12 | Property reference lists wallet, TNS_ADMIN, key/trust store, authentication services, ONS wallet, proxy, and a connection-properties file / configuration provider constants. |
| Unverified | same | 2026-11-12 | The 100000-character window of the reference did not reach where an `ojdbc.properties` file is looked up (the page says properties can come from one) or what configuration providers can load. A hostile repository could plant such a file if the lookup uses the working directory. The developer's guide getting-started page failed to load (redirect loop). Disabled until this is settled. |

## Db2, IBM JCC (unverified, disabled)

| Rule | Docs URL | retrievedOn | Paraphrase |
|---|---|---|---|
| Shape `jdbc:db2://server[:port]/database[:name=value;...]` | https://ibm.com/support/knowledgecenter/SSEPEK_10.0.0/java/src/tpc/imjcc_r0052342.html and https://www.ibm.com/docs/en/db2/11.5.x?topic=hjacds-connecting-data-source-using-drivermanager-interface-data-server-driver-jdbc-sqlj | 2026-11-12 | Options follow a colon, each pair ends with `;`, no whitespace. The 10.0 page also lists `jdbc:db2j:net:`, `jdbc:ibmdb:`, `jdbc:ids:` (refused) and a default port of 446 for z/OS. Only the URL shape is settled. |
| Unverified | https://www.ibm.com/docs/en/db2/11.5.x?topic=cdsudidsdjs-properties-data-server-driver-jdbc-sqlj | 2026-11-12 | The 11.5 property reference is 750 KB and only its first 100 KB (a configuration keyword index) was read: no authoritative list of class-, file-, plugin-, trace- or keystore-valued JCC properties (`pluginName`, `securityMechanism`, `sslKeyStoreLocation`, trace options). Allowlist is `user` only and the vendor is disabled. |
