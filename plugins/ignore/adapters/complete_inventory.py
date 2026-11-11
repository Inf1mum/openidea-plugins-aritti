"""Isolated, bounded complete directory inventory. No application imports or Git execution."""
import hashlib
import json
import os
import stat
import sys
import tempfile
import time

MAX_ENTRIES = 4096
MAX_DIRECTORIES = 512
MAX_DEPTH = 24
MAX_BYTES = 1 << 20
DEADLINE = time.monotonic() + 8


def scan(root):
    entries = []
    pending = [(root, "", 0)]
    directories = 0
    while pending:
        if time.monotonic() > DEADLINE:
            raise ValueError("inventory deadline")
        path, relative, depth = pending.pop()
        if depth > MAX_DEPTH:
            raise ValueError("inventory depth")
        directories += 1
        if directories > MAX_DIRECTORIES:
            raise ValueError("inventory directory count")
        # Iterate without suppressing entry/permission errors. Never follow symlinks.
        with os.scandir(path) as listing:
            children = []
            for entry in listing:
                children.append(entry)
                if len(children) + len(entries) > MAX_ENTRIES:
                    raise ValueError("inventory entry count")
            children.sort(key=lambda entry: entry.name)
        for entry in children:
            name = entry.name
            name.encode("utf-8", errors="strict")
            if not name or name in (".", "..") or any(ord(c) < 32 for c in name) or "\\" in name:
                raise ValueError("unsupported filesystem name")
            rel = relative + "/" + name if relative else name
            if len(rel.encode("utf-8")) > 1024:
                raise ValueError("inventory path bytes")
            metadata = entry.stat(follow_symlinks=False)
            if stat.S_ISDIR(metadata.st_mode):
                kind = "directory"
                pending.append((entry.path, rel, depth + 1))
            elif stat.S_ISREG(metadata.st_mode):
                kind = "file"
            elif stat.S_ISLNK(metadata.st_mode):
                kind = "symlink"
            else:
                raise ValueError("unsupported filesystem kind")
            entries.append([rel, kind, metadata.st_size, metadata.st_mtime_ns, metadata.st_ino, metadata.st_dev])
            if len(entries) > MAX_ENTRIES:
                raise ValueError("inventory entry count")
    entries.sort()
    return entries


def main():
    if len(sys.argv) != 3:
        raise ValueError("inventory arguments")
    root, output = sys.argv[1:]
    if not os.path.isabs(root) or os.path.realpath(root) != root or not stat.S_ISDIR(os.lstat(root).st_mode):
        raise ValueError("canonical ordinary source directory required")
    if os.path.commonpath([root, os.path.abspath(output)]) == root:
        raise ValueError("output must be outside scanned directory")
    first = scan(root)
    if first != scan(root):
        raise ValueError("filesystem changed during capture")
    fingerprint = hashlib.sha256(json.dumps(first, ensure_ascii=False, separators=(",", ":")).encode()).hexdigest()
    result = {"schema": "ignore-complete-v1", "root": root, "complete": True,
              "count": len(first), "fingerprint": fingerprint,
              "entries": [[entry[0], entry[1]] for entry in first]}
    encoded = json.dumps(result, ensure_ascii=False, separators=(",", ":")).encode()
    if len(encoded) > MAX_BYTES:
        raise ValueError("inventory output bytes")
    directory = os.path.dirname(output)
    handle, temporary = tempfile.mkstemp(prefix=".inventory-", dir=directory)
    try:
        with os.fdopen(handle, "wb") as target:
            target.write(encoded)
            target.flush()
            os.fsync(target.fileno())
        os.replace(temporary, output)
    finally:
        if os.path.exists(temporary):
            os.unlink(temporary)


if __name__ == "__main__":
    try:
        main()
    except Exception:
        # Filenames and filesystem error details stay local; plugin shows a bounded constant reason.
        sys.stderr.write("Complete inventory unavailable: error, bounds or concurrent change.\n")
        sys.exit(1)
