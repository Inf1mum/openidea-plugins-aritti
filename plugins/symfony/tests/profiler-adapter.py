#!/usr/bin/env python3
"""Explicit integration tests: run with Python 3 and an installed PHP 8+ CLI."""
import gzip
import json
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

PHP = shutil.which('php')
ADAPTER = (Path(__file__).resolve().parents[1] / 'src/profiler_decode.php').read_text()

@unittest.skipUnless(PHP, 'PHP CLI is required for the explicit profiler adapter integration')
class ProfilerAdapter(unittest.TestCase):
    def setUp(self):
        self.scratch = tempfile.TemporaryDirectory(prefix='symfony-profile-adapter-')
        self.root = Path(self.scratch.name)
        self.path = self.root / 'abcdef'
        generator = '''class Collector { protected $data=['duration'=>12.5,'memory'=>2048,'password'=>'secret','query_count'=>2,'logs'=>[['message'=>'secret SQL'],['message'=>'secret token']]]; }
        echo serialize(['token'=>'abcdef','data'=>['time'=>new Collector]]);'''
        self.raw = subprocess.check_output([PHP, '-n', '-r', generator])

    def tearDown(self):
        self.scratch.cleanup()

    def run_adapter(self, token='abcdef', path=None):
        return subprocess.run([PHP, '-n', '-d', 'memory_limit=64M', '-d', 'max_execution_time=2', '-r', ADAPTER, '--', str(self.root), str(path or self.path), token], capture_output=True, timeout=5)

    def test_raw_and_gzip_only_expose_allowlisted_numeric_metrics(self):
        for value in [self.raw, gzip.compress(self.raw)]:
            self.path.write_bytes(value)
            result = self.run_adapter()
            self.assertEqual(result.returncode, 0, result.stderr)
            summary = json.loads(result.stdout)
            self.assertEqual(summary['collectors'][0]['metrics'], {'duration':12.5, 'memory':2048, 'query_count':2,'stored_logs':2})
            self.assertNotIn(b'password', result.stdout)
            self.assertNotIn(b'secret', result.stdout)

    def test_malformed_wrong_token_and_decompression_limit_are_refused(self):
        for raw in [b'not a profile', gzip.compress(b'x'*1048577), b'x'*1048577]:
            self.path.write_bytes(raw)
            self.assertNotEqual(self.run_adapter().returncode, 0)
        self.path.write_bytes(self.raw)
        self.assertNotEqual(self.run_adapter('123abc').returncode, 0)

    def test_symlink_escape_is_refused_before_reading(self):
        with tempfile.TemporaryDirectory(prefix='foreign-profile-') as other:
            foreign = Path(other)/'abcdef'
            foreign.write_bytes(self.raw)
            self.path.symlink_to(foreign)
            self.assertNotEqual(self.run_adapter().returncode, 0)

if __name__ == '__main__':
    unittest.main()
