#!/usr/bin/env python3
"""Split plugins/jpa-assist/settings.schema.json (the source of every key) into the page schemas under settings/.

New keys land on the page their name matches below, otherwise on "Other". Run without arguments to rewrite the
page files, with --check to fail when they are stale. plugin.toml lists the pages and their actions by hand.
"""
import json
import re
import sys
from pathlib import Path

PLUGIN = Path(__file__).resolve().parent.parent / 'plugins/jpa-assist'
PAGES = ['designer', 'code-generation', 'reverse-engineering', 'database-versioning', 'inspections', 'query-console', 'other']
RULES = [
    (r'jpaAssist\.designer\.minimalMode', 'designer', 'Layout'),
    (r'jpaAssist\.designer\.', 'designer', 'Panels'),
    (r'injection', 'code-generation', 'Injection'),
    (r'mapper', 'code-generation', 'MapStruct mappers'),
    (r'query', 'code-generation', 'Derived queries'),
    (r'projection|unresolvedProjection', 'code-generation', 'Projections'),
    (r'dto', 'code-generation', 'DTOs'),
    (r'repositoryClassSuffix', 'code-generation', 'Repositories'),
    (r'lombok', 'code-generation', 'Lombok'),
    (r'entityLanguage|sqlImport', 'reverse-engineering', 'Entities from SQL'),
    (r'sqlJavaTypeMappings', 'reverse-engineering', 'SQL to Java type mappings'),
    (r'ddlDialect', 'database-versioning', 'DDL'),
    (r'jpaAssist\.(identifierPolicy|maxIdentifierLength|primaryKeyConstraintName|indexForeignKeys|table|column)', 'database-versioning', 'Naming and constraints'),
    (r'sqlTypeMappings', 'database-versioning', 'Java to SQL type mappings'),
    (r'liquibase|schema', 'database-versioning', 'Liquibase'),
    (r'flyway|migrationDescriptionCase', 'database-versioning', 'Flyway'),
    (r'jpaAssist\.inspectionSeverities', 'inspections', 'Severity'),
    (r'jpaAssist\.check', 'inspections', 'Checks'),
    (r'jpaAssist\.jpqlMutationPreview', 'query-console', 'JPQL console'),
]


def split():
    properties = json.loads((PLUGIN / 'settings.schema.json').read_text())['properties']
    pages = {}
    for order, (key, definition) in enumerate(properties.items()):
        page, group = next(((p, g) for pattern, p, g in RULES if re.match(pattern, key)), ('other', 'Other'))
        pages.setdefault(page, []).append((group, order, key, {**definition, 'group': group}))
    result = {}
    for page, rows in pages.items():
        first = {}
        for group, order, *_ in rows:
            first.setdefault(group, order)
        rows.sort(key=lambda row: (first[row[0]], row[1]))
        result[page] = {'properties': {key: definition for _, _, key, definition in rows}}
    return {f'{page}.json': json.dumps(result[page], indent=2, ensure_ascii=False) + '\n' for page in PAGES if page in result}


files = split()
target = PLUGIN / 'settings'
if '--check' in sys.argv:
    stale = [name for name, text in files.items() if not (target / name).exists() or (target / name).read_text() != text]
    stale += [p.name for p in target.glob('*.json') if p.name not in files]
    sys.exit(f'stale settings pages (run scripts/split-jpa-settings.py): {stale}' if stale else 0)
target.mkdir(exist_ok=True)
for old in target.glob('*.json'):
    old.unlink()
for name, text in files.items():
    (target / name).write_text(text)
print({name: len(json.loads(text)['properties']) for name, text in files.items()})
