# Buildout evidence

This directory retains the complete text evidence from the successful
2026-10-05 fresh buildout run. The numbered logs correspond to the 47 gates in
[buildout-verification.json](../buildout-verification.json). That machine record
is copied byte-for-byte from the runner; its log and detail paths are relative
to this directory.

## Entry points

- [Verification and handoff](../BUILDOUT-VERIFICATION.md): exact counts,
  reproduction commands, review corrections and qualification boundaries.
- [Runtime replay results](runtime-replay.json): exact JSON records extracted
  from the [source gate](46-runtime-source-native-wasi.log) and
  [cooked gate](47-runtime-cooked-native-wasi.log), with gate-log references.
- [Original EOD oracle report](eod-source-oracle/eod-source-oracle.json): all
  source pins, scenario/effect counts, native boundary tests and negative controls.
- [Original C# EOD trace](eod-source-oracle/source-trace.json) and
  [native EOD trace](eod-source-oracle/native-trace.json): complete parsed traces;
  their raw outputs, repeat runs and compiler/test logs are retained alongside.
- [Indexed IFF source report](indexed-iff/indexed-iff-oracle.json): exact original
  corpus and output counts, with all original-reader batch logs alongside.
- [Original SPR2 writer report](sprite-oracle.json) and
  [SPR2 reader log](08-original-sprite-reader.log): codec comparison evidence.
- [Publication record](../buildout-publication.json): four feature increments,
  local/published commits and fetched tree equality.

All 88 copied logs, traces and source-oracle reports match their original run
bytes. The replay summary and this index are derived navigation artifacts.
Build products and emitted binary IFF fixtures are not included. The source
scripts reproduce them from the pinned repository inputs.
