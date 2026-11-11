#!/usr/bin/env python3
"""User-declared Generic REST/Java Pattern Tasks contract. PROFILE REQUEST OUTPUT."""
import hashlib
import http.client
import importlib.util
import json
import os
import pathlib
import re
import shutil
import ssl
import struct
import subprocess
import sys
import time
import urllib.parse
sys.dont_write_bytecode = True
_spec = importlib.util.spec_from_file_location('_tasks_private_generic_base', pathlib.Path(__file__).with_name('self_hosted_bridge.py'))
shared = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(shared)
private = shared.private
BridgeError, fail = private.BridgeError, private.fail
FIELDS = {'id', 'summary', 'description', 'state', 'revision'}
FLAGS = {'CASE_INSENSITIVE':2, 'MULTILINE':8, 'DOTALL':32, 'UNICODE_CASE':64, 'COMMENTS':4, 'LITERAL':16, 'UNIX_LINES':1, 'UNICODE_CHARACTER_CLASS':256}
ID = re.compile(r'[A-Za-z0-9._-]{1,64}\Z')
PARAM = re.compile(r'[A-Za-z][A-Za-z0-9_-]{0,63}\Z')
MAX_BODY = 256 * 1024

def identifier(value):
    return isinstance(value, str) and ID.fullmatch(value) is not None and '..' not in value

def path(value, template=False):
    if not isinstance(value, str) or len(value)>512:
        fail('invalid_profile_path')
    if template and value.count('{id}') != 1: fail('invalid_profile_path')
    check = value.replace('{id}', 'safe') if template else value
    if not re.fullmatch(r'/(?:[A-Za-z0-9._~-]+/)*[A-Za-z0-9._~-]+', check) or any(p in ('.','..') for p in check.split('/')):
        fail('invalid_profile_path')
    return value

def pointer(value):
    if not isinstance(value,str) or len(value)>512 or (value and not value.startswith('/')) or re.search(r'~(?![01])',value): fail('invalid_json_pointer')
    return value

