# Swarm F: executable original-component reference and differential replay

This is a bounded **W00.3/W00.4/W17.1 reference-cohort leaf**, stacked on #42 at
`b43e128919033340ab0dee26e80dfcd75f1a1915`. It does not complete the whole C# engine
harness or all-game parity. The local Codex client assignment remains unchanged.

## What actually executes

The existing A lifecycle runner, C# template and expected TSV are copied with
**identical Git blob IDs** from A's `8a0e251d19e222a0a6833d7408ca629f674e1729`.
They retain their original checksummed extraction and explicit storage/content
shims. The new runner invokes them twice without `--record`, so the accepted
49-line fixture is not regenerated. This establishes a reproducible execution of
that existing extracted-method cohort, not a full VM boot.

The additional clock/RNG cohort compiles these unchanged original inputs:

- The complete `TSOClient/tso.simantics/VMClock.cs` class.
- The complete `NetPlay/Model/VMSerializable.cs` interface/helper file.
- The exact `VMClockMarshal` class extracted from `Marshals/VMContextMarshal.cs`.
- The exact `VMContext.NextRandom` method extracted from `VMContext.cs`.

Whole-file Git identities and extraction signatures are checked before compilation;
SHA-256, exact line ranges and generated-driver identity are retained. The original
source baseline is `4c6b3e8f5835b228723caea3c9f683c62f244f73`. None of these source
files is modified. Reusing original classes does not imply every method was tested:
the new cohort exercises clock advance/seconds/UTC and bounded random generation,
not the full marshal or VM serialization contract.

The Rust witness calls the **shipping `sim_core::clock::SimClock` and
`sim_core::rng::SimRng`**, linked from the actual crate built with Cargo's existing
lockfile. The same standalone witness is compiled to native and ordinary
`wasm32-unknown-unknown`; it does not copy the algorithms into a second Rust model.
The Node runner executes the WASM, accepts no host imports or shared memory,
checks the returned buffer bounds, verifies repeated immutable reads, and compares
two independently instantiated modules. No WASI, JavaScript simulation, browser
clock, GPU or network game service is involved.

The existing ignored `avatar_extracted_mono_reference` test is also explicitly
executed with Mono available. Its result remains labelled extracted-avatar-method
coverage; it is not an original-engine or rendered-avatar acceptance test.

## Declared cases and comparison contract

`fixtures/reference/clock-rng-cases.tsv` supplies four scenarios with **380 total
steps**. They cover TS1/TSO clock rates, initial seeds zero/one/u64-max/mixed,
zero/one/large random bounds, conditional extra consumption, fire recovery/cap,
minute and thirty-day calendar rollovers, and seeded standard-time progression.
All UTC starts and game-state fields are assigned before observation. The original
clock's constructor wall-time initializer is overwritten; its value is never read
into the trace. The original runtime/compiler versions are recorded rather than
assumed to be interchangeable across .NET implementations.

The per-step branch is an **authored test-driver policy**, not an original BHAV
instruction. Each step invokes NextRandom with cyclic bounds 0, 1, 2, 100, 65535,
u64-max, then invokes it with 17 for an odd result or 0 otherwise, then advances
the clock. There are no game assets, content-family claims, private EOD values,
authenticated commands, durable outcomes or production writes in these fixtures.

The 18 TSV columns contain case/step identity plus 16 exact scalar fields. The
comparator checks required case order, complete step coverage, canonical decimal
integers and widths, declared input-bound schedule, chained RNG-before state and
field bounds. It rejects two identically empty/truncated/wrongly shaped records
rather than accepting vacuous equality. Integers are compared as exact strings,
never JavaScript Number; u64 values above 2^53 retain all bits.

A valid difference returns **exit 1** and identifies the first case, step, actual
clock tick, field, reference value and candidate value. Invalid/incomplete input
or output failure returns **exit 2**. Equality returns **exit 0**. Supplied trace
files alone are explicitly not execution attestation; the orchestration report
binds the actual executed commands to their inputs, outputs and build identities.

