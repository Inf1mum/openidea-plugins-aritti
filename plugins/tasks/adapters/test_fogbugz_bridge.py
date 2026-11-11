"""Actual HTTP fixture tests. Production has no HTTP/loopback/test CLI switch."""
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
import time
import unittest
import urllib.parse

sys.dont_write_bytecode = True
SOURCE = pathlib.Path(__file__).with_name('fogbugz_bridge.py')
spec = importlib.util.spec_from_file_location('fogbugz_bridge', SOURCE)
bridge = importlib.util.module_from_spec(spec)
spec.loader.exec_module(bridge)
TOKEN = 'test-token-never-publish-123456'

def case(number='123', event='10', opened=True, operations='close', title='A case', description='Latest comment'):
    return ('<response><cases count="1"><case ixBug="'+number+'" operations="'+operations+'">'
            '<sTitle>'+title+'</sTitle><fOpen>'+str(opened).lower()+'</fOpen>'
            '<ixBugEventLatest>'+event+'</ixBugEventLatest><sLatestTextSummary>'+description+'</sLatestTextSummary>'
            '</case></cases></response>').encode()

@contextlib.contextmanager
def server(responses):
    captured = []
    class Handler(http.server.BaseHTTPRequestHandler):
        def do_POST(self):
            raw = self.rfile.read(int(self.headers['Content-Length']))
            captured.append((self.path, dict(self.headers), urllib.parse.parse_qs(raw.decode())))
            status, headers, body = responses.pop(0)
            self.send_response(status)
            for key, value in headers.items(): self.send_header(key, value)
            self.send_header('Content-Length', str(len(body)))
            self.end_headers()
            self.wfile.write(body)
        def log_message(self, *_): pass
    httpd = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Handler)
    thread = threading.Thread(target=httpd.serve_forever, daemon=True);thread.start()
    def transport(endpoint, fields, deadline):
        def factory(host, port, timeout, context):
            assert context.check_hostname and context.verify_mode != 0
            assert host == 'acme.fogbugz.com' and port == 443
            return http.client.HTTPConnection('127.0.0.1', httpd.server_port, timeout=timeout)
        return bridge.post_xml(endpoint, fields, deadline, _connection_factory=factory)
    try: yield transport, captured
    finally: httpd.shutdown(); httpd.server_close();thread.join()

def response(body, status=200, headers=None):
    return status, headers or {'Content-Type':'text/xml; charset=utf-8'}, body

class BridgeTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = pathlib.Path(self.temp.name).resolve()
        self.profile = self.root/'fogbugz-profile.json'
        self.write_profile()
    def tearDown(self): self.temp.cleanup()
    def write_profile(self, **values):
        self.profile.write_text(json.dumps(dict(version=1,endpoint='https://acme.fogbugz.com/api.asp',token=TOKEN,**values)))
        self.profile.chmod(0o600)
    def search_request(self):
        return dict(version=1,operation='search',query='assignedto:me & status:active',page=1,expectedHost='acme.fogbugz.com')
    def close_request(self):
        return dict(version=1,operation='close',expectedHost='acme.fogbugz.com',expectedId='123',expectedEventId='10',expectedProfileId=bridge.profile(self.profile)['profileId'])
    def test_actual_http_form_post_headers_and_sanitized_case(self):
        with server([response(case(title='Case &amp; title',description='Latest '+TOKEN))]) as (transport,captured):
            result=bridge.execute(self.profile,self.search_request(),_transport=transport)
        self.assertTrue(result['ok']);self.assertEqual(result['issues'][0]['number'],'123')
        self.assertEqual(result['issues'][0]['url'],'https://acme.fogbugz.com/default.asp?123')
        self.assertEqual(result['issues'][0]['summary'],'Case & title')
        self.assertNotIn(TOKEN,json.dumps(result));self.assertIn('[redacted]',result['issues'][0]['description'])
        path,headers,fields=captured[0]
        self.assertEqual(path,'/api.asp');self.assertNotIn('token',path)
        self.assertEqual(fields['token'],[TOKEN]);self.assertEqual(fields['cmd'],['search']);self.assertEqual(fields['max'],['50'])
        self.assertEqual(fields['q'],[self.search_request()['query']]);self.assertIn('application/x-www-form-urlencoded',headers['Content-Type'])
        self.assertEqual(result['profileId'],bridge.profile(self.profile)['profileId'])
    def test_reviewed_close_fresh_identity_optimistic_guard_and_verified_state(self):
        responses=[response(case()),response(b'<response><case ixBug="123"/></response>'),response(case(event='11',opened=False))]
        with server(responses) as (transport,captured):result=bridge.execute(self.profile,self.close_request(),_transport=transport)
        self.assertTrue(result['closed']);self.assertFalse(result['alreadyClosed'])
        self.assertEqual([item[2]['cmd'][0] for item in captured],['search','close','search'])
        self.assertEqual(captured[1][2]['ixBug'],['123']);self.assertEqual(captured[1][2]['ixBugEvent'],['10'])
        self.assertEqual(result['issues'][0]['eventId'],'11')
    def test_changed_event_not_closeable_or_wrong_case_never_mutates(self):
        for body in [case(event='11'),case(operations='edit'),case(number='124')]:
            with server([response(body)]) as (transport,captured):
                with self.assertRaises(bridge.BridgeError):bridge.execute(self.profile,self.close_request(),_transport=transport)
            self.assertEqual([item[2]['cmd'][0] for item in captured],['search'])
    def test_changed_profile_before_call_or_during_fresh_read_never_closes(self):
        request=self.close_request();self.profile.write_text(self.profile.read_text().replace(TOKEN,TOKEN+'new'))
        with self.assertRaisesRegex(bridge.BridgeError,'profile_changed'):
            bridge.execute(self.profile,request,_transport=lambda *_:self.fail('must not contact remote'))
        self.write_profile();request=self.close_request();calls=[]
        def transport(endpoint,fields,deadline):
            calls.append(fields['cmd']);self.profile.write_text(self.profile.read_text().replace(TOKEN,TOKEN+'new'));return case()
        with self.assertRaisesRegex(bridge.BridgeError,'profile_changed'):bridge.execute(self.profile,request,_transport=transport)
        self.assertEqual(calls,['search'])
    def test_host_mismatch_and_page2_refuse_before_network(self):
        for key,value in [('expectedHost','other.fogbugz.com'),('page',2),('query','x'*4097)]:
            request=self.search_request();request[key]=value
            with self.assertRaises(bridge.BridgeError):bridge.execute(self.profile,request,_transport=lambda *_:self.fail('no network'))
    def test_redirect_never_followed_and_raw_error_never_returned(self):
        with server([response(TOKEN.encode(),302,{'Location':'https://attacker.example/?token='+TOKEN})]) as (transport,captured):
            with self.assertRaisesRegex(bridge.BridgeError,'redirect_refused'):bridge.execute(self.profile,self.search_request(),_transport=transport)
        self.assertEqual(len(captured),1)
        for raw in [b'<response><error code="3">'+TOKEN.encode()+b'</error></response>',b'<html>'+TOKEN.encode()+b'</html>']:
            with server([response(raw)]) as (transport,_):
                with self.assertRaises(bridge.BridgeError) as error:bridge.execute(self.profile,self.search_request(),_transport=transport)
                self.assertNotIn(TOKEN,str(error.exception))
    def test_dtd_entities_utf16_deep_or_oversize_response_refused(self):
        for raw in [b'<!DOCTYPE response [<!ENTITY x "secret">]><response>&x;</response>',
                    '<response/>'.encode('utf-16'),b'<response>'+b'<x>'*33+b'</x>'*33+b'</response>',b'x'*(bridge.MAX_RESPONSE+1)]:
            with self.assertRaises(bridge.BridgeError):bridge.parse_xml(raw)
    def test_profile_strict_endpoint_mode_owner_duplicate_and_symlink(self):
        raw=self.profile.read_text()
        for endpoint in ['http://acme.fogbugz.com/api.asp','https://acme.fogbugz.com:443/api.asp','https://user:pass@acme.fogbugz.com/api.asp','https://acme.fogbugz.com/api.asp?token=secret','https://evil.example/api.asp','https://a.b.fogbugz.com/api.asp']:
            value=json.loads(raw);value['endpoint']=endpoint;self.profile.write_text(json.dumps(value))
            with self.assertRaises(bridge.BridgeError):bridge.profile(self.profile)
        self.profile.write_text(raw);self.profile.chmod(0o644)
        with self.assertRaises(bridge.BridgeError):bridge.profile(self.profile)
        self.profile.chmod(0o600);link=self.root/'link';link.symlink_to(self.profile)
        with self.assertRaises(bridge.BridgeError):bridge.profile(link)
        self.profile.write_text('{"version":1,"version":1}')
        with self.assertRaises(bridge.BridgeError):bridge.profile(self.profile)
    def test_cli_isolated_error_atomic_output_no_secret_stdout_or_stderr(self):
        request=self.root/'request';out=self.root/'out';request.write_text(json.dumps(dict(self.search_request(),expectedHost='other.fogbugz.com')))
        result=subprocess.run([sys.executable,'-I','-S',str(SOURCE),str(self.profile),str(request),str(out)],capture_output=True,text=True,timeout=10)
        self.assertEqual(result.returncode,0);self.assertEqual(result.stdout,'');self.assertEqual(result.stderr,'')
        value=json.loads(out.read_text());self.assertFalse(value['ok']);self.assertEqual(value['error'],'host_mismatch');self.assertNotIn(TOKEN,out.read_text())
        self.assertEqual(stat.S_IMODE(out.stat().st_mode),0o600);self.assertFalse(list(self.root.glob('.fogbugz-*')))
    def test_failed_close_is_unknown_not_success_and_not_retried(self):
        with server([response(case()),response(b'<response><error>stale '+TOKEN.encode()+b'</error></response>')]) as (transport,captured):
            with self.assertRaisesRegex(bridge.BridgeError,'close_state_unknown'):bridge.execute(self.profile,self.close_request(),_transport=transport)
        self.assertEqual([item[2]['cmd'][0] for item in captured],['search','close'])
    def test_overfull_duplicate_or_missing_case_fields_are_refused(self):
        body=case();one=body.decode().split('<case ',1)[1].split('</case>',1)[0]
        bodies=[b'<response><cases>'+('<case '+one+'</case>').encode()*51+b'</cases></response>',
                b'<response><cases>'+('<case '+one+'</case>').encode()*2+b'</cases></response>',
                b'<response><cases><case ixBug="123"><sTitle>Missing</sTitle></case></cases></response>']
        config=bridge.profile(self.profile)
        for body in bodies:
            with self.assertRaises(bridge.BridgeError):bridge.cases(bridge.parse_xml(body),config)

if __name__=='__main__': unittest.main()
