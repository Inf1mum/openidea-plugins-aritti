#!/usr/bin/env python3
"""Copy consistency checks for plugin command titles and Rust message strings."""
import re
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

# plugin id suffix -> extra title prefixes it may use besides one dominant prefix
# one structure window bundles three Spring products, each keeping its own prefix
ALLOWED_EXTRA_PREFIXES: dict[str, set[str]] = {
    "spring-integration": {"Spring Batch", "Spring Integration", "Spring Modulith"},
}

WORDS = r"at most|up to|exceeds|than|max|the|Java|Quarkus|provisioned|within|its|most|to|of|about|or|depth|Python"
UNITS = {"KiB", "MiB", "GiB", "TiB", "KB", "MB", "GB", "TB"}
GLUED_WORD = re.compile(rf"\b(?:{WORDS})\d")
GLUED_UNIT = re.compile(r"[A-Za-z]\d+(?:KiB|MiB|bytes|lines|minutes|seconds)\b")
DIGITS_THEN_WORD = re.compile(r"(?<=\s)\d+([A-Za-z]{3,})")


def glued(literal: str) -> bool:
    if GLUED_WORD.search(literal) or GLUED_UNIT.search(literal):
        return True
    if literal.count(" ") < 3:
        return False
    return any(m.group(1) not in UNITS for m in DIGITS_THEN_WORD.finditer(literal))

STRING = re.compile(r'"(?:[^"\\\n]|\\.)*"')

problems: list[str] = []

for toml in sorted(ROOT.glob("plugins/*/plugin.toml")):
    name = toml.parent.name
    commands = tomllib.loads(toml.read_text()).get("contributes", {}).get("commands", [])
    prefixes: dict[str, list[str]] = {}
    for c in commands:
        title = c.get("title", "")
        if "..." in title:
            problems.append(f"{name}: '...' in title {title!r} (use …)")
        if not c.get("category"):
            problems.append(f"{name}: command {c.get('id')!r} has no category")
        prefix = title.split(": ", 1)[0] if ": " in title else "(none)"
        prefixes.setdefault(prefix, []).append(title)
    allowed = ALLOWED_EXTRA_PREFIXES.get(name, set())
    found = {p for p in prefixes if p not in allowed}
    if len(found) > 1:
        listing = "; ".join(f"{p}: {len(t)}" for p, t in prefixes.items())
        problems.append(f"{name}: multiple title prefixes ({listing})")

rust = list(ROOT.glob("plugins/*/src/**/*.rs")) + list(ROOT.glob("crates/ui-kit/src/**/*.rs"))
for path in sorted(rust):
    for n, line in enumerate(path.read_text().splitlines(), 1):
        for lit in STRING.findall(line):
            if glued(lit):
                problems.append(f"{path.relative_to(ROOT)}:{n}: glued digit in {lit[:90]}")

if problems:
    print("check-copy FAIL")
    print("\n".join(problems))
    sys.exit(1)
print("check-copy PASS")