The gate builds separate, clearly labelled native and WASM fault witnesses. At
`ts1_rollover`, step 7, one consumes an extra real RNG operation and the other
chooses the wrong authored driver branch. The unchanged reference comparison must
fail at `rng_after` or `branch` respectively. The fault code exists only behind
explicit cfg flags in the test witness, not in the game crate. No returned record
is edited to manufacture a runtime fault. These controls validate the comparator's
detection/localization, not every possible interpreter fault.

## Reproduction and evidence

Prerequisites on an isolated POSIX machine: Python 3.11+, Git, mcs/Mono, the
repository's pinned Rust toolchain with `wasm32-unknown-unknown`, and Node with
WebAssembly. Install them before invoking the runner. The hosted workflow installs
Mono in its disposable runner and records exact compiler/runtime versions. Package
installation is not a production deployment or a reproducible OS-image claim.

```sh
PYTHONDONTWRITEBYTECODE=1 python3 -B -m unittest discover \
  -s tests/parity -p test_reference_trace.py -v
PYTHONDONTWRITEBYTECODE=1 python3 -B tools/replay/run_reference.py \
  --output /tmp/wonderland-f-reference-new
```

The output directory must not exist and must be outside the clean source checkout.
`build/` holds temporary generated C#, executables and Cargo target artifacts.
`evidence/` holds complete original/native/WASM traces, each command's stdout/stderr,
comparison records, tool versions and `execution.json`. The workflow uploads the
**evidence only**, not game resources, dependencies or arbitrary build directories.

The runner verifies source inputs against HEAD and original/reused pins before
execution, records commit/tree and the lockfile, and rechecks the committed bytes
at completion. Cargo builds use an isolated target directory and `--locked`.
C# assembly and WASM bytes receive their own digests. This attests observed
executions and their outputs; it does not promise bit-identical compiler binaries
across different machines. No branch, manifest, golden vector or product source
is rewritten to make a result pass.

Build commands have deadlines, monitored log-size limits, and isolated process
lifetimes. Trace input is bounded to 4 MiB, fixture input to 64 KiB, and generated
records to 10,000 steps. The source and artifact checks assume an isolated checkout,
not a hostile process racing its files. They are not a sandbox for arbitrary code
or an anti-malicious-maintainer security boundary.

## Execution status and next integration

The comparator specifications were published before implementation. The first
hosted run failed because the new CLI did not yet exist; that is a missing-feature
setup failure, **not** a discovered engine regression. The following comparator
run passed. Full original/native/WASM and injected-runtime-fault results are
recorded against the final PR revision after the hosted workflow completes; no
unrun result is promoted by this document.

The broader original-runtime harness still needs original VM bootstrap, effective
content/tuning, command/effect schedules and canonical replicated-state trace
adapters. The clock/RNG component driver does not execute the 30 Hz lot scheduler,
objects, BHAV interpreter, source resource cohort or persistence. Existing A/B
coverage is reused, not counted again as new gameplay features. Protocol freeze,
SDK/imported source closure, private-state isolation, independent-player service
acceptance and the later device/load/release gates remain outstanding.

## Execution ledger

1. Read #42 and A's pinned lifecycle/numeric probes and current clock/RNG/avatar
   tests; keep original-component and full-engine claims distinct.
2. Publish comparator tests first; observe the absent-CLI failure, then the green
   parser/comparison gate. Test inputs are authored fixtures.
3. Reuse the three A blobs unchanged; add exact extraction/compilation and the
   native/WASM witness with explicit fault-only builds.
4. Run final hosted source/trace/conformance gates; preserve failures and tool
   setup errors separately from actual behavioral divergence findings.
5. Publish exact revision, scope and downloadable evidence; no merge/deployment.

Ruling: do not fabricate a complete VM harness from an extracted method cohort.
The executable first cohort is deliberately smaller and reproducible. Its cost
is that W00.3/W00.4 full-original-engine acceptance remains open rather than being
mislabelled complete. The app and native runtime source interfaces stay untouched.
