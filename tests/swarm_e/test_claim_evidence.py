"""Authored validator counterexamples, not executed database evidence."""
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'tools/swarm-e'))
from claim_evidence import UNIT_TESTS, SQL_TESTS, database_identity, disposable_url, parse_tests, validate_command


def log(names):
    return '\n'.join(f'test {name} ... ok' for name in sorted(names)) + (
        f'\n\ntest result: ok. {len(names)} passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.00s\n')


class TestEvidence(unittest.TestCase):
    def test_all_original_named_tests_required(self):
        self.assertEqual(len(parse_tests(log(UNIT_TESTS), UNIT_TESTS)), 5)
        self.assertEqual(len(parse_tests(log(SQL_TESTS), SQL_TESTS)), 11)

    def test_compile_only_log_cannot_certify_database(self):
        with self.assertRaises(ValueError):
            parse_tests('Checking wonderland-persistence\nFinished dev profile\n', SQL_TESTS)

    def test_unit_success_cannot_certify_database(self):
        with self.assertRaises(ValueError):
            parse_tests(log(UNIT_TESTS), SQL_TESTS)

    def test_empty_partial_missing_and_duplicate_logs_fail(self):
        complete = log(SQL_TESTS)
        one = sorted(SQL_TESTS)[0]
        for invalid in ('', complete[:40], complete.replace(f'test {one} ... ok\n', ''),
                        complete + f'test {one} ... ok\n', complete + complete):
            with self.subTest(invalid=invalid[:35]), self.assertRaises(ValueError):
                parse_tests(invalid, SQL_TESTS)

    def test_correct_count_with_wrong_names_fails(self):
        with self.assertRaises(ValueError):
            parse_tests(log({f'not_a_claim_test_{i}' for i in range(11)}), SQL_TESTS)

    def test_ignored_filtered_and_failed_tests_fail(self):
        for invalid in (log(SQL_TESTS).replace('0 ignored', '1 ignored'),
                        log(SQL_TESTS).replace('0 filtered', '1 filtered'),
                        log(SQL_TESTS).replace('0 measured', '1 measured'),
                        log(SQL_TESTS).replace('... ok', '... FAILED', 1),
                        log(SQL_TESTS).replace('... ok', '... ignored', 1),
                        log(SQL_TESTS).replace('result: ok', 'result: FAILED')):
            with self.subTest(invalid=invalid[-120:]), self.assertRaises(ValueError):
                parse_tests(invalid, SQL_TESTS)

    def test_logs_are_bounded(self):
        with self.assertRaises(ValueError):
            parse_tests('x' * (4 * 1024 * 1024 + 1), SQL_TESTS)

    def test_command_exit_is_required_even_with_good_output(self):
        good = {'name': 'database', 'exit_code': 0, 'timed_out': False}
        validate_command(good, 'database')
        for changes in ({'exit_code': 1}, {'exit_code': False}, {'exit_code': None},
                        {'timed_out': True}, {'timed_out': None}, {'name': 'unit'}):
            with self.subTest(changes=changes), self.assertRaises(ValueError):
                validate_command({**good, **changes}, 'database')

    def test_only_opted_in_loopback_disposable_database(self):
        good = 'mysql://test:fixture-password@127.0.0.1:33067/wonderland_claimstest'
        self.assertEqual(disposable_url(good, '1'), 'wonderland_claimstest')
        for url, consent in ((None, '1'), (good, None),
                             (good.replace('127.0.0.1', 'db.example.com'), '1'),
                             (good.replace('wonderland_claimstest', 'production'), '1'),
                             (good.replace(':33067', ':invalid'), '1'),
                             (good + '?socket=/tmp/mysql.sock', '1'),
                             (good + '#fragment', '1'),
                             (good.replace(':33067', ''), '1')):
            with self.subTest(url=url, consent=consent), self.assertRaises(ValueError):
                disposable_url(url, consent)

    def test_server_and_image_family_must_agree(self):
        for engine, version in [('mysql', '8.4.1'), ('mariadb', '11.4.2-MariaDB')]:
            image = {'Id': 'sha256:' + 'a' * 64, 'RepoDigests': [engine + '@sha256:' + 'b' * 64]}
            identity = f'{version}\twonderland_claimstest\tInnoDB\n'
            self.assertEqual(database_identity(engine, identity, image, 'wonderland_claimstest')['version'], version)
            invalid_cases = [(identity.replace('InnoDB', 'MyISAM'), image),
                             (identity.replace('wonderland_claimstest', 'wrong'), image),
                             (identity + identity, image), (identity, {}),
                             (identity, {**image, 'RepoDigests': []}),
                             (identity, {**image, 'RepoDigests': ['other@sha256:' + 'b' * 64]})]
            for invalid, other in invalid_cases:
                with self.subTest(engine=engine, invalid=invalid), self.assertRaises(ValueError):
                    database_identity(engine, invalid, other, 'wonderland_claimstest')
            other_engine = 'mariadb' if engine == 'mysql' else 'mysql'
            with self.assertRaises(ValueError):
                database_identity(other_engine, identity, image, 'wonderland_claimstest')

    def test_missing_database_exits_before_any_compiler_or_database_command(self):
        with tempfile.TemporaryDirectory() as tmp:
            env = os.environ.copy()
            env.pop('DATABASE_URL', None)
            env.pop('WONDERLAND_CLAIMS_DISPOSABLE', None)
            output = Path(tmp) / 'new'
            result = subprocess.run([sys.executable, str(ROOT / 'tools/swarm-e/run_claims.py'),
                                     '--engine', 'mysql', '--output', str(output),
                                     '--server-identity', '/does/not/exist', '--image-identity', '/does/not/exist'],
                                    env=env, capture_output=True, text=True)
            self.assertEqual(result.returncode, 1)
            import json
            report = json.loads((output / 'report.json').read_text())
            self.assertFalse(report['passed'])
            self.assertEqual(report['commands'], [])
            self.assertIn('opt-in', report['failure'])

    def test_old_success_report_cannot_be_reused(self):
        with tempfile.TemporaryDirectory() as tmp:
            old = Path(tmp) / 'report.json'
            old.write_text('{"passed":true}\n')
            result = subprocess.run([sys.executable, str(ROOT / 'tools/swarm-e/run_claims.py'),
                                     '--engine', 'mysql', '--output', tmp,
                                     '--server-identity', '/none', '--image-identity', '/none'],
                                    capture_output=True, text=True)
            self.assertEqual(result.returncode, 2)
            self.assertEqual(old.read_text(), '{"passed":true}\n')


