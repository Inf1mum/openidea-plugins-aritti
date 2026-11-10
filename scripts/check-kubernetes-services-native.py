#!/usr/bin/env python3
"""Check the package's real Services menus through an installed host's projection."""
import argparse
import json
from pathlib import Path
import subprocess
import tempfile
import tomllib

parser = argparse.ArgumentParser()
parser.add_argument('--checkout', type=Path, required=True, help='OpenIDEA checkout with installed frontend test dependencies')
args = parser.parse_args()
root = Path(__file__).resolve().parent.parent
manifest = tomllib.loads((root/'plugins/kube-explorer/plugin.toml').read_text())
fixture = {'commands': manifest['contributes']['commands'], 'menus': [m for m in manifest['contributes']['menus'] if m.get('view') == 'kube']}
source = r'''
import {afterEach,expect,it} from 'vitest';
import {registerAction} from '@/platform/actions';
import {defineBuiltinUiPoints} from '@/features/plugins/points';
import {clearUiContributions,syncUiContributions} from '@/features/plugins/projection';
import {pluginViewItemMenuItems} from '@/features/plugins/uiPoints';
const fixture=__FIXTURE__;
const pluginId='aritti.kube-explorer';
const off=defineBuiltinUiPoints();
const disposers=fixture.commands.map(command=>registerAction({id:`plugin.${pluginId}.${command.id}`,title:command.title,icon:command.icon,run:()=>{}}));
afterEach(()=>{clearUiContributions();disposers.forEach(dispose=>dispose());off();});
it('projects native per-kind actions and submenus without retargeting their scoped argument',()=>{
 syncUiContributions(fixture.menus.map(menu=>({pluginId,point:'menus',key:`${menu.menu}/${menu.view}/${menu.command}`,decl:menu})),[{id:pluginId,name:'Kube Explorer',status:'active',enabled:true} as any]);
 const rows=[
  ['pod',['resource-describe','resource-yaml','resource-logs','resource-log-view','resource-log-sources','resource-shell','resource-attach','resource-debug','resource-port-forward','resource-delete']],
  ['workload',['resource-describe','resource-yaml','resource-logs','resource-log-view','resource-log-sources','resource-port-forward','resource-delete']],
  ['log-workload',['resource-describe','resource-yaml','resource-logs','resource-log-view','resource-log-sources','resource-delete']],
  ['service',['resource-describe','resource-yaml','resource-port-forward','resource-delete']],
  ['secret',['resource-describe','resource-yaml','resource-show-secret','resource-edit-secret','resource-delete']],
  ['resource',['resource-describe','resource-yaml','resource-delete']],
  ['context',['tree-switch']],['namespace',['tree-switch']],['active-context',[]],['active-namespace',[]],['status',[]],['offline',['tree-import-config','tree-review-config']],
  ['group',['tree-expand','tree-refresh']],['group-open',['tree-collapse','tree-refresh']],
  ['secrets',['tree-expand','tree-refresh','tree-create-secret']]
 ] as const;
 const flatten=(items:any[]):any[]=>items.flatMap(item=>item.submenu?flatten(item.submenu):item.kind==='action'?[item]:[]);
 for(const [kind,expected] of rows){
  const key=`svc-${kind}:opaque~scoped-proof`,items=flatten(pluginViewItemMenuItems(pluginId,'kube',key));
  expect(items.map(item=>item.action.replace(`plugin.${pluginId}.`,'' )).sort(),kind).toEqual([...expected].sort());
  expect(items.every(item=>item.arg===key),kind).toBe(true);
 }
 const root=flatten(pluginViewItemMenuItems(pluginId,'kube','svc-cluster-open:opaque~proof')).map(item=>item.action.replace(`plugin.${pluginId}.`,''));
 for(const action of ['tree-watch','tree-open-watch','tree-kubeconfig','tree-import-config','tree-review-config'])expect(root).toContain(action);
 const global=flatten(pluginViewItemMenuItems(pluginId,'kube','svc-cluster-global:opaque')).map(item=>item.action.replace(`plugin.${pluginId}.`,''));
 expect(global).not.toContain('tree-watch');
 const allNamespaces=flatten(pluginViewItemMenuItems(pluginId,'kube','svc-cluster-all-open:opaque')).map(item=>item.action.replace(`plugin.${pluginId}.`,''));expect(allNamespaces).not.toContain('tree-watch');expect(allNamespaces).toContain('tree-open-watch');
 expect(pluginViewItemMenuItems(pluginId,'other','svc-pod:opaque')).toEqual([]);
 const pod=pluginViewItemMenuItems(pluginId,'kube','svc-pod:opaque') as any[];
 expect(pod.filter(item=>item.submenu).map(item=>item.label)).toEqual(['Logs','Container']);
 const ids=(items:any[])=>items.filter(item=>item.kind==='action').map(item=>item.action.replace(`plugin.${pluginId}.`,''));
 expect(ids(pod)).toEqual(['resource-describe','resource-yaml','resource-shell','resource-port-forward','resource-delete']);
 expect(pod.find(item=>item.label==='Container').submenu.map((item:any)=>item.action.replace(`plugin.${pluginId}.`,''))).toEqual(['resource-attach','resource-debug']);
 expect(pod.find(item=>item.label==='Logs').submenu).toHaveLength(3);
 const secret=pluginViewItemMenuItems(pluginId,'kube','svc-secret:opaque') as any[];
 expect(secret.some(item=>item.submenu)).toBe(false);
 expect(ids(secret)).toEqual(['resource-describe','resource-yaml','resource-show-secret','resource-edit-secret','resource-delete']);
});
'''.replace('__FIXTURE__', json.dumps(fixture, ensure_ascii=False))
# A unique, self-owned test file lets us use the host's exact config and mocks.
# It is always removed; no host production files are edited.
with tempfile.NamedTemporaryFile(mode='w', prefix='kube-services-', suffix='.test.ts', dir=args.checkout/'scripts', delete=False) as handle:
    handle.write(source)
    test = Path(handle.name)
try:
    result = subprocess.run([str(args.checkout/'node_modules/.bin/vitest'), 'run', str(test)], cwd=args.checkout)
    raise SystemExit(result.returncode)
finally:
    test.unlink(missing_ok=True)
