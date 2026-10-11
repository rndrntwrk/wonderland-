# Swarm E ownership-claims verification recovery

## Scope and source identity — 9 October 2026

The existing branch `work/swarm-e-ownership-claims` already contains the native
SQLx/MySQL ownership repository at `21d58886c6b1c570b171dc1ad4a8087a41b1d52c`,
tree `cf6a392e575c990d21b5b4f26b045785428d1f21`. Its baseline is PR18,
`a318581f33770540808aefcf132018255a3a1d94`. This change does not rewrite that
repository or claim its implementation as new work.

The existing GitHub source-bootstrap run `37580203606` compiled the SQLx code,
ran five unit tests and a policy mutation, and published the source. It did not
execute `tests/claims_mysql.rs`. No corresponding pull request or real-database
run was returned by the live branch/PR/workflow checks on 9 October.

The downloaded bootstrap artifact `11464143386` has SHA-256
`2bd29aa21ed7c2f2809b078d5b6e1b30449ed4873b49024025c718ba31fd2b37`.
Its CRC, six published source hashes, source commit/tree receipts and resolved
Cargo.lock were checked during recovery. Source restoration is not database
execution or independent review.

## This increment

Replace the one-off write-enabled source bootstrap with ordinary read-only CI.
The new workflow responds to pull requests, matching E branch pushes and manual
dispatch. Two independent jobs run MySQL 8.4 and MariaDB 11.4. Tags select those
release families; each job resolves and records the actual image ID/digest and
starts that immutable ID. This is not a claim that moving release-family tags
are byte-pinned before execution.

Each job creates a fresh, resource-limited container exposed only on loopback.
The account, password and `wonderland_claimstest` database are explicitly
synthetic CI fixtures, not production secrets. The real test suite deliberately
performs DDL and DML in that disposable database. There is no production database,
migration, schema-update permission or external host accepted by this runner.

The existing shipping library, migration, test suite, isolated Cargo.lock, root
workspace, original C# source, shared IDs/wire contracts and C rendering stack
are unchanged. The earlier source-publication workflow is removed; tests never
regenerate a lockfile, patch source, push commits or receive a GitHub write token.

## Required evidence, not just a green command

`tools/swarm-e/run_claims.py` requires an explicit disposable-database opt-in,
a loopback connection URL, actual server/version/database/InnoDB identity and
resolved matching container identity. It runs the pinned compiler, formatting,
five named unit tests, all eleven named SQL tests and strict all-target Clippy.

`claim_evidence.py` rejects missing/duplicate/unexpected test names, partial logs,
failed/ignored/filtered cases, compile-only output, ambiguous summaries, nonzero
command exits, timeouts, stale output directories and the wrong database family.
A reported total without the corresponding individual tests cannot pass.

Each run retains actual exit codes, bounded command logs and their hashes,
compiler identity, complete input hashes before/after, checkout commit/tree and
server/image metadata. Existing tracked source must be clean before and after.
Failure reports and container diagnostics are retained; a failed command cannot
produce a successful evidence report. Matrix failure does not cancel the other
engine's job.

The eleven SQL cases remain those in `OWNERSHIP_CLAIMS.md`: separate namespaces
and full-width IDs, competing acquisition, transfer/renew/release fencing,
expiry/reacquisition, rollback/commit visibility, handoff waiting on a protected
transaction, expired callback rollback, clock sampling after lock waits, epoch
exhaustion and non-InnoDB rejection. Individual SQL tests are serialized because
one changes the table engine. No mock database counts as acceptance.

## Reproduce

In a complete clean checkout, run the tooling tests without a database:

```sh
python3 -m unittest discover -s tests/swarm_e -v
```

The workflow starts each real database and supplies its version/image files.
The equivalent runner requires those files plus an explicitly disposable local
service and a new evidence directory:

```sh
export WONDERLAND_CLAIMS_DISPOSABLE=1
export DATABASE_URL='mysql://claims_test:fixture-password@127.0.0.1:33067/wonderland_claimstest'
python3 tools/swarm-e/run_claims.py --engine mysql \
  --server-identity /tmp/claims/server-identity.tsv \
  --image-identity /tmp/claims/image-identity.json \
  --output /tmp/claims/fresh-run
```

Only use this with an isolated throwaway database. The TSV is the actual result
of `SELECT VERSION(),DATABASE(),@@default_storage_engine`; the image JSON comes
from `docker image inspect --format '{{json .}}'` on the actual container image.
The workflow captures those observations itself; neither file is an authority
credential or a substitute for the executed SQL tests.

## Verification status and boundaries

Local Python checks establish evidence validation, command supervision, missing
configuration rejection and workflow wiring. They do **not** establish SQLx
transaction correctness. The real two-database matrix is pending publication and
execution; no MySQL/MariaDB result is claimed by this recovery.

The current assistant session has repository reads but no write/PR actions or
authenticated GitHub CLI. A separately provided publication helper creates a new
branch and draft PR in an authenticated local environment. Until that invocation
succeeds and GitHub returns the PR, this increment is **local and unsubmitted**.

W15.1's full original-schema/collation/trigger mapping and actor/service contract
integration remain open. W13.1 authenticated WSS routing is not implemented by
this CI repair. Native claims never authorize browser payloads; durable effect
IDs remain separate from fencing epochs. No merge, deployment, independent
approval or completion of all C/E work is included.

## Source anchors

The approved parallel work packages, W15.1, require real SQL mapping/claim race
proof; W13.1 independently specifies authenticated bounded routing. The source
inventory maps W15 to `SqlAvatars.cs`, `LotServerGlobalLink.cs` and `LotContainer.cs`.
The existing claims guide additionally records `SqlAvatarClaims.cs` and its
explicit additive lease/fencing extension. This verification increment does not
silently reconcile or replace any of those distinct source responsibilities.

GitHub documents that pushes made using the job's `GITHUB_TOKEN` do not themselves
start another push workflow. This is why a source-publisher pass must not be
promoted to a claim of subsequent runtime acceptance. Reference:
https://docs.github.com/en/actions/concepts/security/github_token
