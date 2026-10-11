# Development notes — repo-specific operational knowledge

> Contributor-facing notes for the things `make help` and the architecture set
> don't cover: how the hooks behave, what the test lanes actually mean, and the
> environmental traps that cost real hours. Each section records verified
> behavior with the incident that taught it. When code changes make a section
> stale, fix it in the same PR.

## Shared development worktrees

Each checkout runs its own compiled `agentsfleetd` while linked worktrees share Postgres, Dragonfly, Mailpit and Upstash QStash.
Each daemon gets a separate Postgres database, Dragonfly namespace and stable port.
Production and deployed `api-dev` continue using their existing deployment settings.

Load the trusted machine environment using the existing `AGENTSFLEET_RUSTD_ENV_FILE` setting.
It must supply `AUTH_SESSION_CODE_PEPPER` and `ENCRYPTION_MASTER_KEY`.
Docker, Cargo, Python 3, `lsof` and the native C compiler must be available.
Linux also requires `ip` from iproute2.

```bash
FOLLOW_LOGS=0 make up
```

```text
<build and infrastructure output varies>
API: http://127.0.0.1:<WORKTREE_PORT>
Logs: <WORKTREE_STATE>/daemon.log
Namespace: wt_<WORKTREE_CHECKSUM>
```

`make up` starts missing infrastructure, creates the database, compiles this checkout, migrates its database and waits for `/readyz`.
Repeating it rebuilds and restarts only this daemon while preserving its database.
The default `FOLLOW_LOGS=1` follows the daemon log after startup.

The launcher reuses existing `DATABASE_URL_API`, `DATABASE_URL_MIGRATOR`, `DRAGONFLY_URL`, `API_URL` and QStash settings with local addresses.
`API_URL` uses the default network interface's IPv4 address and this worktree's port.
Native clients and QStash therefore receive the same reachable origin.
`AGENTSFLEET_DEV_NAMESPACE` is the only new application setting; the launcher supplies it.
Mailpit shares its inbox, so use distinct development recipients and avoid deleting other worktrees' mail.

### Runner on the existing Linux host

Compile `agentsfleet-runner` from the matching checkout on the existing Linux host.
Register that runner against this worktree's daemon using the existing runner registration workflow.
Keep its returned `AGENTSFLEET_RUNNER_TOKEN` private and separate from tokens for other worktrees.

Set `<RUNNER_REACHABLE_URL>` to this development machine's reachable address and the port printed by `make up`.
Use the existing `RUNNER_STORAGE_HOME` setting to choose the Linux host's storage base.

```bash
AGENTSFLEET_API_URL=<RUNNER_REACHABLE_URL> bash scripts/local-dev.sh runner-env
```

```text
AGENTSFLEET_API_URL=<RUNNER_REACHABLE_URL>
RUNNER_STORAGE_HOME=/var/lib/agentsfleet-runner/worktrees/<REPOSITORY_CHECKSUM>/wt_<WORKTREE_CHECKSUM>
```

Apply those settings with the registered token when starting that checkout's runner on Linux.
The settings helper prints no token and performs no remote host changes.
On macOS, it requires an explicit runner-reachable URL because Linux cannot use this machine's loopback address.

### Shutdown and recovery

```bash
make down
```

```text
<no output on success>
```

This stops only this worktree's daemon and preserves its data and the shared services.
The launcher checks the recorded process start time and executable before signaling it.
An occupied port or changed process identity causes a refusal.

The following command stops shared services for every worktree.
Postgres volumes survive, but pending QStash deliveries and Dragonfly memory may be lost.

```bash
bash scripts/local-dev.sh infra-down
```

```text
<Compose shutdown output varies>
```

The next `make up` restarts infrastructure using the saved snapshot.
An unhealthy or incomplete existing stack refuses startup; inspect its Compose health before stopping it explicitly.
Lock waits are bounded separately: 6 minutes for infrastructure, 20 minutes for builds and 30 minutes for this worktree's process operation. After an interrupted owner, verify that owner has exited before removing its named lock directory.

