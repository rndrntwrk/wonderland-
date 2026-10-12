# Full-game preservation ledger

Run from the repository root with Python 3 and `jsonschema==4.26.0`:

```sh
python3 tools/completion/verify-ledger.py --reference 4c6b3e8f5835b228723caea3c9f683c62f244f73 --mode inventory
python3 tools/completion/verify-ledger.py --reference 4c6b3e8f5835b228723caea3c9f683c62f244f73 --mode release
```

This first increment binds the 21 player surfaces, 30 registered EOD handlers and
20 concrete source obligations to existing source files and current SHA-256s.
Each seeded ID must retain its authoritative source anchors and both mandatory
intake dependencies. The EOD census is rechecked against the real registries. It preserves partial Rust
implementation without promoting it to provider or gameplay qualification.

Inventory acceptance is structural and does not mean full original-source intake
is complete. Original Git-object preservation is now independently checked by
`verify-source-pin.py`; the wider registry/build/submodule closure remains
an explicit unresolved obligation. Version 1 refuses to resolve registration
closure until that broader audited census is implemented. Release intentionally
fails today, naming every unqualified capability. No percent-complete number is
produced, and no existing game scope is removed.

A qualified row requires existing Rust paths, verified original pinned blobs,
resolved dependencies and evidence with exact `original_reference`,
`implementation_commit`, `content_release`, `capability_ids` and `status: passed`.
Evidence is schema-checked: commit identities are full lowercase 40-hex strings,
and capability IDs form a nonempty unique array with exact membership.
Pass the expected implementation and content identities with
`--implementation-commit` and `--content-release`; a report's own identity is not
accepted as the expected release. Original-runtime parity, real providers,
distribution rights and actual browser journeys still require substantive review.
This validator does not manufacture that evidence or verify a report's assertions.

`--ledger` selects a repository-local alternative for failure-injection checks.
Do not rewrite qualified rows by regenerating the initial seed. Extend and review
the ledger as each source cohort and its provider/browser behavior is inspected.

For a fresh checkout, fetch the public reference before verification:

```sh
git -c credential.helper= fetch --no-tags --depth=1 --filter=blob:none https://github.com/riperiperi/FreeSO.git 4c6b3e8f5835b228723caea3c9f683c62f244f73
python3 -m pip install -r tools/completion/requirements.txt
python3 tools/completion/verify-source-pin.py --reference 4c6b3e8f5835b228723caea3c9f683c62f244f73
python3 tools/completion/verify-ledger-admission.py
```

The preservation workflow runs on every pull request, including original files
outside TSOClient. Reports label submitted counts as `declared_qualified` and
`declared_intake_complete`; those declarations cannot override a failed result.
The admission journey executes the real CLI against nine invalid metadata cases;
its synthetic evidence documents are rejection inputs, never gameplay proof.
