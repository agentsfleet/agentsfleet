const routes = [
  { key: "login", match: (cmd) => cmd === "login" },
  { key: "logout", match: (cmd) => cmd === "logout" },
  { key: "workspace", match: (cmd) => cmd === "workspace" },
  { key: "specs.sync", match: (cmd, args) => cmd === "specs" && args[0] === "sync" },
  { key: "spec.init", match: (cmd, args) => cmd === "spec" && args[0] === "init" },
  { key: "run", match: (cmd) => cmd === "run" },
  { key: "runs.list", match: (cmd, args) => cmd === "runs" && args[0] === "list" },
  // M17_001 §3: runs cancel
  { key: "runs.cancel", match: (cmd, args) => cmd === "runs" && args[0] === "cancel" },
  // M22_001 §5: runs replay
  { key: "runs.replay", match: (cmd, args) => cmd === "runs" && args[0] === "replay" },
  // M21_001 §3: runs interrupt
  { key: "runs.interrupt", match: (cmd, args) => cmd === "runs" && args[0] === "interrupt" },
  { key: "doctor", match: (cmd) => cmd === "doctor" },
  { key: "harness", match: (cmd) => cmd === "harness" },
  { key: "skill-secret", match: (cmd) => cmd === "skill-secret" },
  { key: "agent", match: (cmd) => cmd === "agent" },
  { key: "admin", match: (cmd) => cmd === "admin" },
  // M1_001 §5: Zombie commands — flat top-level for common ops
  { key: "zombie.install", match: (cmd) => cmd === "install" },
  { key: "zombie.up", match: (cmd) => cmd === "up" },
  { key: "zombie.status", match: (cmd) => cmd === "status" },
  { key: "zombie.kill", match: (cmd) => cmd === "kill" },
  { key: "zombie.logs", match: (cmd) => cmd === "logs" },
  { key: "zombie.credential", match: (cmd) => cmd === "credential" },
];

export function findRoute(command, args) {
  return routes.find((r) => r.match(command, args)) || null;
}
