#!/usr/bin/env python3
"""Optional stdio LSP bridge for official Qute LS + a private JDT LS session.

No downloads or shell evaluation. Explicit local tool profiles are mandatory.
Launch this as the Java and Qute LSP together so unsaved Java documents reach JDT.
"""
from __future__ import annotations
import argparse
import copy
import hashlib
import json
import os
from pathlib import Path
import queue
import shutil
import tempfile
import subprocess
import sys
import threading
import time
from urllib.parse import unquote, urlparse

MAX_FRAME = 8 * 1024 * 1024
CALLBACKS = frozenset(('qute/template/projects', 'qute/template/project',
    'qute/template/projectDataModel', 'qute/template/binaryTemplates',
    'qute/template/javaTypes', 'qute/template/resolvedJavaType',
    'qute/template/javaDefinition', 'qute/template/javadoc',
    'qute/template/generateMissingJavaMember', 'qute/java/codeLens',
    'qute/java/diagnostics', 'qute/java/documentLink'))

def read_frame(stream):
    headers = {}
    count = 0
    while True:
        line = stream.readline(8193)
        if not line:
            return None
        count += len(line)
        if count > 16384 or len(line) > 8192:
            raise ValueError('oversized LSP header')
        if line in (b'\r\n', b'\n'):
            break
        key, value = line.decode('ascii').split(':', 1)
        key = key.lower()
        if key in headers:
            raise ValueError('duplicate LSP header')
        headers[key] = value.strip()
    length = int(headers.get('content-length', '0'))
    if not 0 < length <= MAX_FRAME:
        raise ValueError('invalid LSP frame size')
    chunks = []
    remaining = length
    while remaining:
        part = stream.read(remaining)
        if not part:
            raise ValueError('truncated LSP body')
        chunks.append(part)
        remaining -= len(part)
    message = json.loads(b''.join(chunks))
    if not isinstance(message, dict) or message.get('jsonrpc') != '2.0':
        raise ValueError('invalid JSON-RPC message')
    return message

def write_frame(stream, message):
    body = json.dumps(message, ensure_ascii=False, separators=(',', ':')).encode()
    if len(body) > MAX_FRAME:
        raise ValueError('oversized outgoing LSP frame')
    stream.write(b'Content-Length: ' + str(len(body)).encode() + b'\r\n\r\n' + body)
    stream.flush()

def child_environment(profile):
    # LSP4MP enumerates environment sources in config completion. Do not hand it
    # unrelated agent/session credentials merely because the IDE inherited them.
    allowed = {'PATH', 'HOME', 'USERPROFILE', 'SYSTEMROOT', 'SystemRoot', 'WINDIR',
        'TEMP', 'TMP', 'TMPDIR', 'LANG', 'LC_ALL', 'LC_CTYPE', 'JAVA_HOME'}
    inherited = profile.get('inherit_environment') or []
    if not isinstance(inherited, list) or len(inherited) > 64 or any(not isinstance(name, str) or not name.isidentifier() for name in inherited):
        raise ValueError('invalid explicit environment allowlist')
    allowed.update(inherited)
    return {name: value for name, value in os.environ.items() if name in allowed}

