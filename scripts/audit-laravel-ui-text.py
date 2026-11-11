#!/usr/bin/env python3
"""Inventory authored UI text, preserving project/protocol/code data.

The lexer identifies explicit UI/error call sites rather than globally replacing
strings. The manifest is a review queue, not a claim of semantic AST completeness.
--check rejects uncovered confirmed call sites; --output writes the full evidence.
"""
import argparse
import bisect
import hashlib
import json
import re
from html.parser import HTMLParser
from pathlib import Path
ROOT = Path(__file__).resolve().parent.parent
CATALOG = json.loads((ROOT/'crates/ui-kit/i18n/catalog.json').read_text())
BRANDS = {'Laravel', 'Artisan', 'PHP', 'Docker', 'Livewire', 'Nova', 'Filament', 'Sentry', 'OpenIDEA', 'Qodana', 'HTTP', 'JSON', 'MCP'}
SHORT = {'Run','Stop','Start','Cancel','Refresh','Preview','Name','Type','Field','Value','Key','Path','File','Folder','Namespace','Model','Table','Method','Controller','View','Views','Component','Components','Locale','Status','Source','Output','Details','Options','Required','Optional','Default','Unknown','Select','Choose','Create','Delete','Add','Remove','Save','Apply','Close','Reset','Undo','Redo','Copy','Open','Search','Port','Host','Connection','Queue','Queues','Runner','General','Recent','Columns','Relations','Indexes','Fields'}
SITES = []
QUALIFICATIONS = {}
STALE_QUALIFICATIONS = []
qualification_file=ROOT/'scripts/laravel-ui-text-qualifications.json'
if qualification_file.exists():
    for entry in json.loads(qualification_file.read_text())['entries']:
        valid=all((ROOT/name).is_file() and hashlib.sha256((ROOT/name).read_bytes()).hexdigest()==digest for name,digest in entry['proofHashes'].items())
        if valid:QUALIFICATIONS[(entry['file'],entry['text'])]=entry
        else:STALE_QUALIFICATIONS.append({'file':entry['file'],'text':entry['text']})

def readable(value):
    prose=re.sub(r'\{[^{}]*\}','',value)
    return bool(value.strip()) and value not in BRANDS and bool(re.search('[A-Za-z]',prose)) and (bool(re.search(r'\s|[?…]',value)) or value in SHORT or bool(re.fullmatch(r'[A-Z][a-z]+',value)))

def add(path, line, kind, value, key=None, confidence='confirmed', source_start=None, source_end=None, format_literal=False, constant=False):
    if value.startswith('@layer ui-legacy {') and ':host' in value: return
    if not readable(value): return
    if '${' in value and key is None:
        plain=re.sub(r'\$\{[^}]*\}','',value)
        if not readable(plain) or 'tr(' in value:return
        confidence='dynamic-template-review'
    digest=hashlib.sha256((str(path.relative_to(ROOT))+'\0'+kind+'\0'+value).encode()).hexdigest()[:16]
    status='translated' if key and all(key in CATALOG[l] for l in ['en','ja','ko','zh-CN']) else 'unwrapped'
    qualification=QUALIFICATIONS.get((str(path.relative_to(ROOT)),value))
    if status=='unwrapped' and qualification:status='qualified-non-ui'
    SITES.append({'id':digest,'file':str(path.relative_to(ROOT)),'line':line,'kind':kind,'text':value,'key':key,'status':status,'confidence':confidence,'catalogCandidates':[k for k,v in CATALOG['en'].items() if v==value],'sourceStart':source_start,'sourceEnd':source_end,'formatLiteral':format_literal,'constantContext':constant,'qualification':qualification['qualification'] if qualification else None,'qualificationProof':qualification['proofHashes'] if qualification else None})

class HtmlSites(HTMLParser):
    def __init__(self,path,base=0): super().__init__(convert_charrefs=True);self.path=path;self.base=base;self.stack=[]
    def handle_starttag(self,tag,attrs):
        attrs=dict(attrs); self.stack.append((tag,attrs)) if tag not in {'input','img','br','hr','link','meta','source','area','wbr'} else None
        for name in ['aria-label','title','placeholder']:
            value=attrs.get(name)
            if value and not value.startswith(('--','<?','"','\'')):
                add(self.path,self.base+self.getpos()[0],f'html.{name}',value,attrs.get('data-i18n-'+name))
    def handle_startendtag(self,tag,attrs):
        before=len(self.stack);self.handle_starttag(tag,attrs);self.stack=self.stack[:before]
    def handle_endtag(self,tag):
        for at in range(len(self.stack)-1,-1,-1):
            if self.stack[at][0]==tag:self.stack=self.stack[:at];break
    def handle_data(self,data):
        if not self.stack or any(tag in {'script','style','textarea'} or 'data-ui-raw' in attrs for tag,attrs in self.stack):return
        value=data.strip()
        if value and not value.startswith(('<?','${','{{','{!!')):
            key=next((attrs['data-i18n'] for _,attrs in reversed(self.stack) if 'data-i18n' in attrs),None)
            add(self.path,self.base+self.getpos()[0],'html.leaf',value,key)

