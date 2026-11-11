#!/usr/bin/env python3
"""Isolated stdlib FogBugz XML adapter: PROFILE REQUEST OUTPUT. No CLI test modes."""
import hashlib
import http.client
import json
import os
import pathlib
import re
import ssl
import stat
import sys
import tempfile
import time
import urllib.parse
import xml.etree.ElementTree as ET

MAX_PROFILE = 16 * 1024
MAX_REQUEST = 16 * 1024
MAX_RESPONSE = 1024 * 1024
MAX_OUTPUT = 1024 * 1024
COLS = 'sTitle,sLatestTextSummary,fOpen,ixBugEventLatest'
TENANT = re.compile(r'[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?\.fogbugz\.com\Z')
DECIMAL = re.compile(r'[1-9][0-9]{0,9}\Z')

class BridgeError(Exception):
    """Only fixed public codes; no exception/response/profile contents escape."""

def fail(code):
    raise BridgeError(code)

def no_symlinks(path):
    path = pathlib.Path(path)
    if not path.is_absolute() or path != pathlib.Path(os.path.normpath(str(path))):
        fail('invalid_file_path')
    current = pathlib.Path(path.anchor)
    for part in path.parts[1:]:
        current /= part
        if stat.S_ISLNK(os.lstat(current).st_mode):
            fail('symlink_refused')
    return path

def bounded_read(path, limit, private=False):
    path = no_symlinks(path)
    fd = os.open(path, os.O_RDONLY | getattr(os, 'O_NOFOLLOW', 0))
    try:
        before = os.fstat(fd)
        if not stat.S_ISREG(before.st_mode) or before.st_size > limit:
            fail('input_bound_or_kind')
        if private and (stat.S_IMODE(before.st_mode) != 0o600 or before.st_uid != os.geteuid() or before.st_nlink != 1):
            fail('profile_requires_owned_0600')
        chunks = bytearray()
        while len(chunks) <= limit:
            piece = os.read(fd, min(16384, limit + 1 - len(chunks)))
            if not piece:
                break
            chunks.extend(piece)
        if len(chunks) > limit:
            fail('input_bound_or_kind')
        after = os.fstat(fd)
        if (before.st_dev, before.st_ino, before.st_size, before.st_mtime_ns, before.st_mode) != (after.st_dev, after.st_ino, after.st_size, after.st_mtime_ns, after.st_mode):
            fail('input_changed')
        now = os.stat(path, follow_symlinks=False)
        if (now.st_dev, now.st_ino, now.st_mtime_ns, now.st_size, now.st_mode) != (before.st_dev, before.st_ino, before.st_mtime_ns, before.st_size, before.st_mode):
            fail('input_changed')
        return bytes(chunks)
    finally:
        os.close(fd)

def json_object(raw, code):
    def pairs(values):
        out = {}
        for key, value in values:
            if key in out:
                fail(code)
            out[key] = value
        return out
    try:
        value = json.loads(raw.decode('utf-8'), object_pairs_hook=pairs,
                           parse_constant=lambda _: fail(code))
    except BridgeError:
        raise
    except Exception:
        fail(code)
    if not isinstance(value, dict):
        fail(code)
    return value

def profile(path):
    raw = bounded_read(path, MAX_PROFILE, private=True)
    value = json_object(raw, 'invalid_profile')
    if set(value) != {'version', 'endpoint', 'token'} or type(value['version']) is not int or value['version'] != 1:
        fail('invalid_profile')
    endpoint, token = value['endpoint'], value['token']
    if not isinstance(endpoint, str) or not isinstance(token, str) or not 16 <= len(token) <= 4096 or any(ord(c) < 33 or ord(c) > 126 for c in token):
        fail('invalid_profile')
    try:
        url = urllib.parse.urlsplit(endpoint)
        host = url.hostname
        if not host or not TENANT.fullmatch(host) or endpoint != 'https://' + host + '/api.asp':
            fail('invalid_endpoint')
    except BridgeError:
        raise
    except Exception:
        fail('invalid_endpoint')
    canonical = str(pathlib.Path(path))
    fingerprint = hashlib.sha256(json.dumps([canonical, endpoint], ensure_ascii=True, separators=(',', ':')).encode() + b'\0' + raw).hexdigest()
    return {'endpoint': endpoint, 'token': token, 'host': host, 'profileId': fingerprint}

def positive_id(value):
    return isinstance(value, str) and DECIMAL.fullmatch(value) is not None and int(value) <= 2147483647

