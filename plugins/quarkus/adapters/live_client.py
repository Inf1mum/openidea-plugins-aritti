"""Test-only local stdio client used by the opt-in real adapter fixtures."""
import os
from pathlib import Path
import queue
import subprocess
import sys
import threading
import time
from qute_jdt_broker import read_frame, write_frame

class LiveClient:
    def __init__(self, profile, root, state, stderr):
        self.root = root
        self.process = subprocess.Popen([sys.executable, str(Path(__file__).with_name('qute_jdt_broker.py')), '--profile', str(profile), '--state-dir', str(state)], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=stderr)
        self.events = queue.Queue()
        self.responses = {}
        self.diagnostics = {}
        self.methods = []
        self.next_id = 100
        def read():
            try:
                while True:
                    message = read_frame(self.process.stdout)
                    self.events.put(message)
                    if message is None:
                        return
            except Exception as error:
                self.events.put(error)
        self.reader = threading.Thread(target=read, daemon=True)
        self.reader.start()

    def send(self, method, params=None, request_id=None):
        message = {'jsonrpc': '2.0', 'method': method, 'params': params}
        if request_id is not None:
            message['id'] = request_id
        write_frame(self.process.stdin, message)

    def pump(self, timeout=.25):
        try:
            message = self.events.get(timeout=timeout)
        except queue.Empty:
            if self.process.poll() is not None:
                raise AssertionError('actual broker stopped')
            return
        if not isinstance(message, dict):
            raise AssertionError('actual broker EOF or malformed framing')
        method = message.get('method')
        if method:
            self.methods.append(method)
            params = message.get('params') or {}
            if method == 'textDocument/publishDiagnostics':
                self.diagnostics[params.get('uri')] = params.get('diagnostics') or []
            if 'id' in message:
                result = None
                if method == 'workspace/configuration':
                    result = []
                    for item in params.get('items') or []:
                        section = item.get('section', '')
                        if section == 'java':
                            result.append({'configuration': {'updateBuildConfiguration': 'automatic'}})
                        elif section == 'qute':
                            result.append({'validation': {'enabled': True}})
                        elif section == 'microprofile':
                            result.append({'tools': {'validation': {'enabled': True}, 'execution': {'mode': 'safe'}, 'inlayHint': {'enabled': True, 'converters': {'enabled': True}, 'defaultValues': {'enabled': True}}}})
                        else:
                            result.append({})
                elif method == 'workspace/workspaceFolders':
                    result = [{'uri': self.root.as_uri(), 'name': 'fixture'}]
                elif method == 'workspace/applyEdit':
                    result = {'applied': False, 'failureReason': 'read-only test fixture'}
                write_frame(self.process.stdin, {'jsonrpc': '2.0', 'id': message['id'], 'result': result})
        elif 'id' in message:
            self.responses[message['id']] = message

    def request(self, method, params, timeout=180):
        self.next_id += 1
        request_id = self.next_id
        self.send(method, params, request_id)
        end = time.monotonic()+timeout
        while request_id not in self.responses and time.monotonic() < end:
            self.pump()
        if request_id not in self.responses:
            raise AssertionError('actual backend request timed out: '+method)
        message = self.responses.pop(request_id)
        if 'error' in message:
            raise AssertionError('actual backend rejected '+method+': '+str(message['error'].get('message')))
        return message.get('result')

    def initialize(self):
        result = self.request('initialize', {'processId': os.getpid(), 'rootUri': self.root.as_uri(), 'workspaceFolders': [{'uri': self.root.as_uri(), 'name': 'fixture'}], 'capabilities': {'workspace': {'configuration': True, 'didChangeWatchedFiles': {'dynamicRegistration': True}}, 'textDocument': {'completion': {'completionItem': {'snippetSupport': True, 'resolveSupport': {'properties': ['documentation', 'detail', 'additionalTextEdits']}}}, 'hover': {'contentFormat': ['markdown', 'plaintext']}}}, 'initializationOptions': {'settings': {'qute': {'validation': {'enabled': True}}, 'microprofile': {'tools': {'validation': {'enabled': True}, 'execution': {'mode': 'safe'}, 'inlayHint': {'enabled': True, 'converters': {'enabled': True}, 'defaultValues': {'enabled': True}}}}}}})
        if not isinstance(result, dict):
            raise AssertionError('actual broker did not initialize')
        self.send('initialized', {})

    def open(self, file, text, language):
        self.send('textDocument/didOpen', {'textDocument': {'uri': file.as_uri(), 'languageId': language, 'version': 1, 'text': text}})

    def change(self, file, text, version):
        self.diagnostics.pop(file.as_uri(), None)
        self.send('textDocument/didChange', {'textDocument': {'uri': file.as_uri(), 'version': version}, 'contentChanges': [{'text': text}]})

    def completion(self, file, line, character):
        result = self.request('textDocument/completion', {'textDocument': {'uri': file.as_uri()}, 'position': {'line': line, 'character': character}})
        return result.get('items') or [] if isinstance(result, dict) else result or []

    def until(self, predicate, description, timeout=90):
        end = time.monotonic()+timeout
        while time.monotonic()<end:
            result = predicate()
            if result:
                return result
            until = time.monotonic()+.5
            while time.monotonic()<until:
                self.pump(.1)
        raise AssertionError('actual model did not satisfy '+description)

    def close(self, graceful=True):
        try:
            if self.process.poll() is None and graceful:
                self.request('shutdown', None, 30)
                self.send('exit', None)
                if self.process.wait(timeout=15) != 0:
                    raise AssertionError('broker did not exit cleanly')
        finally:
            if self.process.poll() is None:
                self.process.stdin.close()
                try:
                    self.process.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    self.process.kill()
                    self.process.wait()
            self.reader.join(timeout=2)
            for stream in (self.process.stdin, self.process.stdout):
                if stream and not stream.closed:
                    stream.close()
