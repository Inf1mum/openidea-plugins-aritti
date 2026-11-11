import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
from qute_jdt_broker import child_environment
from microprofile_broker import MicroProfileBroker, MP_CALLBACKS, DATA_KEY, config_uri

class RecordingMP(MicroProfileBroker):
    def __init__(self):
        super().__init__({}, '/tmp/unused-microprofile-state')
        self.sent=[]
        self.processes={'jdt':None,'qute':None,'microprofile':None}
        self.jdt_ready=True
    def send(self,endpoint,message):
        self.sent.append((endpoint,message))

class MicroProfileProtocolTests(unittest.TestCase):
    def test_overlay_barrier_requires_current_complete_receipt_and_rejects_inflight_stale_result(self):
        from config_overlay import ConfigOverlay
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary).resolve();source=root/'application.properties';source.write_text('key=disk')
            broker=RecordingMP();broker.root=root;broker.overlay=ConfigOverlay(root,root/'snapshot.json')
            broker.overlay.event('textDocument/didOpen',{'textDocument':{'uri':source.as_uri(),'version':1,'text':'key=unsaved'}})
            broker.dirty_configs={source.as_uri()}
            callback={'jsonrpc':'2.0','id':'callback','method':'microprofile/java/inlayHint','params':{}}
            broker.backend('microprofile',callback);request=broker.sent[-1][1]
            self.assertEqual(request['params']['command'],'openidea.configOverlay.synchronize')
            broker.backend('jdt',{'jsonrpc':'2.0','id':request['id'],'result':{'generation':1,'applied':[],'unsupported':[source.as_uri()]}})
            self.assertEqual(broker.sent[-1][1]['error']['code'],-32801)
            broker.backend('microprofile',callback);request=broker.sent[-1][1]
            broker.backend('jdt',{'jsonrpc':'2.0','id':request['id'],'result':{'generation':1,'applied':[source.as_uri()],'unsupported':[]}})
            forwarded=broker.sent[-1][1];self.assertEqual(forwarded['params']['command'],'microprofile/java/inlayHint')
            broker.overlay.event('textDocument/didChange',{'textDocument':{'uri':source.as_uri(),'version':2},'contentChanges':[{'text':'key=new'}]})
            broker.backend('jdt',{'jsonrpc':'2.0','id':forwarded['id'],'result':[{'label':'stale-value'}]})
            self.assertEqual(broker.sent[-1][1]['error']['code'],-32801)
            self.assertNotIn('stale-value',json.dumps(broker.sent[-1]))

    def test_overlay_callback_can_be_cancelled_during_snapshot_barrier(self):
        from config_overlay import ConfigOverlay
        with tempfile.TemporaryDirectory() as temporary:
            broker=RecordingMP();broker.overlay=ConfigOverlay(temporary,Path(temporary)/'snapshot.json')
            broker.backend('microprofile',{'jsonrpc':'2.0','id':3,'method':'microprofile/projectInfo','params':{}})
            broker.backend('microprofile',{'jsonrpc':'2.0','method':'$/cancelRequest','params':{'id':3}})
            self.assertFalse(broker.overlay_syncs);self.assertEqual(broker.sent[-1][1]['error']['code'],-32800)

    def test_all_official_callbacks_use_the_shared_java_session_and_keep_ids(self):
        broker=RecordingMP()
        for index,method in enumerate(sorted(MP_CALLBACKS)):
            broker.backend('microprofile',{'jsonrpc':'2.0','id':index,'method':method,'params':{'uri':'file:///project/application.properties'}})
            endpoint,request=broker.sent[-1]
            self.assertEqual(endpoint,'jdt');self.assertEqual(request['params']['command'],method)
            result={'projectURI':'project'} if method=='microprofile/projectInfo' else {'known':True}
            broker.backend('jdt',{'jsonrpc':'2.0','id':request['id'],'result':result})
            self.assertEqual(broker.sent[-1],('microprofile',{'jsonrpc':'2.0','id':index,'result':result}))
        self.assertIn('project',broker.mp_projects)

    def test_callbacks_wait_for_java_service_ready_and_can_be_cancelled(self):
        broker=RecordingMP();broker.jdt_ready=False
        request={'jsonrpc':'2.0','id':'metadata','method':'microprofile/projectInfo','params':{'uri':'file:///project/application.properties'}}
        broker.backend('microprofile',request);self.assertFalse(broker.sent)
        broker.backend('microprofile',{'jsonrpc':'2.0','method':'$/cancelRequest','params':{'id':'metadata'}})
        self.assertFalse(broker.mp_waiting);self.assertEqual(broker.sent[-1][1]['error']['code'],-32800)
        broker.backend('microprofile',request)
        broker.backend('jdt',{'jsonrpc':'2.0','method':'language/status','params':{'type':'ServiceReady'}})
        self.assertTrue(any(endpoint=='jdt' and message.get('params',{}).get('command')=='microprofile/projectInfo' for endpoint,message in broker.sent))

    def test_java_completion_merges_providers_and_keeps_resolve_ownership(self):
        broker=RecordingMP();request={'jsonrpc':'2.0','id':7,'method':'textDocument/completion','params':{'textDocument':{'uri':'file:///project/App.java'},'position':{'line':0,'character':1}}}
        broker.native(request);outgoing=broker.sent[-2:]
        for endpoint,item in outgoing:
            label='JavaType' if endpoint=='jdt' else 'config.field'
            broker.backend(endpoint,{'jsonrpc':'2.0','id':item['id'],'result':{'items':[{'label':label,'data':{'private':endpoint}}],'isIncomplete':False}})
        endpoint,result=broker.sent[-1];self.assertEqual(endpoint,'native')
        items=result['result']['items'];self.assertEqual({i['label'] for i in items},{'JavaType','config.field'})
        selected=next(item for item in items if item['label']=='config.field')
        broker.native({'jsonrpc':'2.0','id':8,'method':'completionItem/resolve','params':selected})
        endpoint,resolving=broker.sent[-1];self.assertEqual(endpoint,'microprofile');self.assertEqual(resolving['params']['data'],{'private':'microprofile'})
        broker.backend('microprofile',{'jsonrpc':'2.0','id':resolving['id'],'result':{**resolving['params'],'detail':'resolved'}})
        self.assertEqual(broker.sent[-1][1]['result']['data'][DATA_KEY]['backend'],'microprofile')

    def test_direct_configuration_completion_preserves_resolve_endpoint(self):
        broker=RecordingMP();broker.native({'jsonrpc':'2.0','id':1,'method':'textDocument/completion','params':{'textDocument':{'uri':'file:///project/application.properties'}}})
        endpoint,request=broker.sent[-1];self.assertEqual(endpoint,'microprofile')
        broker.backend(endpoint,{'jsonrpc':'2.0','id':request['id'],'result':{'items':[{'label':'greeting.message','data':{'uri':'file:///project/application.properties'}}]}})
        item=broker.sent[-1][1]['result']['items'][0]
        self.assertEqual(item['data'][DATA_KEY]['backend'],'microprofile')

    def test_empty_microprofile_hover_does_not_hide_normal_java_hover(self):
        broker=RecordingMP();broker.groups['g']={'id':1,'method':'textDocument/hover','responses':{'jdt':{'result':{'contents':'Java docs'}},'microprofile':{'result':{'contents':[]}}},'at':0}
        broker.merge('g');self.assertEqual(broker.sent[-1][1]['result'],{'contents':'Java docs'})

    def test_diagnostics_from_both_java_providers_are_aggregated_and_old_versions_drop(self):
        broker=RecordingMP();uri='file:///project/App.java';broker.document_versions[uri]=2
        for endpoint,message in [('jdt','syntax'),('microprofile','config')]:
            broker.backend(endpoint,{'jsonrpc':'2.0','method':'textDocument/publishDiagnostics','params':{'uri':uri,'version':2,'diagnostics':[{'message':message}]}})
        self.assertEqual({item['message'] for item in broker.sent[-1][1]['params']['diagnostics']},{'syntax','config'})
        before=len(broker.sent);broker.backend('microprofile',{'jsonrpc':'2.0','method':'textDocument/publishDiagnostics','params':{'uri':uri,'version':1,'diagnostics':[]}});self.assertEqual(len(broker.sent),before)

    def test_java_model_barrier_also_holds_configuration_queries(self):
        broker=RecordingMP();broker.java_barriers=1
        request={'jsonrpc':'2.0','id':1,'method':'textDocument/completion','params':{'textDocument':{'uri':'file:///project/application.properties'}}}
        broker.native(request);self.assertEqual(broker.waiting_template_requests,[request]);self.assertFalse(broker.sent)

    def test_unrelated_session_environment_is_not_exposed_as_config_candidates(self):
        with patch.dict(os.environ,{'PATH':'/tools','HOME':'/home/user','SESSION_CREDENTIAL':'fixture-secret','APP_PORT':'8080'},clear=True):
            self.assertEqual(child_environment({}),{'PATH':'/tools','HOME':'/home/user'})
            self.assertEqual(child_environment({'inherit_environment':['APP_PORT']})['APP_PORT'],'8080')
            self.assertNotIn('SESSION_CREDENTIAL',child_environment({'inherit_environment':['APP_PORT']}))

    def test_admitted_configuration_names_are_explicit_and_java_state_stays_shared(self):
        for name in ('application.properties','application-prod.properties','META-INF/microprofile-config.properties'):
            self.assertTrue(config_uri('file:///project/'+name))
        for name in ('messages.properties','application.yaml','file.java'):
            self.assertFalse(config_uri('file:///project/'+name))

if __name__=='__main__':
    unittest.main()
