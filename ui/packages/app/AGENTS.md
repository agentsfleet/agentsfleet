<!-- BEGIN:nextjs-agent-rules -->

## This is NOT the Next.js you know

This version has breaking changes — APIs, conventions, and file structure may all differ from your training data. Read the relevant guide in `node_modules/next/dist/docs/` (resolved from this file's directory; in monorepos the `next` package may not be visible from the repo root) before writing any code. Heed deprecation notices.

This block is written and re-added by `next dev` — verify at `node_modules/next/dist/server/lib/generate-agent-files.js`. Removing it from a diff only re-creates the uncommitted change; committing it with your work keeps the tree clean.

<!-- END:nextjs-agent-rules -->

## Build

- Use the root package-manager pin with Bun 1.4.x or higher. Preserve Bun lock format 2 so Vercel selects its Bun 1.4 installation path.
- Handle Tailwind's declared plugin-or-processor result in the browser CSS fixture. Keep Next.js dependency pins intact and retain type checking for browser tests.

## Chat ownership

- Use the installed Next.js docs and APIs before changing chat boundaries.
- Keep native request ownership in `DraftClient` and its scoped transport helpers.
- Model mutually exclusive request stages with typed states. Derive the existing view flags from those states.
- Pass each stream's cancellation signal to its reconciliation reads. Stop must cancel those reads before starting cancellation requests.
- Preserve retry identities after uncertain responses and isolate every account, workspace, creator and conversation.
- Keep chat transcript and composer text on the shared `conversation` token, currently 14px.
- Measure settled-message rendering before adding memoization or virtualization. Preserve the existing output window and reduced-motion behavior.
- Derive activity labels from recorded actions and outcomes. Show action and target while running; use completion verbs only for confirmed success. Keep failed, interrupted and unconfirmed outcomes explicit, including environment and access checks.

- Pair activity icons with text: blue for running, green for confirmed success, red for failure, amber for interruption, and neutral for unknown outcomes.
- Keep activity icons, running indicators and primary action text at 14px. Disclosures have a 36px desktop minimum and 44px mobile minimum, with separate action/target lines on mobile; preserve a single visible keyboard-focus outline.

## Billing history

- Fleet deletion retains tenant billing usage. Read names and identifiers from the ledger, without requiring a live fleet row.
- Keep historical UTC month cutoffs when changing fleet or model filters; reset paging and details. Current month returns to the server's current cutoff.
