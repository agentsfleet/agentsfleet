# M3_007: Fix Website Launch-Blocker Routes And CTAs

**Prototype:** v1.0.0
**Milestone:** M3
**Workstream:** 007
**Date:** Mar 06, 2026
**Status:** ✅ DONE (Mar 06, 2026)
**Priority:** P0 — release blocker
**Batch:** B1 — React-only, no Zig deps
**Depends on:** M3_006 (Implement Clerk Authentication Contract)

---

## 1.0 Singular Function

**Status:** ✅ DONE

Implement one working website function: launch-critical routes and CTAs resolve correctly.

**Dimensions:**
- 1.1 ✅ DONE Fix "View Full Pricing" navigation to avoid 404
- 1.2 ✅ DONE Normalize all Discord links to `https://discord.gg/H9hH2nqQjh`
- 1.3 ✅ DONE Fix Human/Agents mode toggle state mismatch
- 1.4 ✅ DONE Correct footer labels/links in Agents surfaces (Rename to Agents)
- 1.5 ✅ DONE Review and use any hooks/functionality of React from 19.x
   
---

## 2.0 Verification Units

**Status:** ✅ DONE

**Dimensions:**
- 2.1 ✅ DONE Unit/e2e check: pricing CTA lands on `/pricing`
- 2.2 ✅ DONE Unit/e2e check: all Discord CTAs match canonical URL
- 2.3 ✅ DONE Unit check: mode toggle updates visible state consistently
- 2.4 ✅ DONE Unit check: footer link/copy snapshot matches expected output


## 3.0 Branding/Logo/Background

**Status:** ✅ DONE

**Dimensions:**

- 3.1 ✅ DONE Brand/logo redesign and hero visual enhancements
- 3.2 ✅ DONE Background picture animation like https://factory.ai, https://usegitai.com
- 3.3 ✅ DONE https://usezombie.sh (Agents toggle) to have the `Install the Git Extension` like `Install Zombiectl` with a box, Read the docs, Setup your personal dashboard(double bordered), the buttons are cool here and with the url curl -sSL https://usezombie.sh/install | bash (also refer https://actors.dev)
- 3.4 ✅ DONE https://usezombie.com will have Opensource, and Hobby(3 default agents, scott, echo, warden ...) and other Paid Plans with unlimited agents and harness to create, and what ever plan you decide?.
- 3.5 ✅ DONE Privacy/terms legal expansion


## 4.0 Acceptance Criteria

**Status:** ✅ DONE

- [x] 4.1 Launch-blocker website path is stable and repeatable
- [x] 4.2 No known route/CTA regressions remain in primary landing flow
- [x] 4.3 92/92 unit tests passing; E2E suite updated for all new behavior
