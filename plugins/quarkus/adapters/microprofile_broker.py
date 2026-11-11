"""Optional official LSP4MP/Quarkus LS sidecar sharing the broker's JDT model."""
import copy
import json
from pathlib import Path
import subprocess
import threading
import time
from urllib.parse import unquote, urlparse
from qute_jdt_broker import Broker, CALLBACKS, child_environment
from config_overlay import ConfigOverlay
from config_sources import classify

MP_CALLBACKS = frozenset(('microprofile/projectInfo', 'microprofile/propertyDefinition',
    'microprofile/propertyDocumentation', 'microprofile/java/fileInfo',
    'microprofile/java/codeAction', 'microprofile/java/codeActionResolve',
    'microprofile/java/codeLens', 'microprofile/java/inlayHint',
    'microprofile/java/completion', 'microprofile/java/definition',
    'microprofile/java/diagnostics', 'microprofile/java/hover',
    'microprofile/java/workspaceSymbols', 'microprofile/java/projectLabels',
    'microprofile/java/workspaceLabels'))
JAVA_MERGE = frozenset(('textDocument/completion', 'textDocument/hover',
    'textDocument/definition', 'textDocument/codeAction', 'textDocument/codeLens',
    'textDocument/inlayHint'))
DATA_KEY = 'openideaQuarkusBroker'

def useful_hover(value):
    if not isinstance(value, dict):
        return False
    contents = value.get('contents')
    if isinstance(contents, dict):
        return bool(contents.get('value'))
    return bool(contents)

def config_uri(uri):
    return classify(unquote(urlparse(uri).path)) is not None

