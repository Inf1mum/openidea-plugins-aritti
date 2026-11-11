#!/usr/bin/env python3
"""Check host-localized Laravel declarations without changing identity/default data.

The host uses exact English source strings in locales/<language>.json. Shared
web/native surfaces use the separate stable-key UI-kit catalog.
"""
import json
import tomllib
from pathlib import Path
ROOT = Path(__file__).resolve().parent.parent
PLUGIN = ROOT / 'plugins/laravel'
manifest = tomllib.loads((PLUGIN / 'plugin.toml').read_text())
schema = json.loads((PLUGIN / 'settings.schema.json').read_text())
required = set()
def schema_text(node):
    if isinstance(node, dict):
        for key, value in node.items():
            if key in ('title', 'description', 'group') and isinstance(value, str):
                required.add(value)
            elif key not in ('default', 'enum', 'const', 'examples'):
                schema_text(value)
    elif isinstance(node, list):
        for value in node:
            schema_text(value)
schema_text(schema)
def declarations(node):
    if isinstance(node, dict):
        for key, value in node.items():
            if key in ('title', 'description', 'category') and isinstance(value, str):
                required.add(value)
            elif key == 'fileTemplates':
                required.update(item['name'] for item in value)
                declarations(value)
            else:
                declarations(value)
    elif isinstance(node, list):
        for value in node:
            declarations(value)
declarations(manifest.get('contributes', {}))
for language in ('ja', 'ko', 'zh-CN'):
    catalog = json.loads((PLUGIN / 'locales' / (language + '.json')).read_text())
    missing = sorted(required - catalog.keys())
    if missing:
        raise SystemExit(f'{language}: missing host UI translations: {missing}')
    if any(not isinstance(catalog[key], str) or not catalog[key].strip() for key in required):
        raise SystemExit(f'{language}: empty host UI translation')
    # Framework names are brands; all other declared English leaves must translate.
    untranslated = [key for key in required if catalog[key] == key and key != 'Laravel']
    if untranslated:
        raise SystemExit(f'{language}: untranslated host UI: {untranslated}')
print(f'Laravel host locales: {len(required)} exact declaration/settings source keys, 3 languages complete')
