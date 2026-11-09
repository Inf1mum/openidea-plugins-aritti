// Run selected manifest conditions through the host's actual TypeScript parser.
// Usage: node when-smoke.mjs <host/src/platform/context.ts> <commands.json>
// The Zustand store is stubbed; parser/evaluator code is left unchanged.
import assert from 'node:assert/strict';
import { readFile, writeFile, mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';

const [hostSource, manifestCommands] = process.argv.slice(2);
assert(hostSource && manifestCommands, 'provide host context.ts and manifest commands JSON');
const original = await readFile(hostSource, 'utf8');
const dependency = 'import { create } from "zustand";';
assert(original.includes(dependency), 'host store import changed; review the smoke harness');
const isolated = original.replace(dependency, `const create = (init: () => object) => {
  let state = init();
  return { getState: () => state, setState: (value: object, replace?: boolean) => {
    state = replace ? value : { ...state, ...value };
  }};
};`);
const directory = await mkdtemp(join(tmpdir(), 'ddev-when-'));
try {
  const source = join(directory, 'context.ts');
  await writeFile(source, isolated);
  const { evaluateWhen } = await import(pathToFileURL(source).href);
  const commands = JSON.parse(await readFile(manifestCommands, 'utf8'));
  const guarded = ['start', 'restart', 'stop', 'share', 'poweroff', 'delete', 'open-config',
    'shell', 'open-service', 'logs', 'composer', 'php', 'node', 'setup-database', 'copy-db-password'];
  const recovery = ['start', 'poweroff', 'delete', 'open-config', 'logs'];
  const recoveryUnknown = [...recovery, "restart", "stop"];
  const running = guarded.filter(id => id !== 'start');
  const cases = [
    ['no configuration', false, {known: false}, []],
    ['unknown initial state', true, {known: false}, guarded],
    ['running with database', true, {known: true, available: true, running: true, 'can-restart-stop': true, 'can-start': false, 'has-database': true}, running],
    ['starting', true, {known: true, available: true, running: false, 'can-start': false}, ['poweroff', 'delete', 'open-config', 'logs']],
    ['stopped', true, {known: true, available: true, running: false, 'can-start': true}, recovery],
    ['describe failed; installed CLI', true, {known: true, available: true, running: false, 'can-restart-stop': true, 'can-start': true}, recoveryUnknown],
    ['CLI unavailable', true, {known: true, available: false, running: false, 'can-start': false}, ['open-config']],
  ];
  let checks = 0;
  for (const [name, configured, flags, enabled] of cases) {
    const ctx = { 'workspaceContains:.ddev/config.yaml': configured,
      ...Object.fromEntries(Object.entries(flags).map(([key,value]) => [`plugin.aritti.ddev.${key}`,value])) };
    for (const id of guarded) {
      const command = commands.find(command => command.id === id);
      assert(command?.when, `${id} must declare its availability`);
      assert.equal(evaluateWhen(command.when,ctx), enabled.includes(id), `${name}: ${id}`);
      checks++;
    }
  }
  console.log(`${checks} manifest condition checks passed with the host evaluator`);
} finally {
  await rm(directory, {recursive:true, force:true});
}