def load_profile(path):
    path = Path(path)
    if not path.is_absolute() or path.stat().st_size > 128 * 1024:
        raise ValueError('use an absolute bounded profile file')
    profile = json.loads(path.read_text())
    if profile.get('version') != 1:
        raise ValueError('unsupported tool profile version')
    for name in ('qute_command', 'jdt_command', *(['microprofile_command'] if 'microprofile_command' in profile else [])):
        argv = profile.get(name)
        if not isinstance(argv, list) or not argv or len(argv) > 64 or any(not isinstance(a, str) or len(a) > 8192 or '\0' in a for a in argv):
            raise ValueError('invalid argv in tool profile')
        if not Path(argv[0]).is_absolute() or not os.access(argv[0], os.X_OK):
            raise ValueError('profile executable must be an executable absolute path')
    bundles = profile.get('bundles')
    if not isinstance(bundles, list) or not bundles or len(bundles) > 32:
        raise ValueError('explicit JDT extension bundles required')
    hashes = profile.get('sha256')
    if not isinstance(hashes, dict) or not hashes:
        raise ValueError('tool artifact hashes required')
    configuration_source = profile.get('configuration_source')
    if configuration_source is not None and (not isinstance(configuration_source, str) or not Path(configuration_source).is_absolute() or configuration_source not in hashes):
        raise ValueError('JDT configuration source must be an absolute pinned file')
    for bundle in bundles:
        if not isinstance(bundle, str) or not Path(bundle).is_absolute() or not bundle.endswith('.jar') or bundle not in hashes:
            raise ValueError('each bundle must be absolute and hash-pinned')
    for filename, expected in hashes.items():
        p = Path(filename)
        if not p.is_absolute() or not p.is_file() or not isinstance(expected, str) or len(expected) != 64:
            raise ValueError('invalid pinned tool artifact')
        with p.open('rb') as f:
            actual = hashlib.file_digest(f, 'sha256').hexdigest()
        if actual != expected:
            raise ValueError('tool artifact changed; review a new profile')
    runtime_hash = profile.get('runtime_version_sha256')
    if not isinstance(runtime_hash, str) or len(runtime_hash) != 64 or profile['qute_command'][0] != profile['jdt_command'][0] or 'microprofile_command' in profile and profile['microprofile_command'][0] != profile['jdt_command'][0]:
        raise ValueError('profile requires one reviewed Java runtime version')
    version = subprocess.run([profile['qute_command'][0], '-version'], capture_output=True, text=True, check=True, timeout=10, env=child_environment(profile))
    if hashlib.sha256((version.stderr + version.stdout).encode()).hexdigest() != runtime_hash:
        raise ValueError('Java runtime version changed; review a new profile')
    return profile

