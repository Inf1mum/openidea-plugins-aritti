#!/usr/bin/env python3
"""Real bounded Redmine/MantisBT REST transports. PROFILE REQUEST OUTPUT."""
import hashlib
import http.client
import importlib.util
import json
import pathlib
import re
import ssl
import sys
import time
import urllib.parse

# Native materializes both reviewed helpers together in its private directory.
# Isolated Python never imports a module from the current project/sys.path.
sys.dont_write_bytecode = True
_spec = importlib.util.spec_from_file_location('_tasks_private_transport', pathlib.Path(__file__).with_name('fogbugz_bridge.py'))
private = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(private)
BridgeError, fail = private.BridgeError, private.fail
MAX_RESPONSE = 1024 * 1024
DNS = re.compile(r'[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?(?:\.[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?)+\Z')
HEX = re.compile('[a-f0-9]{64}\\Z')
DATE = re.compile(r'\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d(?:\.\d+)?(?:Z|[+-]\d\d:\d\d)\Z')

def base_url(value):
    if not isinstance(value, str) or len(value) > 512:
        fail('invalid_base_url')
    try:
        parsed = urllib.parse.urlsplit(value)
        host = parsed.hostname
        prefix = parsed.path
        if not host or len(host) > 253 or not DNS.fullmatch(host) or host.replace('.', '').isdigit():
            fail('invalid_base_url')
        if not re.fullmatch(r'(?:/[A-Za-z0-9_-]+)*', prefix):
            fail('invalid_base_url')
        if value != 'https://' + host + prefix:
            fail('invalid_base_url')
        return host, prefix
    except BridgeError:
        raise
    except Exception:
        fail('invalid_base_url')

def profile(path):
    raw = private.bounded_read(path, 16 * 1024, private=True)
    data = private.json_object(raw, 'invalid_profile')
    if not {'version','family','baseUrl','apiKey'} <= set(data) or set(data) - {'version','family','baseUrl','apiKey','closeStatusId'}:
        fail('invalid_profile')
    if type(data['version']) is not int or data['version'] != 1 or data['family'] not in ('redmine','mantis'):
        fail('invalid_profile')
    key = data['apiKey']
    if not isinstance(key,str) or not 16 <= len(key) <= 4096 or any(ord(c)<33 or ord(c)>126 for c in key):
        fail('invalid_profile')
    close = data.get('closeStatusId','')
    if not isinstance(close,str) or (close and not private.positive_id(close)):
        fail('invalid_close_status')
    host,prefix = base_url(data['baseUrl'])
    fingerprint = hashlib.sha256(json.dumps([str(pathlib.Path(path)),data['baseUrl'],data['family']],separators=(',',':')).encode()+b'\0'+raw).hexdigest()
    return dict(data,host=host,prefix=prefix,closeStatusId=close,profileId=fingerprint)

def request(value):
    if type(value.get('version')) is not int or value['version'] != 1 or value.get('expectedFamily') not in ('redmine','mantis'):
        fail('invalid_request')
    base_url(value.get('expectedBaseUrl'))
    common = {'version','operation','expectedFamily','expectedBaseUrl'}
    if value.get('operation') == 'search':
        if set(value) != common|{'query','page'} or type(value['page']) is not int or not 1 <= value['page'] <= 1000:
            fail('invalid_request')
        query = value['query']
        if not isinstance(query,str) or len(query.encode('utf-8')) > 4096 or any(ord(c)<32 for c in query):
            fail('invalid_query')
    elif value.get('operation') == 'close':
        if set(value) != common|{'expectedId','expectedProjectId','expectedRevision','expectedProfileId'}:
            fail('invalid_request')
        if not private.positive_id(value['expectedId']) or not private.positive_id(value['expectedProjectId']):
            fail('invalid_issue_identity')
        if not isinstance(value['expectedRevision'],str) or not HEX.fullmatch(value['expectedRevision']) or not isinstance(value['expectedProfileId'],str) or not HEX.fullmatch(value['expectedProfileId']):
            fail('invalid_revision')
    else:
        fail('invalid_operation')
    return value

