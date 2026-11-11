#!/usr/bin/env python3
"""Regression fixtures for the UI-text audit lexer and publication key tracking."""
import ast
import bisect
import hashlib
import json
import re
import tempfile
from pathlib import Path
source=Path(__file__).with_name('audit-laravel-ui-text.py')
tree=ast.parse(source.read_text())
functions=[node for node in tree.body if isinstance(node,ast.FunctionDef)]
constants={}
for node in tree.body:
    if isinstance(node,ast.Assign) and len(node.targets)==1 and isinstance(node.targets[0],ast.Name) and node.targets[0].id in {'BRANDS','SHORT','DELIVERY_BOUNDARIES'}:
        constants[node.targets[0].id]=ast.literal_eval(node.value)
with tempfile.TemporaryDirectory(prefix='laravel-ui-audit-') as directory:
    namespace={'re':re,'json':json,'hashlib':hashlib,'bisect':bisect,'Path':Path,'ROOT':Path(directory),'SITES':[],'QUALIFICATIONS':{},'CATALOG':{language:{'process.run':'Run'} for language in ['en','ja','ko','zh-CN']},**constants}
    exec(compile(ast.Module(body=functions,type_ignores=[]),str(source),'exec'),namespace)
    fixture=Path(directory)/'example.rs'
    fixture.write_text(r'''fn before<'a>(value: &'a str) { let raw=r##"#[test] fn fake() { }"##; Err("Visible before fixture".into()); }
#[cfg(test)]
mod tests { fn hidden() { let open='{'; let close='\u{7D}'; /* outer { /* inner } */ */ Err("Hidden module fixture".into()); } }
fn after() { Err("Visible after fixture".into()); }
#[test]
fn hidden_test() { let open='['; Err("Hidden standalone fixture".into()); }
''')
    namespace['source_sites'](fixture)
    values={site['text'] for site in namespace['SITES']}
    assert values=={'Visible before fixture','Visible after fixture'},values
    namespace['SITES'].clear()
    fixture=Path(directory)/'managed-processes.js'
    fixture.write_text('const t=(key,fallback)=>window.uiKitI18n?.t(`process.${key}`,fallback); const row=el("button",{caption:t("run","Run")});')
    namespace['source_sites'](fixture)
    assert any(site['key']=='process.run' and site['status']=='translated' for site in namespace['SITES']),namespace['SITES']
print('PASS UI audit: Rust char/comment/raw-string boundaries, standalone tests and scoped JS translation keys')
