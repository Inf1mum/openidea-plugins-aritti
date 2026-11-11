import json
from pathlib import Path
import tempfile
import unittest
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
            alias=root/'alias.properties';alias.symlink_to(source)
            overlay.event('textDocument/didOpen',{'textDocument':{'uri':alias.as_uri(),'version':1,'text':'key=other\n'}})
            self.assertEqual(overlay.unsupported,{uri,alias.as_uri()})
            overlay.event('textDocument/didClose',{'textDocument':{'uri':alias.as_uri()}})
            self.assertFalse(overlay.unsupported)
            overlay.event('textDocument/didChange',{'textDocument':{'uri':uri,'version':2},'contentChanges':[{'text':'stale'}]})
            self.assertIn(uri,overlay.unsupported)
            overlay.event('textDocument/didClose',{'textDocument':{'uri':uri}})
            self.assertFalse(json.loads(target.read_text())['documents'])

if __name__=='__main__': unittest.main()
