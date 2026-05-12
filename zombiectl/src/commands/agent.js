// Re-export of the external-agent error map. The dispatcher was deleted
// when the commander refactor landed (cli-tree.js wires `agent add/list/delete`
// directly to the leaf handlers in agent_external.js).

import { AUTH_PRESET, compose } from "../lib/error-map-presets.js";

// Agent commands hit /v1/workspaces/{ws}/agent-keys (POST/GET/DELETE).
// Server-side these can surface validation, conflict on duplicate
// names, and not-found on delete. AUTH_PRESET covers the auth leg;
// area-specific codes expand here as the audit surfaces them.
export const errorMap = compose(AUTH_PRESET);

export { commandAgentAdd, commandAgentList, commandAgentDelete } from "./agent_external.js";
