# Source GoTo durable action boundary

The WLC1 player action enum appends SourceGoto as variant 2. Existing Invoke (variant 0, 94 bytes) and Cancel (variant 1, 76 bytes) layouts are unchanged. SourceGoto is exactly 77 bytes, including its 16-byte WLC1 envelope.

The edge parser binds principal at byte 20, actor slot/generation at bytes 28/32, and command sequence at byte 60. The original request is retained byte-for-byte in durable outcome evidence. The TypeScript and Rust tests share one exact signed-Param0 serialization vector. Truncated, extended, and unknown-variant packets are refused.

Committed and rejected SourceGoto decisions are independently exercised through durable lookup and retry without advancing cursor or outbox. The parser does not implement movement, validate source menu membership, or authorize ownership by itself; existing coordinator control and accepted-batch checks remain required.

Executed 2026-10-15:

- `node --test tests/action-outcomes.test.mjs tests/host-receipts.test.mjs`: 131 passed, 0 failed. Full output: `edge-tests.log`.
- `node_modules/.bin/tsc --noEmit`: passed.
- `node_modules/.bin/prettier --check src/lot/action-outcomes.ts tests/action-outcomes.test.mjs`: passed.
- Independent read-only ABI review confirmed field widths, offsets, old-variant preservation, and durable receipt binding.

These are source protocol checks. They do not establish original-content movement completion or rendered two-account gameplay.