Run `bash scripts/local-dev-test.sh` for launcher regression checks and add `--live` for the two-worktree proof.
`make test-integration-rustd` retains its own disposable stack and destructive reset behavior.
See [datastore scaling](architecture/datastore_scaling.md#shared-local-development) for the isolation boundaries.

## Pushing and git hooks

Hooks live **in this repo** at `.githooks/` (`git config core.hooksPath=.githooks`)
— not in any contributor's dotfiles. Hook fixes are normal repo PRs.

- **pre-push classifies the outgoing range into lint and unit surfaces** by file
  pattern (`*.rs`/`rustd/*` → `lint-rustd` + `test-unit-rustd`, `cli/*` → cli,
  `ui/packages/app/*` → app, website → website, design-system → both it and
  app). Zero matches → `"no test-relevant files — nothing to run"` and the push
  sails through. A genuinely docs-only push skips the suite entirely.
- **Clippy is pre-push's, not pre-commit's.** `lint-rustd` compiles the whole
  workspace under `-D warnings`, which measured 36s to 3m16s per commit — a tax
  on every commit including the small ones. Pre-commit keeps the checks that
  cost seconds: the partial-staging reject, `orly gate work` (the `conform`
  row — UFS, RUST ERR, LOGGING and the rest, over the staged diff), gitleaks,
  and the fast per-surface gates. This is the split `.orly/docs/VERIFY_TIERS.md`
  already describes; the hooks had drifted from it.
- **Pre-commit asserts the orly engine pin before it gates.**
  `.orly/orly.json` records `orly_version`; CI installs exactly that version
  before gating, and `scripts/check_orly_pin.sh` now proves the local binary
  matches it. It exists because a stale global orly reads the pin, proceeds
  anyway, and grades a narrower criteria set than the repository declares —
  one of the five declared commands, while printing a complete-looking list —
  and `orly doctor` on a stale engine printed thirty red "managed file was
  edited" lines and still exited 0. The check asserts rather than installs
  (the commit tier costs seconds, and reinstalling global tooling per commit
  is a surprise), prints both versions on failure, and names the install
  command for the manager the binary actually resolves under: bun-installed
  and npm-installed orlys do not replace each other.
- **The merge trap:** merging `origin/main` *into* a branch makes the pushed
  range include all of main's recent source files — pre-push then runs the full
  unit lanes for what was a docs-only intent, and `test-unit-agentsfleetd` **hangs if
  the test-DB containers are down**. The lean-push pattern for docs-only
  branches: don't local-merge main; push just the docs commit (skips), and sync
  the branch with GitHub's **"Update branch"** button (server-side, no local
  hook).
- **Never run two pushes concurrently.** Each pre-push spawns a DB-backed
  agentsfleetd test suite; two at once deadlock on the shared test Postgres at 0% CPU
  forever and block every subsequent push. Recovery: kill the stuck
  `agentsfleetd-tests --listen` processes and retry serially.
- **Sandboxed agent environments break the SSH transfer.** Hook verification
  passes, then the upload dies with `Broken pipe` / `Connection closed by remote
  host` on every attempt. It is not payload size — run the push with network
  sandboxing disabled and it lands first try.
