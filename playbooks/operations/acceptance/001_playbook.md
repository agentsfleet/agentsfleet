# Acceptance Visual Pass

**Owner:** Human
**Executors:** Human walks the dashboard and records the verdict; Agent checks
the verdict file before the Pull Request (PR) closes.

A green acceptance run and a dashboard a person would trust are different
claims. The `deploy-dev / acceptance` workflow makes the first claim on every
deploy. This playbook makes the second, and it makes it repeatable: the same
steps, the same "you must see" checks, and a verdict written to a file rather
than said in a chat.

| Environment | Dashboard | API | Verdict file |
|---|---|---|---|
| Development | `https://app-dev.agentsfleet.net` | `https://api-dev.agentsfleet.net` | `playbooks/operations/acceptance/verdicts/<BUILD_SHA>.md` |

`<BUILD_SHA>` is the commit the dashboard and API were deployed from. Read it
from the `deploy-dev` workflow run that shipped the build you are looking at.

## Before running

1. Confirm the `deploy-dev / acceptance` run for `<BUILD_SHA>` is green. A red
   automated run is not something a person signs off.
2. Sign in to the development dashboard as a regular workspace member. The
   operator identity is used in step 6 only.
3. Have a second browser tab ready for step 4.

## The walk

Every step names what you must see. If you do not see it, stop, and record the
step as a defect in the verdict file. Do not continue past a failed step.

| Order | Action | You must see |
|---|---|---|
| 1 | Open **Fleets → Install fleet**. Pick any library card and press **Install**. | The install states advance to ready with no confirm dialog and no name field. **Open fleet →** appears. |
| 2 | Press **Open fleet →**. | The fleet's page opens on **Chat** with the status **ACTIVE** and the chat marked **Live**. |
| 3 | In a second tab, open **Fleets**. | A tile for the fleet you installed, reading **Waiting for the next event.** with a live dot. |
| 4 | Back in the first tab, type a short message into **Message this fleet…** and press **Send**. | Your message appears at once as an operator turn. Within two minutes an assistant turn appears below it with text in it. The metrics strip shows tokens and a duration. |
| 5 | Switch to the second tab without reloading it. | The tile for this fleet no longer reads **Waiting for the next event.** and shows the activity. Every other tile is unchanged. |
| 6 | Sign in as the operator identity. Open **Admin → Runners** and press the card of the runner that took the work. | The **Runner leases** table has a row naming your fleet, and the row shows a finished lease rather than a failure sentence. |
| 7 | Return to the fleet and press **Kill**. | The fleet page reads killed, and the fleet's row on **Fleets** reads failed. |

## Record the verdict

Create `playbooks/operations/acceptance/verdicts/<BUILD_SHA>.md` from the
template below and fill every field. Attach the screenshots of steps 4, 5 and
6 to the milestone's PR.

```markdown
# Acceptance visual verdict

- Build: <BUILD_SHA>
- Reviewer: <REVIEWER_EMAIL>
- Date: <ISO_8601_DATE>
- Verdict: pass | fail
- Defects: none | <one line per failed step, naming the step number>
```

`<REVIEWER_EMAIL>` is the address you signed in with. `<ISO_8601_DATE>` is the
day of the walk, in Coordinated Universal Time (UTC), such as `2026-09-07`.

## Check the verdict file

The agent runs this before the PR closes. It fails when the file for the build
is missing, or names no reviewer, or records a fail.

```bash
./playbooks/operations/acceptance/01_verdict_check.sh <BUILD_SHA>
```

Expected output:

```text
✓ verdict for <BUILD_SHA>: pass — <REVIEWER_EMAIL> on <ISO_8601_DATE>
```

## What this does not cover

The automated journey `fleet-execution.spec.ts` asserts the same walk on every
deploy, and it is the gate. This pass adds the human judgement the journey
cannot make: whether the pages read as trustworthy to a person. A verdict file
is not a substitute for a green run, and a green run is not a substitute for a
verdict file.
