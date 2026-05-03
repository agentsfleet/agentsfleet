# usezombie host skills

This directory ships agent skills that drive `zombiectl` non-interactively
from inside a host that supports the Anthropic-style skill format
(Claude Code, Amp, Codex CLI, OpenCode). Each subdirectory is one skill,
named after the slash-command it registers — `usezombie-install-platform-ops/`
becomes `/usezombie-install-platform-ops` in every supported host.

## What gets distributed

The skills travel with `@usezombie/zombiectl` on npm. Two install commands,
in order:

```bash
npm install -g @usezombie/zombiectl    # CLI binary + bundled skills + samples
npx skills add usezombie/usezombie     # symlinks /usezombie-* into host skill dirs
```

`npm install -g` lays down the binary and runs `scripts/postinstall.js`,
which copies the bundled `samples/` tree into `~/.config/usezombie/samples/`
so skills can read the canonical templates from a stable local path
(no URL fetch, no cache — the npm package version is the template version).

`npx skills add` symlinks each skill in this directory into every supported
host's skill path that exists on the user's machine:
`~/.claude/skills/`, `~/.codex/skills/`, `~/.amp/skills/`, `~/.opencode/skills/`.

## Manual symlink fallback

If `npx skills` is unavailable or the user is on a host the registry
doesn't know about yet, the documented fallback is one symlink per skill
per host directory:

```bash
ln -s "$(npm root -g)/@usezombie/zombiectl/skills/usezombie-install-platform-ops" \
  ~/.claude/skills/usezombie-install-platform-ops
```

Same shape for the other host dirs.

## What lives here

| Skill | Slash command | Purpose |
|---|---|---|
| [`usezombie-install-platform-ops/`](./usezombie-install-platform-ops/SKILL.md) | `/usezombie-install-platform-ops` | One-command install of the platform-ops zombie on the user's repo: doctor preflight, repo detection, credential resolution, webhook setup, in-flow webhook self-test, smoke-test steer. |

Future skills follow the same shape — one subdirectory per slash-command,
top-level `SKILL.md` in the Resend-pattern frontmatter format,
references in a sibling `references/` directory.
