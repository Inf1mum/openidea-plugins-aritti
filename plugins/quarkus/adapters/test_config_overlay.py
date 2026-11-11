import json
from pathlib import Path
import tempfile
import unittest
from config_sources import classify, flatten_yaml, parse_env, snapshot_fields
from config_overlay import ConfigOverlay, apply_changes, utf16_offset

class ConfigOverlayTests(unittest.TestCase):
    def test_utf16_sequential_edits_and_crlf(self):
        text='a😀b\r\nc=x\n'
        self.assertEqual(utf16_offset(text,{'line':0,'character':3}),2)
        with self.assertRaises(ValueError): utf16_offset(text,{'line':0,'character':2})
        changed=apply_changes(text,[{'range':{'start':{'line':0,'character':1},'end':{'line':0,'character':3}},'text':'z'},{'range':{'start':{'line':1,'character':2},'end':{'line':1,'character':3}},'text':'y'}])
        self.assertEqual(changed,'azb\r\nc=y\n')

    def test_private_snapshot_tracks_dirty_and_refuses_unknown_or_aliases(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary).resolve();source=root/'application.properties';source.write_text('key=disk\n')
            target=root/'state/snapshot.json';overlay=ConfigOverlay(root,target);uri=source.as_uri()
            overlay.event('textDocument/didOpen',{'textDocument':{'uri':uri,'version':1,'text':'key=disk\n'}})
            self.assertFalse(overlay.dirty)
            overlay.event('textDocument/didChange',{'textDocument':{'uri':uri,'version':2},'contentChanges':[{'text':'key=unsaved\n'}]})
            self.assertEqual(json.loads(target.read_text())['documents'][0]['text'],'key=unsaved\n')
            self.assertEqual(target.stat().st_mode & 0o777,0o600)
            self.assertEqual(source.read_text(),'key=disk\n')
            alias=root/'application-alias.properties';alias.symlink_to(source)
            overlay.event('textDocument/didOpen',{'textDocument':{'uri':alias.as_uri(),'version':1,'text':'key=other\n'}})
            self.assertEqual(overlay.unsupported,{uri,alias.as_uri()})
            overlay.event('textDocument/didClose',{'textDocument':{'uri':alias.as_uri()}})
            self.assertFalse(overlay.unsupported)
            overlay.event('textDocument/didChange',{'textDocument':{'uri':uri,'version':2},'contentChanges':[{'text':'stale'}]})
            self.assertIn(uri,overlay.unsupported)
            overlay.event('textDocument/didClose',{'textDocument':{'uri':uri}})
            self.assertFalse(json.loads(target.read_text())['documents'])

class ConfigSourceTests(unittest.TestCase):
    def test_yaml_flattens_nested_lists_quoted_keys_profiles_and_null_key(self):
        text='---\nquarkus:\n  http:\n    port: 9090 # c\n  datasource:\n    "named":\n      jdbc:\n        url: \'jdbc:x\'\n"%dev":\n  quarkus:\n    http:\n      port: 1\nl:\n  - a\n  - k: v\n    z: 2\n- bad: 1\n'.replace('- bad: 1\n','')
        text+='"io.quarkus.cat":\n  level: DEBUG\none:\n  two:\n    ~: 12\n    three: 123\nempty:\n'
        self.assertEqual(dict(flatten_yaml(text)),{'quarkus.http.port':'9090','quarkus.datasource."named".jdbc.url':'jdbc:x','%dev.quarkus.http.port':'1','l[0]':'a','l[1].k':'v','l[1].z':'2','"io.quarkus.cat".level':'DEBUG','one.two':'12','one.two.three':'123','empty':''})
    def test_yaml_refuses_unsupported_syntax(self):
        for bad in ('a: &x 1\n','a: *x\n','a: {b: 1}\n','a: [1]\n','a: |\n  x\n','a: 1\n---\nb: 2\n','a:\n\tb: 1\n','a: 1\na: 2\n','a: 1\n  b\n','%dev:\n  a: 1\n','a: !t 1\n'):
            with self.assertRaises(ValueError,msg=bad): flatten_yaml(bad)
    def test_env_and_classification(self):
        self.assertEqual(parse_env('# c\nexport A_B="x y"\n_DEV_QUARKUS_HTTP_PORT=81 # z\n'),[('A_B','x y'),('_DEV_QUARKUS_HTTP_PORT','81')])
        with self.assertRaises(ValueError): parse_env('not valid\n')
        self.assertEqual(classify('/p/.env')['ordinal'],295)
        self.assertEqual(classify('/p/config/application.properties')['ordinal'],260)
        self.assertEqual(classify('/p/src/main/resources/application.properties')['ordinal'],250)
        self.assertEqual(classify('/p/application-dev.yml')['profile'],'dev')
        self.assertIsNone(classify('/p/other.yaml'))
        self.assertEqual(snapshot_fields('/p/application.yaml','a:\n  b: "x y"\n')['text'],'a.b=x y\n')
    def test_snapshot_includes_unsaved_new_yaml_and_refuses_flow(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary).resolve();target=root/'s.json';overlay=ConfigOverlay(root,target)
            good=(root/'application.yaml').as_uri();bad=(root/'.env').as_uri()
            overlay.event('textDocument/didOpen',{'textDocument':{'uri':good,'version':1,'text':'a:\n  b: 1\n'}})
            overlay.event('textDocument/didOpen',{'textDocument':{'uri':bad,'version':1,'text':'oops\n'}})
            document=json.loads(target.read_text())['documents'][0]
            self.assertEqual((document['text'],document['format'],document['ordinal']),('a.b=1\n','yaml',255))
            self.assertEqual(overlay.unsupported,{bad})

if __name__=='__main__': unittest.main()
