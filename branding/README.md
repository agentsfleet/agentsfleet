# usezombie brand assets

Single source of truth for the brand-mark + wordmark used across
GitHub avatars, README hero images, the Mintlify docs site, and
press kits. Aligned to `docs/DESIGN_SYSTEM.md` ("Operational
Restraint") — the cyan-mint pulse is currency, used here exactly
once per asset, never decorated.

## Files

| Asset | Use |
|---|---|
| `usezombie-mark.svg` | GitHub avatar, favicon, app icon. 512×512, rounded square `--bg` background, single `--pulse` disc. |
| `usezombie-mark-glow.svg` | Hero contexts where the pulse needs to read as "live" (docs site hero, social cards). Same disc + a static wake-pulse halo. 512×512. |
| `usezombie-wordmark.svg` | Horizontal lockup with Commit Mono "usezombie" wordmark. README hero, docs nav, press kits. 720×160. |

## Where to use each

### GitHub avatar (organisation + profile)

Use `usezombie-mark.svg` directly. GitHub renders SVG avatars at
the standard sizes; the rounded-square cropping inside GitHub's
avatar circle preserves the disc-on-dark composition.

Upload via Settings → Profile → Profile picture (org and user
profiles take the same asset).

### `~/Projects/.github/profile/README.md` (org profile)

Embed the wordmark at the top:

```markdown
<p align="center">
  <img src="https://raw.githubusercontent.com/usezombie/usezombie/main/branding/usezombie-wordmark.svg" alt="usezombie" width="360">
</p>
```

### `~/Projects/docs/` (Mintlify docs site)

Copy `usezombie-mark.svg` into `~/Projects/docs/favicon.svg` and
`usezombie-mark-glow.svg` into the docs hero block. Mintlify
serves SVG favicons natively; no rasterisation needed.

### Repo `README.md`

Embed the wordmark at the top, same shape as the org profile.

## Source colours

The two hex values used in every asset trace back to
`ui/packages/design-system/src/tokens.css`:

- `#0A0D0E` — `--bg` (dark mode brand surface). Theme-fixed in the
  branding context — the mark stays dark even in light surroundings.
- `#5EEAD4` — `--pulse` (the wake-pulse, currency).

If the design system ever shifts those hexes, the branding assets
ship a new release at the same time. The lockup never drifts.
