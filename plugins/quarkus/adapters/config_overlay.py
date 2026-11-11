"""Bounded private snapshots of current unsaved configuration buffers."""
import json
import os
from pathlib import Path
import re
import tempfile
from urllib.parse import unquote, urlparse

MAX_DOCUMENTS = 32
MAX_CHARACTERS = 512 * 1024
MAX_SNAPSHOT = 8 * 1024 * 1024

def bounded_text(text):
    if not isinstance(text, str):
        return False
    try:
        return len(text.encode('utf-16-le')) // 2 <= MAX_CHARACTERS and bool(text.encode('utf-8') or text == '')
    except UnicodeError:
        return False

def utf16_offset(text, position):
    line, character = position.get('line'), position.get('character')
    if not isinstance(line, int) or isinstance(line, bool) or not isinstance(character, int) or isinstance(character, bool) or line < 0 or character < 0:
        raise ValueError('invalid UTF16 position')
    lines = []
    start = 0
    for ending in re.finditer(r'\r\n|\r|\n', text):
        lines.append((start, ending.start()))
        start = ending.end()
    lines.append((start, len(text)))
    if line >= len(lines):
        raise ValueError('line outside current buffer')
    start, end = lines[line]
    units = 0
    for at in range(start, end):
        if units == character:
            return at
        units += 2 if ord(text[at]) > 0xffff else 1
        if units > character:
            raise ValueError('position splits a surrogate pair')
    if units == character:
        return end
    raise ValueError('character outside current line')

def apply_changes(text, changes):
    for change in changes:
        replacement = change.get('text')
        if not isinstance(replacement, str):
            raise ValueError('missing current text')
        span = change.get('range')
        if span is None:
            text = replacement
        else:
            if text is None:
                raise ValueError('current buffer is unknown')
            start, end = utf16_offset(text, span.get('start') or {}), utf16_offset(text, span.get('end') or {})
            if start > end:
                raise ValueError('reversed edit')
            text = text[:start]+replacement+text[end:]
        if not bounded_text(text):
            raise ValueError('current buffer exceeds overlay bound')
    return text

class ConfigOverlay:
    def __init__(self, root, destination):
        self.root = Path(root).resolve()
        self.destination = Path(destination)
        self.generation = 0
        self.documents = {}
        self.dirty = set()
        self.unsupported = set()
        self.write()

    def owned(self, uri):
        parsed = urlparse(uri)
        return parsed.scheme == 'file' and parsed.netloc in ('', 'localhost') and Path(unquote(parsed.path)).resolve().is_relative_to(self.root)

    def event(self, method, params):
        document = params.get('textDocument') or {}
        uri = document.get('uri')
        if not isinstance(uri, str) or not self.owned(uri):
            return False
        if method == 'textDocument/didOpen':
            text = document.get('text')
            known = bounded_text(text) and (uri in self.documents or len(self.documents) < MAX_DOCUMENTS)
            self.documents[uri] = {'uri': uri, 'version': document.get('version'), 'text': text if known else None}
            try:
                path = Path(unquote(urlparse(uri).path))
                clean = known and path.stat().st_size <= MAX_SNAPSHOT and path.read_text(encoding='utf-8') == text
            except (OSError, UnicodeError):
                clean = False
            if not clean:
                self.dirty.add(uri)
        elif method == 'textDocument/didChange':
            old = self.documents.get(uri, {'uri': uri, 'version': -1, 'text': None})
            version = document.get('version')
            try:
                if not isinstance(version, int) or isinstance(version, bool) or version <= old['version']:
                    raise ValueError('noncurrent buffer version')
                text = apply_changes(old['text'], params.get('contentChanges') or [])
            except (ValueError, TypeError):
                text = None
            self.documents[uri] = {'uri': uri, 'version': version, 'text': text}
            self.dirty.add(uri)
        elif method == 'textDocument/didSave':
            self.dirty.discard(uri)
        elif method == 'textDocument/didClose':
            self.dirty.discard(uri)
            self.documents.pop(uri, None)
        else:
            return False
        self.generation += 1
        self.write()
        return True

    def write(self):
        selected = []
        unsupported = set()
        seen = {}
        for uri in sorted(self.dirty):
            entry = self.documents.get(uri)
            if not entry or not isinstance(entry.get('text'), str) or not isinstance(entry.get('version'), int):
                unsupported.add(uri)
                continue
            canonical = str(Path(unquote(urlparse(uri).path)).resolve())
            if canonical in seen:
                unsupported.update((uri, seen[canonical]))
                continue
            seen[canonical] = uri
            if len(selected) >= MAX_DOCUMENTS:
                unsupported.add(uri)
                continue
            selected.append(entry)
        selected = [entry for entry in selected if entry['uri'] not in unsupported]
        payload = {'version': 1, 'rootUri': self.root.as_uri(), 'generation': self.generation, 'documents': selected}
        body = json.dumps(payload, ensure_ascii=False).encode('utf-8')
        if len(body) > MAX_SNAPSHOT:
            unsupported.update(self.dirty)
            payload['documents'] = []
            body = json.dumps(payload).encode('utf-8')
        self.unsupported = unsupported
        self.destination.parent.mkdir(parents=True, exist_ok=True)
        temporary = None
        try:
            with tempfile.NamedTemporaryFile(prefix='.overlay-', dir=self.destination.parent, delete=False) as stream:
                temporary = Path(stream.name)
                os.chmod(temporary, 0o600)
                stream.write(body)
                stream.flush()
            os.replace(temporary, self.destination)
        finally:
            if temporary is not None and temporary.exists():
                temporary.unlink()
