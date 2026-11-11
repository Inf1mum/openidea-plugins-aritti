#!/usr/bin/env python3
"""Verify native conversion retires generated assets without deleting custom UI."""
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[1]
with tempfile.TemporaryDirectory(prefix="native-ui-sync-") as temporary:
    repo = Path(temporary)
    (repo / "scripts").mkdir()
    shutil.copyfile(ROOT / "scripts/sync-plugin-ui.py", repo / "scripts/sync-plugin-ui.py")
    source = repo / "crates/ui-kit/web"
    source.mkdir(parents=True)
    (source / "hub.html").write_text("<main>shared</main>")
    package = repo / "plugins/example"
    package.mkdir(parents=True)
    manifest = package / "plugin.toml"
    base = 'id="aritti.example"\nname="Example"\n[[contributes.views]]\nid="main"\ntitle="Example"\n'
    manifest.write_text(base + 'web="web/resources/hub.html"\n')

    def invoke(*args, good=True):
        result = subprocess.run([sys.executable, str(repo / "scripts/sync-plugin-ui.py"), *args], capture_output=True, text=True)
        assert (result.returncode == 0) == good, result.stdout + result.stderr
        return result

    invoke()
    generated = package / "web/resources/hub.html"
    custom = package / "web/resources/custom.html"
    custom.write_text("custom UI must survive")
    manifest.write_text(base)
    before = (package / "src/ui_config.json").read_bytes()
    invoke("--check", good=False)
    assert generated.exists() and (package / "src/ui_config.json").read_bytes() == before
    invoke()
    assert not generated.exists() and custom.read_text() == "custom UI must survive"
    assert json.loads((package / "src/ui_config.json").read_text())["views"] == {}
    invoke("--check")
    manifest.write_text(base + 'web="web/resources/hub.html"\n')
    invoke()
    generated.write_text("hand-edited content")
    manifest.write_text(base)
    assert "Refusing to remove modified" in invoke(good=False).stderr
    assert generated.read_text() == "hand-edited content"
    generated.write_text("<main>shared</main>")
    for filename in ('tokens.css', 'components.css', 'native-icons.js', 'icons.js', 'widgets.js', 'controls.js'):
        (source / filename).write_text(f"/* fixture {filename} */")
    standalone = repo / 'plugins/standalone'
    (standalone / 'web').mkdir(parents=True)
    (standalone / 'plugin.toml').write_text('id="aritti.standalone"\nname="Standalone"\n[[contributes.customEditors]]\nid="preview"\nweb="web/index.html"\n')
    entry = standalone / 'web/index.html'
    entry.write_text('<main>Handwritten panel without kit</main>')
    assert 'must load the shared UI kit' in invoke('--check', good=False).stderr
    assert not (standalone / 'web/resources').exists()
    markup = ''.join(f'<link rel="stylesheet" href="./resources/{name}.css">' for name in ('tokens', 'components'))
    markup += ''.join(f'<script src="./resources/{name}.js"></script>' for name in ('native-icons', 'icons', 'widgets', 'controls'))
    entry.write_text(markup)
    invoke('--check', good=False)
    assert not (standalone / 'web/resources').exists(), 'standalone check wrote assets'
    invoke()
    assert (standalone / 'web/resources/controls.js').read_text() == '/* fixture controls.js */'
    assert not (standalone / 'src/ui_config.json').exists(), 'standalone editor invented a hub bridge'
    invoke('--check')
    entry.write_text(markup.replace('./resources/controls.js', 'https://example.invalid/controls.js'))
    assert 'must load the shared UI kit' in invoke('--check', good=False).stderr
    entry.write_text('<body data-ui-raw>' + markup + '</body>')
    assert 'cannot opt out' in invoke('--check', good=False).stderr
    # Explicit selection must never touch an excluded package, even when stale.
    manifest.write_text(base + 'web="web/resources/hub.html"\n')
    standalone_manifest = standalone / 'plugin.toml'
    standalone_manifest.write_text('id="aritti.standalone"\nname="Standalone"\n[[contributes.customEditors]]\nid="preview"\nweb="web/index.html"\n')
    entry.write_text(markup)
    invoke('--plugin', 'example')
    generated.write_text('stale excluded package')
    invoke('--exclude', 'example', '--check')
    assert generated.read_text() == 'stale excluded package'
    assert 'Unknown plugins' in invoke('--plugin', 'missing', good=False).stderr
print("PASS: native retirement, no-write check, custom/modified asset safety, standalone-only materialization and local-kit enforcement")
