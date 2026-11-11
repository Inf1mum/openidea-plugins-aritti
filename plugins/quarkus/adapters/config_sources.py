"""Classification and flattening of Quarkus configuration buffers into Properties text.

Ordinals verified against SmallRye Config source: .env 295 (DotEnvConfigSourceProvider), config/application.properties 260,
classpath application.properties 250, microprofile-config.properties 100 (SmallRyeConfigBuilder), YAML classpath 255 and
config/application.yaml 265 (YamlConfigSourceLoader.InClassPath / InFileSystem).
"""
import re
from pathlib import Path

ENV_ORDINAL = 295

def classify(path):
    """Return {format, ordinal, profile, name} for a recognised configuration file, else None."""
    path = Path(path)
    name = path.name
    external = path.parent.name == 'config'
    if name == '.env':
        return {'format': 'env', 'ordinal': ENV_ORDINAL, 'profile': None, 'name': name}
    if name == 'microprofile-config.properties':
        return {'format': 'properties', 'ordinal': 100, 'profile': None, 'name': name}
    match = re.fullmatch(r'application(?:-([^./\\]+))?\.(properties|yaml|yml)', name)
    if not match:
        return None
    profile, extension = match.groups()
    if extension == 'properties':
        return {'format': 'properties', 'ordinal': 260 if external else 250, 'profile': profile, 'name': name}
    return {'format': 'yaml', 'ordinal': 265 if external else 255, 'profile': profile, 'name': name}

def escape_key(key):
    return re.sub(r'([\\ =:#!])', r'\\\1', key)

def escape_value(value):
    value = value.replace('\\', '\\\\').replace('\n', '\\n').replace('\r', '\\r')
    return '\\' + value if value.startswith(' ') else value

def properties_text(pairs):
    return ''.join(f'{escape_key(key)}={escape_value(value)}\n' for key, value in pairs)

def parse_env(text):
    pairs = []
    for number, raw in enumerate(text.splitlines(), 1):
        line = raw.strip()
        if not line or line.startswith('#'):
            continue
        if line.startswith('export '):
            line = line[7:].lstrip()
        key, equals, value = line.partition('=')
        key, value = key.strip(), value.strip()
        if not equals or not re.fullmatch(r'[A-Za-z_][A-Za-z0-9_]*', key):
            raise ValueError(f'.env line {number}: unsupported entry')
        if value[:1] in ('"', "'"):
            if len(value) < 2 or value[-1] != value[0]:
                raise ValueError(f'.env line {number}: unterminated or multi-line quote')
            double = value[0] == '"'
            value = value[1:-1]
            if double:
                value = value.replace('\\n', '\n').replace('\\"', '"').replace('\\\\', '\\')
        else:
            value = re.sub(r'\s+#.*$', '', value)
        pairs.append((key, value))
    return pairs

_REFUSED_VALUE_START = ('&', '*', '!', '|', '>', '{', '[', '@', '`', '%')

def _unquote(text, number):
    quote = text[0]
    out, i = [], 1
    while i < len(text):
        ch = text[i]
        if quote == "'" and ch == "'":
            if text[i + 1:i + 2] == "'":
                out.append("'")
                i += 2
                continue
            return ''.join(out), text[i + 1:]
        if quote == '"' and ch == '"':
            return ''.join(out), text[i + 1:]
        if quote == '"' and ch == '\\':
            i += 1
            esc = text[i:i + 1]
            mapped = {'n': '\n', 't': '\t', 'r': '\r', '"': '"', '\\': '\\', '/': '/', ' ': ' '}.get(esc)
            if mapped is None:
                raise ValueError(f'line {number}: unsupported escape')
            out.append(mapped)
            i += 1
            continue
        out.append(ch)
        i += 1
    raise ValueError(f'line {number}: unterminated quoted scalar')

def _split_key(content, number):
    """Return (key_segment, quoted, rest_after_colon)."""
    if content[0] in ('"', "'"):
        key, rest = _unquote(content, number)
        rest = rest.lstrip()
        if not rest.startswith(':'):
            raise ValueError(f'line {number}: expected mapping')
        return key, True, rest[1:]
    if content[0] in '&*!?|>{[@`%,#' or content.startswith('<<'):
        raise ValueError(f'line {number}: unsupported YAML key syntax')
    match = re.search(r':(?:\s|$)', content)
    if not match:
        raise ValueError(f'line {number}: expected mapping')
    return content[:match.start()].strip(), False, content[match.start() + 1:]

def _scalar(rest, number):
    rest = rest.strip()
    if rest[:1] in ('"', "'"):
        value, tail = _unquote(rest, number)
        if tail.strip() and not tail.strip().startswith('#'):
            raise ValueError(f'line {number}: trailing text after quoted scalar')
        return value
    rest = re.sub(r'\s+#.*$', '', rest)
    if rest.startswith('#'):
        return ''
    if rest[:1] in _REFUSED_VALUE_START or rest.startswith('? ') or rest.startswith('- '):
        raise ValueError(f'line {number}: anchors, aliases, tags, flow and block scalars are not supported')
    return rest

