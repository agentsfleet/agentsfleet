---
type: reference
audience: contributor
verified: 2026-10-16
product_version: 0.61.0
executable: false
---

# M227 test baseline

## What it is

The comparison counts for M227_001's Test Delta row. They were measured on baseline revision
`ad3ed8f92c739b59df71863dbeec81d621dd7d0a`, the merge of pull request #750.

## Why it exists

The lifecycle measures the unit and integration lanes on the recorded comparison revision before the
pull request. That revision's tree was already measured by Continuous Integration (CI), so this page
cites those runs instead of re-running them.

## How it behaves

### Same tree

Pull request #750's last CI runs checked out GitHub's merge ref:
`HEAD is now at 41051e7 Merge d3f9d919405617da65925a8aac71ad56b62d0f50 into 4a53b948c727cfdc860373bb3c77b08f80bb418d`.
Those are the two parents of `ad3ed8f92`. `git merge-tree --write-tree 4a53b948 d3f9d919` gives tree
`0f91362d549323726e20685af256ab2f4b59cfac`, which is also `ad3ed8f92^{tree}`. So the runs measured the
baseline tree byte for byte.

### Counts

Counted from the run logs by summing every `test result:` line (cargo), `Tests … passed` summary (Vitest)
and `N pass` / `N fail` summary (bun).

| Lane | Run | Passed | Failed | Ignored |
|---|---|---|---|---|
| Rust unit (`test-unit-rustd`) | [38524077996](https://github.com/agentsfleet/agentsfleet/actions/runs/38524077996) | 4694 | 0 | 973 |
| App unit + coverage | [38524077962](https://github.com/agentsfleet/agentsfleet/actions/runs/38524077962), job `test-unit-app` | 3453 | 0 | 0 |
| CLI unit + coverage | same run, job `test-unit-cli` | 1642 | 0 | 0 |
| Design-system unit + coverage | same run, job `test-unit-design-system` | 490 | 0 | 0 |
| Website unit + coverage | same run, job `test-unit-website` | 93 | 0 | 0 |
| Rust integration, both tiers under coverage (`test-coverage-rustd` shards) | [38524077970](https://github.com/agentsfleet/agentsfleet/actions/runs/38524077970) | 5586 | 0 | 0 |

`unit` = 4694 + 3453 + 1642 + 490 + 93 = 10372. That covers what `make test-unit-all` runs:
`test-unit-rustd` plus `test-coverage-all`.

`integration` = 5586, the CI integration workflow's own lane (`make test-coverage-rustd`, which runs both
Rust tiers against live datastores). The branch-side count comes from the same recipe.

## Limits

The 973 ignored tests in the Rust unit run are the live-datastore tier. The integration lane executes them,
so they are not missing coverage. The CI integration lane runs both tiers in one invocation, so its total
can't be split by tier from the log. Compare it only with another `make test-coverage-rustd` total.

## Related pages

- [Implementation checks](checks.md)
- [Completion review](ui-completion-review.md)
