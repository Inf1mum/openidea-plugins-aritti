"""Local HTTP contract fixtures; production remains strict HTTPS."""
import contextlib
import copy
import http.client
import http.server
import importlib.util
import json
import pathlib
import shutil
import tempfile
import threading
import time
import unittest

spec=importlib.util.spec_from_file_location('generic',pathlib.Path(__file__).with_name('generic_bridge.py'))
g=importlib.util.module_from_spec(spec);spec.loader.exec_module(g)
KEY='fixture-secret-0123456789'
BASE='https://tracker.example.test/team'

def config():
    return {'version':1,'family':'generic','baseUrl':BASE,'apiKey':KEY,'auth':{'header':'Authorization','prefix':'Bearer '},'search':{'path':'/api/issues','queryParam':'q','pageParam':'page','pageSizeParam':'limit','pageSize':2},'detail':{'path':'/api/issues/{id}'},'webPath':'/issues/{id}','parser':{'kind':'json','items':'/issues','detail':'/issue','fields':{'id':'/key','summary':'/title','description':'/text','state':'/status','revision':'/version'}},'state':{'open':['OPEN'],'closed':['DONE']},'close':{'method':'PATCH','path':'/api/issues/{id}','body':{'status':'DONE'},'guard':'etag','closedState':'DONE'}}
def row(): return {'key':'TASK-1','title':'Example','text':'Actual description','status':'OPEN','version':'1'}
def search(): return {'version':1,'operation':'search','expectedFamily':'generic','expectedBaseUrl':BASE,'query':'a & b','page':1}

class Fixture(http.server.BaseHTTPRequestHandler):
    def log_message(self,*args): pass
    def do_GET(self): self.handle_request()
    def do_PATCH(self): self.handle_request()
    def handle_request(self):
        s=self.server;s.calls.append((self.command,self.path,dict(self.headers)))
        n=int(self.headers.get('Content-Length','0'))
        body=self.rfile.read(n)
        if self.headers.get('Authorization')!='Bearer '+KEY: self.send_error(401);return
        if s.redirect:
            self.send_response(302);self.send_header('Location','http://127.0.0.1:1/leak');self.send_header('Content-Length','0');self.end_headers();return
        if self.command=='PATCH':
            s.mutations+=1
            if self.headers.get('If-Match')!='"version-1"' and s.require_etag:
                self.send_response(412);self.send_header('Content-Length','0');self.end_headers();return
            if s.uncertain:
                self.connection.shutdown(2);self.connection.close();return
            if json.loads(body)!={'status':'DONE'}: self.send_error(400);return
            s.record['status']='DONE';s.record['version']='2'
            data=b'';status=204
        else:
            data=json.dumps({'issues':[s.record]} if '?' in self.path else {'issue':s.record}).encode();status=200
        self.send_response(status);self.send_header('Content-Type','application/json');self.send_header('ETag',s.etag);self.send_header('Content-Length',str(len(data)));self.end_headers();self.wfile.write(data)

@contextlib.contextmanager
def fixture():
    server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Fixture)
    server.record=row();server.calls=[];server.mutations=0;server.redirect=False;server.uncertain=False;server.require_etag=True;server.etag='"version-1"'
    thread=threading.Thread(target=server.serve_forever,daemon=True);thread.start()
    def factory(host,port,timeout,context):
        assert host=='tracker.example.test' and port==443
        return http.client.HTTPConnection('127.0.0.1',server.server_port,timeout=timeout)
    def transport(*args,**kwargs): return g.http_raw(*args,**kwargs,_connection_factory=factory)
    try: yield server,transport
    finally: server.shutdown();server.server_close();thread.join()

