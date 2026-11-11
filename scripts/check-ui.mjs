// Run from any directory. Optional --webkit covers the WebKit context-menu
// gesture regression; --audit refreshes the source comparison report.
import {spawnSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';

const root=fileURLToPath(new URL('../',import.meta.url));
const flags=new Set(process.argv.slice(2));
for(const flag of flags)if(!['--webkit','--audit'].includes(flag)){
  console.error(`Unknown option: ${flag}`);
  process.exit(2);
}
function run(command,args,extraEnv={}){
  const result=spawnSync(command,args,{cwd:root,stdio:'inherit',env:{...process.env,...extraEnv}});
  if(result.error){console.error(result.error.message);process.exit(1);}
  if(result.status!==0)process.exit(result.status||1);
}
const python=process.env.PYTHON||'python3';
run(python,['scripts/build-ui-icons.py','--check']);
run(python,['scripts/build-ui-i18n.py','--check']);
run(python,['scripts/check-laravel-host-locales.py']);
run(python,['scripts/sync-plugin-ui.py','--check']);
for(const script of ['check-plugin-ui','check-ui-kit','check-ui-controls','check-ui-inventory','check-ui-fidelity','check-ui-choices','check-ui-tool-windows','check-ui-icons']){
  run(process.execPath,[`scripts/${script}.mjs`],{BROWSER_ENGINE:'chromium'});
}
run(process.execPath,['scripts/check-ui-background-poll.mjs']);
run(process.execPath,['scripts/check-ui-i18n.mjs']);
if(flags.has('--webkit'))run(process.execPath,['scripts/check-ui-tool-windows.mjs'],{BROWSER_ENGINE:'webkit'});
if(flags.has('--webkit'))for(const script of ['check-ui-background-poll','check-ui-i18n'])run(process.execPath,[`scripts/${script}.mjs`,'--webkit']);
if(flags.has('--audit'))for(const script of ['audit-ui-components','audit-ui-states'])run(process.execPath,[`scripts/${script}.mjs`]);
console.log('PASS shared UI verification');
