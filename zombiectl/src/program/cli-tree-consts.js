// Repeat string literals used by cli-tree.js — extracted here so the
// main file stays under the 350-line FLL ceiling. Mostly CLI flag
// signatures, common option labels, and command-name verbs.
//
// Naming: K_*_OPT_FLAG = the commander flag signature; K_*_DESCRIPTION =
// the human-readable label shown next to it.

export const K_WORKSPACE_ID_FULL_OPT_FLAG = "--workspace-id <id>";
export const K_WORKSPACE_OPT_FLAG = "--workspace <id>";
export const K_WORKSPACE_ID_DESCRIPTION = "Workspace ID";
export const K_ZOMBIE_OPT_FLAG = "--zombie <id>";
export const K_ZOMBIE_ID_DESCRIPTION = "Zombie ID";
export const K_CURSOR_TOKEN = "--cursor <token>";
export const K_LIMIT_N = "--limit <n>";
export const K_NEXT_CURSOR_FROM_A_PREVIOUS_PAGE = "next_cursor from a previous page";
export const K_PAGE_SIZE = "Page size";
export const K_LIST = "list";
export const K_LOGOUT = "logout";
export const K_LOGIN = "login";
export const K_DOCTOR = "doctor";
export const K_SHOW = "show";
export const K_ADD = "add";
