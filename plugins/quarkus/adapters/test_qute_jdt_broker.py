import io
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
from qute_jdt_broker import Broker, CALLBACKS, load_profile, read_frame, write_frame

class RecordingBroker(Broker):
    def __init__(self):
        super().__init__({}, '/tmp/unused-qute-broker-state')
        self.sent = []
        self.processes = {'jdt': None, 'qute': None}
        self.jdt_ready = True
    def send(self, endpoint, message):
        self.sent.append((endpoint, message))

class ProtocolTests(unittest.TestCase):
    def test_frames_handle_unicode_and_reject_partial_duplicate_and_oversized(self):
        message = {'jsonrpc': '2.0', 'method': 'hello', 'params': '😀'}
        stream = io.BytesIO()
        write_frame(stream, message)
        stream.seek(0)
        self.assertEqual(read_frame(stream), message)
        for frame in [b'Content-Length: 10\r\n\r\n{}', b'Content-Length: 2\r\nContent-Length: 2\r\n\r\n{}', b'Content-Length: 999999999\r\n\r\n', b'Content-Length: 2\r\n\r\n[]']:
            with self.assertRaises((ValueError, json.JSONDecodeError)):
                read_frame(io.BytesIO(frame))

    def test_official_callbacks_are_forwarded_to_jdt_and_result_ids_do_not_collide(self):
        broker = RecordingBroker()
        for index, callback in enumerate(sorted(CALLBACKS)):
            broker.backend('qute', {'jsonrpc': '2.0', 'id': index, 'method': callback, 'params': {'templateFileUri': 'file:///project/templates/main.html'}})
            endpoint, request = broker.sent[-1]
            self.assertEqual(endpoint, 'jdt')
            self.assertEqual(request['method'], 'workspace/executeCommand')
            self.assertEqual(request['params']['command'], callback)
            self.assertEqual(request['params']['arguments'], [{'templateFileUri': 'file:///project/templates/main.html'}])
            broker.backend('jdt', {'jsonrpc': '2.0', 'id': request['id'], 'result': {'verified': True}})
            self.assertEqual(broker.sent[-1], ('qute', {'jsonrpc': '2.0', 'id': index, 'result': {'verified': True}}))

    def test_callbacks_wait_for_real_java_service_ready_not_initialize_response(self):
        broker = RecordingBroker()
        broker.jdt_ready = False
        request = {'jsonrpc': '2.0', 'id': 3, 'method': 'qute/template/project', 'params': {'templateFileUri': 'file:///project/main.html'}}
        broker.backend('qute', request)
        self.assertEqual(broker.sent, [])
        self.assertEqual(len(broker.waiting_callbacks), 1)
        broker.backend('jdt', {'jsonrpc': '2.0', 'method': 'language/status', 'params': {'type': 'Started', 'message': 'Ready'}})
        self.assertFalse(broker.jdt_ready)
        broker.backend('jdt', {'jsonrpc': '2.0', 'method': 'language/status', 'params': {'type': 'ServiceReady', 'message': 'ServiceReady'}})
        self.assertTrue(broker.jdt_ready)
        self.assertTrue(any(endpoint == 'jdt' and message.get('method') == 'workspace/executeCommand' for endpoint, message in broker.sent))
        self.assertEqual(broker.waiting_callbacks, [])

    def test_unsaved_java_changes_and_saved_file_notifications_reach_semantic_backend(self):
        broker = RecordingBroker()
        for method in ['textDocument/didOpen', 'textDocument/didChange', 'textDocument/didClose', 'textDocument/didSave']:
            message = {'jsonrpc': '2.0', 'method': method, 'params': {'textDocument': {'uri': 'file:///project/Person.java', 'version': 9}, 'contentChanges': [{'text': 'unsaved field'}]}}
            broker.native(message)
            self.assertEqual(broker.sent[-2], ('jdt', message))
            self.assertEqual(broker.sent[-1][1]['method'], 'textDocument/documentSymbol')
        watched = {'jsonrpc': '2.0', 'method': 'workspace/didChangeWatchedFiles', 'params': {'changes': [{'uri': 'file:///project/Person.java', 'type': 2}]}}
        broker.native(watched)
        self.assertEqual(broker.sent[-3:-1], [('jdt', watched), ('qute', watched)])
        change = {'projectUri': 'file:///project', 'sources': True}
        broker.backend('jdt', {'jsonrpc': '2.0', 'id': 5, 'method': 'workspace/executeClientCommand', 'params': {'command': 'qute/dataModelChanged', 'arguments': [change]}})
        self.assertEqual(broker.sent[-2], ('qute', {'jsonrpc': '2.0', 'method': 'qute/dataModelChanged', 'params': change}))
        self.assertEqual(broker.sent[-1], ('jdt', {'jsonrpc': '2.0', 'id': 5, 'result': None}))

    def test_requests_cancellation_and_backend_errors_keep_native_identity(self):
        broker = RecordingBroker()
        request = {'jsonrpc': '2.0', 'id': 7, 'method': 'textDocument/completion', 'params': {'textDocument': {'uri': 'file:///project/Person.java'}}}
        broker.native(request)
        endpoint, forwarded = broker.sent[-1]
        self.assertEqual(endpoint, 'jdt')
        broker.native({'jsonrpc': '2.0', 'method': '$/cancelRequest', 'params': {'id': 7}})
        self.assertEqual(broker.sent[-1][1]['params']['id'], forwarded['id'])
        error = {'code': -32800, 'message': 'cancelled'}
        broker.backend('jdt', {'jsonrpc': '2.0', 'id': forwarded['id'], 'error': error})
        self.assertEqual(broker.sent[-1], ('native', {'jsonrpc': '2.0', 'id': 7, 'error': error}))

    def test_server_request_ids_are_isolated_between_backends(self):
        broker = RecordingBroker()
        for endpoint in ['qute', 'jdt']:
            broker.backend(endpoint, {'jsonrpc': '2.0', 'id': 1, 'method': 'workspace/configuration', 'params': {'items': []}})
        a, b = broker.sent[-2][1], broker.sent[-1][1]
        self.assertNotEqual(a['id'], b['id'])
        broker.native({'jsonrpc': '2.0', 'id': a['id'], 'result': []})
        self.assertEqual(broker.sent[-1], ('qute', {'jsonrpc': '2.0', 'id': 1, 'result': []}))
        broker.native({'jsonrpc': '2.0', 'id': b['id'], 'result': []})
        self.assertEqual(broker.sent[-1], ('jdt', {'jsonrpc': '2.0', 'id': 1, 'result': []}))

    def test_java_barrier_prevents_stale_template_requests_and_supports_cancellation(self):
        broker = RecordingBroker()
        broker.projects.add('project')
        broker.native({'jsonrpc': '2.0', 'method': 'textDocument/didChange', 'params': {'textDocument': {'uri': 'file:///project/Person.java', 'version': 2}, 'contentChanges': [{'text': 'new model'}]}})
        barrier = broker.sent[-1][1]['id']
        request = {'jsonrpc': '2.0', 'id': 'completion', 'method': 'textDocument/completion', 'params': {'textDocument': {'uri': 'file:///project/main.html'}}}
        broker.native(request)
        self.assertEqual(len(broker.waiting_template_requests), 1)
        broker.native({'jsonrpc': '2.0', 'method': '$/cancelRequest', 'params': {'id': 'completion'}})
        self.assertEqual(broker.waiting_template_requests, [])
        self.assertEqual(broker.sent[-1][1]['error']['code'], -32800)
        broker.native({**request, 'id': 4})
        broker.backend('jdt', {'jsonrpc': '2.0', 'id': barrier, 'result': []})
        self.assertEqual(broker.java_barriers, 0)
        self.assertTrue(any(endpoint == 'qute' and message.get('method') == 'qute/dataModelChanged' for endpoint, message in broker.sent))
        self.assertEqual(broker.sent[-1][0], 'qute')
        self.assertEqual(broker.sent[-1][1]['method'], 'textDocument/completion')

    def test_foreign_document_and_symlink_sources_cannot_enter_the_workspace(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            broker = RecordingBroker()
            broker.root = root
            request = {'jsonrpc': '2.0', 'id': 4, 'method': 'textDocument/completion', 'params': {'textDocument': {'uri': 'file:///unrelated/Person.java'}}}
            broker.native(request)
            self.assertEqual(broker.sent[-1][0], 'native')
            self.assertEqual(broker.sent[-1][1]['error']['code'], -32602)
            outside = root.parent/'outside-qute-protocol-source'
            link = root/'alias.java'
            link.symlink_to(outside)
            broker.native({**request, 'params': {'textDocument': {'uri': link.as_uri()}}})
            self.assertEqual(broker.sent[-1][1]['error']['code'], -32602)

    def test_numeric_and_string_native_ids_are_distinct(self):
        broker = RecordingBroker()
        for request_id in (7, '7'):
            broker.native({'jsonrpc': '2.0', 'id': request_id, 'method': 'textDocument/hover', 'params': {'textDocument': {'uri': 'file:///project/Person.java'}}})
        self.assertEqual(len(broker.native_requests), 2)
        first, second = broker.sent[-2][1], broker.sent[-1][1]
        broker.native({'jsonrpc': '2.0', 'method': '$/cancelRequest', 'params': {'id': 7}})
        self.assertEqual(broker.sent[-1][1]['params']['id'], first['id'])
        broker.native({'jsonrpc': '2.0', 'method': '$/cancelRequest', 'params': {'id': '7'}})
        self.assertEqual(broker.sent[-1][1]['params']['id'], second['id'])

    def test_timeouts_fail_initialization_and_model_sync_instead_of_serving_stale_types(self):
        for origin in ('initialize-jdt', 'invalidate-qute'):
            broker = RecordingBroker()
            broker.pending['old'] = (origin, 1, 0)
            broker.waiting_template_requests = [{'jsonrpc': '2.0', 'id': 8, 'method': 'textDocument/completion'}]
            with patch('qute_jdt_broker.time.monotonic', return_value=200):
                broker.expire()
            self.assertTrue(broker.closed)
            self.assertEqual(broker.sent[-1][0], 'native')
            self.assertIn('error', broker.sent[-1][1])

    def test_empty_base_path_model_normalization_preserves_source_identity_boundaries(self):
        broker = RecordingBroker()
        data = {'templates': [{'templateUri': 'src/main/resources/templates//hello', 'sourceType': 'org.acme.Views'}, {'templateUri': 'custom://locator//hello'}, {'templateUri': 'src/main/resources/templates/../hello'}]}
        broker.normalize_data_model(data)
        self.assertEqual(data['templates'][0], {'templateUri': 'src/main/resources/templates/hello', 'sourceType': 'org.acme.Views'})
        self.assertEqual(data['templates'][1]['templateUri'], 'custom://locator//hello')
        self.assertEqual(data['templates'][2]['templateUri'], 'src/main/resources/templates/../hello')

    def test_tool_artifact_drift_is_rejected_before_process_launch(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            jar = root/'extension.jar'
            jar.write_bytes(b'actual tool')
            profile = {'version': 1, 'qute_command': ['/usr/bin/true'], 'jdt_command': ['/usr/bin/true'], 'bundles': [str(jar)], 'sha256': {str(jar): '0'*64}}
            path = root/'profile.json'
            path.write_text(json.dumps(profile))
            with self.assertRaisesRegex(ValueError, 'artifact changed'):
                load_profile(path)

if __name__ == '__main__':
    unittest.main()
