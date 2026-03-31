/**
 * Levenshtein distance and command suggestion utilities.
 */

const KNOWN_COMMANDS = [
  "login",
  "logout",
  "workspace",
  "spec",
  "specs",
  "run",
  "runs",
  "doctor",
  "harness",
  "skill-secret",
  "agent",
];

const KNOWN_SUBCOMMANDS = {
  workspace: ["add", "list", "remove"],
  spec: ["init"],
  specs: ["sync"],
  runs: ["list"],
  run: ["status"],
  harness: ["source", "compile", "activate", "active"],
  "skill-secret": ["put", "delete"],
  agent: ["scores", "profile"],
};

export function levenshteinDistance(a, b) {
  const m = a.length;
  const n = b.length;
  const dp = Array.from({ length: m + 1 }, () => new Array(n + 1).fill(0));

  for (let i = 0; i <= m; i++) dp[i][0] = i;
  for (let j = 0; j <= n; j++) dp[0][j] = j;

  for (let i = 1; i <= m; i++) {
    for (let j = 1; j <= n; j++) {
      const cost = a[i - 1] === b[j - 1] ? 0 : 1;
      dp[i][j] = Math.min(
        dp[i - 1][j] + 1,
        dp[i][j - 1] + 1,
        dp[i - 1][j - 1] + cost,
      );
    }
  }
  return dp[m][n];
}

export function suggestCommand(input, knownCommands = KNOWN_COMMANDS, subcommands = KNOWN_SUBCOMMANDS) {
  const maxDistance = 3;
  const candidates = [];

  // Match against top-level commands
  for (const cmd of knownCommands) {
    const d = levenshteinDistance(input.toLowerCase(), cmd.toLowerCase());
    if (d > 0 && d <= maxDistance) {
      candidates.push({ label: cmd, distance: d });
    }
  }

  // Match against "command subcommand" combos
  for (const [cmd, subs] of Object.entries(subcommands)) {
    for (const sub of subs) {
      const full = `${cmd} ${sub}`;
      const d = levenshteinDistance(input.toLowerCase(), full.toLowerCase());
      if (d > 0 && d <= maxDistance) {
        candidates.push({ label: full, distance: d });
      }
    }
  }

  candidates.sort((a, b) => a.distance - b.distance);
  return candidates.map((c) => c.label);
}