def flatten_yaml(text):
    """Flatten a single-document literal block YAML mapping; raise ValueError for anything else."""
    lines = []
    seen_content = False
    for number, raw in enumerate(text.splitlines(), 1):
        if raw.startswith('---') or raw.startswith('...'):
            if raw.rstrip() in ('---', '...') and not seen_content and raw.startswith('---'):
                continue
            raise ValueError(f'line {number}: multi-document YAML is not supported')
        stripped = raw.strip()
        if not stripped or stripped.startswith('#'):
            continue
        indent = len(raw) - len(raw.lstrip(' '))
        if '\t' in raw[:indent + 1]:
            raise ValueError(f'line {number}: tab indentation is not allowed')
        seen_content = True
        lines.append([indent, raw.strip(), number])
    pairs = []
    pos = [0]

    def block(indent, path, depth):
        if pos[0] >= len(lines):
            return
        first = lines[pos[0]]
        if first[1] == '-' or first[1].startswith('- '):
            sequence(indent, path, depth)
        else:
            mapping(indent, path, depth)

    def child(indent, number):
        """Indent of a nested block following a key with no inline value, or None."""
        if pos[0] >= len(lines):
            return None
        nxt = lines[pos[0]]
        if nxt[0] > indent:
            return nxt[0]
        if nxt[0] == indent and (nxt[1] == '-' or nxt[1].startswith('- ')):
            return nxt[0]
        return None

    def reject_continuation(indent):
        if pos[0] < len(lines) and lines[pos[0]][0] > indent:
            raise ValueError(f'line {lines[pos[0]][2]}: multi-line scalars or bad indentation are not supported')

    def mapping(indent, path, depth):
        keys = set()
        while pos[0] < len(lines):
            current = lines[pos[0]]
            if current[0] < indent:
                return
            if current[0] > indent:
                raise ValueError(f'line {current[2]}: inconsistent indentation')
            if current[1] == '-' or current[1].startswith('- '):
                raise ValueError(f'line {current[2]}: sequence where mapping expected')
            key, quoted, rest = _split_key(current[1], current[2])
            if key in keys:
                raise ValueError(f'line {current[2]}: duplicate key')
            keys.add(key)
            pos[0] += 1
            if key == '~' and not quoted:
                segment = None
            elif quoted and depth == 0 and key.startswith('%'):
                segment = key
            elif quoted:
                segment = f'"{key}"'
            else:
                if not key:
                    raise ValueError(f'line {current[2]}: empty key')
                segment = key
            sub = path if segment is None else (f'{path}.{segment}' if path else segment)
            if rest.strip() == '' or rest.strip().startswith('#'):
                nested = child(indent, current[2])
                if nested is None:
                    if sub:
                        pairs.append((sub, ''))
                else:
                    block(nested, sub, depth + 1)
            else:
                value = _scalar(rest, current[2])
                reject_continuation(indent)
                if value not in ('~', 'null', 'Null', 'NULL') and sub:
                    pairs.append((sub, value))

    def sequence(indent, path, depth):
        index = 0
        while pos[0] < len(lines):
            current = lines[pos[0]]
            if current[0] < indent:
                return
            if current[0] != indent or not (current[1] == '-' or current[1].startswith('- ')):
                raise ValueError(f'line {current[2]}: inconsistent list indentation')
            item = f'{path}[{index}]'
            index += 1
            body = current[1][1:].lstrip()
            if not body:
                pos[0] += 1
                nested = child(indent, current[2])
                if nested is not None and nested > indent:
                    block(nested, item, depth + 1)
                continue
            offset = len(current[1]) - len(body)
            if body[0] in ('"', "'"):
                try:
                    _, tail = _unquote(body, current[2])
                    is_key = tail.lstrip().startswith(':')
                except ValueError:
                    is_key = False
            else:
                is_key = bool(re.search(r':(?:\s|$)', body)) and body[0] not in '&*!{[|>'
            if is_key:
                lines[pos[0]] = [indent + offset, body, current[2]]
                mapping(indent + offset, item, depth + 1)
            else:
                pos[0] += 1
                value = _scalar(body, current[2])
                reject_continuation(indent)
                pairs.append((item, value))

    if lines:
        if lines[0][0] != 0:
            raise ValueError(f'line {lines[0][2]}: top level must start at column 0')
        block(0, '', 0)
        if pos[0] < len(lines):
            raise ValueError(f'line {lines[pos[0]][2]}: unexpected content')
    return pairs

def snapshot_fields(path, text):
    """Return {format, ordinal, profile, name, text} for the overlay snapshot; ValueError when refused."""
    info = classify(path)
    if info is None:
        raise ValueError('not a recognised configuration source')
    if info['format'] == 'yaml':
        text = properties_text(flatten_yaml(text))
    elif info['format'] == 'env':
        text = properties_text(parse_env(text))
    return {**info, 'text': text}
