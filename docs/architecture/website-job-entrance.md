# Website job entrance

Read this when changing the public composer, sample conversation, signup boundary, or shared brand assets.
The website illustrates the [chat authoring experience](high_level.md#fleet-authoring-and-runtime); the illustration does not author, install, or run a fleet.

## Experience and component boundaries

The welcome centers one composer below “What should your fleet handle?”
Investigate an incident, Catch test regressions, and Automate recurring work open their own animated examples directly.
The recurring-work example prepares a weekly release brief.
Opening an example hides the composer and preserves the person's unsent request. Back to home restores that request.

| Component | Responsibility |
|---|---|
| `Hero` and `JobComposer` | Welcome and one controlled request input |
| `JobSuggestions` | Open the selected example without submitting the visitor's request or creating an account |
| `CliQuickstart` | Shared copyable Terminal, coding-agent names, installation guide, and repository self-hosting link |
| `HowItWorks` | Example container, motion controls, and Create fleet action |
| `IncidentFlow` and `StoryMessage` | Bounded chat viewport, appended messages, named loader, citations, and inline evidence |
| Narrative timing module | Deterministic setup-to-result narrative and text timing |
| `use-scene-motion` and `use-motion-environment` | Playback, visibility, hidden-tab pause, and reduced-motion behavior |
| `use-chat-scroll` | Follow appended output, preserve manual reading position, and jump to latest |

These components use existing design-system primitives and charcoal, off-white, and mint tokens.
Bricolage Grotesque provides headings, Instrument Sans provides messages, and Commit Mono provides commands and code.
Small signal icons connect welcome, assistant replies, and the working loader.

## What the animation shows

```text
Describe the job --> clarify --> /schedule --> review setup --> Save fleet
       |
       +--> created fleet
                     |
              next scheduled check
                     |
       collect evidence --> finding --> proposed change --> access boundary
                     |
              report ready --> Create fleet
                     |
                 loop the example

Separate explicit action: Save to library --> reusable Library revision
Ordinary Save creates the fleet; it makes no Library-publication claim.
```

The sample story reports fleet creation after ordinary Save and makes no Library-publication claim.
The website's machine-readable summaries describe native chat setup and separate, explicit Library publication.
The changed browser assertion and remaining visual proofs still need to run against the completed implementation.

Messages type and append to the current history; the caret blinks during typing.
The signal loader names the current activity, such as understanding the job or preparing the setup.
Later output scrolls only the example's inner viewport.

There are no top stage tabs, Replay control, or Tools & setup preview.
Pause and Read full conversation keep the illustration inspectable.
The conversation viewport is a keyboard focus stop. Read full conversation moves focus to the stable Play control before removing its reveal button.
Manual scrolling stops following the latest output while playback continues. Jump to latest resumes following and moves focus to the stable Pause control.
Focus within the illustration, hidden tabs, and leaving the viewport pause automatic playback.
Visibility recovery preserves the reading position and evidence. Explicit Play resets inspection and following.

Reduced motion shows the complete conversation without typing or rotating loaders.
Mobile uses the same conversation in a bounded viewport.
Selecting evidence reveals sample excerpts within that viewport; closing restores the preceding chat position.

The example labels sample output and unapplied changes. The incident, regression, and agent examples use real Linkwarden repository sources pinned to an immutable revision; run excerpts remain illustrative.
The current runtime's access rules, lease behavior, and delivery limits come from the [runtime comparison](../reviews/workspace-setup/runtime-baseline.md).

The animation makes no claim that it performs automatic repair, GitHub posting or private draft API calls.

## Signup and installer boundaries

Public hosted-access links open the waitlist in a new tab.
`VITE_WAITLIST_URL` overrides the default `https://accounts.agentsfleet.net/waitlist` in `ui/packages/website/src/config.ts`.
All access links use `PUBLIC_ACCESS_URL`, preserving the original page and request when the visitor closes the waitlist.
The request remains in React memory; reload clears it.

Private input is not placed in URL parameters, analytics, session storage, or local storage.
No website-to-account draft handoff is implemented.
The earlier setup preview, inactive draft export, and unused Pretext dependency have been removed.

The installer preview uses `curl -fsSL https://agentsfleet.dev | bash`.
The endpoint serves a script, but its public package remains unpublished, as Indy confirmed.
The website links the installation guide and repository quick start. It has no release-pending or preview copy; the installer was not run during verification.

Claude Code, Codex, and OpenCode are entry suggestions, not claims of implemented assistant-specific installation workflows.
Command-Line Interface (CLI) behavior remains a separate design task.

## Click analytics and approved starting offer

The website records explicit PostHog click attribution for signup actions, installation and self-hosting links, Copy, all three suggestions, Read full conversation, source links, feedback, navigation, and footer links.
The [website event catalog](product_analytics.md#website-marketing) defines event names and stable sources.
Events contain the source, surface, fixed target or mode, and pathname. They never contain request text, clipboard text, query parameters, or the hash.
The shared Terminal exposes a copy-click callback; the website owns the analytics handler.

Indy approved “$0 to start · $25 credit included” on 2026-10-14, replacing the earlier early-access copy.
The welcome and pricing card share this offer. The card explains that included credit runs the first jobs and using a personal key bills the provider directly.
Join waitlist opens the configured waitlist. The source strip uses “Hosted waitlist ↗,” “Self hosting in your infrastructure ↗,” and plain “GitHub ↗” without an icon.
Pricing copy does not promise unlimited usage or behavior after the credit is spent. The pricing banner emits no custom event and excludes automatic capture.
The footer's use cases link opens the existing example conversation.

## Shared branding and external surfaces

Website favicon, app icon, and app brand asset resolve to the canonical design-system SVG.
Local website assets also include a 256-pixel icon fallback, 180-pixel Apple touch icon, and 1280-by-640-pixel social image.
Website metadata references those local assets.

Full wordmarks suit README headings, email headers, and repository social images.
The compact signal suits square avatars, browser icons, and the working loader.
The [brand asset index](../../branding/README.md) records remaining provider-managed branding surfaces.

Clerk-managed authentication emails and branding settings remain external to these source changes.
Updating local assets does not publish GitHub avatar/social images or deploy website metadata.

## Verification record

Recorded 2026-10-14 in the single `feat/website-completion` worktree. Each result establishes the scope named below.

| Check | Observed result | Local evidence |
|---|---|---|
| Website coverage, shuffled with seed `20261014` | 18 test files, 127 tests passed; all 27 production files have 100% statements, branches, functions, and lines | `/private/tmp/agentsfleet-completion-website-coverage.log` |
| `BASE_URL=http://127.0.0.1:5173 make _dry_website` | All 135 browser checks passed on the final code | `/private/tmp/agentsfleet-completion-browser-green.log` |
| `make lint-all` | All repository lint checks passed | `/private/tmp/agentsfleet-completion-lint-all-final.log` |
| Website production build | Completed | `/private/tmp/agentsfleet-completion-build.log` |
| Website size limits | JavaScript 108.9 kilobytes compressed against 128; Cascading Style Sheets (CSS) 14.44 kilobytes compressed against 20 | `/private/tmp/agentsfleet-completion-size.log` |
| `make check-version` | The CLI and Rust workspace release versions match `0.59.0` | `/private/tmp/agentsfleet-completion-version.log` |
| Invite mail package | Canonical daemon-library target passed 30 package tests | `/private/tmp/agentsfleet-invite-mail-validation.log` |

Those recorded runs cover the earlier website snapshot, including click attribution, pricing, and authentication destinations. The ordinary Save copy correction is implemented; its browser proof and the current keyboard and summary changes remain unverified by those runs.
Browser checks cover themes, responsive layout, keyboard access, reduced motion, typing, looping, pause, citations, suggestions, and cancelled signup.
Unused preview and measurement modules were removed with their tests; remaining tests cover the visitor's actions.

PostHog wiring tests exercise the real website and analytics wrapper, replacing only the external client boundary.
They verify click attribution, private-input exclusion, buffering, and failure containment.
Indy assigned live event delivery to the development-app test: “we can check that in the app-dev test”.

The application production build and full repository unit and integration gates remain separate checks at the Pull Request boundary.
The application implementation follows the active M227_001 workspace-chat spec; M227_002 supplies the website, analytics, and branding scope in the same Pull Request.