def profile(filename):
    raw = private.bounded_read(filename,32*1024,private=True)
    c = private.json_object(raw,'invalid_profile')
    required = {'version','family','baseUrl','apiKey','auth','search','detail','webPath','parser','state'}
    if set(c)-{'close'} != required or type(c['version']) is not int or c['version']!=1 or c['family']!='generic': fail('invalid_profile')
    host,prefix=shared.base_url(c['baseUrl'])
    key=c['apiKey']
    if not isinstance(key,str) or not 16<=len(key)<=4096 or any(ord(v)<33 or ord(v)>126 for v in key): fail('invalid_profile')
    auth=c['auth']
    if not isinstance(auth,dict) or set(auth)!={'header','prefix'} or auth['header'] not in ('Authorization','X-API-Key','X-Auth-Token') or auth['prefix'] not in ('','Bearer ','Token '): fail('invalid_auth_contract')
    s=c['search']
    if not isinstance(s,dict) or set(s)!={'path','queryParam','pageParam','pageSizeParam','pageSize'}: fail('invalid_search_contract')
    path(s['path'])
    names=[s[k] for k in ('queryParam','pageParam','pageSizeParam')]
    if any(not isinstance(n,str) or not PARAM.fullmatch(n) for n in names) or len(set(names))!=3 or type(s['pageSize']) is not int or not 1<=s['pageSize']<=50: fail('invalid_search_contract')
    if not isinstance(c['detail'],dict) or set(c['detail'])!={'path'}: fail('invalid_detail_contract')
    path(c['detail']['path'],True);path(c['webPath'],True)
    p=c['parser']
    if not isinstance(p,dict): fail('invalid_parser')
    if p.get('kind')=='json':
        if set(p)!={'kind','items','detail','fields'} or not isinstance(p['fields'],dict) or set(p['fields'])!=FIELDS: fail('invalid_parser')
        for v in [p['items'],p['detail'],*p['fields'].values()]: pointer(v)
    elif p.get('kind')=='java-regex':
        if set(p)!={'kind','pattern','flags','groups'} or not isinstance(p['pattern'],str) or not 1<=len(p['pattern'].encode())<=4096: fail('invalid_parser')
        if not isinstance(p['flags'],list) or len(p['flags'])>8 or len(set(p['flags']))!=len(p['flags']) or any(f not in FLAGS for f in p['flags']): fail('invalid_regex_flags')
        if not isinstance(p['groups'],dict) or set(p['groups'])!=FIELDS or any(type(v) is not int or not 0<=v<=100 for v in p['groups'].values()): fail('invalid_regex_groups')
    else: fail('invalid_parser')
    state=c['state']
    if not isinstance(state,dict): fail('invalid_state_contract')
    if set(state)=={'constant'}:
        if type(state['constant']) is not bool: fail('invalid_state_contract')
    elif set(state)=={'open','closed'}:
        for values in state.values():
            if not isinstance(values,list) or not 1<=len(values)<=50 or any(not isinstance(v,str) or not 1<=len(v)<=128 for v in values) or len(set(values))!=len(values): fail('invalid_state_contract')
        if set(state['open']) & set(state['closed']): fail('invalid_state_contract')
    else: fail('invalid_state_contract')
    if 'close' in c:
        close=c['close']
        if not isinstance(close,dict) or set(close)!={'method','path','body','guard','closedState'}: fail('invalid_close_contract')
        path(close['path'],True)
        if close['method'] not in ('POST','PUT','PATCH') or close['guard'] not in ('etag','fresh-read') or 'constant' in state or close['closedState'] not in state['closed'] or not isinstance(close['body'],dict) or len(json.dumps(close['body']).encode())>8192: fail('invalid_close_contract')
    if key in c['baseUrl'] or key in c['webPath'] or key in c['detail']['path'] or key in c.get('close',{}).get('closedState',''): fail('credential_in_public_contract')
    fingerprint=hashlib.sha256(str(filename).encode()+b'\0'+raw).hexdigest()
    return dict(c,host=host,prefix=prefix,profileId=fingerprint)

def request(v):
    common={'version','operation','expectedFamily','expectedBaseUrl'}
    if type(v.get('version')) is not int or v['version']!=1 or v.get('expectedFamily')!='generic': fail('invalid_request')
    shared.base_url(v.get('expectedBaseUrl'))
    if v.get('operation')=='search':
        if set(v)!=common|{'query','page'} or type(v['page']) is not int or not 1<=v['page']<=5: fail('invalid_request')
        q=v['query']
        if not isinstance(q,str) or len(q.encode())>4096 or any(ord(ch)<32 for ch in q): fail('invalid_query')
    elif v.get('operation')=='close':
        if set(v)!=common|{'expectedId','expectedProjectId','expectedRevision','expectedProfileId'} or not identifier(v['expectedId']) or v['expectedProjectId']!='': fail('invalid_request')
        if any(not isinstance(v[k],str) or not shared.HEX.fullmatch(v[k]) for k in ('expectedRevision','expectedProfileId')): fail('invalid_request')
    else: fail('invalid_operation')
    return v

