# Website job entrance

Read this when changing the public composer, sample conversation, signup boundary, or shared brand assets.
The website illustrates the [chat authoring experience](high_level.md#fleet-authoring-and-runtime); the illustration does not author, install, or run a fleet.

## Experience and component boundaries

“What should your fleet handle?” is the welcome's one question, and one composer sits below it.
Investigate an incident, Catch test regressions, and Automate recurring work open their own animated examples directly.
The recurring-work example prepares a weekly release brief.

Opening an example hides the composer and keeps the person's unsent request. Back to home restores that request.

| Component | Responsibility |
|---|---|
| `Hero` and `JobComposer` | Welcome and one controlled request input |
| `JobSuggestions` | Open the selected example without submitting the visitor's request or creating an account |
| `CliQuickstart` | Shared copyable Terminal, coding-agent names, installation guide, and repository self-hosting link |
| `HowItWorks` | Example container, motion controls, and Create fleet action |
| `IncidentFlow` and `StoryMessage` | Bounded chat viewport, appended messages, named loader, citations, and inline evidence |
| Narrative timing module | Deterministic setup-to-result narrative and text timing |
| `use-scene-motion` and `use-motion-environment` | Playback, visibility, hidden-tab pause, and reduced-motion behavior |
| `use-chat-scroll` | Follow appended output, keep the manual reading position, and jump to latest |

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

Messages type and append to the current history; the caret blinks during typing.
The signal loader names the current activity, such as understanding the job or preparing the setup.
Later output scrolls only the example's inner viewport.

There are no top stage tabs, Replay control, or Tools & setup preview.
Pause and Read full conversation keep the illustration inspectable.
The conversation viewport is a keyboard focus stop.

Read full conversation moves focus to the stable Play control before removing its reveal button.
Manual scrolling stops following the latest output while playback continues. Jump to latest resumes following and moves focus to the stable Pause control.

Focus within the illustration, hidden tabs, and leaving the viewport pause automatic playback.
Visibility recovery keeps the reading position and evidence. Explicit Play resets inspection and following.

Reduced motion shows the complete conversation without typing or rotating loaders.
Mobile uses the same conversation in a bounded viewport.
Selecting evidence reveals sample excerpts within that viewport; closing restores the preceding chat position.

The example labels sample output and unapplied changes. The incident, regression, and agent examples use real Linkwarden repository sources pinned to an immutable revision; run excerpts stay illustrative.
The current runtime's access rules, lease behavior, and delivery limits come from the [runtime comparison](../reviews/workspace-setup/runtime-baseline.md).

The animation makes no claim that it performs automatic repair, GitHub posting or private draft API calls.

## Signup and installer boundaries

Public hosted-access links open the waitlist in a new tab.
`VITE_WAITLIST_URL` overrides the default `https://accounts.agentsfleet.net/waitlist` in the website's `config.ts`.
All access links use `PUBLIC_ACCESS_URL`, keeping the original page and request when the visitor closes the waitlist.

The request stays in React memory; reload clears it.
Private input is not placed in URL parameters, analytics, session storage, or local storage.
No website-to-account draft handoff is implemented.

The earlier setup preview, inactive draft export, and unused Pretext dependency have been removed.

The installer preview uses `curl -fsSL https://agentsfleet.dev | bash`.
The endpoint serves a script, but its public package stays unpublished, as Indy confirmed.
The website links the installation guide and repository quick start.

The website has no release-pending or preview copy; the installer was not run during verification.
Claude Code, Codex, and OpenCode are entry suggestions, not claims of implemented assistant-specific installation workflows.
Command-Line Interface (CLI) behavior stays a separate design task.

## Click analytics and approved starting offer

The website records explicit PostHog click attribution for signup actions, installation and self-hosting links, and Copy.
It also records all three suggestions, Read full conversation, source links, feedback, navigation, and footer links.
The [website event catalog](product_analytics.md#website-marketing) defines event names and stable sources.

Events contain the source, surface, fixed target or mode, and pathname. They never contain request text, clipboard text, query parameters, or the hash.
The shared Terminal exposes a copy-click callback; the website owns the analytics handler.

Indy approved “$0 to start · $25 credit included” on 2026-10-14, replacing the earlier early-access copy.
The welcome and pricing card share this offer. The card explains that included credit runs the first jobs and using a personal key bills the provider directly.

Join waitlist opens the configured waitlist. The source strip uses “Hosted waitlist ↗,” “Self hosting in your infrastructure ↗,” and plain “GitHub ↗” without an icon.

Pricing copy does not promise unlimited usage or behavior after the credit is spent. The pricing banner emits no custom event and excludes automatic capture.

The footer's use cases link opens the existing example conversation.

## Shared branding and external surfaces

Website favicon, app icon, and app brand asset resolve to the canonical design-system Scalable Vector Graphics (SVG) file.
Local website assets also include a 256-pixel icon fallback, 180-pixel Apple touch icon, and 1280-by-640-pixel social image.
Website metadata references those local assets.

Full wordmarks suit README headings, email headers, and repository social images.
The compact signal suits square avatars, browser icons, and the working loader.
The [brand asset index](../../branding/README.md) records remaining provider-managed branding surfaces.

Clerk-managed authentication emails and branding settings stay external to these source changes.
Updating local assets does not publish GitHub avatar/social images or deploy website metadata.

## Verification record

The website coverage gate requires 100% statements, branches, functions, and lines, and `make _dry_website` runs the browser checks.
Browser checks cover themes, responsive layout, keyboard access, reduced motion, typing, looping, pause, citations, suggestions, and cancelled signup.
Unused preview and measurement modules were removed with their tests; remaining tests cover the visitor's actions.

PostHog wiring tests exercise the real website and analytics wrapper, replacing only the external client boundary.
They verify click attribution, private-input exclusion, buffering, and failure containment.
Indy assigned live event delivery to the development-app test: “we can check that in the app-dev test”.

The application production build and full repository unit and integration gates stay separate checks at the Pull Request boundary.
M227_001 delivered the workspace-chat application and M227_002 the website, analytics, and branding scope in the same Pull Request.
