---
type: explanation
audience: contributor
verified: 2026-10-14
product_version: 0.60.0
executable: false
---

# Workspace chat: end-to-end ASCII flow

## What it is

A complete proposed journey from authenticated workspace entry to private authoring, reviewed Save, runtime work, explicit publication, improvement, and deletion. Paths and types come from [the resource reference](../../architecture/api_resources.md); inspected runtime facts come from [the runtime map](runtime-baseline.md). Nothing here claims the backend is implemented.

## Why it exists

Reviewers need the state boundaries and failure exits, not just a happy-path screen sequence. Private content, shared fleet state, owner billing, and Library revisions have different readers.

## How it behaves

### 1. Entry and scope

```text
sign in
  |
  +--> account/workspace lookup fails --> show Retry; keep entry intent
  |
  +--> confirmed no workspace ---------> Create workspace --> lookup again
  |
  +--> authorized workspace W
          |
          +--> New fleet --> blank composer (NO draft row yet)
          +--> Drafts ----> GET W/drafts --> creator's private list
          +--> Fleets ----> existing bounded list --> shared conversation
          +--> Library ---> GET W/fleet-libraries --> gallery
          +--> Activity --> real runs + existing Approvals links

switch account/workspace/subject
  --> tear down old stream + pending rendering + in-memory composer state
  --> key all reads/sends by account, workspace, subject kind, subject ID
  --> fetch authorized new subject; never reuse a private prefix by fleet ID alone
```

### 2. Three ways in, one candidate authority

```text
BLANK                      LIBRARY                        EXISTING FLEET
New fleet                  Inspect G/revision             Improve F/setup_revision
   |                           |                                  |
GET W/models               GET G (+ review)                    GET F
   |                           |                                  |
choose owner model         +--> unchanged Install                 |
   |                       |       --> POST W/fleets              |
first Send / edit          |           (bypass drafts)             |
   |                       +--> Customize --> POST W/drafts <-----+
POST W/drafts                    source pins exact revision
 {source, model, message?,
  operation_id}
   |
201 private Draft; first message accepted atomically if supplied
   |
resolve uncertainty --> same operation_id/body --> same Draft
changed body/key reuse -->409; purged receipt tombstone -->410
```

### 3. Conversation and durable recovery

```text
person Send --> POST D/messages {message,operation_id}
                    |
                transaction: creator/access/state + key/body + next sequence
                    |
                202 {status:accepted, sequence, replayed}
                    |
       +------------+-------------------------------+
       |                                            |
GET D/messages                                  GET D/events/stream
durable queue/state/answer/usage/failure         private live hints/deltas
       |                                            |
       +--> reconnect / lost hint --> refetch -------+

pending send + lost network --> same body key --> accepted receipt, once
new Send after acknowledgement --> fresh body key --> next sequence
unleased fleet message --> existing messages read durable pending admissions
append deferred / receipt absent --> same queued shape + logical event ID
sweep / retry / lease --> one visible message and one eventual event
start.accepted separately requires append + saved receipt
```

### 4. Runner selection and admission

```text
runner POST /v1/runners/me/leases {holds,accepted_kinds}
   |
lock host --> sample database time --> count unexpired mixed leases
   |
renewal also locks host --> expired original lease is rejected, never revived
   |
choose eligible kind; empty kind cannot starve another
   |
   +--> fleet --> existing affinity/fenced runtime flow
   |
   +--> authoring --> recheck creator/workspace owner/model/policy
                      |
                  lock owner wallet
                      |
                  count running + provider-uncertain attempts
                      |
                  max pinned worst-case cost among existing + proposed
                      |
                  balance >= (running+1)*cost ?
                     /                    \
                   no                      yes
                   |                        |
            visible failed message     atomically claim attempt + fence
            + account action           + model capability + expiry
                                            |
                                200 lease.kind = authoring
```

Draft count is unlimited. Claims and renewals share the guarded worker_count decision. Provider-uncertain obligations remain counted for wallet admission. Balance checks do not reserve funds against runtime debits.

### 5. Bounded model work and small edits