def http_raw(c,method,target,payload,deadline,headers=None,*,_connection_factory=None):
    conn=None
    try:
        if not target.startswith(c['prefix']+'/') or any(ord(ch)<32 for ch in target): fail('invalid_request_path')
        factory=_connection_factory or http.client.HTTPSConnection
        conn=factory(c['host'],port=443,timeout=private.remaining(deadline),context=ssl.create_default_context())
        h={'Accept':'application/json' if c['parser']['kind']=='json' else 'text/plain, application/xml, text/xml, text/html','Accept-Encoding':'identity','User-Agent':'OpenIDEA-Tasks-Generic/1',c['auth']['header']:c['auth']['prefix']+c['apiKey']}
        h.update(headers or {})
        body=None if payload is None else json.dumps(payload,separators=(',',':')).encode()
        if body is not None: h['Content-Type']='application/json; charset=utf-8'
        conn.request(method,target,body=body,headers=h);sock=conn.sock;r=conn.getresponse()
        if 300<=r.status<400: fail('redirect_refused')
        if r.status==412: fail('revision_precondition_failed')
        if r.status not in (200,204): fail('http_failure')
        if r.getheader('Content-Encoding','identity').lower() not in ('','identity'): fail('unsupported_response_encoding')
        n=r.getheader('Content-Length')
        if n is not None and (not n.isdecimal() or int(n)>MAX_BODY): fail('response_bound')
        data=bytearray()
        while len(data)<=MAX_BODY and not r.isclosed():
            live=conn.sock or sock
            if live: live.settimeout(private.remaining(deadline))
            chunk=r.read1(min(16384,MAX_BODY+1-len(data)))
            if not chunk: break
            data.extend(chunk)
        if len(data)>MAX_BODY: fail('response_bound')
        private.remaining(deadline)
        etag=r.getheader('ETag','')
        if len(etag)>256 or not re.fullmatch(r'"[!#-~]+"',etag): etag=''
        return bytes(data),etag
    except BridgeError: raise
    except Exception: fail('transport_failure')
    finally:
        if conn: conn.close()

def select(value,p):
    for part in p.split('/')[1:] if p else []:
        key=part.replace('~1','/').replace('~0','~')
        if isinstance(value,dict) and key in value: value=value[key]
        elif isinstance(value,list) and re.fullmatch(r'0|[1-9][0-9]*',key) and int(key)<len(value): value=value[int(key)]
        else: fail('missing_declared_field')
    return value

def java_rows(raw,p,deadline):
    if len(raw)>MAX_BODY: fail('response_bound')
    try: raw.decode('utf-8')
    except Exception: fail('invalid_response_encoding')
    flags=sum(FLAGS[f] for f in p['flags'])
    data=b''
    for item in (p['pattern'].encode(),str(flags).encode(),','.join(str(p['groups'][k]) for k in sorted(FIELDS)).encode(),raw): data+=struct.pack('>I',len(item))+item
    java=shutil.which('java',path='/usr/bin:/usr/local/bin:/opt/homebrew/bin')
    if not java: fail('java17_required')
    try:
        result=subprocess.run([java,'-Xmx64m','--source','17',str(pathlib.Path(__file__).with_name('GenericRegex.java'))],input=data,stdout=subprocess.PIPE,stderr=subprocess.DEVNULL,timeout=min(3,private.remaining(deadline)),env={'PATH':'/usr/bin:/bin'},cwd='/')
        if result.returncode or len(result.stdout)>1024*1024: fail('java_regex_failure')
        rows=json.loads(result.stdout)
        if not isinstance(rows,list) or len(rows)>50: fail('java_regex_failure')
        return [dict(zip(sorted(FIELDS),row)) for row in rows]
    except subprocess.TimeoutExpired: fail('java_regex_timeout')
    except BridgeError: raise
    except Exception: fail('java_regex_failure')

def rows(raw,c,detail,deadline):
    p=c['parser']
    if p['kind']=='java-regex': return java_rows(raw,p,deadline)
    document=shared.parse_json(raw)
    items=select(document,p['detail'] if detail else p['items'])
    items=[items] if detail else items
    if not isinstance(items,list) or len(items)>50: fail('issue_count_bound')
    return [{k:select(item,ptr) for k,ptr in p['fields'].items()} for item in items]

