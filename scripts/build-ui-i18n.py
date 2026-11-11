#!/usr/bin/env python3
"""Materialize the shared offline UI catalogs; --check never writes."""
import argparse
import json
import re
from pathlib import Path
root = Path(__file__).resolve().parent.parent
parser = argparse.ArgumentParser()
parser.add_argument('--check', action='store_true')
args = parser.parse_args()
catalog = json.loads((root/'crates/ui-kit/i18n/catalog.json').read_text())
keys = set(catalog['en'])
for language, entries in catalog.items():
    if set(entries) != keys or any(not isinstance(value, str) or not value for value in entries.values()):
        raise SystemExit(f'Incomplete UI catalog: {language}')
    for key,value in entries.items():
        if set(re.findall(r'\{([A-Za-z][A-Za-z0-9_]*)\}',value)) != set(re.findall(r'\{([A-Za-z][A-Za-z0-9_]*)\}',catalog['en'][key])):
            raise SystemExit(f'UI placeholder mismatch: {language}/{key}')
# Legacy generator choices live in const tables and localize only at presentation.
legacy_source=(root/'plugins/laravel/src/generate.rs').read_text()
legacy_entries={f'generator.legacyKind.{kind}':label for kind,label in re.findall(r'kind\(\s*"([^"]+)"\s*,\s*"([^"]+)"',legacy_source)}
for flag,label in re.findall(r'\(\s*"(-[^"]+)"\s*,\s*"([^"]+)"\s*\)',legacy_source):
    legacy_entries[f"generator.flag.{flag}.{re.sub('[^a-z0-9]','-',label.lower())}"]=label
for key,label in legacy_entries.items():
    if key not in keys or catalog['en'][key]!=label:
        raise SystemExit(f'Unregistered/changed static generator choice: {key}')
# Native kind tables remain pure; catalog_for_project translates their UI labels.
native_source=(root/'plugins/laravel/src/native_catalog.rs').read_text()
for kind,label in re.findall(r'Kind\s*\{\s*id:\s*"([^"]+)"\s*,\s*label:\s*"([^"]+)"',native_source):
    key='project.native.kind.'+kind
    if key not in keys or catalog['en'][key]!=label:
        raise SystemExit(f'Unregistered/changed native generator label: {key}')
# Only explicit static UI-key call sites are checked; dynamic application values are not catalog entries.
patterns = [r'data-i18n(?:-(?:title|placeholder|aria-label))?="([^"]+)"', r'\btr\(\s*"([^"]+)"', r'ui_text::(?:t|text|format)\(\s*"([^"]+)"']
for directory in [root/'crates/ui-kit/web', root/'plugins/laravel/src']:
    for source in directory.rglob('*'):
        if source.suffix not in ('.html','.js','.rs') or source.name == 'i18n-catalog.js':
            continue
        text = source.read_text()
        if source.suffix=='.rs':
            declaration=re.search(r'const UI_ERROR_TEMPLATES:[\s\S]*?=\s*&\[([\s\S]*?)\n\];',text)
            if declaration:
                for key,literal in re.findall(r'\(\s*"([^"]+)"\s*,\s*("(?:\\.|[^"\\])*")\s*,?\s*\)',declaration[1]):
                    fallback=json.loads(literal)
                    if key not in keys or catalog['en'][key]!=fallback:
                        raise SystemExit(f'Unregistered/changed owned UI error template: {key}')
        for pattern in patterns:
            for key in re.findall(pattern, text):
                if key not in keys:
                    raise SystemExit(f'Unregistered UI key {key} in {source.relative_to(root)}')
# Native lookups use generated static slices, avoiding JSON parsing/allocation
# in the WebAssembly activation/render fuel budget.
def rust_string(value):
    result='"'
    for char in value:
        result+= {'"':'\\"','\\':'\\\\','\n':'\\n','\r':'\\r','\t':'\\t'}.get(char, ('\\u{'+format(ord(char),'x')+'}') if ord(char)<32 or ord(char)==127 else char)
    return result+'"'
ordered=sorted(keys)
rust='// Generated from crates/ui-kit/i18n/catalog.json by scripts/build-ui-i18n.py.\n'
rust+='const KEYS: &[&str] = &[\n'+''.join('    '+rust_string(key)+',\n' for key in ordered)+'];\n'
for language,identifier in [('en','EN'),('ja','JA'),('ko','KO'),('zh-CN','ZH_CN')]:
    rust+='const '+identifier+': &[&str] = &[\n'+''.join('    '+rust_string(catalog[language][key])+',\n' for key in ordered)+'];\n'
rust+='pub(super) fn lookup(language: &str, key: &str) -> Option<&\'static str> {\n    let index = KEYS.binary_search(&key).ok()?;\n    let values = match language { "ja" => JA, "ko" => KO, "zh-CN" => ZH_CN, _ => EN };\n    values.get(index).copied()\n}\n'
web='// Generated from crates/ui-kit/i18n/catalog.json by scripts/build-ui-i18n.py.\nwindow.uiKitTranslations = Object.freeze('+json.dumps(catalog,ensure_ascii=False)+');\n'
for target,result in [(root/'crates/ui-kit/web/i18n-catalog.js',web),(root/'plugins/laravel/src/ui_text_catalog.rs',rust)]:
    if args.check:
        if not target.exists() or target.read_text()!=result:
            raise SystemExit(f'UI catalog artifact is out of date: {target.relative_to(root)}')
    else:
        target.write_text(result)
