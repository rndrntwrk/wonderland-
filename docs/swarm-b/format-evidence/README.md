# Format authoring verification evidence

These logs record the native gates for the remaining source-backed Swarm B
format changes on 5 October 2026. Source pin:
`4c6b3e8f5835b228723caea3c9f683c62f244f73`.

See [the format contract](../format-authoring-completion.md) for supported
versions, conversion policies, exact remaining boundaries, ignored test names
and reproducible commands. The coordinator owns final browser verification and
independent review.

`native-formats.log` records 123 passing tests and 3 preceding opt-in ignored
tests. `native-content.log` records 14 passing tests. `native-cooker.log` records
26 passing tests. All three `clippy-*.log` files record strict all-target Clippy;
the two `fmt-*.log` files are empty because their checks succeeded.

`source-avatar-corpus.log` contains read-only paths, SHA-256 hashes, sizes and
exact payload comparison results for 455 original files. The corresponding
checked-in runner is `crates/legacy-formats/tests/source_avatar_corpus.rs`. No
original asset payload is redistributed in this evidence directory.

`legacy-text-source-probe.cs` compiles with the unchanged original
`TSOClient/tso.files/Utils/BCFReadProxy.cs` under Mono 6.8.0.105. Its output is
`legacy-text-source-probe.log`. Text reader calls execute the actual original
proxy. CFP delta rows are independent C# calculations of the pinned source
formula; they do not claim execution of the entire CFP class or simulation.

`new-cooker-codecs.log` records the ten explicit new codec validation/import
cases, including rejection of incomplete headers marked simulation-critical.
