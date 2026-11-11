#!/usr/bin/env python3
import importlib.util
import json
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('cmp', ROOT / 'scripts' / 'compare-jpa-output.py')
cmp = importlib.util.module_from_spec(spec)
spec.loader.exec_module(cmp)
JPA = ROOT / 'plugins' / 'jpa-assist' / 'parity'
FIXTURES = {'boot-jpa-java-jakarta', 'boot-jpa-kotlin-jakarta', 'legacy-javax-hibernate5', 'envers-flyway-liquibase', 'mapstruct-lombok-dto', 'multimodule-gradle'}


def run(ours, theirs, names, ext='.java'):
    with tempfile.TemporaryDirectory() as t:
        t = Path(t)
        for side, text in (('o', ours), ('t', theirs)):
            (t / side).mkdir()
            if text is not None:
                (t / side / f'f{ext}').write_bytes(text.encode())
        return cmp.compare(t / 'o', t / 't', names)[0]


class Normalizers(unittest.TestCase):
    def same(self, a, b, names, ext='.java'):
        r = run(a, b, names, ext)
        self.assertEqual(r['status'], 'identical', r)

    def test_crlf_and_trailing_newline(self):
        self.same('class A {\r\n}\r\n', 'class A {\n}', [])

    def test_whitespace(self):
        self.same('class A {\n    int  x;\n\n}\n', 'class A {\nint x;\n}\n', ['whitespace'])
        self.assertEqual(run('a  b', 'a b', [])['status'], 'different')

    def test_import_order_and_unused(self):
        a = 'import b.B;\nimport a.A;\nimport u.Unused;\nclass C { A a; B b; }\n'
        b = 'import a.A;\nimport b.B;\nclass C { A a; B b; }\n'
        self.same(a, b, ['imports'])
        self.assertEqual(run(a, b, [])['status'], 'different')

    def test_used_import_difference_is_semantic(self):
        r = run('import a.A;\nclass C { A a; }\n', 'class C { A a; }\n', ['imports'])
        self.assertEqual(r['status'], 'different')

    def test_annotation_arg_order(self):
        self.same('@OneToMany(fetch = LAZY, cascade = {ALL, MERGE})\nclass A {}', '@OneToMany(cascade = {ALL, MERGE}, fetch = LAZY)\nclass A {}', ['annotation-args'])

    def test_annotation_positional_not_reordered(self):
        self.assertEqual(run('@A(1, 2)\nclass X {}', '@A(2, 1)\nclass X {}', ['annotation-args'])['status'], 'different')

    def test_member_order(self):
        a = 'class A {\n int x;\n void f() { }\n}\n'
        b = 'class A {\n void f() { }\n int x;\n}\n'
        self.same(a, b, ['whitespace', 'member-order'])
        self.assertEqual(run(a, b, ['whitespace'])['status'], 'different')

    def test_quote_style(self):
        self.same("name: 'x'\n", 'name: "x"\n', ['quotes'], '.yaml')

    def test_sql_case_whitespace_quoting(self):
        a = 'create table `t` (id bigint not null,  name varchar(5));\n'
        b = 'CREATE TABLE "t" (\n  id BIGINT NOT NULL, name VARCHAR(5)\n);\n'
        self.same(a, b, ['sql'], '.sql')

    def test_sql_statement_order_optional(self):
        a, b = 'create table a (x int);\ncreate table b (y int);\n', 'create table b (y int);\ncreate table a (x int);\n'
        self.assertEqual(run(a, b, ['sql'], '.sql')['status'], 'different')
        self.same(a, b, ['sql', 'sql-statement-order'], '.sql')

    def test_sql_string_literal_case_kept(self):
        self.assertEqual(run("insert into t values ('a');", "insert into t values ('A');", ['sql'], '.sql')['status'], 'different')

    def test_xml_attribute_order(self):
        self.same('<a x="1" y="2"><b/></a>', '<a y="2"\n x="1">\n  <b/>\n</a>', ['xml'], '.xml')
        self.assertEqual(run('<a x="1"/>', '<a x="2"/>', ['xml'], '.xml')['status'], 'different')

    def test_json_key_order(self):
        self.same('{"a":1,"b":{"c":2,"d":3}}', '{\n "b": {"d": 3, "c": 2},\n "a": 1\n}', ['json'], '.json')


