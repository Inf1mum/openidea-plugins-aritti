"""Opt-in actual Qute/JDT integration; set QUTE_BROKER_PROFILE to a reviewed profile.

Uses a disposable Maven Quarkus fixture. The language server may download Maven
artifacts. No existing project, server setting, database or account is changed.
"""
import importlib.util
import json
import os
from pathlib import Path
import queue
import subprocess
import sys
import tempfile
import threading
import time
import unittest
from qute_jdt_broker import read_frame, write_frame

PROFILE = os.environ.get('QUTE_BROKER_PROFILE')

@unittest.skipUnless(PROFILE, 'set QUTE_BROKER_PROFILE to run the actual optional tools')
class LiveQuteTests(unittest.TestCase):
    def test_inferred_types_unsaved_changes_close_and_watched_disk_changes(self):
        self.run_fixture(False)

    def test_explicit_empty_base_path_retains_inferred_types_and_freshness(self):
        self.run_fixture(True)

    def run_fixture(self, empty_base):
        with tempfile.TemporaryDirectory(prefix='openidea-qute-live-') as temporary:
            directory = Path(temporary).resolve()
            root = directory/'project'
            resources = root/('src/main/resources/templates' if empty_base else 'src/main/resources/templates/Templates')
            sources = root/'src/main/java/org/acme'
            resources.mkdir(parents=True)
            sources.mkdir(parents=True)
            (root/'pom.xml').write_text('<project><modelVersion>4.0.0</modelVersion><groupId>org.acme</groupId><artifactId>qute-test</artifactId><version>1</version><properties><maven.compiler.release>17</maven.compiler.release></properties><dependencies><dependency><groupId>io.quarkus</groupId><artifactId>quarkus-qute</artifactId><version>3.40.1</version></dependency></dependencies></project>')
            templates = sources/'Templates.java'
            java = 'package org.acme; import io.quarkus.qute.CheckedTemplate; import io.quarkus.qute.TemplateInstance; public class Templates { @CheckedTemplate public static class Views { public static native TemplateInstance hello(Person person); } }'
            if empty_base:
                java = java.replace('@CheckedTemplate ', '@CheckedTemplate(basePath="") ')
            templates.write_text(java)
            person_file = sources/'Person.java'
            person = 'package org.acme; public class Person { public String name; public int age; }'
            person_file.write_text(person)
            template = resources/'hello.html'
            text = '{person.}'
            template.write_text(text)
            receipt = {'tool_profile': str(Path(PROFILE).resolve()), 'empty_base_path': empty_base, 'phases': [], 'messages': []}
            stderr_path = directory/'servers.stderr'
            with stderr_path.open('wb') as errors:
                process = subprocess.Popen([sys.executable, str(Path(__file__).with_name('qute_jdt_broker.py')), '--profile', str(Path(PROFILE).resolve()), '--state-dir', str(directory/'state')], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=errors)
                events = queue.Queue()
                def read():
                    try:
                        while True:
                            message = read_frame(process.stdout)
                            events.put(message)
                            if message is None:
                                return
                    except Exception as error:
                        events.put(error)
                threading.Thread(target=read, daemon=True).start()
                def send(message):
                    write_frame(process.stdin, message)
                def receive(deadline):
                    while time.monotonic() < deadline:
                        try:
                            message = events.get(timeout=.25)
                        except queue.Empty:
                            if process.poll() is not None:
                                self.fail('broker stopped; see optional receipt stderr')
                            continue
                        self.assertIsInstance(message, dict, 'broker EOF or invalid framing')
                        receipt['messages'].append(message)
                        if 'method' in message and 'id' in message:
                            method, params = message['method'], message.get('params') or {}
                            result = None
                            if method == 'workspace/configuration':
                                result = []
                                for item in params.get('items') or []:
                                    section = item.get('section')
                                    result.append({'configuration': {'updateBuildConfiguration': 'automatic'}, 'import': {'maven': {'enabled': True}}} if section == 'java' else {'validation': {'enabled': True}} if section == 'qute' else {})
                            elif method == 'workspace/workspaceFolders':
                                result = [{'uri': root.as_uri(), 'name': 'qute-test'}]
                            elif method == 'workspace/applyEdit':
                                result = {'applied': False, 'failureReason': 'read-only integration fixture'}
                            send({'jsonrpc': '2.0', 'id': message['id'], 'result': result})
                        return message
                    self.fail('timed out waiting for actual language-server response')
                def wait_response(request_id, timeout=180):
                    deadline = time.monotonic()+timeout
                    while True:
                        message = receive(deadline)
                        if message.get('id') == request_id and ('result' in message or 'error' in message):
                            return message
                next_id = 10
                def expect_completion(expected, excluded, name):
                    nonlocal next_id
                    deadline = time.monotonic()+90
                    while time.monotonic() < deadline:
                        next_id += 1
                        send({'jsonrpc': '2.0', 'id': next_id, 'method': 'textDocument/completion', 'params': {'textDocument': {'uri': template.as_uri()}, 'position': {'line': 0, 'character': 8}}})
                        response = wait_response(next_id, 90)
                        self.assertNotIn('error', response)
                        result = response.get('result')
                        items = result.get('items') or [] if isinstance(result, dict) else result or []
                        labels = [item.get('label', '') for item in items]
                        names = [label.split(' : ')[0] for label in labels]
                        if expected in names and excluded not in names:
                            receipt['phases'].append({'name': name, 'labels': labels})
                            return
                        # Keep servicing standard requests while allowing JDT imports/reconcile.
                        until = time.monotonic()+1
                        while time.monotonic() < until:
                            try:
                                message = events.get(timeout=.1)
                            except queue.Empty:
                                continue
                            events.put(message)
                            receive(until+.2)
                    self.fail('actual Qute completion did not reflect '+name)
                started = time.monotonic()
                try:
                    send({'jsonrpc': '2.0', 'id': 1, 'method': 'initialize', 'params': {'processId': os.getpid(), 'rootUri': root.as_uri(), 'workspaceFolders': [{'uri': root.as_uri(), 'name': 'qute-test'}], 'capabilities': {'workspace': {'configuration': True, 'didChangeWatchedFiles': {'dynamicRegistration': True}}, 'textDocument': {'completion': {'completionItem': {'snippetSupport': True}}}}, 'initializationOptions': {'settings': {'qute': {'validation': {'enabled': True}}}}}})
                    self.assertIn('result', wait_response(1))
                    send({'jsonrpc': '2.0', 'method': 'initialized', 'params': {}})
                    for file, contents, language in [(person_file, person, 'java'), (templates, java, 'java'), (template, text, 'qute-html')]:
                        send({'jsonrpc': '2.0', 'method': 'textDocument/didOpen', 'params': {'textDocument': {'uri': file.as_uri(), 'languageId': language, 'version': 1, 'text': contents}}})
                    expect_completion('name', 'displayName', 'inferred CheckedTemplate Java member')
                    send({'jsonrpc': '2.0', 'method': 'textDocument/didChange', 'params': {'textDocument': {'uri': person_file.as_uri(), 'version': 2}, 'contentChanges': [{'text': person.replace('String name', 'String displayName')}]}})
                    self.assertIn('String name', person_file.read_text(), 'unsaved edit must remain unsaved')
                    expect_completion('displayName', 'name', 'unsaved Java rename')
                    send({'jsonrpc': '2.0', 'method': 'textDocument/didChange', 'params': {'textDocument': {'uri': person_file.as_uri(), 'version': 3}, 'contentChanges': [{'text': person}]}})
                    send({'jsonrpc': '2.0', 'method': 'textDocument/didClose', 'params': {'textDocument': {'uri': person_file.as_uri()}}})
                    expect_completion('name', 'displayName', 'discard and close restore disk model')
                    person_file.write_text(person.replace('String name', 'String diskTitle'))
                    send({'jsonrpc': '2.0', 'method': 'workspace/didChangeWatchedFiles', 'params': {'changes': [{'uri': person_file.as_uri(), 'type': 2}]}})
                    expect_completion('diskTitle', 'name', 'saved external Java change')
                    person_file.write_text(person)
                    send({'jsonrpc': '2.0', 'method': 'workspace/didChangeWatchedFiles', 'params': {'changes': [{'uri': person_file.as_uri(), 'type': 2}]}})
                    expect_completion('name', 'diskTitle', 'saved external Java restore')
                    send({'jsonrpc': '2.0', 'id': 999, 'method': 'shutdown', 'params': None})
                    self.assertIn('result', wait_response(999, 30))
                    send({'jsonrpc': '2.0', 'method': 'exit', 'params': None})
                    self.assertEqual(process.wait(timeout=15), 0)
                    receipt['exit_code'] = process.returncode
                finally:
                    if process.poll() is None:
                        # EOF lets the broker terminate both owned backends itself.
                        process.stdin.close()
                        try:
                            process.wait(timeout=10)
                        except subprocess.TimeoutExpired:
                            process.kill()
                            process.wait()
                    receipt['seconds'] = round(time.monotonic()-started, 2)
                    destination = os.environ.get('QUTE_BROKER_RECEIPT_DIR')
                    if destination:
                        output = Path(destination)
                        output.mkdir(parents=True, exist_ok=True)
                        (output/'live-qute-receipt.json').write_text(json.dumps(receipt, indent=2)+'\n')
                        (output/('live-qute-empty-base-receipt.json' if empty_base else 'live-qute-default-receipt.json')).write_text(json.dumps(receipt, indent=2)+'\n')
                        (output/'live-qute.stderr').write_bytes(stderr_path.read_bytes())
                    for stream in (process.stdin, process.stdout):
                        if stream and not stream.closed:
                            stream.close()

if __name__ == '__main__':
    unittest.main()
