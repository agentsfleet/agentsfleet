const routes = [
  { key: "login", match: (cmd) => cmd === "login" },
  { key: "logout", match: (cmd) => cmd === "logout" },
  { key: "workspace", match: (cmd) => cmd === "workspace" },
  { key: "specs.sync", match: (cmd, args) => cmd === "specs" && args[0] === "sync" },
  { key: "doctor", match: (cmd) => cmd === "doctor" },
  { key: "admin", match: (cmd) => cmd === "admin" },
  // External agent key management + integration grants
  { key: "agent", match: (cmd) => cmd === "agent" },
  { key: "grant", match: (cmd) => cmd === "grant" },
  // Zombie commands — flat top-level for common ops
  { key: "zombie.install", match: (cmd) => cmd === "install" },
  { key: "zombie.list", match: (cmd) => cmd === "list" },
  { key: "zombie.status", match: (cmd) => cmd === "status" },
  { key: "zombie.kill", match: (cmd) => cmd === "kill" },
  { key: "zombie.logs", match: (cmd) => cmd === "logs" },
  { key: "zombie.steer", match: (cmd) => cmd === "steer" },
  { key: "zombie.events", match: (cmd) => cmd === "events" },
  { key: "zombie.credential", match: (cmd) => cmd === "credential" },
];

export function findRoute(command, args) {
  return routes.find((r) => r.match(command, args)) || null;
}
