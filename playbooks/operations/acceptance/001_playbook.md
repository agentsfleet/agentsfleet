---
type: how-to
audience: operator
verified: 2026-11-08
product_version: 0.75.0
executable: false
---

# Acceptance visual pass

## What this does

A person walks the development dashboard and records a verdict file for one build.
The `deploy-dev / acceptance` job and `fleet-execution.spec.ts` gate every deploy; this pass adds human judgement on whether the pages read as trustworthy.
Neither replaces the other.
The Human walks and records; the agent checks the verdict file before the Pull Request (PR) closes.

| Dashboard | API | Verdict file |
|---|---|---|
| `https://app-dev.agentsfleet.net` | `https://api-dev.agentsfleet.net` | `playbooks/operations/acceptance/verdicts/<BUILD_SHA>.md` |

`<BUILD_SHA>` is the commit the dashboard and API deployed from, read from the `deploy-dev` run.

## Before you begin

1. Confirm the `deploy-dev / acceptance` run for `<BUILD_SHA>` is green; never sign off a red run.
2. Sign in to the development dashboard as a regular workspace member.
   The operator identity is for step 7 only.
3. Keep a second and a third browser tab ready.

## Steps

Each step names what you must see.
If you do not see it, stop and record the step as a defect.

1. Open **Fleets → New fleet**, give the setup chat a job, review the setup, and press **Save fleet**.
   You must see **Open fleet →**.
2. Press **Open fleet →**.
   You must see the fleet on **Chat**, status **ACTIVE**, marked **Live**.
3. In the second tab, open **Fleets**.
   You must see this fleet's tile reading **Waiting for the next event.** with a live dot.
4. In the third tab, open **Approvals**.
   If a **Pending** row names this fleet, press ✓; the fleet cannot mint a credential, and step 5 cannot finish, until it is answered.
   You must see the row move to **Approved**, with DECIDED naming you.
5. In the first tab, type a short message into **Message your fleet…** and press **Send**.
   You must see your operator turn at once, an assistant turn with text within two minutes, and tokens and a duration in the metrics strip.
6. Switch to the second tab without reloading.
   You must see this fleet's tile showing the activity, and every other tile unchanged.
7. Sign in as the operator identity, open **Admin → Runners**, and press the runner that took the work.
   You must see a **Runner leases** row naming your fleet with a finished lease, not a failure sentence.
8. Return to the fleet and press **Kill**.
   You must see the fleet page read killed, and its **Fleets** row read **killed**.

## Check connector registrations

A provider checks `redirect_uri` against its own registered list, where no test can see it.
An unauthenticated authorize call returns the same login redirect for a correct, stale or foreign URL, so this stays a human step, once per provider per environment.
For each connector on **Integrations**, press **Connect** and read the first provider page.
The `redirect_uri` parameter in the address bar is the value under test.

| What you see | Meaning |
|---|---|
| The provider's consent screen, or a return to Integrations | The registration matches. |
| `redirect_uri is not associated with this application` (GitHub), `Invalid Redirect Uri` (Zoho), or any wording naming the redirect | Stale. Fix the provider's App settings, not the code. The daemon mints `https://<APP_HOST>/api/connectors/<provider>/callback`; stale ones usually hold `https://<API_HOST>/v1/connectors/<provider>/callback`. |
| A provider page saying the app is unavailable | The platform app bag is missing or wrong for this environment; see `credentials_test.sh`. |
| The provider's sign-in page | Nothing is proved. Sign in and read the next page, or record the provider as unclassified. Never read sign-in as a pass. |
| Nothing happens, and the row still reads **Not connected** | A dashboard defect, not a registration one; record the provider as unclassified. |

Record each stale registration as a defect row in the verdict.

## Verify it works

1. Create the verdict file from this template and fill every field.
   `<REVIEWER_EMAIL>` is your sign-in address; `<ISO_8601_DATE>` is the walk's UTC date, such as `2026-09-07`.

   ```markdown
   # Acceptance visual verdict

   - Build: <BUILD_SHA>
   - Reviewer: <REVIEWER_EMAIL>
   - Date: <ISO_8601_DATE>
   - Verdict: pass | fail
   - Defects: none | <one line per failed step, naming the step number>
   ```

2. Attach screenshots of steps 5, 6 and 7 to the milestone's PR.
3. The agent checks the file before the PR closes.
   It fails when the file is missing, names no reviewer or date, or records anything but `pass`.

   ```bash
   ./playbooks/operations/acceptance/01_verdict_check.sh <BUILD_SHA>
   ```

   ```text
   ✓ verdict for <BUILD_SHA>: pass — <REVIEWER_EMAIL> on <ISO_8601_DATE>
   ```
