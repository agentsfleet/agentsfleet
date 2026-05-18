// Browser service — wraps lib/browser.ts's openUrl. Login is the only
// caller today; the Effect form keeps the dispatcher honest about
// which commands can shell out to a browser.
//
// `open` is fire-and-forget by design: `openUrl` swallows spawn errors
// and returns a boolean. The success/failure signal is the boolean,
// not a thrown error, so the service's error channel is `never`.

import { Context, Effect, Layer } from "effect";
import { openUrl as openUrlRaw } from "../lib/browser.ts";

export interface BrowserShape {
  readonly open: (url: string) => Effect.Effect<boolean>;
}

export class Browser extends Context.Tag("Browser")<Browser, BrowserShape>() {}

export const BrowserLive: Layer.Layer<Browser> = Layer.succeed(Browser, {
  open: (url: string) => Effect.promise(() => openUrlRaw(url)),
});

// Test-time helper — substitutes a capture-only `open` so login tests
// don't shell out. Returns the original Layer shape so the type
// checker treats it as interchangeable with BrowserLive.
export const BrowserNoop = (
  onOpen?: (url: string) => void,
): Layer.Layer<Browser> =>
  Layer.succeed(Browser, {
    open: (url: string) =>
      Effect.sync(() => {
        onOpen?.(url);
        return false;
      }),
  });
