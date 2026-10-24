# Native saved assets prerequisite

Journey: select an owned Sim, inspect its actual saved balance and owned outfit instances, refresh/repeat after a lost read, and close or sign out without stale data returning. Purchasing, selling, changing outfits, live budget mutation and object inventory remain disabled/unimplemented.

Reuse `AccountAuthority`'s durable `avatar_balances`, `avatar_outfits` and explicit ownership-initialization guard. Add an authenticated page reader (64 instances per transport page, no game inventory cap), strict account HTTP/forwarding path, bounded provider read and native account UI. Original CAS grants remain three outfits and zero balance; reads must never initialize legacy ownership or fund/debit anything. Preserve existing full private `ownedAssets` callers.

- [x] Reproduce the missing public assets route with real workerd/SQLite before implementation.
- [x] Add read-only paged authority/HTTP plumbing; qualify ownership, malformed/cross-origin access, repeat reads and SQLite restart. Test-only inventory pagination rows do not prove purchases or original artwork.
- [x] Add provider read guards and a native saved-possessions panel; qualify selected-avatar scope, stale read/logout and actual original-account browser UI. Keep Buy/Build disabled.
- [ ] Reuse native host, build only matching changed browser inputs, and inspect desktop/mobile evidence. Review/publish a scoped draft stacked on #85 with exact tree verification.
- [ ] Consolidate external content/access/build/owner-upload prerequisites separately from unfinished source integration. No new agents, grants, installs, spending, private uploads, merge or deployment.

Product and rendered qualification are complete at `dd0ba061`. Remote delivery is recorded by the final PR/publication receipt rather than inferred from this pre-publication plan.