class MicroProfileBroker(Broker):
    def __init__(self, *args, **kwargs):
        super().__init__(*args, **kwargs)
        self.mp_waiting = []
        self.mp_projects = set()
        self.mp_callbacks = {}
        self.initialize_results = {}
        self.groups = {}
        self.group_requests = {}
        self.resolve_routes = {}
        self.direct_result_routes = {}
        self.diagnostic_sets = {}
        self.document_versions = {}
        self.dirty_configs = set()
        self.overlay = None
        self.overlay_syncs = {}
        self.callback_generations = {}

    def start(self, message):
        super().start(message)
        if self.profile.get('configuration_overlay'):
            self.overlay = ConfigOverlay(self.root, self.session_dir/'config-overlay.json')
        argv = [a.replace('{workspace}', str(self.session_dir)).replace('{root}', str(self.root)) for a in self.profile['microprofile_command']]
        process = subprocess.Popen(argv, cwd=self.root, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=__import__('sys').stderr, env=child_environment(self.profile))
        self.processes['microprofile'] = process
        threading.Thread(target=self.read_loop, args=('microprofile', process.stdout), daemon=True).start()

    def send(self, endpoint, message):
        method = message.get('method')
        if 'microprofile' in self.processes:
            if endpoint == 'qute' and method in ('workspace/didChangeWatchedFiles', 'workspace/didChangeConfiguration', '$/setTrace'):
                super().send('microprofile', message)
            if endpoint == 'jdt' and method in ('textDocument/didOpen', 'textDocument/didChange', 'textDocument/didClose', 'textDocument/didSave'):
                super().send('microprofile', message)
        super().send(endpoint, message)

    def route(self, message):
        params = message.get('params') or {}
        method = message.get('method')
        data = params.get('data')
        if method in ('completionItem/resolve', 'codeAction/resolve', 'inlayHint/resolve') and isinstance(data, dict) and DATA_KEY in data:
            endpoint = data[DATA_KEY].get('backend')
            return endpoint if endpoint in ('jdt', 'microprofile') else 'qute'
        if method == 'workspace/executeCommand':
            command = params.get('command', '')
            if command in MP_CALLBACKS:
                return 'jdt'
            if command.startswith('microprofile.'):
                return 'microprofile'
        uri = (params.get('textDocument') or {}).get('uri', '')
        if config_uri(uri):
            return 'qute' if 'id' in message and (self.java_barriers or self.sync_failed) else 'microprofile'
        if uri.lower().endswith('.java') and method in JAVA_MERGE:
            if self.dirty_configs and (self.overlay is None or self.overlay.unsupported):
                # The official JDT config sources read saved resources. Do not
                # expose disk-derived values/edits as current unsaved config facts.
                return 'jdt'
            return 'qute' if self.java_barriers or self.sync_failed else 'combined-java'
        return super().route(message)

    def tagged(self, item, endpoint):
        if not isinstance(item, dict):
            return item
        tagged = dict(item)
        tagged['data'] = {DATA_KEY: {'backend': endpoint, 'present': 'data' in item, 'value': item.get('data')}}
        return tagged

    def request(self, endpoint, message, origin, original_id):
        if endpoint == 'combined-java':
            group = self.token('java')
            self.groups[group] = {'id': original_id, 'method': message['method'], 'responses': {}, 'at': time.monotonic(), 'generation': self.overlay.generation if self.overlay is not None else None}
            self.group_requests[json.dumps(original_id)] = group
            for backend in ('jdt', 'microprofile'):
                super().request(backend, message, 'merge:'+group+':'+backend, original_id)
            return None
        resolving = message.get('method') in ('completionItem/resolve', 'codeAction/resolve', 'inlayHint/resolve')
        params = message.get('params') or {}
        data = params.get('data')
        if resolving and isinstance(data, dict) and isinstance(data.get(DATA_KEY), dict):
            metadata = data[DATA_KEY]
            params = dict(params)
            if metadata.get('present'):
                params['data'] = metadata.get('value')
            else:
                params.pop('data', None)
            message = {**message, 'params': params}
        token = super().request(endpoint, message, origin, original_id)
        if token is not None and resolving:
            self.resolve_routes[token] = endpoint
        if token is not None and origin == 'native' and message.get('method') in ('textDocument/completion', 'textDocument/codeAction', 'textDocument/codeLens', 'textDocument/inlayHint'):
            self.direct_result_routes[token] = (endpoint, message['method'])
        return token

    def native(self, message):
        method = message.get('method')
        params = message.get('params') or {}
        uri = (params.get('textDocument') or {}).get('uri')
        if method == '$/cancelRequest':
            original = params.get('id')
            group = self.group_requests.get(json.dumps(original))
            if group:
                for token, (origin, _, _) in self.pending.items():
                    if origin.startswith('merge:'+group+':'):
                        endpoint = origin.rsplit(':', 1)[1]
                        self.send(endpoint, {'jsonrpc': '2.0', 'method': '$/cancelRequest', 'params': {'id': token}})
                return
        before_dirty = bool(self.dirty_configs)
        if self.overlay is not None and uri and config_uri(uri):
            self.overlay.event(method, params)
        if uri and config_uri(uri) and self.root is not None:
            parsed = urlparse(uri)
            path = Path(unquote(parsed.path)).resolve() if parsed.scheme == 'file' else None
            if path is not None and path.is_relative_to(self.root):
                if method == 'textDocument/didOpen':
                    text = (params.get('textDocument') or {}).get('text')
                    try:
                        clean = path.stat().st_size <= 8*1024*1024 and path.read_text(encoding='utf-8') == text
                    except (OSError, UnicodeError):
                        clean = False
                    if not clean:
                        self.dirty_configs.add(uri)
                elif method == 'textDocument/didChange':
                    self.dirty_configs.add(uri)
                elif method in ('textDocument/didClose', 'textDocument/didSave'):
                    self.dirty_configs.discard(uri)
        if not before_dirty and self.dirty_configs:
            for (backend, source), diagnostics in list(self.diagnostic_sets.items()):
                if backend == 'microprofile' and source and source.lower().endswith('.java'):
                    self.diagnostic_sets.pop((backend, source))
                    retained = [item for (owner, file), values in self.diagnostic_sets.items() if file == source for item in values]
                    self.send('native', {'jsonrpc': '2.0', 'method': 'textDocument/publishDiagnostics', 'params': {'uri': source, 'diagnostics': retained}})
            if self.overlay is None or self.overlay.unsupported:
                self.send('native', {'jsonrpc': '2.0', 'method': 'window/logMessage', 'params': {'type': 3, 'message': 'Save or close unsupported changed configuration files to refresh Java configuration assistance. Configuration editor assistance remains available.'}})
        if (before_dirty and not self.dirty_configs) or (self.overlay is not None and uri and config_uri(uri) and method in ('textDocument/didChange', 'textDocument/didClose', 'textDocument/didSave')):
            self.reset_qute_models()
        if uri and method in ('textDocument/didOpen', 'textDocument/didChange'):
            self.document_versions[uri] = (params.get('textDocument') or {}).get('version')
            for key in list(self.diagnostic_sets):
                if key[1] == uri:
                    self.diagnostic_sets.pop(key)
        if uri and method == 'textDocument/didClose':
            self.document_versions.pop(uri, None)
            for key in list(self.diagnostic_sets):
                if key[1] == uri:
                    self.diagnostic_sets.pop(key)
        if method == 'initialized':
            self.send('microprofile', message)
        elif method in ('exit', 'shutdown') and 'microprofile' in self.processes:
            if method == 'exit':
                self.send('microprofile', message)
            else:
                super().request('microprofile', message, 'discard', None)
        super().native(message)

    def reset_qute_models(self):
        super().reset_qute_models()
        if self.mp_projects and 'microprofile' in self.processes:
            self.send('microprofile', {'jsonrpc': '2.0', 'method': 'microprofile/propertiesChanged',
                'params': {'type': [1, 3], 'projectURIs': sorted(self.mp_projects)}})

    def finish_initialize(self):
        if not all(endpoint in self.initialize_results for endpoint in ('qute', 'microprofile')):
            return
        if any('error' in item for item in self.initialize_results.values()):
            self.error('native', self.init_message['id'], 'Qute or MicroProfile initialization failed')
            self.closed = True
            return
        result = copy.deepcopy(self.initialize_results['qute']['result'])
        caps = dict(self.jdt_capabilities)
        for endpoint in ('qute', 'microprofile'):
            for name, value in self.initialize_results[endpoint]['result'].get('capabilities', {}).items():
                if name in ('completionProvider', 'executeCommandProvider') and isinstance(value, dict):
                    existing = caps.get(name) if isinstance(caps.get(name), dict) else {}
                    merged = {**existing, **value}
                    for key in ('triggerCharacters', 'commands'):
                        if key in existing or key in value:
                            merged[key] = sorted(set(existing.get(key) or []) | set(value.get(key) or []))
                    caps[name] = merged
                else:
                    caps[name] = value
        result['capabilities'] = caps
        self.send('native', {'jsonrpc': '2.0', 'id': self.init_message['id'], 'result': result})

    def merge(self, group):
        state = self.groups.pop(group)
        self.group_requests.pop(json.dumps(state['id']), None)
        responses = state['responses']
        if self.overlay is not None and state.get('generation') != self.overlay.generation:
            responses.pop('microprofile', None)
        for response in responses.values():
            if 'result' in response:
                self.remember_virtual_uris(response['result'])
        good = {endpoint: reply.get('result') for endpoint, reply in responses.items() if 'result' in reply}
        if not good:
            self.send('native', {'jsonrpc': '2.0', 'id': state['id'], 'error': next(iter(responses.values()))['error']})
            return
        method = state['method']
        if method == 'textDocument/hover':
            result = good.get('microprofile') if useful_hover(good.get('microprofile')) else good.get('jdt')
        elif method == 'textDocument/completion':
            items, incomplete, seen = [], False, set()
            for endpoint, result in good.items():
                values = result.get('items') or [] if isinstance(result, dict) else result or []
                incomplete |= isinstance(result, dict) and result.get('isIncomplete', False)
                for item in values:
                    identity = json.dumps([item.get('label'), item.get('kind'), item.get('textEdit'), item.get('insertText')], sort_keys=True)
                    if identity not in seen:
                        seen.add(identity)
                        items.append(self.tagged(item, endpoint))
            result = {'isIncomplete': incomplete, 'items': items}
        else:
            result = []
            seen = set()
            for endpoint, value in good.items():
                values = value if isinstance(value, list) else [value] if value else []
                for item in values:
                    identity = json.dumps(item, sort_keys=True)
                    if identity not in seen:
                        seen.add(identity)
                        result.append(self.tagged(item, endpoint) if method in ('textDocument/codeAction', 'textDocument/codeLens', 'textDocument/inlayHint') else item)
        self.send('native', {'jsonrpc': '2.0', 'id': state['id'], 'result': result})

    def backend(self, endpoint, message):
        method = message.get('method')
        if method == 'textDocument/publishDiagnostics':
            params = message.get('params') or {}
            uri = params.get('uri')
            version = params.get('version')
            current = self.document_versions.get(uri)
            if version is not None and current is not None and version < current:
                return
            values = params.get('diagnostics') or []
            if endpoint == 'microprofile' and self.dirty_configs and uri and uri.lower().endswith('.java'):
                values = []
            self.diagnostic_sets[(endpoint, uri)] = values
            diagnostics = [item for (backend, source), values in self.diagnostic_sets.items() if source == uri for item in values]
            self.send('native', {'jsonrpc': '2.0', 'method': method, 'params': {**params, 'diagnostics': diagnostics}})
            return
        if method is None:
            token = str(message.get('id'))
            entry = self.pending.get(token)
            synchronizing = self.overlay_syncs.pop(token, None)
            if synchronizing is not None:
                self.pending.pop(token, None)
                callback, generation = synchronizing
                receipt = message.get('result') or {}
                applied = receipt.get('applied') if isinstance(receipt, dict) else None
                valid = self.overlay.generation == generation and not self.overlay.unsupported and isinstance(receipt, dict) and receipt.get('generation') == generation and not receipt.get('unsupported') and isinstance(applied, list) and all(isinstance(uri, str) for uri in applied) and self.overlay.dirty.issubset(set(applied))
                if not valid:
                    self.error('microprofile', callback['id'], 'Configuration snapshot is stale or unsupported', -32801)
                else:
                    self.dispatch_mp_callback(callback, generation)
                return
            generation = self.callback_generations.pop(token, None)
            if generation is not None and self.overlay.generation != generation:
                self.pending.pop(token, None)
                self.mp_callbacks.pop(token, None)
                if entry:
                    self.error('microprofile', entry[1], 'Configuration changed during Java analysis', -32801)
                return
            if entry:
                origin, original, _ = entry
                if origin in ('initialize-qute', 'initialize-microprofile'):
                    self.pending.pop(token)
                    self.initialize_results['qute' if origin == 'initialize-qute' else 'microprofile'] = message
                    self.finish_initialize()
                    return
                if origin == 'initialize-jdt':
                    super().backend(endpoint, message)
                    if not self.closed:
                        params = copy.deepcopy(self.init_message.get('params') or {})
                        params['processId'] = __import__('os').getpid()
                        super().request('microprofile', {'jsonrpc': '2.0', 'method': 'initialize', 'params': params}, 'initialize-microprofile', original)
                    return
                if origin.startswith('merge:'):
                    self.pending.pop(token)
                    group = origin[len('merge:'):].rsplit(':', 1)[0]
                    state = self.groups.get(group)
                    if state is not None:
                        state['responses'][endpoint] = message
                        if len(state['responses']) == 2:
                            self.merge(group)
                    return
                direct = self.direct_result_routes.pop(token, None)
                if direct and 'result' in message:
                    route, request_method = direct
                    result = message['result']
                    if request_method == 'textDocument/completion' and isinstance(result, dict):
                        message = {**message, 'result': {**result, 'items': [self.tagged(item, route) for item in result.get('items') or []]}}
                    elif isinstance(result, list):
                        message = {**message, 'result': [self.tagged(item, route) for item in result]}
                route = self.resolve_routes.pop(token, None)
                if route and 'result' in message:
                    message = {**message, 'result': self.tagged(message['result'], route)}
                callback = self.mp_callbacks.pop(token, None)
                if callback == 'microprofile/projectInfo' and isinstance(message.get('result'), dict):
                    project = message['result'].get('projectURI')
                    if isinstance(project, str) and project:
                        self.mp_projects.add(project)
        if endpoint == 'microprofile' and method == '$/cancelRequest':
            original = (message.get('params') or {}).get('id')
            for callback, at in list(self.mp_waiting):
                if json.dumps(callback['id']) == json.dumps(original):
                    self.mp_waiting.remove((callback, at))
                    self.error('microprofile', original, 'request cancelled', -32800)
                    return
            for token, (callback, _) in list(self.overlay_syncs.items()):
                if json.dumps(callback['id']) == json.dumps(original):
                    self.overlay_syncs.pop(token)
                    self.pending.pop(token, None)
                    self.send('jdt', {'jsonrpc': '2.0', 'method': '$/cancelRequest', 'params': {'id': token}})
                    self.error('microprofile', original, 'request cancelled', -32800)
                    return
            for token, (origin, request_id, _) in self.pending.items():
                if origin == 'microprofile' and json.dumps(request_id) == json.dumps(original):
                    self.send('jdt', {'jsonrpc': '2.0', 'method': '$/cancelRequest', 'params': {'id': token}})
                    return
            return
        if endpoint == 'microprofile' and method in MP_CALLBACKS and 'id' in message:
            if not self.jdt_ready:
                if len(self.mp_waiting) < 256:
                    self.mp_waiting.append((message, time.monotonic()))
                else:
                    self.error('microprofile', message['id'], 'Java project import is not ready; callback limit reached')
                return
            if self.overlay is not None:
                generation = self.overlay.generation
                token = super().request('jdt', {'jsonrpc': '2.0', 'method': 'workspace/executeCommand', 'params': {'command': 'openidea.configOverlay.synchronize', 'arguments': [generation]}}, 'overlay-sync', message['id'])
                if token:
                    self.overlay_syncs[token] = (message, generation)
            else:
                self.dispatch_mp_callback(message)
            return
        if endpoint == 'jdt' and method == 'workspace/executeClientCommand' and (message.get('params') or {}).get('command') == 'microprofile/propertiesChanged':
            args = (message.get('params') or {}).get('arguments') or []
            self.send('microprofile', {'jsonrpc': '2.0', 'method': 'microprofile/propertiesChanged', 'params': args[0] if args else None})
            if 'id' in message:
                self.send('jdt', {'jsonrpc': '2.0', 'id': message['id'], 'result': None})
            return
        super().backend(endpoint, message)
        if endpoint == 'jdt' and method == 'language/status' and (message.get('params') or {}).get('type') == 'ServiceReady':
            waiting, self.mp_waiting = self.mp_waiting, []
            for callback, _ in waiting:
                self.backend('microprofile', callback)

    def dispatch_mp_callback(self, message, generation=None):
        method = message['method']
        token = super().request('jdt', {'jsonrpc': '2.0', 'method': 'workspace/executeCommand',
            'params': {'command': method, 'arguments': [message.get('params')]}}, 'microprofile', message['id'])
        if token:
            self.mp_callbacks[token] = method
            if generation is not None:
                self.callback_generations[token] = generation

    def expire(self):
        now = time.monotonic()
        for token, (callback, _) in list(self.overlay_syncs.items()):
            entry = self.pending.get(token)
            if entry is None or now-entry[2] > 180:
                self.overlay_syncs.pop(token, None)
                self.pending.pop(token, None)
                self.error('microprofile', callback['id'], 'Configuration overlay synchronization timed out')
        super().expire()
        for group, state in list(self.groups.items()):
            if time.monotonic() - state['at'] > 180:
                self.groups.pop(group)
                self.group_requests.pop(json.dumps(state['id']), None)
                self.error('native', state['id'], 'Java/MicroProfile combined request timed out')
        retained = []
        for callback, at in self.mp_waiting:
            if time.monotonic() - at > 180:
                self.error('microprofile', callback['id'], 'Java project import did not become ready')
            else:
                retained.append((callback, at))
        self.mp_waiting = retained