class Classification(unittest.TestCase):
    def test_cosmetic_vs_semantic(self):
        cos = run('class A {\n int x;\n int y;\n}\n', 'class A {\n int y;\n int x;\n}\n', [])
        self.assertEqual((cos['status'], cos['classification']), ('different', 'cosmetic'))
        self.assertIn('--- ours/', cos['diff'])
        sem = run('class A { int x; }', 'class A { long x; }', [])
        self.assertEqual(sem['classification'], 'semantic')


class Files(unittest.TestCase):
    def test_missing_files_and_exit_codes(self):
        with tempfile.TemporaryDirectory() as t:
            t = Path(t)
            (t / 'o').mkdir(); (t / 't').mkdir()
            (t / 'o' / 'a.txt').write_text('x\n')
            (t / 'o' / 'same.txt').write_text('s\n')
            (t / 't' / 'same.txt').write_text('s\r\n')
            (t / 't' / 'b.txt').write_text('y\n')
            res = {r['file']: r['status'] for r in cmp.compare(t / 'o', t / 't', [])}
            self.assertEqual(res, {'a.txt': 'only-ours', 'b.txt': 'only-theirs', 'same.txt': 'identical'})
            self.assertEqual(cmp.main(['--ours', str(t / 'o'), '--theirs', str(t / 't')]), 1)
            (t / 'o' / 'a.txt').unlink(); (t / 't' / 'b.txt').unlink()
            self.assertEqual(cmp.main(['--ours', str(t / 'o'), '--theirs', str(t / 't'), '--json']), 0)

    def test_empty_directories_never_pass(self):
        with tempfile.TemporaryDirectory() as t:
            (Path(t) / 'o').mkdir(); (Path(t) / 't').mkdir()
            self.assertEqual(cmp.main(['--ours', t + '/o', '--theirs', t + '/t']), 1)


class Scenarios(unittest.TestCase):
    def test_scenarios_parse_and_reference_real_rows(self):
        ids = {r['id'] for r in json.loads((JPA / 'requirements.json').read_text())['requirements']}
        files = sorted((JPA / 'differential' / 'scenarios').glob('*.json'))
        self.assertTrue(files)
        seen = set()
        for f in files:
            d = json.loads(f.read_text())
            for k in ('id', 'title', 'ledgerRows', 'fixture', 'inputFile', 'ourAction', 'originalAction', 'inputs', 'capture', 'normalize'):
                self.assertIn(k, d, f.name)
            self.assertNotIn(d['id'], seen)
            seen.add(d['id'])
            self.assertTrue(f.name.startswith(d['id']), f.name)
            self.assertIn(d['fixture'], FIXTURES, f.name)
            self.assertFalse(set(d['ledgerRows']) - ids, f.name)
            self.assertFalse(set(d['normalize']) - set(cmp.NORMALIZERS), f.name)

    def test_scenario_fixture_paths_exist(self):
        for f in sorted((JPA / 'differential' / 'scenarios').glob('*.json')):
            d = json.loads(f.read_text())
            root = JPA / 'fixtures' / d['fixture']
            refs = [d['inputFile'], d.get('goldenRef')] + d.get('inputFiles', []) + [s['from'] for s in d.get('swap', [])]
            for ref in filter(None, refs):
                target = JPA / ref if ref.startswith('fixtures/') else root / ref
                self.assertTrue(target.exists(), f'{f.name}: {ref}')

    def test_missing_fixture_path_is_detected(self):
        spec = importlib.util.spec_from_file_location('chk', ROOT / 'scripts' / 'check-jpa-receipts.py')
        chk = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(chk)
        with tempfile.TemporaryDirectory() as t:
            parity = Path(t)
            (parity / 'fixtures' / 'f').mkdir(parents=True)
            (parity / 'fixtures' / 'f' / 'A.java').write_text('x')
            scenarios = parity / 'differential' / 'scenarios'
            scenarios.mkdir(parents=True)
            (scenarios / 'DF-001-a.json').write_text(json.dumps({'fixture': 'f', 'inputFile': 'A.java'}))
            (scenarios / 'DF-002-b.json').write_text(json.dumps({'fixture': 'f', 'inputFile': 'src/Guessed.java'}))
            self.assertEqual(chk.check_fixtures(parity), ['DF-002-b.json: missing fixture path src/Guessed.java'])


if __name__ == '__main__':
    unittest.main()
