// Load states for the workspace's stored-secret list.
//
// The list is LOAD-BEARING for the Add-model dialog, not decoration: submit
// resolves rotate-vs-create against it and refuses a name owned by a
// different provider, and the secrets POST is a server-side upsert — so
// submitting against a list that never arrived silently overwrites whatever
// already holds that name. The dialog therefore fails closed: Save stays
// disabled unless the list is `ready`.
//
// `ready` is sticky by design (see ModelsRegistryTable.refreshSecrets): a
// failed REFRESH keeps the last good list live rather than locking a form
// that still has usable data; only a list that never loaded blocks.

export const SECRETS_LOAD = {
  /** Never requested — the dialog has not been opened yet. */
  idle: "idle",
  loading: "loading",
  ready: "ready",
  error: "error",
} as const;

export type SecretsLoad = (typeof SECRETS_LOAD)[keyof typeof SECRETS_LOAD];
