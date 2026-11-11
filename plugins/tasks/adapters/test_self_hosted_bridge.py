"""Real loopback HTTP fixtures for production Redmine/MantisBT protocol paths."""
import contextlib
import http.client
import http.server
import importlib.util
import json
import os
import pathlib
import stat
import subprocess
import sys
import tempfile
import threading
import unittest
import urllib.parse
sys.dont_write_bytecode=True
SOURCE=pathlib.Path(__file__).with_name('self_hosted_bridge.py')
spec=importlib.util.spec_from_file_location('self_hosted_bridge',SOURCE)
bridge=importlib.util.module_from_spec(spec);spec.loader.exec_module(bridge)
TOKEN='fixture-api-key-never-print-12345'

def raw_issue(family='redmine',closed=False,updated='2026-11-11T10:00:00Z'):
    return dict(id=12,project={'id':3},status={'id':5 if closed else 1},
                **({'subject':'Issue & summary','updated_on':updated} if family=='redmine' else {'summary':'Issue & summary','updated_at':updated}),
                description='Description '+TOKEN)

def redmine_statuses():return {'issue_statuses':[{'id':1,'is_closed':False},{'id':5,'is_closed':True}]}
def mantis_statuses(workflow=None):
    return {'configs':[{'option':'bug_closed_status_threshold','value':5},
                       {'option':'status_enum_string','value':[{'id':1,'name':'new'},{'id':5,'name':'closed'}]},
                       {'option':'status_enum_workflow','value':{} if workflow is None else workflow}]}
def reply(data,status=200,headers=None):
    return status,dict({'Content-Type':'application/json'},**(headers or {})),b'' if status==204 else json.dumps(data).encode()

@contextlib.contextmanager
def server(responses):
    captured=[]
    class Handler(http.server.BaseHTTPRequestHandler):
        def handle_request(self):
            data=self.rfile.read(int(self.headers.get('Content-Length','0')))
            captured.append((self.command,self.path,dict(self.headers),json.loads(data) if data else None))
            status,headers,body=responses.pop(0)
            self.send_response(status)
            for key,value in headers.items():self.send_header(key,value)
            self.send_header('Content-Length',str(len(body)));self.end_headers();self.wfile.write(body)
        do_GET=handle_request;do_PUT=handle_request;do_PATCH=handle_request
        def log_message(self,*_):pass
    httpd=http.server.ThreadingHTTPServer(('127.0.0.1',0),Handler)
    thread=threading.Thread(target=httpd.serve_forever,daemon=True);thread.start()
    def transport(config,method,path,payload,deadline,headers=None):
        def factory(host,port,timeout,context):
            assert host=='tracker.example' and port==443
            assert context.check_hostname and context.verify_mode!=0
            return http.client.HTTPConnection('127.0.0.1',httpd.server_port,timeout=timeout)
        return bridge.http_json(config,method,path,payload,deadline,headers,_connection_factory=factory)
    try:yield transport,captured
    finally:httpd.shutdown();httpd.server_close();thread.join()

