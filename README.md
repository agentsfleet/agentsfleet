<div align="center">

<img src="branding/usezombie-mark-glow.png" width="180" alt="usezombie" />

# Your deploy failed. The agent already knows why.

[![Try Free — $5 Credit](https://img.shields.io/badge/usezombie-Try_Free_·_%245_Credit-5EEAD4?style=for-the-badge)](https://usezombie.com)
[![Docs](https://img.shields.io/badge/Docs-blue?style=for-the-badge)](https://docs.usezombie.com)
[![CI](https://github.com/usezombie/usezombie/actions/workflows/test.yml/badge.svg?branch=main)](https://github.com/usezombie/usezombie/actions/workflows/test.yml?query=branch%3Amain)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg?style=for-the-badge)](https://opensource.org/licenses/MIT)

</div>

---

> **Early Access Preview.** APIs and CLI may change before GA.

A **Zombie** wakes on your events (webhook · cron · steer), gathers evidence against your infra, posts an evidenced diagnosis to Slack, and records every action in a replayable event log. Markdown-defined. BYOK. Hosted on `api.usezombie.com`.

```bash
npm install -g @usezombie/zombiectl
# inside Claude Code, Amp, Codex CLI, or OpenCode:
/usezombie-install-platform-ops
```

$5 starter credit on signup, no card required. $0.01 per event receipt + $0.10 per stage execution against the credit pool. BYOK token cost goes to your provider.

## Local development

```bash
git clone https://github.com/usezombie/usezombie.git && cd usezombie
cp .env.example .env && bun install
make up && make test
```

Point `zombiectl` at a non-prod backend with `--api`, `ZOMBIE_API_URL`, or `zombiectl login --api <url>`.

## Repos

| Repo | What it is |
|---|---|
| [usezombie/usezombie](https://github.com/usezombie/usezombie) | Control plane + worker + CLI (this repo) |
| [usezombie/docs](https://github.com/usezombie/docs) | User docs ([docs.usezombie.com](https://docs.usezombie.com)) |

All agents read [AGENTS.md](AGENTS.md). MIT — Copyright (c) 2026 usezombie.
