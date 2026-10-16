# agentsfleet brand kit

The signal mark represents a configured fleet receiving a signal and doing its job. Eight strokes converge on a steady centre. The logo itself stays static; motion describes work in the product.

The shared identity is owned by [`BrandMark.tsx`](../ui/packages/design-system/src/design-system/BrandMark.tsx), [`tokens.css`](../ui/packages/design-system/src/tokens.css), and the [design system guide](../docs/DESIGN_SYSTEM.md). These exports match that identity. The website and app use the shared package; the documentation site mirrors its palette and assets.

## Ready to upload

| File | Use |
| --- | --- |
| `avatar-512.png`, `github-avatar.png` | Square avatar for GitHub, X, or community profiles; padded for circular crops |
| `avatar-1024.png` | Larger square avatar and press image |
| `agentsfleet-mark.svg` | Scalable mark on charcoal |
| `agentsfleet-mark-transparent.svg`, `.png` | Mint mark without a background; use on dark surfaces |
| `agentsfleet-dark.svg`, `.png` | Transparent horizontal wordmark for dark backgrounds |
| `agentsfleet-light.svg`, `.png` | Transparent horizontal wordmark for light backgrounds |
| `github-social.png`, `.svg` | 1280 × 640 repository social preview |
| `favicon.svg`, `favicon.ico` | Browser icon |

Use the PNG avatars for profile uploads. No accounts or external profiles have been updated by generating these files.

The `agentsfleet-mark-glow.svg` and `.png` files now contain the same static signal mark. Those existing filenames are retained by the companion docs site; there is no glow treatment.

## Colours and typography

| Role | Dark | Light |
| --- | --- | --- |
| Page | `#151716` | `#f8f6f0` |
| Main text | `#f2f0e9` | `#14251f` |
| Signal / link | `#b8f0ce` | `#06695e` |

Interface text uses Instrument Sans, display headings Bricolage Grotesque, and code Commit Mono. SVG wordmarks outline Instrument Sans at weight 500. Social previews outline Bricolage Grotesque headings and Instrument Sans labels. SVG and PNG exports retain that lettering without installed fonts. Keep the brand lockup static and avoid stretching, gradients, and shadows. The chat activity loader can rotate the signal while it describes work. Keep at least one quarter of the mark’s width clear around it.

## Propagation

- Website favicon and app icons reference `ui/packages/design-system/assets/brand.svg`.
- Docs use `logo/dark.svg`, `logo/light.svg`, `logo/mark-glow.svg`, and `favicon.svg` in the companion docs branch.
- When changing the identity, update the shared component and tokens first, then regenerate this kit and refresh the docs mirrors together.
- Deployment and uploads to GitHub, X, or other services are separate actions.

## Branding coverage checked October 14, 2026

The full wordmark belongs in README headers and social previews. The signal mark stays readable in small avatars and browser tabs.
Website/app SVG icons resolve to the shared design-system source. ICO fallback, Apple touch icon, and website social metadata carry the same identity.
The former orange design-system asset copies now match the shared mark and wordmarks.
Invite HTML uses the charcoal, warm-white, and mint palette with a full mail-safe text wordmark.

GitHub organization name, description, and website were updated through the authorized API and read back.
Organization-avatar and repository-social-image uploads still need a signed-in browser; GitHub's public organization-update API has no avatar field.
The prepared files above are ready for those uploads. Local README changes take effect remotely only when published.

Remaining external surfaces: Clerk Account Portal branding, verification/reset emails, SMTP sender display name and delivery-domain settings,
GitHub App icon and OAuth consent branding, npm/package social pages, community profiles, and product screenshots in docs.
CLI installer publication and the unresolved agentsfleet.sh domain are separate release work.