def literals(text, rust=False, include_trivia=False):
    """Yield byte-compatible Python-character spans outside line/block comments."""
    at=0
    while at<len(text):
        if text.startswith('//',at):
            begin=at;at=text.find('\n',at) if '\n' in text[at:] else len(text)
            if include_trivia:yield begin,at,None
            continue
        if text.startswith('/*',at):
            begin=at;depth=1;at+=2
            while at<len(text) and depth:
                if text.startswith('/*',at):depth+=1;at+=2
                elif text.startswith('*/',at):depth-=1;at+=2
                else:at+=1
            if include_trivia:yield begin,at,None
            continue
        raw=re.compile(r'r(#{0,16})"').match(text,at)
        if raw:
            begin=at;content=at+len(raw[0]);end=text.find('"'+raw[1],content)
            if end<0:return
            stop=end+1+len(raw[1]);yield begin,stop,text[content:end];at=stop;continue
        if text[at] in {'"','`'}:
            quote=text[at];begin=at;at+=1
            while at<len(text):
                if text[at]=='\\':at+=2
                elif text[at]==quote:at+=1;break
                else:at+=1
            raw=text[begin+1:at-1]
            if quote=='"':
                try:raw=json.loads(text[begin:at])
                except json.JSONDecodeError:pass
            yield begin,at,raw;continue
        if text[at]=="'" and rust:
            char=re.match(r"'(?:\\u\{[0-9A-Fa-f_]+\}|\\x[0-9A-Fa-f]{2}|\\.|[^'\\\n])'",text[at:])
            begin=at;at+=len(char[0]) if char else 1
            if include_trivia and char:yield begin,at,None
            continue
        if text[at]=="'":
            # Skip short Rust character literals; JavaScript source uses these
            # for UI strings as well, so emit them for .js callers below.
            begin=at;at+=1
            while at<len(text) and at-begin<10000:
                if text[at]=='\\':at+=2
                elif text[at]=="'":at+=1;yield begin,at,text[begin+1:at-1];break
                elif text[at]=='\n':at=begin+1;break
                else:at+=1
            continue
        at+=1

# Reviewed call graph: these pure generators are invoked by the workspace
# handle/resolve delivery wrappers. Hashes of both ends are emitted below.
DELIVERY_BOUNDARIES = {
    'generator_plan.rs': 'generator_workspace.rs',
    'generator_presets.rs': 'generator_workspace.rs',
    'generator_date.rs': 'generator_workspace.rs',
    'package_generators.rs': 'generator_workspace.rs',
    'native_catalog.rs': 'generator_workspace.rs',
    'native_module_scaffold.rs': 'generator_workspace.rs',
    'generator_preset_actions.rs': 'generator_workspace.rs',
    'docker_discovery.rs': 'artisan_docker.rs',
    'related_generation_config.rs': 'related_generation_selection.rs',
}

def owned_template_map(text):
    if not re.search(r'ui_text::owned_error\(\s*&message\s*,\s*UI_ERROR_TEMPLATES\s*\)',text):return {}
    tokens=list(literals(text,rust=True,include_trivia=True));mask=list(text)
    for start,stop,_ in tokens:
        for at in range(start,stop):mask[at]=' ' if text[at]!='\n' else '\n'
    code=''.join(mask);start=code.find('const UI_ERROR_TEMPLATES')
    if start<0:return {}
    end=code.find('];',start);values=[value for a,b,value in tokens if value is not None and start<a<end]
    return {re.sub(r'\{[^{}]*\}','{}',template):key for key,template in zip(values[::2],values[1::2])}

