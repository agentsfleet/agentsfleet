// Signal names usable with process.on(name, handler) and child.kill(name).
// The integer signal numbers live in os.constants.signals — those are for
// syscalls; process.on() expects the string name, which is what we export.

export const SIGINT = "SIGINT";
export const SIGTERM = "SIGTERM";