def filters(query,family):
    try:
        pairs = urllib.parse.parse_qsl(query,keep_blank_values=True,strict_parsing=True,max_num_fields=8)
    except Exception:
        fail('unsupported_query')
    values = {}
    allowed = {'project_id','tracker_id','status_id','assigned_to_id','issue_id'} if family=='redmine' else {'project_id','filter_id'}
    for key,value in pairs:
        if key not in allowed or key in values:
            fail('unsupported_query')
        valid = private.positive_id(value)
        if family=='redmine' and key=='status_id': valid = valid or value in ('open','closed','*')
        if family=='redmine' and key=='assigned_to_id': valid = valid or value=='me'
        if family=='redmine' and key=='issue_id': valid = len(value.split(',')) <= 20 and all(private.positive_id(n) for n in value.split(','))
        if not valid: fail('unsupported_query')
        values[key]=value
    if family=='redmine':
        values.setdefault('status_id','open');values.setdefault('assigned_to_id','me')
    elif 'project_id' not in values:
        fail('mantis_project_required')
    return values

def http_json(config,method,path,payload,deadline,headers=None,*,_connection_factory=None):
    connection = None
    try:
        if not path.startswith(config['prefix']+'/') or '\r' in path or '\n' in path:
            fail('invalid_request_path')
        factory = _connection_factory or http.client.HTTPSConnection
        connection = factory(config['host'],port=443,timeout=private.remaining(deadline),context=ssl.create_default_context())
        request_headers = {'Accept':'application/json','Accept-Encoding':'identity','User-Agent':'OpenIDEA-Tasks-SelfHosted/1'}
        request_headers['X-Redmine-API-Key' if config['family']=='redmine' else 'Authorization'] = config['apiKey'] if config['family']=='redmine' else 'Bearer '+config['apiKey']
        request_headers.update(headers or {})
        body = None if payload is None else json.dumps(payload,separators=(',',':')).encode('utf-8')
        if body is not None: request_headers['Content-Type']='application/json; charset=utf-8'
        connection.request(method,path,body=body,headers=request_headers)
        socket = connection.sock
        response = connection.getresponse()
        if 300 <= response.status < 400: fail('redirect_refused')
        if response.status == 412: fail('revision_precondition_failed')
        if response.status not in (200,204): fail('http_failure')
        length = response.getheader('Content-Length')
        if length is not None and (not length.isdecimal() or int(length)>MAX_RESPONSE): fail('response_bound')
        if response.getheader('Content-Encoding','identity').lower() not in ('','identity'): fail('unsupported_response_encoding')
        if response.status != 204 and response.getheader('Content-Type','').split(';')[0].strip().lower() != 'application/json': fail('invalid_json_response')
        data = bytearray()
        while len(data)<=MAX_RESPONSE and not response.isclosed():
            timeout = private.remaining(deadline)
            live = connection.sock or socket
            if live is not None: live.settimeout(timeout)
            part = response.read1(min(16384,MAX_RESPONSE+1-len(data)))
            if not part: break
            data.extend(part)
        if len(data)>MAX_RESPONSE: fail('response_bound')
        private.remaining(deadline)
        parsed = {} if response.status==204 else parse_json(bytes(data))
        etag = response.getheader('ETag','')
        # An opaque strict strong token is safe to forward only as If-Match.
        if etag and (len(etag)>256 or not re.fullmatch(r'"[!#-~]+"',etag)): etag=''
        return parsed,etag
    except BridgeError:
        raise
    except Exception:
        fail('transport_failure')
    finally:
        if connection is not None: connection.close()

def parse_json(raw):
    if len(raw)>MAX_RESPONSE: fail('response_bound')
    if b'\0' in raw: fail('invalid_json_response')
    try:
        data = private.json_object(raw,'invalid_json_response')
        pending=[(data,0)];count=0
        while pending:
            value,depth=pending.pop();count+=1
            if depth>32 or count>20000: fail('json_structure_bound')
            if isinstance(value,dict): pending.extend((v,depth+1) for v in value.values())
            elif isinstance(value,list): pending.extend((v,depth+1) for v in value)
        if 'error' in data or 'errors' in data: fail('api_failure')
        return data
    except BridgeError: raise
    except Exception: fail('invalid_json_response')

def numeric(value):
    if type(value) is not int or not 1<=value<=2147483647: fail('invalid_issue_identity')
    return str(value)