def validate_request(value):
    if type(value.get('version')) is not int or value['version'] != 1:
        fail('invalid_request')
    operation = value.get('operation')
    if operation == 'search':
        if set(value) != {'version', 'operation', 'query', 'page', 'expectedHost'}:
            fail('invalid_request')
        if not isinstance(value['query'], str) or not value['query'].strip() or len(value['query'].encode('utf-8')) > 4096 or any(ord(c) < 32 for c in value['query']):
            fail('invalid_query')
        if type(value['page']) is not int or value['page'] != 1:
            fail('unsupported_page')
    elif operation == 'close':
        if set(value) != {'version', 'operation', 'expectedHost', 'expectedId', 'expectedEventId', 'expectedProfileId'}:
            fail('invalid_request')
        if not positive_id(value['expectedId']) or not positive_id(value['expectedEventId']):
            fail('invalid_case_identity')
        if not isinstance(value['expectedProfileId'], str) or not re.fullmatch('[a-f0-9]{64}', value['expectedProfileId']):
            fail('invalid_profile_identity')
    else:
        fail('invalid_operation')
    if not isinstance(value['expectedHost'], str) or not TENANT.fullmatch(value['expectedHost']):
        fail('invalid_expected_host')
    return value

def remaining(deadline):
    seconds = deadline - time.monotonic()
    if seconds <= 0:
        fail('timeout')
    return seconds

def post_xml(endpoint, fields, deadline, *, _connection_factory=None):
    """HTTPS-only production transport. Internal factory exists for fixture tests."""
    url = urllib.parse.urlsplit(endpoint)
    connection = None
    try:
        context = ssl.create_default_context()
        factory = _connection_factory or http.client.HTTPSConnection
        connection = factory(url.hostname, port=443, timeout=remaining(deadline), context=context)
        body = urllib.parse.urlencode(fields).encode('ascii')
        connection.request('POST', '/api.asp', body=body, headers={
            'Content-Type': 'application/x-www-form-urlencoded; charset=utf-8',
            'Accept': 'application/xml, text/xml', 'Accept-Encoding': 'identity',
            'User-Agent': 'OpenIDEA-Tasks-FogBugz/1'})
        transport_socket = connection.sock
        response = connection.getresponse()
        if 300 <= response.status < 400:
            fail('redirect_refused')
        if response.status != 200:
            fail('http_failure')
        if response.getheader('Content-Encoding', 'identity').lower() not in ('', 'identity'):
            fail('unsupported_response_encoding')
        content_type = response.getheader('Content-Type', '').split(';')[0].strip().lower()
        if content_type not in ('application/xml', 'text/xml'):
            fail('invalid_xml_response')
        content_length = response.getheader('Content-Length')
        if content_length is not None and (not content_length.isdecimal() or int(content_length) > MAX_RESPONSE):
            fail('response_bound')
        data = bytearray()
        while len(data) <= MAX_RESPONSE:
            if response.isclosed():
                break
            timeout = remaining(deadline)
            live_socket = connection.sock or transport_socket
            if live_socket is not None:
                live_socket.settimeout(timeout)
            chunk = response.read1(min(16384, MAX_RESPONSE + 1 - len(data)))
            if not chunk:
                break
            data.extend(chunk)
        if len(data) > MAX_RESPONSE:
            fail('response_bound')
        remaining(deadline)
        return bytes(data)
    except BridgeError:
        raise
    except Exception:
        fail('transport_failure')
    finally:
        if connection is not None:
            connection.close()

def parse_xml(raw):
    if len(raw) > MAX_RESPONSE:
        fail('response_bound')
    if b'\0' in raw:
        fail('invalid_xml_response')
    try:
        text = raw.decode('utf-8')
    except Exception:
        fail('invalid_xml_response')
    if re.search(r'<!\s*(?:DOCTYPE|ENTITY)\b', text, flags=re.IGNORECASE):
        fail('xml_dtd_entity_refused')
    # Do not admit nested XML structures large enough to exhaust recursive parsers.
    if text.count('<') > 10000:
        fail('xml_structure_bound')
    try:
        parser = ET.XMLPullParser(events=('start', 'end'))
        depth = 0
        for at in range(0, len(raw), 16384):
            parser.feed(raw[at:at + 16384])
            for event, _ in parser.read_events():
                depth += 1 if event == 'start' else -1
                if depth > 32:
                    fail('xml_structure_bound')
        root = ET.fromstring(raw)
    except BridgeError:
        raise
    except Exception:
        fail('invalid_xml_response')
    if root.tag != 'response':
        fail('invalid_xml_response')
    if root.find('.//error') is not None:
        fail('api_failure')
    return root

def scalar(node, name, required=True):
    found = node.findall(name)
    if len(found) != 1 or len(found[0]) != 0:
        if required:
            fail('invalid_case_response')
        return ''
    return found[0].text or ''

