# Claude handoff: recorded UI data

Implement [M249_004](../pending/M249_004_P1_API_DOCS_UI_RECORDED_DATA_COMPLETION.md) in a separate worktree and PR.

1. Read repository instructions and the spec. Verify M249_001 and M249_002 are merged; record both SHAs.
2. Fetch main and create your worktree from that merged main. Do not edit the running UI or CLI worktree.
3. Work on independent data Sections alongside M249_003. Finish Plugin wiring after its merge; merge M249_003 before M249_004.
4. Re-inventory source types, ledger queries, authorization, producers and UI adapters. Replace family paths with exact files.
5. Complete the spec's independent design, comprehension, adversarial and readiness reviews. Resolve producer blockers before implementing the affected Section.
6. Supply complete Usage projections, safe key metadata, existing effective secret access, revision ordinals and confirmed work/context facts.
7. Keep unknowns explicit. No browser-page aggregation, inferred permission, guessed publication or fabricated context percentage.
8. Preserve the approved UI, native assistant-ui owners and workspace-backed model credentials. No primary vault or compatibility cutover.
9. Run native repository, integration, coverage, rendered-page, authenticated acceptance and bundle checks. Document remaining data differences.
10. Open your separate PR, attach evidence and return its URL, head SHA, field inventory and gate results. Do not merge it.

This is backend/UI data completion. It does not inherit the UI refresh's remaining visual repairs.

## Separate assistance on the UI PR

UI assistance uses a GitHub issue and a tested pushed checkpoint from Aiwa. Do not start from an uncommitted working tree.

The current cleanup issue is [#838](https://github.com/agentsfleet/agentsfleet/issues/838).
Its code base is `34f5158853567aab9ba53fe27012e72f6be95f38` on `feat/m249-fleet-interface-refresh`.

The issue must name the exact base SHA, owned files, required tests, exclusions and return instructions.

Return a branch and head SHA for Aiwa's review. Do not merge or open a competing UI PR.

Aiwa integrates the reviewed change and reruns final gates. The UI PR description includes `Closed #838`.
