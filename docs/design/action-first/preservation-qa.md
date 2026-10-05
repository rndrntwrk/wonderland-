# Feature-preservation correction: verification

This correction follows the user's instruction to modernize the existing game while retaining its capabilities. The earlier prototype's five identities, three whole looks, fixed catalog, profile cap and room limits were implementation shortcuts, not accepted game requirements. The [preservation scope](preservation-scope.md) remains the authority; the [capability map](player-capability-map.md) distinguishes implemented controls from integrations still required.

## Verified preservation

- The original `TSOClient` and `Other` trees are unchanged against fork baseline `4c6b3e8f5835b228723caea3c9f683c62f244f73`.
- The existing normal browser save loads all six profiles, including Éloïse on roster page two. Five cards per page are layout, not an identity or account limit.
- Éloïse's Home retains the armchair at `(2, 2)`, the coffee table at `(3, 2)` and balance `950`. Maya retains her separate empty Home and balance `1250`.
- Version-1 saves retain historical portrait references rather than receiving invented original outfit IDs. Binary content remains outside the save envelope.
- Core regressions cover independent appearance keys, source policy, catalogs beyond the illustrated fixture, variable lot geometry, scoped roommate/shared-lot grants, payer and object ownership, wardrobe default replacement, provider replay retention and conflict-safe save behavior.

## Original character resources

The complete local source set resolves 361 male and 461 female collection entries, with all 2,466 skin-variant dependencies ready. Actual male and female selections compose a head, body and both hands. Resource bytes are original verification inputs, not repository content. See [content integration](content-integration.md) for provenance and source-format corrections.

Browser loading the complete source bank established 424 distinct head outfit choices and 398 distinct body outfit choices. The initial browser test exposed an unbraced view-macro comparison in the Next button and a missing menu icon; both were corrected. The renderer then reported the actual cause of its blank stage: this browser has no WebGL2 context. The character stage now has a Canvas2D fallback that projects, depth-sorts and textures the same source triangles, while retaining WebGL2 where available. The fallback is a character-preview renderer; it does not substitute for the world renderer or simulation.

## Browser journeys

| Journey | Observed result |
| --- | --- |
| Roster beyond five | Next selects Éloïse on page two; Previous returns to page one. |
| Existing saved possessions | Both placed instances and balance remain unchanged at the normal URL. |
| Map selection | Clicking Harbor Café selects its map marker and opens an attached Visit action. |
| Travel | Visit shows its pending state, then opens the café after acknowledgment. |
| Object actions | Selecting the coffee machine opens Make coffee, Clean and Inspect at the object; Clean retains its unavailable reason. |
| Actual character rendering | A source-selected pack of 20 heads and 20 bodies renders textured male and female avatars through the software fallback. Head, body and skin change independently; manual rotation changes the visible geometry. |
| Creator layout | Head/Body tabs retain their selections. One grid, skin/gender, name and primary action fit the 1363 × 937 viewport; the content panel collapses after Apply. |
| Pagination | The 20-choice Head grid advances to page two and exposes Male Head 9/10; the previous page remains available. |
| Rejection and retry | In the explicit rejection fixture, the first Riley creation retains its selected head/body/skin/name. Retry accepts exactly one new profile and selects it on roster page two. |
| Reduced motion | Enabling Reduce motion stops automatic rotation; manual rotation still changes the same textured model. |
| Wardrobe boundary | Change outfit opens the source-rendered character stage. Missing owned-wardrobe data explains its unavailability; it is not filled with invented owned outfits. |

## Build gates

The native workspace suite passes with 284 tests, no failures and three opt-in original-corpus tests ignored. The ordinary importer tests and separate local original-resource probes ran successfully. Required gates also include formatting, native clippy, browser-target clippy and a release Trunk build. Browser evidence is recorded separately because compilation cannot establish visible rendering.

## Remaining integration boundaries

Live authentication, account creation, wardrobe services, messaging, authoritative purchases, architecture, simulation and the production 3D world renderer remain integration work. The map and lot backgrounds are illustrated preview scenes. Build and wall/roof controls expose capability and availability without pretending that unavailable architecture operations succeeded. Original source features have not been deleted; the full browser conversion is not claimed complete.

The correction targets the existing draft PR #10, stacked on #5. It is not merged or deployed by this work.
