#!/usr/bin/env python3
"""Create a reviewed hash-pinned local profile; never downloads or runs a shell."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import subprocess
import sys
import zipfile
from qute_jdt_broker import child_environment

def metadata(jar, member):
    with zipfile.ZipFile(jar) as archive:
        return archive.read(member).decode().replace('\r\n ', '').replace('\r\n', '\n')

def bundle_version(jar):
    manifest = metadata(jar, 'META-INF/MANIFEST.MF')
    match = re.search(r'^Bundle-Version: (.+)$', manifest, re.M)
    if not match:
        raise ValueError('missing bundle version')
    return match.group(1).strip()

def create(args):
    java = Path(args.java).resolve()
    jdt = Path(args.jdt_home).resolve()
    quarkus = Path(args.quarkus_extension).resolve()
    microprofile = Path(args.microprofile_extension).resolve()
    if not java.is_file() or not os.access(java, os.X_OK):
        raise ValueError('select an executable Java runtime')
    version = subprocess.run([str(java), '-version'], capture_output=True, text=True, check=True, timeout=10, env=child_environment({'inherit_environment': args.inherit_environment}))
    version_text = version.stderr + version.stdout
    major = re.search(r'version "(\d+)', version_text)
    if not major or int(major.group(1)) < 21:
        raise ValueError('the reviewed JDT profile requires Java 21 or newer')
    qute = quarkus/'server/com.redhat.qute.ls-uber.jar'
    qute_jdt = quarkus/'jars/com.redhat.qute.jdt.jar'
    core = microprofile/'jars/org.eclipse.lsp4mp.jdt.core.jar'
    if 'version=0.25.0' not in metadata(qute, 'META-INF/maven/com.redhat.microprofile/com.redhat.qute.ls/pom.properties'):
        raise ValueError('reviewed Qute LS 0.25.0 is required')
    if not bundle_version(qute_jdt).startswith('0.25.0.') or not bundle_version(core).startswith('0.18.0.'):
        raise ValueError('reviewed Qute JDT 0.25.0 and LSP4MP JDT 0.18.0 bundles are required')
    launchers = list((jdt/'plugins').glob('org.eclipse.equinox.launcher_*.jar'))
    jdt_cores = list((jdt/'plugins').glob('org.eclipse.jdt.ls.core_*.jar'))
    if len(launchers) != 1 or len(jdt_cores) != 1 or not bundle_version(jdt_cores[0]).startswith('1.62.0.'):
        raise ValueError('reviewed JDT LS 1.62.0 distribution is required')
    machine = platform.machine().lower()
    system = platform.system()
    folder = {'Darwin': 'config_mac', 'Linux': 'config_linux', 'Windows': 'config_win'}.get(system)
    if folder is None:
        raise ValueError('unsupported operating system')
    if machine in ('arm64', 'aarch64'):
        folder += '_arm'
    configuration = jdt/folder
    if not (configuration/'config.ini').is_file():
        raise ValueError('matching JDT platform configuration is missing')
    bundles = sorted((quarkus/'jars').glob('*.jar')) + sorted((microprofile/'jars').glob('*.jar'))
    if not bundles or len(bundles) > 32:
        raise ValueError('unexpected JDT extension bundle inventory')
    overlay = getattr(args, 'config_overlay_bundle', None)
    if overlay:
        overlay = Path(overlay).resolve()
        if not args.with_microprofile or bundle_version(overlay) != '1.0.0' or 'Bundle-SymbolicName: openidea.quarkus.config.overlay' not in metadata(overlay, 'META-INF/MANIFEST.MF'):
            raise ValueError('reviewed configuration overlay bundle 1.0.0 and MicroProfile required')
        bundles.append(overlay)
    # Hash the complete selected JDT distribution, not just its launcher. Profiles
    # never bless a subsequently modified plugin or executable dependency.
    mp_server = microprofile/'server/org.eclipse.lsp4mp.ls-uber.jar'
    quarkus_server = quarkus/'server/com.redhat.quarkus.ls.jar'
    extra = [mp_server, quarkus_server] if args.with_microprofile else []
    if args.with_microprofile and 'version=0.18.0' not in metadata(mp_server, 'META-INF/maven/org.eclipse.lsp4mp/org.eclipse.lsp4mp.ls/pom.properties'):
        raise ValueError('reviewed MicroProfile LS 0.18.0 required')
    if args.with_microprofile and 'version=0.25.0' not in metadata(quarkus_server, 'META-INF/maven/com.redhat.microprofile/com.redhat.quarkus.ls/pom.properties'):
        raise ValueError('reviewed Quarkus LS extension 0.25.0 required')
    pinned = [java, qute, *extra, *bundles, *sorted((jdt/'plugins').glob('*.jar')), configuration/'config.ini']
    hashes = {}
    for file in pinned:
        with file.open('rb') as stream:
            hashes[str(file)] = hashlib.file_digest(stream, 'sha256').hexdigest()
    profile = {
        'version': 1,
        'inherit_environment': args.inherit_environment,
        'runtime_version_sha256': hashlib.sha256(version_text.encode()).hexdigest(),
        'qualified_profile': {'qute': '0.25.0 / vscode-quarkus 1.24.0-923', 'lsp4mp': '0.18.0 / vscode-microprofile 0.18.0-844', 'jdtls': '1.62.0-202610131109', 'java': major.group(1), 'platform': system + '/' + machine},
        'configuration_source': str(configuration/'config.ini'),
        'qute_command': [str(java), '-Xlog:disable', '-DwatchParentProcess=false', '-cp', str(qute), 'com.redhat.qute.ls.QuteServerLauncher'],
        'jdt_command': [str(java), '-Xlog:disable', '-Declipse.application=org.eclipse.jdt.ls.core.id1', '-Dosgi.bundles.defaultStartLevel=4', '-Declipse.product=org.eclipse.jdt.ls.core.product', '-Dlog.level=OFF', '-Xmx1G', '-jar', str(launchers[0]), '-configuration', '{workspace}/configuration', '-data', '{workspace}/data'],
        'bundles': [str(file) for file in bundles],
        'sha256': hashes,
    }
    if overlay:
        profile['configuration_overlay'] = True
        profile['jdt_command'].insert(2, '-Dopenidea.configOverlay={workspace}/config-overlay.json')
    if args.with_microprofile:
        profile['microprofile_command'] = [str(java), '-Xlog:disable', '-DwatchParentProcess=false', '-cp', str(mp_server)+os.pathsep+str(quarkus_server), 'org.eclipse.lsp4mp.ls.MicroProfileServerLauncher']
    return profile

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--java', required=True)
    parser.add_argument('--jdt-home', required=True)
    parser.add_argument('--quarkus-extension', required=True)
    parser.add_argument('--microprofile-extension', required=True)
    parser.add_argument('--output', required=True)
    parser.add_argument('--inherit-environment', action='append', default=[], help='explicit additional environment name made visible to the servers')
    parser.add_argument('--with-microprofile', action='store_true', help='also launch official MicroProfile LS with Quarkus extension')
    parser.add_argument('--config-overlay-bundle', help='reviewed local companion bundle for unsaved configuration sources')
    args = parser.parse_args()
    try:
        profile = create(args)
        path = Path(args.output)
        if not path.is_absolute():
            raise ValueError('profile output must be absolute')
        with path.open('x', encoding='utf-8') as stream:
            json.dump(profile, stream, indent=2)
            stream.write('\n')
        print('Created local Qute/JDT profile:', path)
    except (OSError, ValueError, KeyError, zipfile.BadZipFile, subprocess.SubprocessError) as error:
        print('Profile refused:', str(error), file=sys.stderr)
        return 1
    return 0

if __name__ == '__main__':
    raise SystemExit(main())
