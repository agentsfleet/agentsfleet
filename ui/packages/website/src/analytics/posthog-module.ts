// Module-path constant for the lazy posthog-js loader. Lives here so
// the typeof-import type alias (which needs a string literal) and the
// runtime dynamic import can share a single source of truth.
export const POSTHOG_MODULE = "posthog-js";