class SelfHostedTest(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory();self.root=pathlib.Path(self.temp.name).resolve();self.profile=self.root/'profile';self.configure()
    def tearDown(self):self.temp.cleanup()
    def configure(self,family='redmine',**extra):
        values=dict(version=1,family=family,baseUrl='https://tracker.example/tracker',apiKey=TOKEN,closeStatusId='5');values.update(extra)
        self.profile.write_text(json.dumps(values));self.profile.chmod(0o600);self.family=family
    def search(self,query=None,page=1):
        if query is None:query='project_id=3&status_id=open&assigned_to_id=me' if self.family=='redmine' else 'project_id=3&filter_id=9'
        return dict(version=1,operation='search',expectedFamily=self.family,expectedBaseUrl='https://tracker.example/tracker',query=query,page=page)
    def close(self):
        config=bridge.profile(self.profile)
        statuses={'1':False,'5':True}
        parsed=bridge.issue(raw_issue(self.family),config,statuses,{},'3')
        return dict(version=1,operation='close',expectedFamily=self.family,expectedBaseUrl=config['baseUrl'],expectedId='12',expectedProjectId='3',expectedRevision=parsed['revision'],expectedProfileId=config['profileId'])
    def test_redmine_search_actual_get_header_pagination_sanitized_output(self):
        responses=[reply(redmine_statuses()),reply({'issues':[raw_issue()], 'total_count':77,'offset':50,'limit':50})]
        with server(responses) as (transport,captured):result=bridge.execute(self.profile,self.search(page=2),_transport=transport)
        self.assertTrue(result['hasMore']);self.assertEqual(result['issues'][0]['number'],'12');self.assertEqual(result['issues'][0]['projectId'],'3')
        self.assertEqual(result['issues'][0]['url'],'https://tracker.example/tracker/issues/12');self.assertEqual(result['issues'][0]['guard'],'fresh-read');self.assertNotIn(TOKEN,json.dumps(result))
        method,path,headers,body=captured[1]
        self.assertEqual(method,'GET');self.assertTrue(path.startswith('/tracker/issues.json?'));self.assertEqual(headers['X-Redmine-API-Key'],TOKEN);self.assertIsNone(body)
        query=urllib.parse.parse_qs(urllib.parse.urlsplit(path).query)
        self.assertEqual(query['offset'],['50']);self.assertEqual(query['limit'],['50']);self.assertEqual(query['assigned_to_id'],['me'])
    def test_redmine_reviewed_allowed_status_update_and_verified_closed(self):
        original=raw_issue();original['allowed_statuses']=[{'id':5,'name':'Closed'}]
        responses=[reply(redmine_statuses()),reply({'issue':original}),reply({},204),reply(redmine_statuses()),reply({'issue':raw_issue(closed=True,updated='2026-11-11T10:01:00Z')})]
        with server(responses) as (transport,captured):result=bridge.execute(self.profile,self.close(),_transport=transport)
        self.assertTrue(result['closed']);self.assertEqual(captured[2][0],'PUT');self.assertEqual(captured[2][3],{'issue':{'status_id':5}})
        self.assertTrue(captured[1][1].endswith('include=allowed_statuses'));self.assertNotIn('If-Match',captured[2][2])
    def test_redmine_unallowed_status_or_changed_revision_never_puts(self):
        for original in [dict(raw_issue(),allowed_statuses=[]),dict(raw_issue(updated='2026-11-11T10:01:00Z'),allowed_statuses=[{'id':5}])]:
            with server([reply(redmine_statuses()),reply({'issue':original})]) as (transport,captured):
                with self.assertRaises(bridge.BridgeError):bridge.execute(self.profile,self.close(),_transport=transport)
            self.assertTrue(all(item[0]=='GET' for item in captured))
    def test_mantis_actual_filter_page_authorization_and_etag_guard(self):
        self.configure('mantis')
        responses=[reply(mantis_statuses()),reply({'issues':[raw_issue('mantis')]},headers={'ETag':'"page"'})]
        with server(responses) as (transport,captured):result=bridge.execute(self.profile,self.search(page=3),_transport=transport)
        self.assertEqual(result['issues'][0]['guard'],'etag');self.assertEqual(result['issues'][0]['url'],'https://tracker.example/tracker/view.php?id=12')
        self.assertEqual(captured[1][2]['Authorization'],'Bearer '+TOKEN)
        query=urllib.parse.parse_qs(urllib.parse.urlsplit(captured[1][1]).query)
        self.assertEqual(query['page'],['3']);self.assertEqual(query['page_size'],['50']);self.assertEqual(query['filter_id'],['9'])
    def test_mantis_close_actual_if_match_patch_verified_status(self):
        self.configure('mantis')
        responses=[reply(mantis_statuses()),reply({'issues':[raw_issue('mantis')]},headers={'ETag':'"strong-current"'}),reply({'issues':[raw_issue('mantis',closed=True)]}),reply(mantis_statuses()),reply({'issues':[raw_issue('mantis',closed=True)]},headers={'ETag':'"new"'})]
        with server(responses) as (transport,captured):result=bridge.execute(self.profile,self.close(),_transport=transport)
        self.assertTrue(result['closed']);self.assertEqual(captured[2][0],'PATCH');self.assertEqual(captured[2][2]['If-Match'],'"strong-current"');self.assertEqual(captured[2][3],{'status':{'id':5}})
    def test_mantis_missing_etag_or_workflow_disallowed_never_patches(self):
        self.configure('mantis')
        for metadata in [mantis_statuses(),mantis_statuses({'1':'2:feedback'})]:
            with server([reply(metadata),reply({'issues':[raw_issue('mantis')]})]) as (transport,captured):
                with self.assertRaises(bridge.BridgeError):bridge.execute(self.profile,self.close(),_transport=transport)
            self.assertTrue(all(item[0]=='GET' for item in captured))
    def test_mantis_precondition_failure_is_reported_and_no_retry(self):
        self.configure('mantis')
        responses=[reply(mantis_statuses()),reply({'issues':[raw_issue('mantis')]},headers={'ETag':'"strong"'}),reply({'error':TOKEN},412)]
        with server(responses) as (transport,captured):
            with self.assertRaisesRegex(bridge.BridgeError,'revision_precondition_failed'):bridge.execute(self.profile,self.close(),_transport=transport)
        self.assertEqual(len(captured),3)
    def test_missing_closed_metadata_refuses_instead_of_guessing_status(self):
        for family,data in [('redmine',{'issue_statuses':[{'id':1}]}),('mantis',{'configs':[]})]:
            self.configure(family)
            with server([reply(data)]) as (transport,captured):
                with self.assertRaisesRegex(bridge.BridgeError,'status_metadata_unavailable'):bridge.execute(self.profile,self.search(),_transport=transport)
            self.assertEqual(len(captured),1)
    def test_free_text_unknown_filters_and_missing_mantis_project_refused(self):
        for family,query in [('redmine','words to search'),('redmine','subject=foo'),('redmine','project_id=3&project_id=4'),('mantis','filter_id=3'),('mantis','project_id=3&q=foo')]:
            self.configure(family)
            with self.assertRaises(bridge.BridgeError):bridge.execute(self.profile,self.search(query=query),_transport=lambda *_:self.fail('no network'))
    def test_profile_exact_host_prefix_permissions_and_stale_proof(self):
        for base in ['http://tracker.example','https://tracker.example:443','https://user@tracker.example','https://tracker.example/a/../b','https://tracker.example/a%2fb','https://127.0.0.1','https://tracker.example/','https://tracker.example/?key=x']:
            self.configure(baseUrl=base)
            with self.assertRaises(bridge.BridgeError):bridge.profile(self.profile)
        self.configure();request=self.close();self.configure(apiKey=TOKEN+'changed')
        with self.assertRaisesRegex(bridge.BridgeError,'profile_changed'):bridge.execute(self.profile,request,_transport=lambda *_:self.fail('no network'))
        self.configure();self.profile.chmod(0o644)
        with self.assertRaises(bridge.BridgeError):bridge.profile(self.profile)
    def test_redirect_and_raw_server_error_never_leak_secret(self):
        for response in [reply({'error':TOKEN},302,{'Location':'https://other.example/'+TOKEN}),reply({'errors':[TOKEN]})]:
            with server([response]) as (transport,captured):
                with self.assertRaises(bridge.BridgeError) as error:bridge.execute(self.profile,self.search(),_transport=transport)
            self.assertNotIn(TOKEN,str(error.exception));self.assertEqual(len(captured),1)
    def test_cli_isolated_atomic_errors_without_credentials(self):
        request=self.root/'request';output=self.root/'output';request.write_text(json.dumps(dict(self.search(),expectedFamily='mantis')))
        result=subprocess.run([sys.executable,'-I','-S',str(SOURCE),str(self.profile),str(request),str(output)],capture_output=True,text=True,timeout=10)
        self.assertEqual(result.returncode,0);self.assertEqual(result.stdout+result.stderr,'');self.assertEqual(json.loads(output.read_text())['error'],'profile_target_mismatch')
        self.assertNotIn(TOKEN,output.read_text());self.assertEqual(stat.S_IMODE(output.stat().st_mode),0o600)
    def test_malformed_json_depth_duplicate_keys_and_output_identity(self):
        for raw in [b'{"issues":[],"issues":[]}',b'{"error":"'+TOKEN.encode()+b'"}',b'{}'+b' '*bridge.MAX_RESPONSE]:
            with self.assertRaises(bridge.BridgeError):bridge.parse_json(raw)
        for field,value in [('project',{'id':4}),('updated_on','not-a-date'),('id',True)]:
            original=raw_issue();original[field]=value
            with self.assertRaises(bridge.BridgeError):bridge.issue(original,bridge.profile(self.profile),{'1':False,'5':True},None,'3')
if __name__=='__main__':unittest.main()