def clean(value, limit, token):
    value = value.replace(token, '[redacted]')
    value = ''.join(c for c in value if ord(c) >= 32 or c in '\n\t')
    return value.encode('utf-8')[:limit].decode('utf-8', errors='ignore')

def cases(root, config):
    containers = root.findall('cases')
    if len(containers) != 1:
        fail('invalid_case_response')
    nodes = containers[0].findall('case')
    if len(nodes) > 50 or len(nodes) != len(list(containers[0])):
        fail('case_count_bound')
    issues, seen = [], set()
    for node in nodes:
        number = node.get('ixBug')
        child_number = scalar(node, 'ixBug', required=False)
        if not positive_id(number) or number in seen or (child_number and child_number != number):
            fail('invalid_case_identity')
        seen.add(number)
        title = scalar(node, 'sTitle')
        state = scalar(node, 'fOpen')
        event = scalar(node, 'ixBugEventLatest')
        if not title.strip() or state not in ('true', 'false') or not positive_id(event):
            fail('invalid_case_response')
        operations = node.get('operations', '').split(',')
        issues.append({'number': number, 'summary': clean(title, 1024, config['token']),
                       'description': clean(scalar(node, 'sLatestTextSummary', required=False), 16384, config['token']),
                       'url': 'https://' + config['host'] + '/default.asp?' + number,
                       'closed': state == 'false', 'eventId': event,
                       'canClose': state == 'true' and 'close' in operations})
    return issues

def execute(profile_path, request, *, _transport=None):
    request = validate_request(request)
    config = profile(profile_path)
    if request['expectedHost'] != config['host']:
        fail('host_mismatch')
    if request['operation'] == 'close' and request['expectedProfileId'] != config['profileId']:
        fail('profile_changed')
    transport = _transport or post_xml
    deadline = time.monotonic() + 8
    def call(fields):
        fields = dict(fields, token=config['token'])
        return parse_xml(transport(config['endpoint'], fields, deadline))
    def search(query):
        return cases(call({'cmd': 'search', 'q': query, 'cols': COLS, 'max': '50'}), config)
    base = {'version': 1, 'ok': True, 'operation': request['operation'], 'host': config['host'], 'profileId': config['profileId']}
    if request['operation'] == 'search':
        issues = search(request['query'])
        return dict(base, issues=issues, page=1, hasMore=len(issues) == 50)
    current = search(request['expectedId'])
    if len(current) != 1 or current[0]['number'] != request['expectedId']:
        fail('case_identity_mismatch')
    issue = current[0]
    if issue['eventId'] != request['expectedEventId']:
        fail('case_changed')
    if issue['closed']:
        return dict(base, issues=[issue], closed=True, alreadyClosed=True)
    if not issue['canClose']:
        fail('close_not_allowed')
    if profile(profile_path)['profileId'] != config['profileId']:
        fail('profile_changed')
    # Optimistic event identity protects the read/review/write boundary remotely.
    try:
        call({'cmd': 'close', 'ixBug': request['expectedId'], 'ixBugEvent': request['expectedEventId'], 'cols': COLS})
        verified = search(request['expectedId'])
        if len(verified) != 1 or verified[0]['number'] != request['expectedId'] or not verified[0]['closed']:
            fail('close_state_unknown')
    except Exception:
        fail('close_state_unknown')
    return dict(base, issues=verified, closed=True, alreadyClosed=False)

def atomic_output(path, value):
    data = json.dumps(value, ensure_ascii=False, separators=(',', ':')).encode('utf-8')
    if len(data) > MAX_OUTPUT:
        fail('output_bound')
    target = pathlib.Path(path)
    parent = no_symlinks(target.parent)
    if target.is_symlink():
        fail('symlink_refused')
    fd, temp = tempfile.mkstemp(prefix='.fogbugz-', dir=parent)
    try:
        os.fchmod(fd, 0o600)
        with os.fdopen(fd, 'wb') as stream:
            stream.write(data)
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(temp, target)
    finally:
        if os.path.exists(temp):
            os.unlink(temp)

def main(argv):
    if len(argv) != 4:
        return 2
    try:
        request = json_object(bounded_read(argv[2], MAX_REQUEST), 'invalid_request')
        result = execute(argv[1], request)
    except BridgeError as error:
        result = {'version': 1, 'ok': False, 'error': str(error)}
    except Exception:
        result = {'version': 1, 'ok': False, 'error': 'adapter_failure'}
    try:
        atomic_output(argv[3], result)
    except Exception:
        return 2
    return 0

if __name__ == '__main__':
    sys.exit(main(sys.argv))
