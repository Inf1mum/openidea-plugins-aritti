# GraphQL subscription adapter qualification

Run from the repository root with Python3 and an approved Node22+ runtime:

```sh
python3 plugins/spring-web/tests/graphql-subscriptions/qualify.py --receipt /tmp/graphql-subscription-receipt.json
```

The fixture uses only the Python standard library and Node's built-in WebSocket.
It opens disposable loopback sockets, checks the actual RFC6455 handshake and
masked client frames, and exercises the same chunked-source bootstrap used by
the plugin's released-host-compatible process invocation. No application,
credentials, external server, npm dependency or database is used.

Cases: Next/server Complete, client Complete/close cancellation, operation error,
operation message before acknowledgement, duplicate acknowledgement, invalid
JSON, bounded output termination and refusal to follow a cross-authority HTTP302.
The receipt records the actual Node version, embedded helper hash and frame/result
facts. This is an OS-tool contract fixture, not native OpenIDEA acceptance.

In the plugin, open one `.graphql`/`.gql` document containing an explicit
`subscription` operation and optional explicit fragment declarations. Run **Start
Reviewed GraphQL Subscription**, enter the exact ws(s) endpoint, JSON variables
and JSON connection initialization object, then review the complete payload.
Only the supplied payload is sent; no application/Actuator authentication is
copied. **Poll GraphQL Subscription Output** reads the owned client, and **Cancel
GraphQL Subscription** asks it to send Complete and close before a bounded kill
fallback. The optional `process:node` capability is required. Each session is
finite (120 seconds) and also ends when its plugin/window owner is disposed.
The input document is not proof of active server schema or resolver registration.
