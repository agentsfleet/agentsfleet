// command-registry — collects every CLI command into a single table that
// the dispatcher in cli.js routes through runCommand({ name, errorMap,
// handler }). Each entry carries:
//   - name: route key (matches routes.js, becomes the analytics command tag)
//   - errorMap: per-UZ-* code → user-facing { code, message }; populated
//     by §2. Empty {} is a valid pass-through (runCommand falls back to
//     the server's bare code+message).
//   - handler: (args) => Promise<exitCode>
//
// All 14 commands MUST appear here; the audit (§4) enforces it.

function entry(name, handler, errorMap = {}) {
  return { name, handler, errorMap };
}

export function registerProgramCommands(handlers) {
  return {
    login: entry("login", handlers.login),
    logout: entry("logout", handlers.logout),
    workspace: entry("workspace", handlers.workspace),
    doctor: entry("doctor", handlers.doctor),
    agent: entry("agent", handlers.agent),
    grant: entry("grant", handlers.grant),
    tenant: entry("tenant", handlers.tenant),
    billing: entry("billing", handlers.billing),
    "zombie.install": entry("zombie.install", handlers.zombieInstall),
    "zombie.list": entry("zombie.list", handlers.zombieList),
    "zombie.status": entry("zombie.status", handlers.zombieStatus),
    "zombie.kill": entry("zombie.kill", handlers.zombieKill),
    "zombie.stop": entry("zombie.stop", handlers.zombieStop),
    "zombie.resume": entry("zombie.resume", handlers.zombieResume),
    "zombie.delete": entry("zombie.delete", handlers.zombieDelete),
    "zombie.logs": entry("zombie.logs", handlers.zombieLogs),
    "zombie.steer": entry("zombie.steer", handlers.zombieSteer),
    "zombie.events": entry("zombie.events", handlers.zombieEvents),
    "zombie.credential": entry("zombie.credential", handlers.zombieCredential),
  };
}
