const routes = [
  { key: "login", match: (cmd) => cmd === "login" },
  { key: "logout", match: (cmd) => cmd === "logout" },
  { key: "workspace", match: (cmd) => cmd === "workspace" },
  { key: "specs.sync", match: (cmd, args) => cmd === "specs" && args[0] === "sync" },
  { key: "spec.init", match: (cmd, args) => cmd === "spec" && args[0] === "init" },
  { key: "run", match: (cmd) => cmd === "run" },
  { key: "runs.list", match: (cmd, args) => cmd === "runs" && args[0] === "list" },
  { key: "doctor", match: (cmd) => cmd === "doctor" },
  { key: "harness", match: (cmd) => cmd === "harness" },
  { key: "skill-secret", match: (cmd) => cmd === "skill-secret" },
  { key: "agent", match: (cmd) => cmd === "agent" },
  { key: "admin", match: (cmd) => cmd === "admin" },
];

export function findRoute(command, args) {
  return routes.find((r) => r.match(command, args)) || null;
}