class Broker:
    def __init__(self, profile, state_dir, source=None, target=None):
        self.profile = profile
        if not Path(state_dir).is_absolute():
            raise ValueError('state directory must be absolute')
        self.state_dir = Path(state_dir).resolve()
        self.source = source if source is not None else sys.stdin.buffer.raw
        self.target = target or sys.stdout.buffer
        self.events = queue.Queue(maxsize=64)
        self.processes = {}
        self.pending = {}
        self.server_requests = {}
        self.native_requests = {}
        self.next_id = 0
        self.init_message = None
        self.jdt_capabilities = {}
        self.initialized = False
        self.root = None
        self.root_uri = None
        self.closed = False
        self.jdt_ready = False
        self.waiting_callbacks = []
        self.callback_methods = {}
        self.projects = set()
        self.java_barriers = 0
        self.sync_failed = False
        self.waiting_template_requests = []
        self.session_dir = None
        self.virtual_uris = set()

    def token(self, prefix):
        self.next_id += 1
        return f'broker:{prefix}:{self.next_id}'

    def read_loop(self, name, stream):
        try:
            while True:
                message = read_frame(stream)
                if message is None:
                    self.events.put((name, None))
                    return
                self.events.put((name, message))
        except Exception:
            self.events.put((name, {'_protocol_error': True}))

    def send(self, endpoint, message):
        write_frame(self.target if endpoint == 'native' else self.processes[endpoint].stdin, message)

    def error(self, endpoint, request_id, reason, code=-32603):
        self.send(endpoint, {'jsonrpc': '2.0', 'id': request_id, 'error': {'code': code, 'message': reason}})

    def request(self, endpoint, message, origin, original_id):
        if len(self.pending) >= 1024:
            if origin in ('native', 'qute', 'jdt', 'microprofile'):
                self.error(origin, original_id, 'language-server request limit reached')
            return None
        token = self.token(endpoint)
        self.pending[token] = (origin, original_id, time.monotonic())
        self.send(endpoint, {**message, 'id': token})
        if origin == 'native':
            self.native_requests[json.dumps(original_id)] = (endpoint, token)
        return token

    def start(self, message):
        params = message.get('params') or {}
        folders = params.get('workspaceFolders') or []
        uri = params.get('rootUri') or (folders[0]['uri'] if len(folders) == 1 else None)
        parsed = urlparse(uri or '')
        if parsed.scheme != 'file' or parsed.netloc not in ('', 'localhost') or len(folders) > 1:
            raise ValueError('one local file workspace required')
        source_root = Path(unquote(parsed.path))
        root = source_root.resolve()
        if source_root != root:
            raise ValueError('use the canonical workspace path; symlink roots need URI alias mapping')
        if not root.is_dir():
            raise ValueError('workspace root is not a directory')
        self.root, self.root_uri = root, uri
        owner = self.state_dir / hashlib.sha256((str(root) + json.dumps(self.profile, sort_keys=True)).encode()).hexdigest()[:24]
        owner.mkdir(parents=True, exist_ok=True)
        state = Path(tempfile.mkdtemp(prefix='session-', dir=owner))
        self.session_dir = state
        if self.profile.get('configuration_source'):
            configuration = state/'configuration'
            configuration.mkdir()
            shutil.copyfile(self.profile['configuration_source'], configuration/'config.ini')
        self.init_message = message
        for endpoint in ('jdt', 'qute'):
            argv = [a.replace('{workspace}', str(state)).replace('{root}', str(root)) for a in self.profile[endpoint + '_command']]
            self.processes[endpoint] = subprocess.Popen(argv, cwd=root, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=sys.stderr, env=child_environment(self.profile))
            threading.Thread(target=self.read_loop, args=(endpoint, self.processes[endpoint].stdout), daemon=True).start()
        jdt_params = copy.deepcopy(params)
        jdt_params['processId'] = os.getpid()
        options = jdt_params.setdefault('initializationOptions', {})
        options['bundles'] = self.profile['bundles']
        # Enable the standard JDT command forwarding API used by official clients.
        options['extendedClientCapabilities'] = {'classFileContentsSupport': True, 'advancedOrganizeImportsSupport': True}
        options.setdefault('settings', {}).setdefault('java', {}).setdefault('configuration', {})['updateBuildConfiguration'] = 'automatic'
        token = self.token('initialize-jdt')
        self.pending[token] = ('initialize-jdt', message['id'], time.monotonic())
        self.send('jdt', {'jsonrpc': '2.0', 'id': token, 'method': 'initialize', 'params': jdt_params})

    def invalidate_after(self, uri):
        if self.java_barriers == 0:
            self.sync_failed = False
        self.java_barriers += 1
        self.request('jdt', {'jsonrpc': '2.0', 'method': 'textDocument/documentSymbol',
            'params': {'textDocument': {'uri': uri}}}, 'invalidate-qute', None)

    def reset_qute_models(self):
        if self.projects:
            self.send('qute', {'jsonrpc': '2.0', 'method': 'qute/dataModelChanged',
                'params': {'projects': [{'uri': project, 'sources': []} for project in sorted(self.projects)]}})

    def route(self, message):
        params = message.get('params') or {}
        if message.get('method') in ('workspace/symbol', 'workspaceSymbol/resolve') or message.get('method', '').startswith('java/'):
            return 'jdt'
        uri = (params.get('textDocument') or {}).get('uri', '')
        if uri.startswith('jdt://contents/'):
            return 'jdt'
        if message.get('method') == 'workspace/executeCommand' and params.get('command') in CALLBACKS:
            return 'jdt'
        return 'jdt' if uri.lower().endswith('.java') else 'qute'

    def native(self, message):
        method = message.get('method')
        if not method:
            forwarded = self.server_requests.pop(str(message.get('id')), None)
            if forwarded:
                endpoint, original = forwarded
                self.send(endpoint, {**message, 'id': original})
            return
        if method == 'initialize':
            if self.init_message:
                self.error('native', message['id'], 'already initialized', -32600)
                return
            self.start(message)
            return
        if not self.processes:
            if 'id' in message:
                self.error('native', message['id'], 'initialize first', -32002)
            return
        if method == '$/cancelRequest':
            original = (message.get('params') or {}).get('id')
            for queued in list(self.waiting_template_requests):
                if json.dumps(queued['id']) == json.dumps(original):
                    self.waiting_template_requests.remove(queued)
                    self.error('native', original, 'request cancelled', -32800)
                    return
            target = self.native_requests.get(json.dumps(original))
            if target:
                self.send(target[0], {**message, 'params': {'id': target[1]}})
            return
        if method == 'initialized':
            self.initialized = True
            self.send('qute', message)
            return
        if method == 'exit':
            self.send('qute', message)
            self.send('jdt', message)
            self.closed = True
            return
        if method == 'shutdown':
            self.request('qute', message, 'native', message['id'])
            self.request('jdt', message, 'discard', None)
            return
        params = message.get('params') or {}
        uri = (params.get('textDocument') or {}).get('uri')
        if self.root is not None and uri is not None:
            parsed = urlparse(uri)
            path = Path(unquote(parsed.path)).resolve() if parsed.scheme == 'file' else None
            known_virtual = parsed.scheme == 'jdt' and uri in self.virtual_uris
            if not known_virtual and (parsed.netloc not in ('', 'localhost') or path is None or not path.is_relative_to(self.root)):
                if 'id' in message:
                    self.error('native', message['id'], 'document is outside the owning local workspace', -32602)
                return
        if self.root is not None and method == 'workspace/didChangeWatchedFiles':
            changes = []
            for change in params.get('changes') or []:
                parsed = urlparse(change.get('uri', ''))
                if parsed.scheme == 'file' and parsed.netloc in ('', 'localhost') and Path(unquote(parsed.path)).resolve().is_relative_to(self.root):
                    changes.append(change)
            message = {**message, 'params': {**params, 'changes': changes}}
        if method == 'workspace/didChangeWorkspaceFolders':
            # A different workspace needs a fresh Java/Qute pair and state ownership.
            return
        endpoint = self.route(message)
        if 'id' in message:
            if endpoint == 'qute' and self.sync_failed and not self.java_barriers:
                self.error('native', message['id'], 'Java model synchronization is unavailable; reconcile Java source or restart the broker')
                return
            if endpoint == 'qute' and self.java_barriers:
                if len(self.waiting_template_requests) >= 256:
                    self.error('native', message['id'], 'Java model synchronization queue is full')
                else:
                    self.waiting_template_requests.append(message)
            else:
                self.request(endpoint, message, 'native', message['id'])
        elif method.startswith(('workspace/', '$/')):
            for target in ('jdt', 'qute'):
                self.send(target, message)
            if method == 'workspace/didChangeWatchedFiles':
                for change in (message.get('params') or {}).get('changes') or []:
                    changed_uri = change.get('uri', '')
                    if changed_uri.lower().endswith('.java'):
                        self.invalidate_after(changed_uri)
        else:
            self.send(endpoint, message)
            if endpoint == 'jdt' and method in ('textDocument/didOpen', 'textDocument/didChange', 'textDocument/didClose', 'textDocument/didSave'):
                self.invalidate_after(uri)

    def remember_virtual_uris(self, value):
        if isinstance(value, list):
            for item in value:
                self.remember_virtual_uris(item)
        elif isinstance(value, dict):
            for name, item in value.items():
                if name in ('data', 'arguments'):
                    continue
                if name in ('uri', 'targetUri') and isinstance(item, str) and item.startswith('jdt://contents/') and len(item) <= 8192 and len(self.virtual_uris) < 512:
                    self.virtual_uris.add(item)
                else:
                    self.remember_virtual_uris(item)

    def normalize_data_model(self, result):
        # Qute/JDT 0.25 emits templates//name for an explicitly empty basePath.
        # Canonicalize repeated separators only for its conventional physical
        # source-root paths; never rewrite custom locator IDs or traversals.
        if not isinstance(result, dict):
            return
        for template in result.get('templates') or []:
            if not isinstance(template, dict):
                continue
            uri = template.get('templateUri')
            if isinstance(uri, str) and uri.startswith('src/main/resources/templates/') and not any(part in ('.', '..') for part in uri.split('/')):
                template['templateUri'] = '/'.join(part for part in uri.split('/') if part)

    def backend(self, endpoint, message):
        if 'method' not in message:
            entry = self.pending.pop(str(message.get('id')), None)
            if not entry:
                return
            origin, original_id, _ = entry
            if 'result' in message:
                self.remember_virtual_uris(message['result'])
            if origin == 'initialize-jdt':
                if 'error' in message:
                    self.error('native', original_id, 'JDT initialization failed')
                    self.closed = True
                    return
                self.jdt_capabilities = (message.get('result') or {}).get('capabilities') or {}
                self.send('jdt', {'jsonrpc': '2.0', 'method': 'initialized', 'params': {}})
                params = copy.deepcopy(self.init_message.get('params') or {})
                params['processId'] = os.getpid()
                self.request('qute', {'jsonrpc': '2.0', 'method': 'initialize', 'params': params}, 'initialize-qute', original_id)
            elif origin == 'initialize-qute':
                if 'result' in message:
                    result = message['result']
                    # Qute capabilities apply to templates; Java requests route to JDT.
                    caps = dict(self.jdt_capabilities)
                    caps.update(result.get('capabilities') or {})
                    result['capabilities'] = caps
                self.send('native', {**message, 'id': original_id})
            elif origin == 'invalidate-qute':
                self.java_barriers = max(0, self.java_barriers - 1)
                self.sync_failed = self.sync_failed or 'error' in message
                self.reset_qute_models()
                if not self.java_barriers:
                    waiting, self.waiting_template_requests = self.waiting_template_requests, []
                    for request in waiting:
                        if self.sync_failed:
                            self.error('native', request['id'], 'Java model synchronization failed')
                        else:
                            self.native(request)
            elif origin != 'discard':
                callback = self.callback_methods.pop(str(message.get('id')), None)
                if callback == 'qute/template/projectDataModel' and 'result' in message:
                    self.normalize_data_model(message.get('result'))
                if callback in ('qute/template/project', 'qute/template/projects') and 'result' in message:
                    result = message.get('result')
                    infos = result if isinstance(result, list) else [result]
                    for info in infos:
                        if isinstance(info, dict) and isinstance(info.get('uri'), str) and len(info['uri']) <= 1024:
                            self.projects.add(info['uri'])
                self.native_requests.pop(json.dumps(original_id), None) if origin == 'native' else None
                self.send(origin, {**message, 'id': original_id})
            return
        method = message['method']
        if endpoint == 'qute' and method == '$/cancelRequest':
            original = (message.get('params') or {}).get('id')
            for queued, at in list(self.waiting_callbacks):
                if json.dumps(queued['id']) == json.dumps(original):
                    self.waiting_callbacks.remove((queued, at))
                    self.error('qute', original, 'request cancelled', -32800)
                    return
            for token, (origin, request_id, _) in self.pending.items():
                if origin == 'qute' and json.dumps(request_id) == json.dumps(original):
                    self.send('jdt', {'jsonrpc': '2.0', 'method': '$/cancelRequest', 'params': {'id': token}})
                    return
            return
        if endpoint == 'jdt' and method == 'language/status' and (message.get('params') or {}).get('type') == 'ServiceReady':
            self.jdt_ready = True
            waiting, self.waiting_callbacks = self.waiting_callbacks, []
            for callback, _ in waiting:
                self.backend('qute', callback)
        if endpoint == 'qute' and method in CALLBACKS and 'id' in message:
            if not self.jdt_ready:
                if len(self.waiting_callbacks) >= 256:
                    self.error('qute', message['id'], 'Java project import is not ready; callback limit reached')
                else:
                    self.waiting_callbacks.append((message, time.monotonic()))
                return
            token = self.request('jdt', {'jsonrpc': '2.0', 'method': 'workspace/executeCommand',
                'params': {'command': method, 'arguments': [message.get('params')]}}, 'qute', message['id'])
            if token is not None:
                self.callback_methods[token] = method
            return
        if endpoint == 'jdt' and method == 'workspace/executeClientCommand':
            params = message.get('params') or {}
            if params.get('command') == 'qute/dataModelChanged':
                args = params.get('arguments') or []
                self.send('qute', {'jsonrpc': '2.0', 'method': 'qute/dataModelChanged', 'params': args[0] if args else None})
                if 'id' in message:
                    self.send('jdt', {'jsonrpc': '2.0', 'id': message['id'], 'result': None})
                return
        if method.startswith('qute/') and endpoint == 'jdt' and 'id' not in message:
            self.send('qute', message)
            return
        # Both backend sessions share the single owning workspace and native client.
        if 'id' in message:
            if len(self.server_requests) >= 1024:
                self.error(endpoint, message['id'], 'native client request limit reached')
                return
            token = self.token('server-' + endpoint)
            self.server_requests[token] = (endpoint, message['id'])
            self.send('native', {**message, 'id': token})
        else:
            self.send('native', message)

    def close(self):
        for p in self.processes.values():
            if p.poll() is None:
                p.terminate()
        for p in self.processes.values():
            try:
                p.wait(timeout=3)
            except subprocess.TimeoutExpired:
                p.kill()
                p.wait()
        if self.session_dir is not None:
            shutil.rmtree(self.session_dir)
            self.session_dir = None

    def expire(self):
        retained = []
        for callback, at in self.waiting_callbacks:
            if time.monotonic() - at > 180:
                self.error('qute', callback['id'], 'Java project import did not become ready')
            else:
                retained.append((callback, at))
        self.waiting_callbacks = retained
        for token, (origin, original, at) in list(self.pending.items()):
            if time.monotonic() - at > 180:
                self.pending.pop(token)
                self.callback_methods.pop(token, None)
                if origin == 'native':
                    self.native_requests.pop(json.dumps(original), None)
                if origin == 'invalidate-qute':
                    for request in self.waiting_template_requests:
                        self.error('native', request['id'], 'Java model synchronization timed out; restart broker')
                    self.waiting_template_requests = []
                    self.closed = True
                if origin in ('native', 'qute', 'jdt', 'microprofile'):
                    self.error(origin, original, 'backend request timed out')
                elif origin.startswith('initialize-'):
                    self.error('native', original, 'language-server initialization timed out')
                    self.closed = True

    def run(self):
        threading.Thread(target=self.read_loop, args=('native', self.source), daemon=True).start()
        try:
            while not self.closed:
                self.expire()
                if self.closed:
                    break
                try:
                    endpoint, message = self.events.get(timeout=.25)
                except queue.Empty:
                    continue
                if message is None or message.get('_protocol_error'):
                    if endpoint != 'native':
                        print('Qute broker backend stopped or violated framing', file=sys.stderr)
                    break
                if os.environ.get('QUTE_BROKER_TRACE') == '1':
                    print('broker', endpoint, message.get('method', 'response'), str(message.get('id', ''))[:80], file=sys.stderr, flush=True)
                if endpoint == 'native':
                    self.native(message)
                else:
                    self.backend(endpoint, message)
        finally:
            self.close()

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--profile', required=True)
    parser.add_argument('--state-dir', required=True)
    args = parser.parse_args()
    try:
        profile = load_profile(args.profile)
        if 'microprofile_command' in profile:
            from microprofile_broker import MicroProfileBroker
            MicroProfileBroker(profile, args.state_dir).run()
        else:
            Broker(profile, args.state_dir).run()
    except (ValueError, OSError, KeyError, TypeError, subprocess.SubprocessError) as error:
        print('Qute broker refused profile, workspace or protocol: ' + str(error), file=sys.stderr)
        return 1
    return 0

if __name__ == '__main__':
    raise SystemExit(main())
