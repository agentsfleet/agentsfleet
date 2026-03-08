# M5_005: Enable PostHog Tracking In Website

**Prototype:** v1.0.0
**Milestone:** M5
**Workstream:** 005
**Date:** Mar 06, 2026
**Status:** PENDING
**Priority:** P1 — product analytics baseline
**Batch:** B1 — uses PostHog JS SDK, not posthog-zig
**Depends on:** None

---

## 1.0 Singular Function

**Status:** PENDING

Implement one working telemetry function: website emits PostHog events for core conversion and navigation actions.

**Dimensions:**
- 1.1 PENDING Initialize PostHog client with environment-gated config
- 1.2 PENDING Capture core events (`signup_started`, `signup_completed`, `team_pilot_booking_started`)
- 1.3 PENDING Enforce event schema naming and required properties
- 1.4 PENDING Add privacy-safe guardrails for event payloads

---

## 2.0 Verification Units

**Status:** PENDING

**Dimensions:**
- 2.1 PENDING Unit test: event helper emits expected schema
- 2.2 PENDING Integration test: core CTA clicks produce PostHog requests
- 2.3 PENDING Integration test: disabled analytics mode emits no external calls

---

## 3.0 Acceptance Criteria

**Status:** PENDING

- [ ] 3.1 Core website conversion events arrive in PostHog reliably
- [ ] 3.2 Event payloads are schema-consistent and privacy-safe
- [ ] 3.3 Demo evidence captured for live event emission from website actions

---

## 4.0 Out of Scope

- Zombied runtime event tracking (tracked in M5_006)
- Complex analytics dashboard design

---

## 5.0 `/install` Edge Function — Vercel (Option B)

**Status:** PENDING

### 5.1 Rationale

`curl -sSL https://usezombie.sh/install | bash` must be served from the Vercel
marketing site — not from `zombied`. `zombied` is a daemon with productive
work to do; static/script delivery is Vercel's job. A Vercel Edge Function is
preferred over a plain static file because it enables:

- **User-Agent branching** — curl gets the shell script; a browser gets a
  redirect to the docs quickstart page.
- **Install telemetry** — each curl invocation fires a `zombiectl_install`
  PostHog event (OS, arch, version requested) without touching `zombied`.
- **Zero cold start** — edge runtime, globally distributed.

### 5.2 File Layout

```
website/
  api/
    install.ts          ← Vercel Edge Function
  scripts/
    install.sh          ← canonical install shell script (source of truth)
  vercel.json           ← route: /install → api/install.ts
```

### 5.3 Edge Function Behaviour (`api/install.ts`)

```
GET /install
  User-Agent contains "curl" or "wget"
    → 200 text/plain  — return contents of install.sh
    → fire PostHog server-side event: zombiectl_install
        properties: { os, arch, version: "latest", source: "curl" }

  User-Agent is a browser
    → 302 Location: https://docs.usezombie.com/quickstart
```

### 5.4 Shell Script (`scripts/install.sh`)

The script must:

1. Detect OS (`uname -s` → `darwin` | `linux`) and arch (`uname -m` →
   `arm64` | `x86_64`).
2. Resolve latest release tag from GitHub API
   (`https://api.github.com/repos/usezombie/zombiectl/releases/latest`).
3. Construct binary download URL:
   `https://github.com/usezombie/zombiectl/releases/download/{tag}/zombiectl-{os}-{arch}`.
4. Download to `~/.local/bin/zombiectl` (Linux) or `/usr/local/bin/zombiectl`
   (macOS), `chmod +x`.
5. Print success + next step: `zombiectl login`.
6. Exit non-zero with a human-readable error on any failure.

### 5.5 Dimensions

- 5.1 PENDING Scaffold `api/install.ts` Vercel Edge Function with UA detection
- 5.2 PENDING Write `scripts/install.sh` with OS/arch detection and binary install
- 5.3 PENDING Wire PostHog server-side event `zombiectl_install` from edge function
- 5.4 PENDING Add `vercel.json` route for `/install`
- 5.5 PENDING Unit test: edge function returns script on curl UA, redirect on browser UA
- 5.6 PENDING Smoke test: `curl -sSL https://usezombie.sh/install` returns valid bash

### 5.6 Acceptance Criteria

- [ ] `curl -sSL https://usezombie.sh/install | bash` installs `zombiectl` on macOS arm64, macOS x86_64, Linux amd64
- [ ] Browser visit to `/install` redirects to docs quickstart (no script shown)
- [ ] Each install fires `zombiectl_install` event in PostHog with OS + arch properties
- [ ] Script exits non-zero with a clear message on unsupported platform
