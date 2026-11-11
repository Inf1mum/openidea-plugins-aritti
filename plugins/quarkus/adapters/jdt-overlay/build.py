#!/usr/bin/env python3
"""Compile the optional companion against the exact reviewed JDT/MP tool profile."""
import argparse
import pathlib
import subprocess
import os
import shutil

HERE = pathlib.Path(__file__).resolve().parent

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--jdtls', required=True, type=pathlib.Path)
    parser.add_argument('--microprofile-jdt', required=True, type=pathlib.Path)
    parser.add_argument('--java-bin', type=pathlib.Path, default=pathlib.Path('/usr/bin'))
    parser.add_argument('--test', action='store_true')
    args = parser.parse_args()
    jars = sorted((args.jdtls / 'plugins').glob('*.jar')) + [args.microprofile_jdt]
    if not jars or not args.microprofile_jdt.is_file():
        parser.error('Existing reviewed JDT LS plugins and MicroProfile JDT jar required')
    build = HERE / 'build'
    shutil.rmtree(build, ignore_errors=True)
    classes = build / 'classes'
    classes.mkdir(parents=True)
    cp = os.pathsep.join(str(jar.resolve()) for jar in jars)
    sources = sorted((HERE / 'src').rglob('*.java'))
    if args.test:
        sources += sorted((HERE / 'tests').rglob('*.java'))
    subprocess.run([str(args.java_bin / 'javac'), '--release', '21', '-cp', cp, '-d', str(classes),
                    *map(str, sources)], check=True)
    if args.test:
        subprocess.run([str(args.java_bin / 'java'), '-ea', '-cp', str(classes) + os.pathsep + cp,
                        'openidea.quarkus.overlay.OverlaySourceTest'], check=True)
        for test_class in (classes / 'openidea/quarkus/overlay').glob('OverlaySourceTest*.class'):
            test_class.unlink()
    output = build / 'openidea.quarkus.config.overlay.jar'
    subprocess.run([str(args.java_bin / 'jar'), '--create', '--file', str(output), '--date=2000-01-01T00:00:00Z', '--manifest',
                    str(HERE / 'META-INF/MANIFEST.MF'), '-C', str(classes), '.', '-C', str(HERE), 'plugin.xml'], check=True)
    print(output)

if __name__ == '__main__':
    main()
