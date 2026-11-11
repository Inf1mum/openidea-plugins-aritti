"""Opt-in official LSP4MP/Quarkus/JDT source/configuration integration."""
import json
import os
from pathlib import Path
import tempfile
import time
import unittest
from live_client import LiveClient

PROFILE = os.environ.get('MICROPROFILE_BROKER_PROFILE')

@unittest.skipUnless(PROFILE, 'set MICROPROFILE_BROKER_PROFILE to a reviewed three-server profile')
class LiveMicroProfileTests(unittest.TestCase):
    def test_source_metadata_unsaved_java_config_validation_and_java_semantics(self):
        with tempfile.TemporaryDirectory(prefix='openidea-microprofile-live-') as temporary:
            directory=Path(temporary).resolve();root=directory/'project';sources=root/'src/main/java/org/acme';resources=root/'src/main/resources'
            sources.mkdir(parents=True);resources.mkdir(parents=True)
            (root/'pom.xml').write_text('<project><modelVersion>4.0.0</modelVersion><groupId>org.acme</groupId><artifactId>mp-test</artifactId><version>1</version><properties><maven.compiler.release>17</maven.compiler.release></properties><dependencies><dependency><groupId>io.quarkus</groupId><artifactId>quarkus-qute</artifactId><version>3.40.1</version></dependency></dependencies></project>')
            source=sources/'GreetingConfig.java';java='package org.acme; import io.smallrye.config.ConfigMapping; @ConfigMapping(prefix="greeting") public interface GreetingConfig { String message(); int count(); }';source.write_text(java)
            config=resources/'application.properties';disk='greeting.message=hello\ngreeting.count=7\n';config.write_text(disk)
            receipt={'phases':[]};start=time.monotonic();log=directory/'servers.stderr'
            with log.open('wb') as errors:
                client=LiveClient(Path(PROFILE).resolve(),root,directory/'state',errors)
                try:
                    client.initialize();client.open(source,java,'java');client.open(config,'greeting.','quarkus-properties')
                    def keys(expected,excluded):
                        def check():
                            values=client.completion(config,0,9)
                            labels=[item.get('label','').split(' = ')[0] for item in values]
                            if expected in labels and excluded not in labels:
                                return [label for label in labels if label.startswith('greeting.')]
                        return client.until(check,expected+' metadata')
                    labels=keys('greeting.message','greeting.welcome');receipt['phases'].append({'name':'source mapping metadata','labels':labels})
                    changed=java.replace('String message()', 'String welcome()');client.change(source,changed,2)
                    self.assertIn('String message()',source.read_text())
                    labels=keys('greeting.welcome','greeting.message');receipt['phases'].append({'name':'unsaved Java mapping rename','labels':labels})
                    client.change(source,java,3);client.send('textDocument/didClose',{'textDocument':{'uri':source.as_uri()}})
                    labels=keys('greeting.message','greeting.welcome');receipt['phases'].append({'name':'Java mapping restore and close','labels':labels})
                    client.change(config,'greeting.count=wrong\n',2)
                    def invalid():
                        values=client.diagnostics.get(config.as_uri())
                        return values if values and any('wrong' in item.get('message','') for item in values) else None
                    diagnostics=client.until(invalid,'unsaved config type diagnostic')
                    receipt['phases'].append({'name':'unsaved config type diagnostic','diagnostics':[item['message'] for item in diagnostics if 'wrong' in item.get('message','')]})
                    self.assertEqual(config.read_text(),disk)
                    client.change(config,'greeting.count=42\n',3)
                    client.until(lambda: config.as_uri() in client.diagnostics and not any('wrong' in item.get('message','') for item in client.diagnostics[config.as_uri()]),'unsaved config fix clears diagnostic')
                    receipt['phases'].append({'name':'unsaved config fix clears diagnostic'})
                    # Config key completion remains supplied by the plugin's existing
                    # source provider; LSP4MP's supported Java slice is hover/inlays.
                    client.send('textDocument/didClose',{'textDocument':{'uri':config.as_uri()}})
                    app=sources/'App.java';body='package org.acme; import org.eclipse.microprofile.config.inject.ConfigProperty; class App { @ConfigProperty(name="greeting.message") String value; }';app.write_text(body);client.open(app,body,'java')
                    at=body.index('greeting.message')+3
                    def java_hover():
                        result=client.request('textDocument/hover',{'textDocument':{'uri':app.as_uri()},'position':{'line':0,'character':at}})
                        return result if 'hello' in json.dumps(result) else None
                    hover=client.until(java_hover,'Java ConfigProperty default hover')
                    receipt['phases'].append({'name':'Java ConfigProperty semantic hover','contains_default': 'hello' in json.dumps(hover)})
                    def java_inlays():
                        result=client.request('textDocument/inlayHint',{'textDocument':{'uri':app.as_uri()},'range':{'start':{'line':0,'character':0},'end':{'line':1,'character':0}}})
                        return result if 'hello' in json.dumps(result) else None
                    hints=client.until(java_inlays,'Java ConfigProperty semantic default inlays')
                    receipt['phases'].append({'name':'Java ConfigProperty semantic default inlays','labels':[item.get('label') for item in hints]})
                    if json.loads(Path(PROFILE).read_text()).get('configuration_overlay'):
                        client.open(config,disk,'quarkus-properties')
                        client.change(config,'greeting.message=unsaved-current\ngreeting.count=7\n',2)
                        def overlay_hints(expected):
                            result=client.request('textDocument/inlayHint',{'textDocument':{'uri':app.as_uri()},'range':{'start':{'line':0,'character':0},'end':{'line':1,'character':0}}})
                            return result if expected in json.dumps(result) and ('hello' not in json.dumps(result) or expected=='hello') else None
                        hints=client.until(lambda:overlay_hints('unsaved-current'),'Java inlay uses unsaved configuration')
                        self.assertEqual(config.read_text(),disk)
                        receipt['phases'].append({'name':'unsaved configuration effective Java inlay','labels':[item.get('label') for item in hints]})
                        client.change(config,'greeting.count=7\n',3)
                        def deleted():
                            result=client.request('textDocument/hover',{'textDocument':{'uri':app.as_uri()},'position':{'line':0,'character':at}})
                            body=json.dumps(result)
                            return result if result and 'unsaved-current' not in body and 'hello' not in body else None
                        client.until(deleted,'deleted unsaved key removes disk value')
                        receipt['phases'].append({'name':'unsaved key deletion removes disk value'})
                        client.send('textDocument/didClose',{'textDocument':{'uri':config.as_uri()}})
                        client.until(lambda:overlay_hints('hello'),'closing configuration restores disk source')
                        receipt['phases'].append({'name':'configuration close restores disk effective value'})
                    client.open(config,'greeting.','quarkus-properties')
                    items=client.until(lambda:[item for item in client.completion(config,0,9) if item.get('label','').split(' = ')[0]=='greeting.message'],'configuration completion item')
                    resolved=client.request('completionItem/resolve',items[0]);self.assertIn('greeting.message',resolved.get('label',''));receipt['phases'].append({'name':'configuration completion resolve returns to original backend'})
                    client.close();receipt['exit_code']=client.process.returncode
                finally:
                    receipt['last_config_diagnostics']=client.diagnostics.get(config.as_uri());client.close(False);receipt['seconds']=round(time.monotonic()-start,2);receipt['methods']=sorted(set(client.methods))
                    destination=os.environ.get('MICROPROFILE_BROKER_RECEIPT_DIR')
                    if destination:
                        output=Path(destination);output.mkdir(parents=True,exist_ok=True);(output/'live-microprofile-receipt.json').write_text(json.dumps(receipt,indent=2)+'\n');(output/'live-microprofile.stderr').write_bytes(log.read_bytes())

if __name__=='__main__':
    unittest.main()
