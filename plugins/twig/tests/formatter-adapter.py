#!/usr/bin/env python3
"""Run explicitly with TWIG_CS_FIXER_PHAR pointing to the official 4.1.1 release."""
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

PHP = shutil.which('php')
PHAR = os.environ.get('TWIG_CS_FIXER_PHAR')
SHA = '4d099a3be74ea41b0c8f1b2a7033c53a77e7d0dfba36725c2bbaaa19453ed2e5'
ADAPTER = (Path(__file__).resolve().parents[1]/'src/formatter.php').read_text()

@unittest.skipUnless(PHP and PHAR, 'Set TWIG_CS_FIXER_PHAR and install PHP for the explicit formatter integration')
class FormatterAdapter(unittest.TestCase):
    def setUp(self):
        self.scratch = tempfile.TemporaryDirectory(prefix='twig-formatter-review-')
        self.root = Path(self.scratch.name)
        self.original = self.root/'view.twig'
        self.original.write_text('{{ untouched }}')
        (self.root/'.twig-cs-fixer.php').write_text("<?php file_put_contents(__DIR__.'/executed', 'bad'); throw new Exception('must not load config');")

    def tearDown(self):
        self.scratch.cleanup()

    def run_adapter(self, source, sha=SHA):
        return subprocess.run([PHP, '-n', '-d', 'memory_limit=128M', '-d', 'max_execution_time=2', '-r', ADAPTER, '--', PHAR, sha], input=json.dumps({'source':source}), cwd=self.root, capture_output=True, text=True, timeout=5)

    def test_reviewed_tool_formats_in_memory_and_never_loads_project_configuration(self):
        self.assertEqual(hashlib.sha256(Path(PHAR).read_bytes()).hexdigest(), SHA)
        fixtures = [('😀 {{foo|upper}}\r\n', '😀 {{ foo|upper }}\r\n'), ('{%macro badge(label="x")%}{{label}}{%endmacro%}', '{% macro badge(label = "x") %}{{ label }}{% endmacro %}'), ('<div>  text </div>\n{{ f(a:[1,2]) }}', '<div>  text </div>\n{{ f(a: [1, 2]) }}')]
        before = set(self.root.iterdir())
        for source, expected in fixtures:
            result = self.run_adapter(source)
            self.assertEqual(result.returncode, 0, result.stderr)
            payload = json.loads(result.stdout)
            self.assertEqual(payload['source'], source)
            self.assertEqual(payload['result'], expected)
            self.assertEqual(json.loads(self.run_adapter(expected).stdout)['result'], expected)
        self.assertEqual(set(self.root.iterdir()), before)
        self.assertEqual(self.original.read_text(), '{{ untouched }}')

    def test_changed_tool_and_malformed_input_are_refused(self):
        self.assertNotEqual(self.run_adapter('{{foo}}', '0'*64).returncode, 0)
        self.assertNotEqual(self.run_adapter('{{foo(').returncode, 0)
        self.assertFalse((self.root/'executed').exists())

if __name__ == '__main__':
    unittest.main()
