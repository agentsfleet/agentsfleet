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
| `agentsfleet-dark.svg` | Horizontal lockup, Commit Mono "agentsfleet" wordmark on the dark brand surface. Docs nav (dark mode), press kits. 720×160. |
| `agentsfleet-light.svg` | Light-surface variant of the lockup. Docs nav (light mode). 720×160. |
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

Embed the dark lockup at the top:

```markdown
<p align="center">
  <img src="https://raw.githubusercontent.com/usezombie/usezombie/main/branding/agentsfleet-dark.svg" alt="agentsfleet" width="360">
</p>
```

### `~/Projects/docs/` (Mintlify docs site)

Copy `favicon.svg` + `favicon.ico` to the docs repo root, and
`agentsfleet-dark.svg` / `agentsfleet-light.svg` over
`logo/dark.svg` / `logo/light.svg` (the paths `docs.json` names).
`agentsfleet-mark-glow.svg` replaces `logo/mark-glow.svg` for the
hero block. Mintlify serves SVG favicons natively; no
rasterisation needed.

### Repo `README.md`

Embed `agentsfleet-mark-glow.png` at the top (current shape) or
the dark lockup, same shape as the org profile.

## Source colours

The two hex values used in every asset trace back to
`ui/packages/design-system/src/tokens.css`:

- `#0A0D0E` — `--bg` (dark mode brand surface). Theme-fixed in the
  branding context — the mark stays dark even in light surroundings.
- `#5EEAD4` — `--pulse` (the wake-pulse, currency).

If the design system ever shifts those hexes, the branding assets
ship a new release at the same time. The lockup never drifts.