def object_id(value):
    if not isinstance(value,dict): fail('invalid_issue_response')
    return numeric(value.get('id'))

def issue(raw,config,statuses,workflow=None,project=None):
    if not isinstance(raw,dict): fail('invalid_issue_response')
    number = numeric(raw.get('id'));project_id=object_id(raw.get('project'));status=object_id(raw.get('status'))
    if project is not None and project_id != project: fail('project_mismatch')
    if status not in statuses: fail('unknown_status_metadata')
    summary = raw.get('subject' if config['family']=='redmine' else 'summary')
    description = raw.get('description','')
    updated = raw.get('updated_on' if config['family']=='redmine' else 'updated_at')
    if not isinstance(summary,str) or not summary.strip() or not isinstance(description,str) or not isinstance(updated,str) or not DATE.fullmatch(updated): fail('invalid_issue_response')
    revision = hashlib.sha256(json.dumps([number,project_id,status,summary,description,updated],ensure_ascii=True,separators=(',',':')).encode()).hexdigest()
    closed = statuses[status]
    target = config['closeStatusId']
    can_close = bool(target and statuses.get(target) is True and not closed)
    if config['family']=='mantis': can_close = can_close and transition(workflow,status,target)
    return {'number':number,'projectId':project_id,'summary':private.clean(summary,1024,config['apiKey']),
            'description':private.clean(description,16384,config['apiKey']),
            'url':config['baseUrl']+('/issues/'+number if config['family']=='redmine' else '/view.php?id='+number),
            'closed':closed,'revision':revision,'canClose':can_close,'closeStatusId':target,
            'guard':('fresh-read' if config['family']=='redmine' else 'etag') if can_close else 'none'}

def transition(workflow,current,target):
    if workflow in ({},[]): return True
    if not isinstance(workflow,dict): return False
    value=workflow.get(current)
    if not isinstance(value,str) or len(value)>4096: return False
    ids=[]
    for part in value.split(','):
        identifier,sep,_=part.partition(':')
        if not sep or not private.positive_id(identifier): return False
        ids.append(identifier)
    return target in ids

def redmine_statuses(data):
    values=data.get('issue_statuses')
    if not isinstance(values,list) or not 1<=len(values)<=200: fail('status_metadata_unavailable')
    result={}
    for item in values:
        if not isinstance(item,dict) or type(item.get('is_closed')) is not bool: fail('status_metadata_unavailable')
        key=numeric(item.get('id'))
        if key in result: fail('status_metadata_unavailable')
        result[key]=item['is_closed']
    return result

def mantis_statuses(data):
    values=data.get('configs')
    if not isinstance(values,list) or len(values)>20: fail('status_metadata_unavailable')
    mapped={}
    for item in values:
        if not isinstance(item,dict) or not isinstance(item.get('option'),str) or item['option'] in mapped: fail('status_metadata_unavailable')
        mapped[item['option']]=item.get('value')
    threshold=mapped.get('bug_closed_status_threshold');enum=mapped.get('status_enum_string')
    if type(threshold) is not int or not 1<=threshold<=2147483647 or not isinstance(enum,list) or not 1<=len(enum)<=200: fail('status_metadata_unavailable')
    result={}
    for item in enum:
        key=object_id(item)
        if key in result: fail('status_metadata_unavailable')
        result[key]=int(key)>=threshold
    return result,mapped.get('status_enum_workflow')

