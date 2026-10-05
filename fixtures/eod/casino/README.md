# Casino source fixtures

`sources.json` pins the original C# implementation and UI files at commit
`4c6b3e8f5835b228723caea3c9f683c62f244f73` with whole-file SHA-256 hashes. The
fixture includes a **bounded executable original-helper oracle**. It does not
execute the complete casino handlers or qualify a production provider or VM.

## Executed original helpers

Run from the repository root:

```sh
PYTHONDONTWRITEBYTECODE=1 python3 fixtures/eod/casino/verify.py \
  --report fixtures/eod/casino/helper-report.json
```

The runner requires Python 3, Git, and Mono `mcs`/`mono`. The original source
files and pinned Git objects must be available locally. It compiles into a
temporary directory and removes the generated executable after the two runs.
It never regenerates `expected-helper.txt`.

The runner verifies all eleven manifest files against both their whole-file
hashes and the exact pinned Git bytes. It compiles the complete original
`AbstractPlayingCardsDeck.cs` unchanged. It also compiles two verbatim byte
slices from `VMEODBlackjackPlugin.cs`: the Blackjack value dictionary, and the
complete `BlackjackPlayer` class with its following enums. The manifest and
report record each slice's byte bounds, source line bounds, and SHA-256 hash.
Generated scaffolding supplies namespace/import/class containers and an empty
opaque `VMEODClient` type for an unused stored identity field. It changes no
helper method or value entry and supplies no VM, account, transport, or UI
behavior.

`SourceHelperOracle.cs` calls those original helpers. The checked-in literal
trace covers:

- Circular one-deck repetition, six-deck multiplicity, discard exhaustion,
  card names, shorthand, and copy semantics. Random order is unobserved.
- Blackjack natural and split eligibility, soft and multiple-ace arithmetic,
  stand, bust, and double state transitions.
- Win, loss, push, dealer bust/natural, integer odd-bet natural payout,
  insurance, doubled payout, exact split card order, split natural/double
  settlement, and split aces.

Both executions must match all 35 lines of the literal expected trace exactly.
Eight negative controls alter individual expected results and must be rejected
by the same comparator: circular repetition, soft 17, ten-value splitting,
odd natural payout, odd insurance, doubled push, combined split settlement,
and split-ace settlement. These are comparator controls; they do not represent
execution of mutated original handlers. `helper-report.json` records the
verified inputs, two-run result, trace hash, and rejected controls.

This evidence is deliberately limited to the compiled helper scope. The
original `HoldemHand` evaluator, Slots and Roulette handlers, graphical client,
production VM, account provider, and original-versus-native `System.Random`
sequences are not executed or qualified by this oracle. Native tests remain a
separate layer of evidence.

## Native algorithm and host fixtures

Literal source-derived cases live beside the relevant native algorithm so they compile without a JSON dependency:

- `src/plugins/casino/tests.rs`: exact 100-percent slot wheel counts and payout patterns.
- `src/plugins/casino/cards.rs`: all poker categories, ace-low behavior, ties, dealer qualification, side/ante payouts, Blackjack natural/soft-hand arithmetic, and all 2,598,960 five-card hands checked against independent combinatorial category counts.
- `src/plugins/casino/roulette.rs`: legal layout geometry, double zero, chip removal/serialization, and payouts.
- `src/plugins/casino/table.rs`: controlled private shoe fixtures for Blackjack split/double/insurance and HoldEm side/call/final settlement.

The real `NativeHost` acceptance suite is `crates/eod-runtime/tests/casino_handlers.rs`. Its explicit native seed `2` produces first slot stops `[1, 5, 3]` at 100 percent payback and a two-unit return for a one-unit stake. These are native deterministic fixtures; no claim is made that the original C# random generator produces those stops for that seed.

The private component harness may inject a shoe only inside `#[cfg(test)]`. No public UI, VM event, or provider operation accepts a deck or card override.
