"""CI wiring regressions. These do not execute MySQL or prove its semantics."""
from pathlib import Path
import re
import unittest

ROOT = Path(__file__).resolve().parents[2]
NEW = ROOT / '.github/workflows/swarm-e-claims.yml'
OLD = ROOT / '.github/workflows/swarm-e-claims-bootstrap.yml'


class WorkflowWiring(unittest.TestCase):
    def source(self):
        return (NEW if NEW.exists() else OLD).read_text()

    def test_ordinary_pr_and_manual_events(self):
        text = self.source()
        self.assertIn('pull_request:', text)
        self.assertIn('workflow_dispatch:', text)
        self.assertNotIn('pull_request_target:', text)

    def test_no_writer_credentials_or_source_assembly(self):
        text = self.source()
        for forbidden in ('contents: write', 'GH_TOKEN', 'git push', 'git commit',
                          'git apply', 'generate-lockfile', 'github.token'):
            self.assertNotIn(forbidden, text)
        self.assertIn('persist-credentials: false', text)

    def test_both_engines_have_independent_jobs(self):
        text = self.source()
        self.assertIn('image: mysql:8.4', text)
        self.assertIn('image: mariadb:11.4', text)
        self.assertIn('fail-fast: false', text)

    def test_actual_database_runner_is_required(self):
        self.assertIn('python3 tools/swarm-e/run_claims.py', self.source())
        self.assertNotIn('continue-on-error:', self.source())

    def test_source_and_failure_evidence_are_retained(self):
        text = self.source()
        self.assertIn('if: always()', text)
        self.assertIn('if-no-files-found: error', text)
        self.assertIn('git diff --exit-code', text)
        self.assertIn('image-identity.json', text)
        self.assertIn('server-identity.tsv', text)

    def test_tests_cannot_target_external_database(self):
        text = self.source()
        self.assertIn('127.0.0.1::3306', text)
        self.assertIn('wonderland_claimstest', text)
        self.assertIn('WONDERLAND_CLAIMS_DISPOSABLE: "1"', text)

    def test_action_revisions_and_lifetime_are_bounded(self):
        text = self.source()
        actions = re.findall(r'uses:\s+(\S+)', text)
        self.assertTrue(actions)
        self.assertTrue(all(re.fullmatch(r'[^@\s]+@[a-f0-9]{40}', a) for a in actions))
        self.assertIn('timeout-minutes:', text)


if __name__ == '__main__':
    unittest.main()
