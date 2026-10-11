#!/usr/bin/env python3
"""Fail-closed evidence checks for W15.1; no database or repository writes."""
from __future__ import annotations
import re
from urllib.parse import unquote, urlsplit

UNIT_TESTS = frozenset({
    'policy::tests::exact_current_live_credential_only', 'policy::tests::epoch_never_wraps',
    'tests::invalid_ids_owners_and_leases_fail_without_io',
    'tests::database_widths_and_namespaces_are_distinct', 'tests::deadlines_never_wrap',
})
SQL_TESTS = frozenset({
    'mapping_and_retained_epoch', 'competing_acquisitions_have_one_winner',
    'transfer_fences_old_credentials_and_renew_preserves_epoch',
    'expiry_cannot_be_renewed_and_reacquire_advances_epoch',
    'callback_failure_rolls_back_business_write',
    'current_fenced_write_commits_and_new_repository_observes_claim',
    'handoff_waits_for_fenced_transaction_then_rejects_old_writer',
    'expiry_during_callback_rolls_back_before_commit',
    'database_time_is_read_after_waiting_for_row_lock',
    'exhausted_epoch_fails_without_mutating_record', 'refuses_non_transactional_schema',
})
SUMMARY = re.compile(
    r'^test result: (ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored; '
    r'(\d+) measured; (\d+) filtered out; finished in [0-9.]+s$', re.M)
CASE = re.compile(r'^test ([A-Za-z0-9_:]+) \.\.\. (ok|FAILED|ignored(?:,.*)?)$', re.M)


def parse_tests(log: str, expected: frozenset[str]) -> list[str]:
    """Require every named test once, one summary, and no skipped/filtered tests."""
    if not isinstance(log, str) or len(log) > 4 * 1024 * 1024:
        raise ValueError('Missing or oversized test log')
    summaries = SUMMARY.findall(log)
    if len(summaries) != 1:
        raise ValueError('Exactly one complete test summary is required')
    status, passed, failed, ignored, measured, filtered = summaries[0]
    if status != 'ok' or (int(passed), int(failed), int(ignored), int(measured), int(filtered)) != (len(expected), 0, 0, 0, 0):
        raise ValueError('Missing, failed, ignored, measured or filtered tests')
    cases = CASE.findall(log)
    names = [name for name, _ in cases]
    if len(names) != len(set(names)) or set(names) != expected:
        raise ValueError('Named test set is incomplete, duplicated or unexpected')
    if any(status != 'ok' for _, status in cases):
        raise ValueError('A named test did not pass')
    return sorted(names)


def disposable_url(url: str | None, consent: str | None) -> str:
    """The runner is deliberately restricted to a loopback disposable database."""
    if consent != '1' or not isinstance(url, str) or not url:
        raise ValueError('Explicit disposable-database opt-in and DATABASE_URL are required')
    try:
        parsed = urlsplit(url)
        port = parsed.port
    except ValueError:
        raise ValueError('Invalid disposable database configuration') from None
    database = unquote(parsed.path.removeprefix('/'))
    if (parsed.scheme != 'mysql' or parsed.hostname not in {'127.0.0.1', 'localhost', '::1'}
            or not parsed.username or not parsed.password or not port
            or parsed.query or parsed.fragment
            or not re.fullmatch(r'[A-Za-z0-9_]{1,48}_claimstest', database)):
        raise ValueError('Only an explicitly ported loopback *_claimstest database is allowed')
    return database


def database_identity(engine: str, text: str, image: dict, expected_database: str) -> dict:
    if not isinstance(text, str) or len(text) > 4096 or not isinstance(image, dict):
        raise ValueError('Invalid database identity record')
    rows = text.strip().splitlines()
    if len(rows) != 1 or len(rows[0].split('\t')) != 3:
        raise ValueError('Missing or ambiguous actual database identity')
    version, database, storage = rows[0].split('\t')
    if database != expected_database or storage != 'InnoDB':
        raise ValueError('Wrong database or transactional engine')
    if engine == 'mysql':
        version_ok = version.startswith('8.4.') and 'mariadb' not in version.lower()
    elif engine == 'mariadb':
        version_ok = version.startswith('11.4.') and 'mariadb' in version.lower()
    else:
        version_ok = False
    if not version_ok:
        raise ValueError('Server version does not match the selected database family')
    image_id = image.get('Id')
    digests = image.get('RepoDigests')
    if (not isinstance(image_id, str) or not re.fullmatch(r'sha256:[a-f0-9]{64}', image_id)
            or not isinstance(digests, list) or not digests
            or not all(isinstance(d, str) and re.fullmatch(
                rf'(?:docker\.io/library/)?{engine}@sha256:[a-f0-9]{{64}}', d) for d in digests)):
        raise ValueError('Missing or mismatched resolved container identity')
    return {'engine': engine, 'version': version, 'database': database, 'storage': storage,
            'image_id': image_id, 'repo_digests': digests}


def validate_command(record: dict, expected_name: str) -> None:
    if record.get('name') != expected_name or type(record.get('exit_code')) is not int or record['exit_code'] != 0:
        raise ValueError(f'Required command did not succeed: {expected_name}')
    if record.get('timed_out') is not False:
        raise ValueError(f'Missing timeout outcome: {expected_name}')
