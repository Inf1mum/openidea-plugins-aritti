# Manifest command-condition smoke test

Requires Node.js 24+ and the OpenIDEA v0.1.40 source checkout. From the repository
root, export the actual manifest commands with Python 3.11+ and run:

```sh
python3 -c 'import tomllib,json; print(json.dumps(tomllib.load(open("plugins/ddev/plugin.toml","rb"))["contributes"]["commands"]))' > /tmp/ddev-commands.json
node plugins/ddev/tests/when-smoke.mjs /path/to/openidea/src/platform/context.ts /tmp/ddev-commands.json
```

The harness loads the host's actual TypeScript parser/evaluator and substitutes
only its Zustand store dependency. It checks the plugin's real manifest rules
against seven states: no configuration, unknown initial state, running with a
database, starting, stopped, installed CLI with failed description, and missing
CLI. Failed description with an installed CLI enables Start/Restart/Stop, while
a known stopped project enables Start and keeps Restart/Stop disabled, matching
the original action predicates. Fifteen selected command rules yield **105 checks**.

Native tests separately verify the context flags the plugin publishes. The
harness verifies condition grammar/behavior, including hyphenated plugin keys;
it does not render desktop menus or test real-time terminal completion.