class TestRealCommandCapture(unittest.TestCase):
    def test_failed_command_is_retained_and_never_certified(self):
        from run_claims import execute
        with tempfile.TemporaryDirectory() as tmp:
            records = []
            with self.assertRaises(ValueError):
                execute('database', [sys.executable, '-c', "print('looks successful');raise SystemExit(3)"],
                        Path(tmp), records)
            self.assertEqual(records[0]['exit_code'], 3)
            self.assertFalse(records[0]['timed_out'])
            self.assertIn('looks successful', (Path(tmp) / 'database.log').read_text())

    def test_timeout_is_a_failure_with_a_real_exit_record(self):
        from run_claims import execute
        with tempfile.TemporaryDirectory() as tmp:
            records = []
            with self.assertRaises(ValueError):
                execute('database', [sys.executable, '-c', 'import time;time.sleep(30)'],
                        Path(tmp), records, timeout=0.1)
            self.assertEqual(records[0]['exit_code'], 124)
            self.assertTrue(records[0]['timed_out'])

    def test_successful_capture_checks_actual_log_hash(self):
        from run_claims import execute
        import hashlib
        with tempfile.TemporaryDirectory() as tmp:
            records = []
            text = execute('compiler', [sys.executable, '-c', "print('actual subprocess')"],
                           Path(tmp), records)
            self.assertEqual(text, 'actual subprocess\n')
            self.assertEqual(records[0]['exit_code'], 0)
            self.assertEqual(records[0]['log_sha256'], hashlib.sha256(text.encode()).hexdigest())


if __name__ == '__main__':
    unittest.main()