def source_sites(path):
    text=path.read_text();line_starts=[0]+[i+1 for i,char in enumerate(text) if char=='\n'];tokens=list(literals(text,rust=path.suffix=='.rs',include_trivia=True));strings=[token for token in tokens if token[2] is not None];by_start={start:(stop,value) for start,stop,value in strings};mask=list(text)
    for start,stop,_ in tokens:
        for i in range(start,stop):mask[i]=' ' if text[i]!='\n' else '\n'
    code=''.join(mask)
    code=re.sub(r'//[^\n]*|/\*[\s\S]*?\*/',lambda m: ''.join('\n' if char=='\n' else ' ' for char in m[0]),code)
    tests=[]
    for match in re.finditer(r'#\[(?:cfg\(test\)|test)\]',code):
        begin=code.find('{',match.end());semi=code.find(';',match.end())
        if begin<0 or semi>=0 and semi<begin:tests.append((match.start(),semi+1));continue
        depth=1;end=begin+1
        while end<len(code) and depth:
            if code[end]=='{':depth+=1
            elif code[end]=='}':depth-=1
            end+=1
        tests.append((match.start(),end))
    # Exact authored-error match arms localize at UI delivery boundaries while
    # planner/parser functions remain host-free. Only explicit literal=>t(key)
    # proofs in this same source file count; dynamic messages stay uncovered.
    boundary={}
    if path.suffix=='.rs':
        for start,stop,value in strings:
            match=re.match(r'\s*=>\s*(?:\{\s*)?(?:return\s+)?crate::ui_text::t\(\s*"([^"]+)"\s*,',text[stop:stop+250])
            if match:boundary[value]=match[1]
    owned_templates=owned_template_map(text)
    publication_templates={}
    if path.name in {'actions.rs','class_actions.rs','lang_actions.rs'}:
        publisher=path.with_name('actions.rs')
        publisher_text=publisher.read_text()
        if 'actions::localized_title(&rewrite.title)' in path.with_name('lib.rs').read_text():
            start=publisher_text.find('fn localized_title(')
            end=publisher_text.find('\nfn ',start+1)
            section=publisher_text[start:end if end>=0 else len(publisher_text)]
            tokens=list(literals(section,rust=True))
            for i,(begin,stop,value) in enumerate(tokens[:-1]):
                if re.search(r'ui_text::format\(\s*$',section[max(0,begin-80):begin]):
                    publication_templates[re.sub(r'\{[^{}]*\}','{}',tokens[i+1][2])]=value
    # Reviewed publication adapters: the raw producers retain cache/source facts.
    # Each proof requires its actual consumer hook and all referenced catalog keys.
    root_file=ROOT/'plugins/laravel/src/lib.rs'
    root_publication=root_file.read_text() if root_file.exists() else ''
    reviewed_publishers=[]
    if path.name=='lang_actions.rs' and 'lang_actions::localized_title(&action.title)' in root_publication:
        reviewed_publishers.append(text.split('pub(crate) fn localized_title(',1)[-1])
        publication_templates['Extract to translation key "{}.{}"']='final.lang.extractTitle'
    if path.name=='eloquent_members.rs' and 'eloquent_members::localize_completions(' in root_publication:
        reviewed_publishers.append(text.split('pub(crate) fn localize_completions(',1)[-1].split('pub fn ',1)[0])
        if 'final.signature.forwarded' in root_publication:
            boundary['Declared constructor parameters forwarded through project metadata.']='final.signature.forwarded'
    if path.name in {'metadata_domains.rs','metadata_fields.rs'}:
        publication=path.with_name('metadata.rs').read_text()
        for raw,key in [('Ambiguous project sources','metadata.detail.ambiguousSources'),('Ambiguous icon registrations','metadata.detail.ambiguousIcons'),('source cast class or enum','metadata.detail.castClass')]:
            if key in publication and raw in publication:boundary[raw]=key
    if path.name=='model_overview.rs' and 'present_tree(&mut rendered, &items)' in text:
        reviewed_publishers.append(text.split('fn present_tree(',1)[-1])
    if path.name=='console_editor.rs' and 'lens.argument.as_deref()' in root_publication and 'final.console.run' in root_publication:
        publication_templates['Run {}…']='final.console.run'
    if path.name=='eloquent_checks.rs' and 'eloquent_checks::localized_message(&finding)' in root_publication:
        reviewed_publishers.append(text.split('pub(crate) fn localized_message(',1)[-1])
    for section in reviewed_publishers:
        for key,template in re.findall(r'ui_text::(?:format|t)\(\s*"([^"]+)"\s*,\s*"((?:\\.|[^"\\])*)"',section):
            try:template=json.loads('"'+template+'"')
            except ValueError:continue
            publication_templates[re.sub(r'\{[^{}]*\}','{}',template)]=key
    label_owner=None
    if path.name in {'class_actions.rs','lang_actions.rs'}:
        label_source=path.with_name('actions.rs')
        publication=path.with_name('lib.rs').read_text()
        label_text=label_source.read_text()
        if 'actions::localized_title(&rewrite.title)' in publication and 'fn localized_title(title:' in label_text:
            label_owner=str(label_source.relative_to(ROOT))
            for start,stop,value in literals(label_text,rust=True):
                match=re.match(r'\s*=>\s*(?:return\s+)?crate::ui_text::t\(\s*"([^"]+)"\s*,',label_text[stop:stop+250])
                if match:boundary[value]=match[1]
    if path.name=='function_checks.rs':
        publication=path.with_name('lib.rs').read_text()
        section=publication.split('fn function_problems(',1)[-1].split('fn livewire_method_problems(',1)[0]
        if 'function_checks::env_outside_config' in section and 'finding.code=="env-outside-config"' in section.replace(' ','') and 'editor.final.envOutsideConfig' in section:
            boundary['env() outside config files returns null once the configuration is cached: read a config() value instead.']='editor.final.envOutsideConfig'
    translation_prefix='process.' if path.name=='managed-processes.js' and 'window.uiKitI18n?.t(`process.${key}`' in text else ''
    if path.name=='route_runtime.rs' and '.map_err(ui_error)' in text:
        for shape,key in [('Route JSON is missing {}.','native.dockerRoute.2941409fc8'),('Route {} exceeds text limits.','native.dockerRoute.27dad91e8f')]:
            if all(('Route JSON is missing '+field+'.') in text and ('Route '+field+' exceeds text limits.') in text for field in ['method','uri','action','name']):
                owned_templates[shape]=key
    package_details={};package_templates={}
    package_source=ROOT/'plugins/laravel/src/package_integrations.rs'
    if path.name in {'package_integrations.rs','package_dusk.rs','package_livewire.rs','package_livewire_views.rs','package_native.rs','component_metadata.rs'} and package_source.exists():
        presentation=package_source.read_text().split('pub(crate) fn presentation_detail(',1)[-1]
        end=presentation.find('\n#[cfg(test)]')
        if end>=0:presentation=presentation[:end]
        for raw,key,english in re.findall(r'"([^"]+)"\s*=>\s*(?:\{\s*)?Some\(\(\s*"([^"]+)"\s*,\s*"([^"]+)"\s*,?\s*\)\)',presentation):
            package_details[raw]='package.detail.'+key
        for key,template in re.findall(r'ui_text::format\(\s*"([^"]+)"\s*,\s*"([^"]+)"',presentation):
            package_templates[re.sub(r'\{[^{}]*\}','{}',template)]=key
    if path.name=='metadata.rs' or path.name.startswith('metadata_') or path.name in {'component_metadata.rs','index.rs','package_livewire_views.rs','package_native.rs','package_integrations.rs','schema_sources.rs'}:
        publisher=ROOT/'plugins/laravel/src/metadata_ui.rs'
        publication=(ROOT/'plugins/laravel/src/metadata.rs').read_text()
        command=(ROOT/'plugins/laravel/src/lib.rs').read_text()
        if publisher.exists() and 'presentation_issue(index,issue)' in re.sub(r'\s+','',publication) and 'metadata::presentation_issue(index,issue)' in re.sub(r'\s+','',command):
            publisher_text=publisher.read_text()
            owned_templates.update(owned_template_map(publisher_text))
            for begin,stop,value in literals(publisher_text,rust=True):
                match=re.match(r'\s*=>\s*(?:\{\s*)?Some\(crate::ui_text::t\(\s*"([^"]+)"',publisher_text[stop:stop+200])
                if match:boundary[value]=match[1]
            if path.name=='schema_sources.rs':
                sql=(ROOT/'plugins/laravel/src/migrations.rs').read_text()
                if 'sql_issue_label' in publisher_text and 'index.sql_sources.get(path)' in re.sub(r'\s+','',publisher_text):
                    for english,key in re.findall(r'"([^"]+)"\s*=>\s*"(process\.sql\.issue[0-9]+)"',sql):boundary[english]=key
    native_labels={}
    if path.name=='native_catalog.rs' and 'project.native.kind.{}' in text and 'row["label"]' in text:
        native_labels={label:'project.native.kind.'+kind for kind,label in re.findall(r'Kind\s*\{\s*id:\s*"([^"]+)"\s*,\s*label:\s*"([^"]+)"',text)}
    if path.name=='package_generators.rs':
        presentation=(ROOT/'plugins/laravel/web/generators.js').read_text()
        if 'generator.kind.' in presentation and 'item.id' in presentation:
            native_labels.update({label:'generator.kind.'+kind for kind,label in re.findall(r'\{"id":"([^"]+)","label":"([^"]+)"',text)})
    boundary_owner=None
    if path.name in DELIVERY_BOUNDARIES:
        owner=path.with_name(DELIVERY_BOUNDARIES[path.name])
        owner_text=owner.read_text()
        if owner_text.count('.map_err(ui_error)') >= 1 and re.search(r'fn ui_error\(error\s*:\s*Error\)',owner_text):
            boundary_owner=str(owner.relative_to(ROOT))
            owned_templates.update(owned_template_map(owner_text))
            for start,stop,value in literals(owner_text,rust=True):
                match=re.match(r'\s*=>\s*(?:\{\s*)?(?:return\s+)?crate::ui_text::t\(\s*"([^"]+)"\s*,',owner_text[stop:stop+250])
                if match:boundary[value]=match[1]
    stack=[];at=0
    while at<len(text):
        if at in by_start:
            stop,value=by_start[at];line=bisect.bisect_right(line_starts,at)
            translated=next((entry for entry in reversed(stack) if entry['name'].endswith(('ui_text::t','ui_text::text','ui_text::format')) or entry['name'] in {'tr','t'}),None)
            key=None;kind=None
            if translated:
                if translated['arg']==0:translated['translation_key']=value;at=stop;continue
                if translated['arg']!=1:at=stop;continue
                key=translated.get('translation_key');key=(translation_prefix+key) if key and translated['name']=='t' else key;kind='native.translated' if path.suffix=='.rs' else 'js.translated'
            if not kind:
                for entry in reversed(stack):
                    name=entry['name'];arg=entry['arg']
                    if name.endswith(('host::input','host::pick','host::pick_many','host::begin_task')) and arg==0:kind='native.prompt';break
                    if name.endswith(('host::confirm','prompts::confirm_exact')) and arg in {0,1}:kind='native.confirm';break
                    if name.endswith('host::notify') and arg in {1,2}:kind='native.notification';break
                    if name.endswith('Body::Message') and arg==0:kind='native.message';break
                    if name.endswith(('Button::new','PickItem::new')) and arg==1:kind='native.control';break
                    if name.endswith(('Error::Invalid','Error::Failed','Error::NotFound','Err','ok_or','ok_or_else')):kind='native.error';break
                    if name in {'element','el'} and arg==1:kind='js.control';break
                    if name.endswith(('.status','.title','.description')) and arg==0:kind='native.status';break
                    if name in {'preview.push_str','review.push_str','prompt.push_str','markdown.push_str'}:kind='native.review';break
                    if name in {'issues.push','issues.extend'} or name.endswith(('.issues.push','.issues.extend')):kind='native.issue';break
                    if entry.get('ui_columns'):kind='native.column';break
                prefix=text[max(0,at-120):at]
                if not kind and re.search(r'(?:textContent|placeholder|title)\s*=\s*$',prefix):kind='js.text'
                if not kind and re.search(r'"(?:label|title|searchPlaceholder|placeholder)"\s*:\s*$',prefix):kind='native.model-label'
                if not kind and path.suffix=='.rs' and re.search(r'\b(?:title|label|description|detail|message|placeholder|status|tooltip|documentation)\s*:\s*(?:Some\(\s*)?(?:format!\(\s*)?$',prefix):kind='native.struct-label'
                if not kind and path.suffix=='.rs' and re.search(r'\.(?:title|label|description|detail|message|placeholder|status|tooltip|documentation)\s*=\s*(?:Some\(\s*)?(?:format!\(\s*)?$',prefix):kind='native.struct-label'
                if not kind and re.search(r'let\s+(?:mut\s+)?(?:preview|review|prompt|help|status|title|markdown)\s*=\s*format!\s*\(\s*$',prefix):kind='native.review'
            in_test=path.suffix=='.rs' and any(start<=at<end for start,end in tests)
            format_literal=any(entry['name'] in {'format!','format_args!','write!','writeln!'} and entry['arg']==0 for entry in stack)
            if kind=='native.struct-label' and key is None and value in package_details:
                key=package_details[value];kind='native.package-provenance.boundary'
            if kind=='native.struct-label' and key is None and re.sub(r'\{[^{}]*\}','{}',value) in package_templates:
                key=package_templates[re.sub(r'\{[^{}]*\}','{}',value)];kind='native.package-template.boundary'
            if kind in {'native.struct-label','native.model-label'} and key is None and value in native_labels:
                key=native_labels[value];kind='native.catalog-label.boundary'
            shape=re.sub(r'\{[^{}]*\}','{}',value)
            if kind=='native.struct-label' and key is None and shape in publication_templates:
                key=publication_templates[shape];kind='native.action-template.boundary@plugins/laravel/src/actions.rs'
            if kind in {'native.error','native.issue'} and key is None and shape in owned_templates:
                key=owned_templates[shape];kind='native.error.owned-template-boundary'
            if kind in {'native.error','native.issue','native.struct-label'} and key is None and value in boundary:
                key=boundary[value];kind='native.error.boundary'+('@'+boundary_owner if boundary_owner else '')
            if kind and not in_test:add(path,line,kind,value,key,source_start=at,source_end=stop,format_literal=format_literal,constant=any(entry.get('constant')for entry in stack))
            if path.suffix=='.js' and re.search(r'<(?:section|div|form|label|button)\b',value):
                parser=HtmlSites(path,line-1)
                try:parser.feed(value)
                except Exception:pass
            at=stop;continue
        char=code[at]
        if char in '([{':
            name=''
            if char=='(':
                match=re.search(r'([A-Za-z_][A-Za-z0-9_:.!\-]*)\s*$',code[max(0,at-160):at]);name=match[1] if match else ''
                if path.suffix=='.js' and ':' in name and '::' not in name:name=name.rsplit(':',1)[-1]
                if name in {'push','extend'} and re.search(r'\bissues\s*\.\s*'+name+r'\s*$',code[max(0,at-160):at]):name='issues.'+name
            prefix=text[max(0,at-40):at]
            stack.append({'char':char,'name':name,'arg':0,'start':at,'ui_columns':char=='[' and bool(re.search(r'"columns"\s*:\s*$',prefix)),'constant':path.suffix=='.rs' and (any(entry.get('constant')for entry in stack) or bool(re.search(r'\bconst\s+\w+\s*:[^;=]*=\s*&?\s*$',code[max(0,at-200):at])))})
        elif char in ')]}':
            if stack:stack.pop()
        elif char==',' and stack:stack[-1]['arg']+=1
        at+=1

