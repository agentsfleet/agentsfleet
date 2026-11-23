import { readFileSync } from "node:fs";

// Vite can split shared code into different chunks as the other routes change.
// Count every script and module preload that the built homepage requests.
const html = readFileSync(new URL("./dist/index.html", import.meta.url), "utf8");
const scripts = [...html.matchAll(/(?:src|href)="(\/assets\/[^"]+\.js)"/g)]
  .map(([, asset]) => `dist${asset}`);

export default [
  { name: "landing (critical path)", path: [...new Set(scripts)], limit: "128 kB", gzip: true },
  { name: "landing css", path: "dist/assets/index-*.css", limit: "20 kB", gzip: true },
];
