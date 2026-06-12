# agentsfleet brand assets

Single source of truth for the brand-mark + wordmark used across
GitHub avatars, README hero images, the Mintlify docs site, and
press kits. Aligned to `docs/DESIGN_SYSTEM.md` ("Operational
Restraint") — the cyan-mint pulse is currency, used here exactly
once per asset, never decorated.

## Files

| Asset | Use |
|---|---|
| `agentsfleet-mark.svg` | GitHub avatar, app icon. 512×512, rounded square `--bg` background, single `--pulse` disc. |
| `agentsfleet-mark-glow.svg` / `.png` | Hero contexts where the pulse needs to read as "live" (repo README hero, docs site hero, social cards). Same disc + a static wake-pulse halo. 512×512. |
| `agentsfleet-dark.svg` | Horizontal lockup, Commit Mono "agentsfleet" wordmark for dark surfaces — transparent background, `--pulse` disc, `--text` ink. Docs nav (dark mode), press kits. 600×160. |
| `agentsfleet-light.svg` | Light-surface variant — transparent background, `--pulse-dim` disc + `--bg` ink for contrast on white. Docs nav (light mode). 600×160. |
| `favicon.svg` / `favicon.ico` | The mark cropped for favicon use. Website `public/`, `~/Projects/docs/` root. |

## Where to use each

### GitHub avatar (organisation + profile)

Use `agentsfleet-mark.svg` directly. GitHub renders SVG avatars at
the standard sizes; the rounded-square cropping inside GitHub's
avatar circle preserves the disc-on-dark composition.

Upload via Settings → Profile → Profile picture (org and user
profiles take the same asset). The org slug stays `usezombie`
until the org-rename cutover spec lands.

### `~/Projects/.github/profile/README.md` (org profile)

The lockups are transparent, so a single `<img>` is unreadable on
one of GitHub's two themes — always embed the theme-paired form:

```markdown
<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)"
      srcset="https://raw.githubusercontent.com/usezombie/usezombie/main/branding/agentsfleet-dark.svg">
    <img src="https://raw.githubusercontent.com/usezombie/usezombie/main/branding/agentsfleet-light.svg" alt="agentsfleet" width="360">
  </picture>
</p>
```

(The live org profile currently sources the docs-repo mirrors —
`usezombie/docs` `logo/{dark,light}.svg`; same bytes once the
propagation PR lands, so either source renders identically.)

### `~/Projects/docs/` (Mintlify docs site)

Copy `favicon.svg` + `favicon.ico` to the docs repo root, and
`agentsfleet-dark.svg` / `agentsfleet-light.svg` over
`logo/dark.svg` / `logo/light.svg` (the paths `docs.json` names).
`agentsfleet-mark-glow.svg` replaces `logo/mark-glow.svg` for the
hero block. Mintlify serves SVG favicons natively; no
rasterisation needed.

### Repo `README.md`

Embed `agentsfleet-mark-glow.png` at the top (current shape) or
the lockup pair via the same `<picture>` pattern as the org
profile.

## Source colours

Every hex in these assets traces back to
`ui/packages/design-system/src/tokens.css`:

- `#0A0D0E` — `--bg` (dark mode brand surface; also the light
  lockup's ink). Theme-fixed in the square marks (avatar, favicon,
  glow) — those stay dark even in light surroundings; the lockups
  are theme-paired instead (pick dark or light per surface).
- `#5EEAD4` — `--pulse` (the wake-pulse, currency).
- `#0D9488` — the light-theme `--pulse-dim` (the light lockup's
  disc, holds contrast on white; the root `--pulse-dim` is
  `#2DD4BF` per `docs/DESIGN_SYSTEM.md`).
- `#E6EAEC` — `--text` (dark-theme value; the dark lockup's ink).

If the design system ever shifts those hexes, the branding assets
ship a new release at the same time. The lockup never drifts.
