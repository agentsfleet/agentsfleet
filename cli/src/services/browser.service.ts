// Browser service tag. Pairs with browser.layer.ts (live impl wrapping
// lib/browser.ts:openUrl). Login is the only consumer today; the Effect
// form keeps the dispatcher honest about which commands can shell out
// to a browser.
//
// `open` is fire-and-forget by design: openUrl swallows spawn errors
// and returns a boolean. The success/failure signal is the boolean,
// not a thrown error, so the service's error channel is `never`.

import { Effect, Context } from "effect";

export interface BrowserShape {
  readonly open: (url: string) => Effect.Effect<boolean>;
}

export class Browser extends Context.Service<Browser, BrowserShape>()(
  "agentsfleet/runtime/Browser",
) {}
