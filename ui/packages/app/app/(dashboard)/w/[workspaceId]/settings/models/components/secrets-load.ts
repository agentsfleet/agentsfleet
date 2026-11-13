// Load states for the workspace's stored-secret list.
//
// The form needs metadata for compatible saved-key choices and held-name
// validation. Keep creation disabled until the list arrives. Storage itself
// is create-if-absent; rotation uses the explicit replacement action.
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
