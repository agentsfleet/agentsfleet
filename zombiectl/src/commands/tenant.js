// Tenant errorMap + re-exports of the provider posture handlers.
// The hand-rolled dispatcher was deleted when the commander refactor
// landed; cli-tree.js routes `tenant provider {show|add|delete}`
// directly to the leaf handlers in tenant_provider.js.

import { AUTH_PRESET, compose } from "../lib/error-map-presets.js";

// Tenant provider posture and billing snapshot. Auth-only at the CLI
// surface; provider-side validation codes are not yet keyed here and
// fall through to bare server messages.
export const errorMap = compose(AUTH_PRESET);

export {
  commandTenantProviderShow,
  commandTenantProviderAdd,
  commandTenantProviderDelete,
} from "./tenant_provider.js";