```text
lease --> candidate ID/revision + opaque model capability
   |
POST authoring-leases/{id}/model-calls + runner bearer
   |
daemon checks binding/expiry/revocation --> commit dispatch identity
   |
daemon-only provider key --> one physical send --> save bounded result/usage
   |
exact completed replay --> same result, no send; uncertain intent --> no resend

model call assembly
  system <=2048 tokens + tools <=4096 + bounded history + candidate excerpts
  tokenizer input <=32768 AND input + output reservation <= model window
  calls <=8; total output <=8192; wall <=300s; each request <=30s
          |
          +--> cannot fit / model unavailable --> failed with code, no fallback
          |
          +--> model asks clarification -------> answer; candidate can stay absent
          |
          +--> proposes patch operations ------> shared daemon validator
                                                   |
                                               base candidate ID/revision + fence valid?
                                                  /                 \
                                                 no                  yes
                                                 |                    |
                                            reject412        fresh immutable ID + next revision
                                                                    |
                                                               ack returns ID/revision; replay keeps version

runner reports --> known usage charged once; unknown usage stays unknown
late report after Stop/delete --> no answer/candidate resurrection
```

### 6. Stop by accepted-message cutoff

```text
accepted sequences: 1  2  3
person presses Stop through 3 --> POST D/cancellations {through_sequence:"3"}
                                      |
                              max(existing cutoff,3)
                                      |
                              cancel queued <=3; fence running <=3
                                      |
                              200 cutoff + stopped sequences

later Send sequence4 --> survives delayed repeated Stop through3
completion wins transaction first --> completed stays completed
Stop wins first --> late text rejected; incurred accounting still settles once
```

### 7. Drawer and review

```text
idle drawer edit --> PATCH D If-Match
 {title?,authoring_model_ref?,runtime_model_ref?,config_ops?,instruction_edits?}
   |
   +-->428 no tag /412 stale /409 queued or running /400 invalid batch
   |
   +-->200 current candidate + new tag
          authoring-model-only edit affects future sends, preserves setup digest
          disabled selection --> choose eligible owner model --> reload retains it
                 |
GET D ---------> review
                 +--> effective exact definition/name/model/source pins
                 +--> immutable requirements: tools/access/triggers/budgets
                 +--> readiness: current connection/grant/model/wallet checks
                 +--> review_digest (timestamps excluded)

missing check --> open adjacent authorized Account / Connections / Approvals
changed access/model resolution --> new digest --> person reviews again
```

### 8. Save and explicit publication

```text
                       reviewed idle candidate
                         /        |         \
                     Save      Save+run      Save to library
                      |           |                |
POST W/fleets         |           |         POST W/fleet-libraries
header replay key ----+-----------+          header replay key
start none            |        run_now             |
                      |           |          immutable revision + receipt
                      +-----+-----+                |
                            |                     keep explicit receipt
                         create fleet             later Save reuses source pin
                            |                     never republishes automatically

Save is NOT Library publication. Library entry and fleet identity are independent.
```

### 9. Retained installing identity and grant barrier

```text
POST W/fleets --> authorize/review --> transaction: key+digest+fleet ID
                                           |
                                     status installing
                                           |
source --> policy --> owner model/wallet --> credentials/connections
                                           |
                                     approvals/grants
                                       /       \
                               missing          ready
                                  |               |
                             201 installing    runtime stream/setup
                             checks/actions        |
                                  |          checked activation CAS
                      approve + explicit retry     |
                                  +----------------+

no worker waits for human approval
same-key POST / PATCH F active + current If-Match resumes retained fleet
request budget exhausted --> partial installing, never guessed active
crash after any step --> retry same identity, duplicate-safe setup
```

### 10. Activation, run now, and schedules

```text
installing + current setup + grants + policy
               |
transaction --> guarded ACTIVE + one stable initial admission (Pending)
               |
               +--> start none: no initial event
               +--> start run_now: Pending event OR Blocked code
               |       Pending --> append + saved receipt --> Accepted
               |       retry/sweep keeps the same initial event identity
               |
commit --------+--> GitHub/Slack may route immediately (existing rules)
               |
               +--> register reviewed schedules now
                         |
                  synced OR failed + Retry sync

active fleet + failed schedule push is visible; retry never repeats run_now
run_now admission precedes cron registration; it need not finish before other work
```

### 11. Kill during setup