parser=argparse.ArgumentParser();parser.add_argument('--output',type=Path);parser.add_argument('--check',action='store_true');args=parser.parse_args()
paths=list((ROOT/'plugins/laravel/src').glob('*.rs'))+list((ROOT/'plugins/laravel/web').glob('*.html'))+list((ROOT/'plugins/laravel/web').glob('*.js'))+list((ROOT/'crates/ui-kit/web').glob('*.html'))+list((ROOT/'crates/ui-kit/web').glob('*.js'))
for path in sorted(paths):
    if path.name=='ui_text_catalog.rs':continue # generated offline data, verified by build-ui-i18n
    if path.name in {'i18n-catalog.js','native-icons.js','icons.js'}:continue
    if path.suffix=='.html':
        scan=HtmlSites(path);scan.feed(path.read_text())
    else:source_sites(path)
unique={}
for site in SITES:unique[(site['file'],site['line'],site['kind'],site['text'])]=site
sites=sorted(unique.values(),key=lambda s:(s['file'],s['line'],s['kind']))
unwrapped=[site for site in sites if site['status']=='unwrapped']
confirmed=[site for site in unwrapped if site['confidence']=='confirmed']
report={'version':1,'catalogLanguages':['en','ja','ko','zh-CN'],'catalogKeys':len(CATALOG['en']),'staleQualifications':STALE_QUALIFICATIONS,'sourceDigests':{str(path.relative_to(ROOT)):hashlib.sha256(path.read_bytes()).hexdigest() for path in sorted(paths)},'summary':{'sites':len(sites),'translated':sum(site['status']=='translated' for site in sites),'qualifiedNonUi':sum(site['status']=='qualified-non-ui' for site in sites),'unwrapped':len(unwrapped),'confirmedUnwrapped':len(confirmed)},'sites':sites}
if args.output:args.output.write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n')
print(json.dumps(report['summary']))
if args.check and confirmed:raise SystemExit('Unwrapped UI sites remain; review the manifest before claiming full UI localization.')
