#!/usr/bin/env python3
"""Exercise staging safety at the actual CLI boundary, entirely in a scratch repo."""
from pathlib import Path
import json
import shutil
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[1]
config = json.loads((ROOT / "api14/candidates.json").read_text())
with tempfile.TemporaryDirectory(prefix="api14-staging-check-") as temporary:
    scratch = Path(temporary)
    repo = scratch / "repo"
    for directory in config["shared"] + [f"plugins/{name}" for name in config["candidates"]] + ["api14"]:
        shutil.copytree(ROOT / directory, repo / directory, ignore=shutil.ignore_patterns("target", "dist", ".git"))
    (repo / "scripts").mkdir()
    shutil.copyfile(ROOT / "scripts/stage-api14-candidates.py", repo / "scripts/stage-api14-candidates.py")
    def invoke(output, *extra, good=True):
        result = subprocess.run([sys.executable, str(repo / "scripts/stage-api14-candidates.py"), *[arg for name in sorted(config["candidates"]) for arg in ["--plugin", name]], "--output", str(output), *extra], capture_output=True, text=True)
        if (result.returncode == 0) != good:
            raise AssertionError(result.stdout + result.stderr)
        return result.stderr
    output = scratch / "candidate"
    invoke(output, "--dry-run")
    assert not output.exists(), "dry-run wrote output"
    invoke(output)
    assert (output / "plugins/blade/src/injections.rs").is_file()
    assert (output / "plugins/devcontainer/src/run_target.rs").is_file()
    assert (output / "plugins/ignore/src/file_hints.rs").is_file()
    assert (output / "plugins/ignore/src/actions.rs").is_file()
    assert (output / "plugins/ignore/src/preferences.rs").is_file()
    assert (output / "plugins/ignore/templates/Global").is_dir()
    assert "register_plugin!(Blade, syntax, diagnostics, hover, providers, injections)" in (output / "plugins/blade/src/lib.rs").read_text()
    assert "rev = \"95843ea12d0abf3e7726735489458e5903b86e7e\"" in (output / "crates/ui-kit/Cargo.toml").read_text()
    receipt = (output / "receipt.json").read_bytes()
    assert "already exists" in invoke(output, good=False)
    assert (output / "receipt.json").read_bytes() == receipt, "restaging overwrote output"
    assert "separate" in invoke(repo / "bad-output", good=False)
    assert "target directory" in invoke(scratch / "bad-target", "--target-dir", str(repo / "target"), good=False)
    source = repo / "plugins/blade/src/lib.rs"
    old = source.read_bytes()
    source.write_bytes(old + b"\n// deliberate stale source\n")
    assert "drift" in invoke(scratch / "drift-output", "--dry-run", good=False)
    assert not (scratch / "drift-output").exists()
    source.write_bytes(old)
    patch = repo / "api14/overlays/blade.patch"
    patch.write_bytes(patch.read_bytes() + b"\n")
    assert "drift" in invoke(scratch / "overlay-output", good=False)
    assert not (scratch / "overlay-output").exists()
print("PASS actual CLI: dry-run no writes, complete overlays, exact SDK staging, no overwrite, output/target isolation, source drift and overlay drift refusal")