def issue(row,c):
    if not isinstance(row,dict) or set(row)!=FIELDS or any(not isinstance(v,str) for v in row.values()): fail('invalid_issue_response')
    if not identifier(row['id']) or not row['summary'].strip() or len(row['summary'].encode())>4096 or len(row['description'].encode())>16384 or not row['revision'] or len(row['revision'].encode())>1024 or len(row['state'].encode())>128: fail('invalid_issue_response')
    if any(c['apiKey'] in row[k] for k in ('id','revision','state')): fail('credential_in_response_identity')
    state=c['state']
    if 'constant' in state: closed=state['constant']
    elif row['state'] in state['closed']: closed=True
    elif row['state'] in state['open']: closed=False
    else: fail('unknown_declared_state')
    revision=hashlib.sha256(json.dumps([row[k] for k in sorted(FIELDS)],ensure_ascii=True,separators=(',',':')).encode()).hexdigest()
    can=bool(c.get('close') and not closed)
    url=lambda template:c['baseUrl']+template.replace('{id}',urllib.parse.quote(row['id'],safe=''))
    return {'number':row['id'],'projectId':'','summary':private.clean(row['summary'],1024,c['apiKey']),'description':private.clean(row['description'],16384,c['apiKey']),'url':url(c['webPath']),'api':url(c['detail']['path']),'closed':closed,'revision':revision,'canClose':can,'closeStatusId':c.get('close',{}).get('closedState',''),'guard':c['close']['guard'] if can else 'none'}

def execute(filename,value,*,_transport=None):
    v=request(value);c=profile(filename)
    if v['expectedBaseUrl']!=c['baseUrl']: fail('profile_target_mismatch')
    if v['operation']=='close' and v['expectedProfileId']!=c['profileId']: fail('profile_changed')
    deadline=time.monotonic()+12;transport=_transport or http_raw
    def call(method,target,payload=None,headers=None): return transport(c,method,c['prefix']+target,payload,deadline,headers)
    def convert(raw,detail=False):
        parsed=rows(raw,c,detail,deadline)
        result=[issue(row,c) for row in parsed]
        if len({r['number'] for r in result})!=len(result): fail('duplicate_issue_identity')
        return result,parsed
    def detail():
        raw,etag=call('GET',c['detail']['path'].replace('{id}',urllib.parse.quote(v['expectedId'],safe='')))
        converted,parsed=convert(raw,True)
        if len(converted)!=1 or converted[0]['number']!=v['expectedId']: fail('issue_identity_mismatch')
        return converted[0],parsed[0],etag
    base={'version':1,'ok':True,'family':'generic','host':c['host'],'baseUrl':c['baseUrl'],'profileId':c['profileId'],'operation':v['operation'],'page':v.get('page',1),'hasMore':False}
    if v['operation']=='search':
        s=c['search'];query=urllib.parse.urlencode({s['queryParam']:v['query'],s['pageParam']:str(v['page']),s['pageSizeParam']:str(s['pageSize'])})
        raw,_=call('GET',s['path']+'?'+query);issues,_=convert(raw)
        if len(issues)>s['pageSize']: fail('issue_count_bound')
        return dict(base,issues=issues,hasMore=len(issues)==s['pageSize'])
    current,row,etag=detail()
    if current['revision']!=v['expectedRevision']: fail('issue_changed')
    if current['closed']: return dict(base,issues=[current],closed=True,alreadyClosed=True)
    if not current['canClose']: fail('close_not_configured')
    close=c['close']
    if close['guard']=='etag' and not etag: fail('strong_etag_unavailable')
    if profile(filename)['profileId']!=c['profileId']: fail('profile_changed')
    try:
        call(close['method'],close['path'].replace('{id}',urllib.parse.quote(v['expectedId'],safe='')),close['body'],{'If-Match':etag} if close['guard']=='etag' else None)
        result,verified,_=detail()
        if not result['closed'] or verified['state']!=close['closedState']: fail('close_state_unknown')
    except BridgeError as error:
        if str(error)=='revision_precondition_failed': raise
        fail('close_state_unknown')
    except Exception: fail('close_state_unknown')
    return dict(base,issues=[result],closed=True,alreadyClosed=False)

def main(argv):
    if len(argv)!=4: return 2
    try: result=execute(argv[1],private.json_object(private.bounded_read(argv[2],16*1024),'invalid_request'))
    except BridgeError as error: result={'version':1,'ok':False,'error':str(error)}
    except Exception: result={'version':1,'ok':False,'error':'adapter_failure'}
    try: private.atomic_output(argv[3],result)
    except Exception: return 2
    return 0
if __name__=='__main__': sys.exit(main(sys.argv))
