#!/usr/bin/env python3
"""Opt-in DDEV/container smoke tests. Creates and deletes only its own project."""
import argparse
import json
import os
from pathlib import Path
import shutil
import socket
import subprocess
import tempfile
import xml.etree.ElementTree as ET

PLUGIN = Path(__file__).resolve().parents[1]
ENV = dict(os.environ, DDEV_NONINTERACTIVE="true", CARGO_INCREMENTAL="0")
HELPERS = {"ddev-router", "ddev-ssh-agent"}


def run(args, cwd=None, env=None, timeout=180):
    return subprocess.run(args, cwd=cwd, env=env or ENV, check=True, timeout=timeout)


def running_containers():
    result = subprocess.run(
        ["docker", "ps", "--format", "{{.Names}}"],
        env=ENV, check=True, capture_output=True, text=True, timeout=15,
    )
    return set(result.stdout.splitlines())


def debugger_packet(connection):
    length = bytearray()
    while True:
        piece = connection.recv(1)
        if piece == b"\0":
            break
        if not piece or len(length) > 10:
            raise RuntimeError("Invalid DBGp packet length")
        length.extend(piece)
    size = int(length)
    if size > 1024 * 1024:
        raise RuntimeError("DBGp packet too large")
    body = bytearray()
    while len(body) < size:
        piece = connection.recv(size - len(body))
        if not piece:
            raise RuntimeError("Unexpected DBGp EOF")
        body.extend(piece)
    if connection.recv(1) != b"\0":
        raise RuntimeError("Invalid DBGp terminator")
    return ET.fromstring(body)


def xdebug_probe(root):
    run(["ddev", "xdebug", "on"], root)
    with socket.socket() as server:
        # DDEV's connect-back adapter reaches the host via host.docker.internal.
        server.bind(("0.0.0.0", 0))
        server.listen(1)
        server.settimeout(20)
        port = server.getsockname()[1]
        args = ["ddev", "exec", "--raw", "--", "php", "-dxdebug.mode=debug",
                "-dxdebug.start_with_request=yes", "-dxdebug.client_host=host.docker.internal",
                f"-dxdebug.client_port={port}", "/var/www/html/app/sub dir/probe.php"]
        with subprocess.Popen(args, cwd=root, env=ENV, stdout=subprocess.PIPE,
                              stderr=subprocess.PIPE) as process:
            try:
                connection, _ = server.accept()
                with connection:
                    connection.settimeout(10)
                    init = debugger_packet(connection)
                    if not init.attrib["fileuri"].endswith("/app/sub%20dir/probe.php"):
                        raise RuntimeError("Unexpected Xdebug source mapping")
                    connection.sendall(b"run -i 1\0")
                    response = debugger_packet(connection)
                    if response.attrib.get("status") != "stopping":
                        raise RuntimeError("Unexpected Xdebug run response")
                process.communicate(timeout=15)
                if process.returncode != 0:
                    raise RuntimeError("Xdebug debuggee failed")
            finally:
                if process.poll() is None:
                    process.kill()
                    process.communicate()
    print("Live Xdebug DBGp connect-back and mapped file URI passed", flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--xdebug", action="store_true", help="also test live DBGp connect-back")
    options = parser.parse_args()
    wasm = PLUGIN / "dist/plugin.wasm"
    if not wasm.is_file():
        parser.error("Build plugins/ddev first (dist/plugin.wasm is required)")
    before = running_containers()
    root = Path(tempfile.mkdtemp(prefix="ddev-live-parity-")).resolve()
    name = "ddev-live-parity-" + root.name.rsplit("-", 1)[-1]
    configured = False
    try:
        (root / "public").mkdir()
        (root / "app/sub dir").mkdir(parents=True)
        (root / "public/index.php").write_text('<?php echo "ddev-parity-http-ok";')
        (root / "app/composer.json").write_text(json.dumps({
            "name": "parity/live-probe", "description": "Disposable DDEV probe",
            "scripts": {"parity": '@php -r \'echo "composer-root:" . basename(getcwd());\''},
        }))
        run(["ddev", "config", "--auto", f"--project-name={name}", "--project-type=php",
             "--docroot=public", "--php-version=8.4", "--performance-mode=none",
             "--composer-root=app", "--database=mariadb:11.8"], root)
        configured = True
        with (root / ".ddev/config.yaml").open("a") as config:
            config.write("\nomit_containers: [ddev-ssh-agent]\nhooks:\n"
                         "  pre-composer:\n    - exec-host: touch .parity-pre-composer\n"
                         "  post-composer:\n    - exec-host: touch .parity-post-composer\n")
        run(["ddev", "start"], root, timeout=600)
        live_env = dict(ENV, DDEV_LIVE_PROJECT=str(root))
        run(["cargo", "test", "--manifest-path", str(PLUGIN / "Cargo.toml"),
             "live_ddev_commands_and_description", "--", "--ignored"], env=live_env)
        run(["cargo", "run", "--manifest-path", str(PLUGIN / "tests/wasm-smoke/Cargo.toml"),
             "--bin", "workflows", "--", str(wasm)], env=live_env, timeout=600)
        if options.xdebug:
            xdebug_probe(root)
        print("Live smoke passed; IDE UI and native Database credential/connection APIs remain fixture-backed.", flush=True)
    finally:
        cleaned = not configured
        if configured:
            # Disable DDEV's default cleanup of unrelated obsolete containers.
            result = subprocess.run(
                ["ddev", "delete", "--omit-snapshot", "--yes", "--clean-containers=false", name],
                cwd=root, env=ENV, timeout=120,
            )
            cleaned = result.returncode == 0
        if cleaned:
            shutil.rmtree(root)
        else:
            print(f"Cleanup failed; disposable project retained at {root}", flush=True)
        remaining = running_containers()
        foreign_ddev = {item for item in remaining - HELPERS if item.startswith("ddev-")}
        if not foreign_ddev:
            for helper in sorted((remaining - before) & HELPERS):
                run(["docker", "stop", helper], timeout=30)
        if not cleaned:
            raise RuntimeError("Disposable project cleanup failed")


if __name__ == "__main__":
    main()
