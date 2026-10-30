# M235 development source-read evidence

Implementation branch: `feat/m235-ci-evidence`.
Comparison revision: `7b18cd4166b87b9289c9d94cd31000f96d500e15`.
Repository test baselines and implementation grades remain pending.

## Intent and scope

Fix generic guarded GET downloads first, then verify GitHub job-log text in a separate read-only development fleet.
Keep per-hop admission, credential boundaries, private-target handling and shared no-follow clients intact.
Later instruction, repair and evaluation Sections remain incomplete; this source probe cannot substitute for M224's demonstrations.

## Observed source failure

Authenticated native metadata identifies job `123968144417`, run `41027232532`, attempt `1` and commit `d029957192e53ce4a3a83a928c1ed78a23e75683`.
The completed job failed; its recorded interval is `2026-10-30T08:33:29Z` through `2026-10-30T08:35:32Z`.
Its log endpoint again returned HTTP `302`, with safe destination origin `https://productionresultssa6.blob.core.windows.net`.

No signed URL or credential was displayed or saved.
Native development doctor passes for workspace `01a154ce-2ad5-7ce5-9493-f3a9d95a0b0e`.
The separate fleet's own successful log read remains unproved.
