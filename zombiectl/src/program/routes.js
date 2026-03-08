const routes = [
  { key: "login", match: (cmd) => cmd === "login" },
  { key: "logout", match: (cmd) => cmd === "logout" },
  { key: "workspace", match: (cmd) => cmd === "workspace" },
  { key: "specs.sync", match: (cmd, args) => cmd === "specs" && args[0] === "sync" },
  { key: "run", match: (cmd) => cmd === "run" },
  { key: "runs.list", match: (cmd, args) => cmd === "runs" && args[0] === "list" },
  { key: "doctor", match: (cmd) => cmd === "doctor" },
  { key: "harness", match: (cmd) => cmd === "harness" },
  { key: "skill-secret", match: (cmd) => cmd === "skill-secret" },
];

export function findRoute(command, args) {
  return routes.find((r) => r.match(command, args)) || null;
}