class Tests(unittest.TestCase):
    def setUp(self):
        self.tmp=tempfile.TemporaryDirectory();self.path=pathlib.Path(self.tmp.name).resolve()/'profile.json';self.c=config();self.save()
    def tearDown(self): self.tmp.cleanup()
    def save(self): self.path.write_text(json.dumps(self.c));self.path.chmod(0o600)
    def close_request(self,r):
        return {'version':1,'operation':'close','expectedFamily':'generic','expectedBaseUrl':BASE,'expectedId':'TASK-1','expectedProjectId':'','expectedRevision':r['issues'][0]['revision'],'expectedProfileId':r['profileId']}
    def error(self,code,fn):
        with self.assertRaises(g.BridgeError) as error: fn()
        self.assertEqual(str(error.exception),code);self.assertNotIn(KEY,str(error.exception))
    def test_http_search_pagination_auth_redaction_and_close(self):
        with fixture() as (s,t):
            s.record['title']='Visible '+KEY
            result=g.execute(self.path,search(),_transport=t)
            self.assertEqual(result['issues'][0]['summary'],'Visible [redacted]')
            self.assertNotIn(KEY,json.dumps(result));self.assertEqual(result['issues'][0]['api'],BASE+'/api/issues/TASK-1')
            self.assertEqual(s.calls[0][1],'/team/api/issues?q=a+%26+b&page=1&limit=2')
            self.assertFalse(result['hasMore']);self.assertEqual(result['issues'][0]['guard'],'etag')
            closed=g.execute(self.path,self.close_request(result),_transport=t)
            self.assertTrue(closed['issues'][0]['closed']);self.assertEqual(s.mutations,1)
            self.assertEqual(s.calls[2][2]['If-Match'],'"version-1"')
    def test_redirect_is_not_followed(self):
        with fixture() as (s,t):
            s.redirect=True;self.error('redirect_refused',lambda:g.execute(self.path,search(),_transport=t));self.assertEqual(len(s.calls),1)
    def test_identity_and_revision_refuse_mutation(self):
        with fixture() as (s,t):
            r=g.execute(self.path,search(),_transport=t);v=self.close_request(r)
            s.record['key']='OTHER';self.error('issue_identity_mismatch',lambda:g.execute(self.path,v,_transport=t))
            s.record['key']='TASK-1';s.record['version']='changed';self.error('issue_changed',lambda:g.execute(self.path,v,_transport=t));self.assertEqual(s.mutations,0)
    def test_profile_fingerprint_and_target(self):
        with fixture() as (s,t):
            r=g.execute(self.path,search(),_transport=t);self.c['auth']['prefix']='Token ';self.save()
            self.error('profile_changed',lambda:g.execute(self.path,self.close_request(r),_transport=t))
            v=search();v['expectedBaseUrl']='https://other.example.test';self.error('profile_target_mismatch',lambda:g.execute(self.path,v,_transport=t))
            self.assertEqual(len(s.calls),1)
    def test_uncertain_mutation_never_retried(self):
        with fixture() as (s,t):
            r=g.execute(self.path,search(),_transport=t);s.uncertain=True
            self.error('close_state_unknown',lambda:g.execute(self.path,self.close_request(r),_transport=t));self.assertEqual(s.mutations,1)
    def test_strong_etag_required(self):
        with fixture() as (s,t):
            r=g.execute(self.path,search(),_transport=t);s.etag='W/"weak"'
            self.error('strong_etag_unavailable',lambda:g.execute(self.path,self.close_request(r),_transport=t));self.assertEqual(s.mutations,0)
    def test_precondition_and_fresh_read(self):
        with fixture() as (s,t):
            r=g.execute(self.path,search(),_transport=t);s.etag='"changed"'
            self.error('revision_precondition_failed',lambda:g.execute(self.path,self.close_request(r),_transport=t));self.assertEqual(s.mutations,1)
        self.c['close']['guard']='fresh-read';self.save()
        with fixture() as (s,t):
            s.require_etag=False;r=g.execute(self.path,search(),_transport=t)
            self.assertEqual(r['issues'][0]['guard'],'fresh-read');self.assertTrue(g.execute(self.path,self.close_request(r),_transport=t)['closed'])
    def test_unknown_state_and_constant_contract(self):
        with fixture() as (s,t):
            s.record['status']='mystery';self.error('unknown_declared_state',lambda:g.execute(self.path,search(),_transport=t))
        self.c['state']={'constant':False};self.save();self.error('invalid_close_contract',lambda:g.profile(self.path))
        del self.c['close'];self.save()
        with fixture() as (s,t):
            s.record['status']='anything';self.assertFalse(g.execute(self.path,search(),_transport=t)['issues'][0]['canClose'])
    def test_pointer_is_literal_and_bounds(self):
        self.assertEqual(g.select({'a/b':{'~x':'value'}},'/a~1b/~0x'),'value')
        for bad in ('/a/../b','//evil','/api/{id}/{id}','/api/%2e%2e','/api/{id}?token=x'):
            self.error('invalid_profile_path',lambda:g.path(bad,True))
        self.path.chmod(0o644);self.error('profile_requires_owned_0600',lambda:g.profile(self.path))
    @unittest.skipUnless(shutil.which('java'),'Java 17 optional external requirement')
    def test_actual_java_pattern_named_group_backref_and_quote(self):
        p={'pattern':r'(?<key>TASK-\d+)\|(?<title>\QTitle.+\E)\|(?<text>[^|]+)\|(?<state>OPEN)\|(?<rev>\d+)\|\k<key>','flags':[],'groups':{'id':1,'summary':2,'description':3,'state':4,'revision':5}}
        rows=g.java_rows(b'TASK-1|Title.+|Description|OPEN|1|TASK-1',p,time.monotonic()+6)
        self.assertEqual(rows[0]['summary'],'Title.+');self.assertEqual(rows[0]['id'],'TASK-1')
        p['flags']=['CASE_INSENSITIVE'];self.assertEqual(len(g.java_rows(b'task-1|title.+|d|open|1|task-1',p,time.monotonic()+6)),1)
    @unittest.skipUnless(shutil.which('java'),'Java 17 optional external requirement')
    def test_java_invalid_capture_and_catastrophic_timeout(self):
        p={'pattern':'(a)','flags':[],'groups':dict.fromkeys(g.FIELDS,2)}
        self.error('java_regex_failure',lambda:g.java_rows(b'a',p,time.monotonic()+6))
        p={'pattern':'(a+)+$','flags':[],'groups':dict.fromkeys(g.FIELDS,0)}
        start=time.monotonic();self.error('java_regex_timeout',lambda:g.java_rows(b'a'*100000+b'!',p,time.monotonic()+6));self.assertLess(time.monotonic()-start,4)

if __name__=='__main__': unittest.main()
