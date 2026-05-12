/**
 * Command-action verbs — the string the CLI parser reads as `action`
 * inside per-group dispatchers (e.g. `if (action === ACTION_ADD)`).
 *
 * One name, one source. RULE UFS — every dispatcher reads from here.
 */

export const ACTION_ADD = "add";
export const ACTION_LIST = "list";
export const ACTION_DELETE = "delete";
export const ACTION_SHOW = "show";
export const ACTION_USE = "use";
export const ACTION_INSTALL = "install";
export const ACTION_STATUS = "status";
export const ACTION_KILL = "kill";
export const ACTION_STOP = "stop";
export const ACTION_RESUME = "resume";
export const ACTION_LOGS = "logs";
export const ACTION_EVENTS = "events";
export const ACTION_STEER = "steer";
export const ACTION_CREDENTIAL = "credential";
export const ACTION_CREDENTIALS = "credentials";
export const ACTION_PROVIDER = "provider";
