#!/usr/bin/env python3
"""Stage hash-bound opt-in API14 overlays; never retarget canonical packages."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tomllib

ROOT = Path(__file__).resolve().parents[1]
CONFIG = ROOT / "api14/candidates.json"
EXCLUDED = {"target", "dist", ".git", "__pycache__"}


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def inventory(directory):
    result = {}
    for path in sorted(directory.rglob("*")):
        if any(part in EXCLUDED for part in path.relative_to(directory).parts):
            continue
        if path.is_symlink():
            raise ValueError(f"symlink input refused: {path}")
        if path.is_file():
            result[str(path.relative_to(ROOT))] = digest(path)
    return result


def inputs(config, names):
    result = {}
    for directory in config["shared"] + [f"plugins/{name}" for name in names]:
        result.update(inventory(ROOT / directory))
    for name in names:
        for key in ("overlay", "lock"):
            path = config["candidates"][name][key]
            result[path] = digest(ROOT / path)
    return result


def check_inputs(config, names):
    actual = inputs(config, names)
    expected = {p: h for name in names for p, h in config["candidates"][name]["inputs"].items()}
    changed = sorted(p for p in actual.keys() | expected.keys() if actual.get(p) != expected.get(p))
    if changed:
        raise ValueError("source/overlay/lock drift; review and rebase explicitly, refusing: " + ", ".join(changed))
    return actual


def sdk_manifest(path, config, extra=None):
    text = path.read_text()
    data = tomllib.loads(text)
    old = data["dependencies"]["openidea-plugin"]
    if old != {"git": config["sdk_git"], "tag": "v0.1.39"}:
        raise ValueError(f"unexpected SDK dependency shape: {path}")
    replacement = f'openidea-plugin = {{ git = "{config["sdk_git"]}", rev = "{config["sdk_rev"]}" }}'
    text, count = re.subn(r'^openidea-plugin\s*=.*$', replacement, text, flags=re.M)
    if count != 1:
        raise ValueError(f"ambiguous SDK dependency: {path}")
    if extra:
        for name, definition in extra.items():
            if name in data["dependencies"]:
                raise ValueError(f"unexpected existing staged-only dependency {name}")
            text = text.replace("[dependencies]\n", f"[dependencies]\n{name} = {definition}\n", 1)
    path.write_text(text)


def run(argv, cwd, log, env, expected=0):
    with log.open("w") as output:
        result = subprocess.run([str(a) for a in argv], cwd=cwd, env=env, stdout=output, stderr=subprocess.STDOUT)
    if result.returncode != expected:
        raise ValueError(f"command exit {result.returncode}, expected {expected}; see {log}")
    return {"argv": [str(a) for a in argv], "exit": result.returncode, "log": str(log)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--plugin", action="append", required=True, choices=["blade", "devcontainer", "ignore"])
    parser.add_argument("--output", type=Path, required=True, help="new directory outside this repository")
    parser.add_argument("--dry-run", action="store_true", help="verify exact inputs and print plan; no writes or fetches")
    parser.add_argument("--validate", action="store_true", help="tests, Wasm build, real main14 linker/checker, pinned13 refusal")
    parser.add_argument("--checker13", type=Path, help="pinned v0.1.39 openidea-plugin executable")
    parser.add_argument("--component-checker14", type=Path, help="main14 real Manifest::load + check_component probe; accepts package dir")
    parser.add_argument("--target-dir", type=Path, help="dedicated reusable candidate Cargo target; default OUTPUT/target")
    parser.add_argument("--offline", action="store_true")
    args = parser.parse_args()
    names = sorted(set(args.plugin))
    config = json.loads(CONFIG.read_text())
    before = check_inputs(config, names)
    default_artifacts = {f"plugins/{name}/dist/plugin.wasm": digest(ROOT / f"plugins/{name}/dist/plugin.wasm") for name in names if (ROOT / f"plugins/{name}/dist/plugin.wasm").is_file()}
    output = args.output.expanduser().resolve()
    if output == ROOT or ROOT in output.parents or output in ROOT.parents:
        raise ValueError("output must be separate from repository and its ancestors")
    if output.exists():
        raise ValueError("output already exists; refusing to merge/overwrite a previous candidate")
    target = (args.target_dir or output / "target").expanduser().resolve()
    if target == ROOT or ROOT in target.parents or target in ROOT.parents:
        raise ValueError("target directory must be outside repository and its ancestors")
    if args.validate and not (args.checker13 and args.component_checker14):
        raise ValueError("validation requires both explicit matching checker paths")
    plan = {"plugins": names, "sdk_git": config["sdk_git"], "sdk_rev": config["sdk_rev"], "engine": "^1.4", "output": str(output), "input_files": len(before)}
    print(json.dumps(plan, indent=2), flush=True)
    if args.dry_run:
        return
    output.mkdir(parents=True)
    for directory in config["shared"] + [f"plugins/{name}" for name in names]:
        shutil.copytree(ROOT / directory, output / directory, ignore=shutil.ignore_patterns(*EXCLUDED))
    for name in names:
        candidate = config["candidates"][name]
        patch = ROOT / candidate["overlay"]
        # No fuzzy patching or mutable /tmp prototype lookup. Preconditions bind every source file.
        subprocess.run(["git", "apply", "--check", str(patch)], cwd=output, check=True)
        subprocess.run(["git", "apply", str(patch)], cwd=output, check=True)
        plugin = output / "plugins" / name
        sdk_manifest(plugin / "Cargo.toml", config, candidate.get("extra_dependencies"))
        shutil.copyfile(ROOT / candidate["lock"], plugin / "Cargo.lock")
        manifest = tomllib.loads((plugin / "plugin.toml").read_text())
        if manifest["engines"]["openidea"] != "^1.4":
            raise ValueError("overlay did not declare API14 engine")
    for directory in config["shared"]:
        sdk_manifest(output / directory / "Cargo.toml", config)
    if check_inputs(config, names) != before:
        raise ValueError("canonical inputs changed during staging; staged output is invalid")
    receipt = {"plan": plan, "canonical_inputs": before, "canonical_default_artifacts": default_artifacts, "script_sha256": digest(Path(__file__)), "catalog_sha256": digest(CONFIG), "canonical_untouched": True, "validation": {}, "complete": False}
    receipt_path = output / "receipt.json"
    receipt_path.write_text(json.dumps(receipt, indent=2) + "\n")
    if args.validate:
        env = dict(os.environ)
        env["CARGO_TARGET_DIR"] = str(target)
        receipt["checkers"] = {str(path.resolve()): digest(path.resolve()) for path in [args.checker13, args.component_checker14]}
        if args.offline:
            env["CARGO_NET_OFFLINE"] = "true"
        for name in names:
            plugin = output / "plugins" / name
            logs = output / "logs" / name
            logs.mkdir(parents=True)
            cargo = ["cargo"]
            flags = ["--locked"] + (["--offline"] if args.offline else [])
            steps = []
            steps.append(run(cargo + ["test"] + flags, plugin, logs / "tests.log", env))
            steps.append(run(cargo + ["build", "--target", "wasm32-wasip2", "--release"] + flags, plugin, logs / "build.log", env))
            package = tomllib.loads((plugin / "Cargo.toml").read_text())["package"]["name"].replace("-", "_")
            (plugin / "dist").mkdir()
            shutil.copyfile(Path(env["CARGO_TARGET_DIR"]) / "wasm32-wasip2/release" / (package + ".wasm"), plugin / "dist/plugin.wasm")
            steps.append(run([args.component_checker14.resolve(), plugin], plugin, logs / "main14-check.log", env))
            steps.append(run([args.checker13.resolve(), "check", plugin], plugin, logs / "pinned13-refusal.log", env, expected=1))
            refusal = (logs / "pinned13-refusal.log").read_text()
            if "^1.4" not in refusal or "1.3" not in refusal or "does not match" not in refusal:
                raise ValueError("pinned13 failure was not the expected engine refusal")
            if digest(plugin / "Cargo.lock") != digest(ROOT / config["candidates"][name]["lock"]):
                raise ValueError("staged lock changed despite --locked")
            receipt["validation"][name] = {"steps": steps, "wasm_sha256": digest(plugin / "dist/plugin.wasm")}
    if check_inputs(config, names) != before:
        raise ValueError("canonical inputs changed during build; refusing untouched claim")
    if any(digest(ROOT / path) != sha for path, sha in default_artifacts.items()):
        raise ValueError("canonical default artifact changed during staging/build")
    receipt["complete"] = True
    receipt_path.write_text(json.dumps(receipt, indent=2) + "\n")
    print(f"Complete: {receipt_path}")


if __name__ == "__main__":
    try:
        main()
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        print(f"API14 staging refused: {error}", file=sys.stderr)
        sys.exit(1)
