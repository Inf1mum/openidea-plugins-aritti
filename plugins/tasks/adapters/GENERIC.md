# Generic REST / Java-regex adapter

`generic_bridge.py` is the `generic` family of `selfHostedProfiles`. It uses the same
external credential-profile rules as [Redmine/Mantis](SELF_HOSTED.md) (0600 file, HTTPS DNS
host, no redirects, strict bounds) and runs as
`python3 -I -S generic_bridge.py PROFILE REQUEST OUTPUT`. Free-text Search Issues sends the
query to the profile-declared parameter. Java 17+ is needed only for `java-regex` parsers.

Profile (JSON parser; `apiKey` stays in this file only):

```json
{"version":1,"family":"generic","baseUrl":"https://tracker.example/team","apiKey":"USER_PROVISIONED_KEY",
 "auth":{"header":"Authorization","prefix":"Bearer "},
 "search":{"path":"/api/issues","queryParam":"q","pageParam":"page","pageSizeParam":"limit","pageSize":25},
 "detail":{"path":"/api/issues/{id}"},"webPath":"/issues/{id}",
 "parser":{"kind":"json","items":"/issues","detail":"/issue",
  "fields":{"id":"/key","summary":"/title","description":"/text","state":"/status","revision":"/version"}},
 "state":{"open":["OPEN"],"closed":["DONE"]},
 "close":{"method":"PATCH","path":"/api/issues/{id}","body":{"status":"DONE"},"guard":"etag","closedState":"DONE"}}
```

`parser.kind` may instead be `java-regex` (`pattern`, `flags`, `groups` mapping the five fields
to capture groups). `close` is optional; without it the family is read-only. Guard `etag` needs
a strong ETag; `fresh-read` re-reads before mutation and cannot exclude a concurrent update.
Issue ids are `[A-Za-z0-9._-]{1,64}`; the project id is always empty.