```text
member with fleet:write --> PATCH F {status:killed} If-Match resource tag
                                   |
                          transaction: kill + cancel generation
                          + linked draft back to editing
                                   |
late activation guarded update -->0 rows --> cannot report active
late same-key create replay ------>same killed fleet
next Save ----------------------->fresh key + new fleet
fleet DELETE ------------------->fleet:admin + killed only
private draft DELETE ----------->no shared-fleet delete authority
```

### 12. Runtime after activation

```text
GitHub raw body --> verify signature --> install/subscription --> Reply::None --+
Slack mention/thread --> active routing --> untrusted context + thread reply ---+
QStash signed fire --> owned schedule lookup + replay claim --------------------+
manual message + UUIDv7 key --> durable admission (receipt may wait) -----------+
                                                                              |
                                                             admission + inbox receipt
                                                                              |
                                                             readiness mark / wake hint
                                                                              |
                                                             affinity claim + fence
                                                                              |
                                                             owner grants/model + tools
                                                                              |
                                                             optional narrowed child tools
                                                                              |
                                                             report transaction
                                            result + charge + session + slot release
                                            + optional outbound reply obligation
                                                    /                         \
                                               app/CLI               Slack outbound worker
                                                                       delivered/abandoned
```

Fresh queue wait uses daemon wall clocks plus monotonic poll duration. Renewal rereads budget and rejects expiry under the host guard. Child narrowing, affinity, stale-report fences, and lost-wake fallback remain. Required proofs cover stopped Slack copy and receipt-less recovery before/after append; these tests have not run.

### 13. Shared history and private prefix

```text
GET F + fleet messages/stream --> authorized shared run history/results
       |
       +--> creator AND still authorized for linked Draft?
                yes --> private setup prefix + visible expiry
                no  --> shared history only (no private placeholder text)

run completed != reply delivered
Working / Waiting / Paused / Stopped / Needs attention use real state
```

### 14. Improve and Restore

```text
Improve --> private draft from exact F/setup revision --> chat + drawer + review
                     |
POST F/revisions If-Match setup tag + header key + candidate/digest
                     |
                atomic new setup revision + schedule reconciliation
                     |
          new leases use revision; active lease keeps captured setup
          live revocation and renewed budgets still apply

schedule edit If-Match tag covers recipe + setup + override
Restore {override:null} --> exact inherited recipe under same guarded writer
policy marker drifts --> Needs re-review + block new admission until reviewed apply
Library head stays unchanged until explicit POST G/revisions
```

### 15. Private deletion and close expiry

```text
DELETE D --> fence/deletion generation + remove server private text
                + content-free receipt/accounting -->204
                |
                +--> reads404, stream closes, browser cache clears
                +--> online runner sees delete at renewal <=10s
                +--> disconnected lease expires <=30s
                +--> spool logical expiry <=24h from first write
                +--> online timer / restart cleanup BEFORE content read/upload
                +--> temporary + quarantined copies share expiry
                +--> powered-off disk copies may remain; keys may still decrypt
                +--> upstream/provider/backups follow disclosed retention
                +--> shared published/applied setup remains independent

close --> expiry30 days later, displayed in chat
active authoring/setup --> never expires while active
cancelled setup --> editing, close expiry removed
```

### 16. One cutover boundary

```text
review amended docs + owning M227 spec + folded website/branding spec
   |
implement sections in SAME branch/worktree; no backend code in this rewrite
   |
record expected + observed app/daemon/runner/CLI revisions/hashes/origins
   |
read-only build preflight BEFORE fixture mutation; mismatch stops here
   |
unit/integration/browser/privacy/spend/retry proofs + repository gates
   |
separate published-docs branch ready + schedule-fix merge confirmed
   |
ONE product PR --> owner merge --> coordinated daemon/runner/app/CLI cutover
   |
chosen acceptance-data reset + migration + matched release smoke
   |
injected failed-release recovery --> matched final smoke --> resume traffic
```

## Limits

Source inspection cannot prove the separate schedule fix, current coverage, live provider retention, or implementation correctness. Content-revision provider fencing stays L8's follow-up. The final resource reference governs whenever a diagram abbreviates a payload.

## Related pages

- [Resource reference](../../architecture/api_resources.md)
- [Creation](../../architecture/data_flow.md#fleet-creation-and-setup-recovery)
- [Private-copy limits](../../architecture/runner_fleet.md#authoring-leases)
- [Layouts](../designs/workspace-chat-options.md)