def execute(profile_path,value,*,_transport=None):
    value=request(value);config=profile(profile_path)
    if value['expectedFamily']!=config['family'] or value['expectedBaseUrl']!=config['baseUrl']: fail('profile_target_mismatch')
    if value['operation']=='close' and value['expectedProfileId']!=config['profileId']: fail('profile_changed')
    transport=_transport or http_json;deadline=time.monotonic()+8
    def call(method,suffix,params=None,payload=None,headers=None):
        path=config['prefix']+suffix
        if params: path+='?'+urllib.parse.urlencode(params,doseq=True)
        return transport(config,method,path,payload,deadline,headers)
    family=config['family']
    query=filters(value['query'],family) if value['operation']=='search' else None
    project=query.get('project_id') if query is not None else value['expectedProjectId']
    def metadata():
        if family=='redmine': return redmine_statuses(call('GET','/issue_statuses.json')[0]),None
        params=[('option[]','bug_closed_status_threshold'),('option[]','status_enum_string'),('option[]','status_enum_workflow'),('project_id',project)]
        return mantis_statuses(call('GET','/api/rest/config',params=params)[0])
    statuses,workflow=metadata()
    base={'version':1,'ok':True,'family':family,'host':config['host'],'baseUrl':config['baseUrl'],'profileId':config['profileId'],'operation':value['operation']}
    def convert(raw): return issue(raw,config,statuses,workflow,project)
    def detail(identifier):
        if family=='redmine':
            data,etag=call('GET','/issues/'+identifier+'.json',params={'include':'allowed_statuses'})
            raw=data.get('issue')
        else:
            data,etag=call('GET','/api/rest/issues/'+identifier)
            values=data.get('issues')
            if not isinstance(values,list) or len(values)!=1: fail('invalid_issue_response')
            raw=values[0]
        parsed=convert(raw)
        if parsed['number']!=identifier: fail('issue_identity_mismatch')
        return parsed,raw,etag
    if value['operation']=='search':
        page=value['page'];offset=(page-1)*50
        if family=='redmine':
            params=dict(query,offset=str(offset),limit='50',sort='id:asc');data,_=call('GET','/issues.json',params=params)
            total=data.get('total_count');returned_offset=data.get('offset');limit=data.get('limit')
            if type(total) is not int or total<0 or type(returned_offset) is not int or returned_offset!=offset or type(limit) is not int or not 1<=limit<=50: fail('invalid_pagination')
            raw=data.get('issues');more=offset+(len(raw) if isinstance(raw,list) else 0)<total
        else:
            data,_=call('GET','/api/rest/issues',params=dict(query,page=str(page),page_size='50'))
            raw=data.get('issues');more=isinstance(raw,list) and len(raw)==50
        if not isinstance(raw,list) or len(raw)>50: fail('issue_count_bound')
        issues=[convert(item) for item in raw]
        if len({item['number'] for item in issues})!=len(issues): fail('duplicate_issue_identity')
        return dict(base,issues=issues,page=page,hasMore=more)
    selected,raw,etag=detail(value['expectedId'])
    if selected['revision']!=value['expectedRevision']: fail('issue_changed')
    if selected['closed']: return dict(base,issues=[selected],closed=True,alreadyClosed=True)
    if not selected['canClose']: fail('close_not_configured_or_allowed')
    if family=='redmine':
        allowed=raw.get('allowed_statuses')
        if not isinstance(allowed,list) or len(allowed)>200 or not any(object_id(item)==config['closeStatusId'] for item in allowed): fail('close_status_not_allowed')
    elif not etag:
        fail('strong_etag_unavailable')
    if profile(profile_path)['profileId']!=config['profileId']: fail('profile_changed')
    # Fresh status metadata and current issue bind the exact configured target;
    # Redmine has no documented CAS here. Mantis supports a real If-Match guard.
    try:
        if family=='redmine': call('PUT','/issues/'+value['expectedId']+'.json',payload={'issue':{'status_id':int(config['closeStatusId'])}})
        else: call('PATCH','/api/rest/issues/'+value['expectedId'],payload={'status':{'id':int(config['closeStatusId'])}},headers={'If-Match':etag})
        statuses,workflow=metadata()
        result,verified_raw,_=detail(value['expectedId'])
        if not result['closed'] or object_id(verified_raw['status'])!=config['closeStatusId']: fail('close_state_unknown')
    except BridgeError as error:
        if str(error)=='revision_precondition_failed': raise
        fail('close_state_unknown')
    except Exception: fail('close_state_unknown')
    return dict(base,issues=[result],closed=True,alreadyClosed=False)

def main(argv):
    if len(argv)!=4: return 2
    try:
        value=private.json_object(private.bounded_read(argv[2],16*1024),'invalid_request')
        output=execute(argv[1],value)
    except BridgeError as error: output={'version':1,'ok':False,'error':str(error)}
    except Exception: output={'version':1,'ok':False,'error':'adapter_failure'}
    try: private.atomic_output(argv[3],output)
    except Exception: return 2
    return 0
if __name__=='__main__': sys.exit(main(sys.argv))