- **Flaky under parallel hook load:** `agentsfleet test/browser-resolve-platforms`
  (and occasionally the app's provider-selection tests) time out at ~5 s but pass
  in isolation. Retry serially before suspecting the diff.
- **`main` is branch-protected** (required checks; direct pushes are declined).
  Everything lands via a feature branch + PR — including specs.
- Pre-push runs the lanes matching what you pushed: `*.rs` or anything under
  `rustd/` triggers `test-unit-rustd`, and each TypeScript package triggers its own.

## Test lanes — what the names mean

The lane definitions, the toolchain reason both cargo lanes `cd` into `rustd/`,
and the coverage bar are canonical in
[`architecture/testing.md`](./architecture/testing.md) §"Public lanes". What
follows is the repo-workflow half that page does not carry.

- **"Live e2e" / acceptance = the Playwright ladder**, not backend integration:
  `ui/packages/app` → `bun run test:e2e:acceptance` (signup, login, lifecycle,
  kill, billing, multi-workspace). Local twins of the CI jobs:
  `make acceptance-e2e` (app suite; local run auto-starts dev on :3101, needs
  Clerk DEV creds in the worktree-root `.env`) and `make acceptance-cli`
  (agentsfleet). CI runs the same suite against the dev deployment on PR and prod
  post-deploy.

- **Runner execute-loop without a language model:** `afr_agent`'s scripted
  engine (`rustd/crates/afr_agent/src/scripted.rs`) runs a fixed turn — tool
  calls, streamed text, memory items — so a sandbox or wire fault cannot hide
  behind a model's choices. It sits behind the crate's `test-util` feature,
  which only dev-dependencies enable (`afr_supervisor`, `afr_providers`,
  `agentsfleetd`), so no shipped binary carries it.

## Runner kernel lane on Debian 13

Use an OrbStack Debian 13 machine for `make test-runner-kernel`, matching the runner hosts' distribution. Run these commands from the repository root on macOS.
OrbStack supplies the kernel; choosing Debian changes the machine's userland.

The lane needs root, bubblewrap, Landlock, the Enhanced Read-Only File System (EROFS), control groups (cgroup) v2 and IPv4 forwarding.
It fails when a required mechanism is missing.

The lane starts the production `afr_sandbox::BubblewrapEngine` with the built toolbox image.
It exercises confinement, filesystem and resource limits, network policies, tool execution, warm starts, freeze/resume and cleanup.
Unit and service integration tests cover additional runner logic.

Caller tests start with a lease carrying a steer and instructions, then drive the real supervisor, agent loop and tool catalog.
They verify workspace edits and local Git commits inside bubblewrap.
They also verify memory, schedules, thread messages and guarded HTTPS reads through the supervisor.

These cases receive an admitted lease; they do not test the steer API or `SKILL.md` loading.

The model, daemon and upstream endpoints are fakes shared with existing tests.
The service integration lane checks daemon storage and Slack delivery.

1. Choose the machine name. Use a fresh name to check the setup without replacing an existing machine.

   ```bash
   kernel_machine=afr-kernel
   ```

   Expected: no output.

2. Create the machine. OrbStack creates a Linux user matching your macOS username and shares the Mac checkout at the same path.

   ```bash
   orb create debian:trixie "$kernel_machine"
   ```

   Expected: exit 0; progress output varies. An existing name is refused.

3. Install the builder's shared package list and the kernel lane's host tools.

   ```bash
   orb -m "$kernel_machine" bash scripts/toolbox/install-builder-tools.sh \
     bubblewrap build-essential pkg-config git nftables iproute2 e2fsprogs cryptsetup kmod
   ```

   Expected: apt installs the packages and exits 0. Host Git creates the lane's repository fixtures; the toolbox's Git serves sandbox commands.

4. Install rustup, then let the workspace select its pinned Rust toolchain and components.

   ```bash
   orb -m "$kernel_machine" bash -lc \
     'curl --proto "=https" --tlsv1.2 -fsSL https://sh.rustup.rs -o /tmp/agentsfleet-rustup.sh &&
      sh /tmp/agentsfleet-rustup.sh -y --profile minimal --default-toolchain none &&
      . "$HOME/.cargo/env" && cd rustd && rustup toolchain install && rustup show active-toolchain'
   ```

   Expected: the channel from `rustd/rust-toolchain.toml`, selected by that file. Keep any `RUSTUP_TOOLCHAIN` override equal to the pin.

5. Check passwordless sudo and the toolbox filesystem. OrbStack's default user already has passwordless sudo, which the lane's `sudo -E` uses.

   ```bash
   orb -m "$kernel_machine" bash -lc \
     'sudo -n true && { grep -qw erofs /proc/filesystems || sudo modprobe erofs; }'
   ```

   Expected: exit 0 and no output. A sudo prompt or a missing kernel module means the machine needs repair before the lane runs.

6. Enable IPv4 forwarding, as the runner-host preparation does.

   ```bash
   orb -m "$kernel_machine" sudo sysctl -w net.ipv4.ip_forward=1
   ```

   Expected: `net.ipv4.ip_forward = 1`. Repeat after a machine restart if forwarding is disabled.

7. Verify the distribution.

   ```bash
   orb -m "$kernel_machine" cat /etc/debian_version
   ```

   Expected: `13.<point-release>`; the setup check on 2026-10-10 reported `13.7`.

8. Run the kernel lane. Keep Linux build outputs and toolbox images on the machine's native disk, separate from the Mac checkout's outputs.
   Create Cargo's directory as your user before the root toolbox build, so their shared cache parent stays writable.

   ```bash
   orb -m "$kernel_machine" bash -lc \
     '. "$HOME/.cargo/env" && mkdir -p "$HOME/.cache/agentsfleet/kernel-target" &&
      CARGO_BUILD_JOBS=1 \
      CARGO_TARGET_DIR="$HOME/.cache/agentsfleet/kernel-target" \
      make test-runner-kernel TOOLBOX_DIR="$HOME/.cache/agentsfleet/toolbox"'
   ```

   Expected: every kernel-lane check passes, including `test_toolbox_carries_the_tools`; the command exits 0. Cargo builds as your user; its runner runs only the lane binary as root.

The toolbox snapshot comes from `scripts/toolbox/manifest.txt`. Continuous Integration (CI) remains the release build of record.
Different builder-package versions can produce different toolbox bytes; each image's release record names the versions that built it.

After the lane, `orb -m "$kernel_machine" sudo fstrim -v /` can return unused Linux disk blocks to macOS.
The reported trimmed size varies; use `df -h /System/Volumes/Data` on the Mac to check actual headroom.

To rebuild an existing machine, save any Linux-only files first. **`orb delete "$kernel_machine"` erases that machine's Linux disk**, including its build cache and toolbox images.

The shared Mac checkout survives. Delete only your disposable machine, then repeat the steps above.

## agentsfleet CLI conventions

- **Hidden flags are registered on the root program**, never on a subcommand —
  commander renders any subcommand with options as `cmd [options] …`, which
  widens the auto-computed help column past 80 chars and breaks both the
  byte-exact help golden and the 80-column test. `.hideHelp()` hides the flag
  from `cmd --help` but not the `[options]` term in the parent listing. Accepted
  trade-off: the flag parses as a global no-op on other subcommands.
- **Effect-TS follows the Supabase CLI reference** at
  `~/Projects/oss/cli/apps/cli/src/next/` — service surface, layer composition,
  handler shape, error mapping. Any divergence (even Tag-construction or layer
  order) needs an explicit maintainer ack **before** the diff lands; surface the
  reference shape, the proposed divergence, and the why.

## Deploys (Vercel)

- Token at runtime: `VERCEL_TOKEN=$(op read 'op://ZMB_CD_DEV/vercel-api-token/credential')`
  — never print it. Team scope `indykishs-projects`.
- Projects → domains: `agentsfleet-website` (marketing) · `agentsfleet-app`
  (dashboard) · `agentsfleet-agents-dev` (serves the `agentsfleet.dev` installer
  domain; static output `ui/agentsfleet.dev/dist/`).
- **Preview URLs return 401** (`ssoProtection: all_except_custom_domains`); prod
  custom domains are raw-reachable. To curl a preview, fetch the project's
  automation-bypass secret (`GET /v9/projects/{name}` → `.protectionBypass |
  keys[0]`) and send `x-vercel-protection-bypass: <secret>`.
- Vercel ignores Cloudflare-Pages `_redirects`/`_headers`; static dirs need
  `framework=Other` + a `vercel.json` for rewrites/headers. The `agentsfleet.dev`
  root rewrite + `text/x-shellscript` content-type live in
  `ui/agentsfleet.dev/dist/vercel.json` (two explicit header sources — the
  optional-group regex form does not match bare `/`). Deploys ride the git
  integration: preview-on-PR, prod-on-merge.
- Parsing `/v6/deployments` JSON: use `jq` — python's `json.load` chokes on
  control characters in the response.

## Dashboard performance — read dev numbers carefully

The dev-mode `/agents` 1.5–5 s is mostly **not** a backend bug: Turbopack
on-demand route compilation (zero in prod) + local dev calling the **remote**
`api-dev` backend (`lib/api/client.ts` `API_ORIGIN` default) + uncompressed dev
RSC streaming. The `route?_rsc=…` request is the App Router navigation payload,
not the JSON API. The Approvals 5 s repeat is an intentional poll; the Clerk
`/touch`+`/tokens` pair is SDK session management doubled by StrictMode in dev.
The one prod-relevant lever (own perf PR): the server components make 3
*sequential* remote hops (`getToken → workspaces → agents`) — parallelize
billing with workspace resolution and skip the workspaces round-trip when the
`active_workspace_id` cookie is set. Measure a Vercel preview first; never
optimize against dev numbers.

### Authenticated route bundle gate

Build the production app before measuring route size:

```bash
NEXT_PUBLIC_API_URL=http://127.0.0.1:3000 bun run --cwd ui/packages/app build
bun run --cwd ui/packages/app size
```

The report prints framework runtime, the shared authenticated entry, and every
authenticated route. The shared entry must stay at or below 250 Kibibytes
(KiB). Each dashboard route may add at most 100 KiB. Command-line
authentication may total at most 240 KiB.

The size command uses `size-limit` and its file plugin. A dynamic configuration
discovers authenticated routes and emitted chunks from `.next`; only the four
product budgets are maintained. The command fails when a discovered file set
exceeds its limit or a required build manifest is absent or malformed.
Continuous Integration (CI) runs the build and size commands in one job so the
gate measures the output produced immediately before it.

## Synced tooling (not repo-owned)

`scripts/audit-*.sh`, `scripts/lib/`, and `scripts/llmevals/` appear untracked —
they re-sync from the operating-model tooling via `upgrade-ai-tools`. Don't
commit them, don't treat a worktree missing them as data loss, and don't block
worktree cleanup on them.
