import { readFileSync } from "node:fs";
import { isAbsolute, relative, resolve } from "node:path";

type ReadRow = { ts: number; path: string; blob: string; section: string };
const DIGEST = /^[a-f0-9]+$/;
const STRING_TYPE = "string";

function escaped(value: string): string { return JSON.stringify(value).slice(1, -1); }

function readRow(value: unknown): ReadRow | undefined {
  if (!value || typeof value !== "object" || Array.isArray(value)) return undefined;
  const row = value as Partial<ReadRow>;
  if (!Number.isSafeInteger(row.ts) || row.ts! < 0 || typeof row.path !== STRING_TYPE || typeof row.blob !== STRING_TYPE || typeof row.section !== STRING_TYPE) return undefined;
  if (!DIGEST.test(row.blob) || row.path.includes("\0")) return undefined;
  return row as ReadRow;
}

function current(log: string, root: string): void {
  for (const line of readFileSync(log, "utf8").split("\n")) {
    let row;
    try { row = readRow(JSON.parse(line)); } catch { continue; }
    if (!row) continue;
    const file = resolve(root, row.path);
    const within = relative(root, file);
    if (isAbsolute(within) || within === ".." || within.startsWith("../")) continue;
    const hash = Bun.spawnSync(["git", "-C", root, "hash-object", file], { stdout: "pipe", stderr: "ignore" });
    if (hash.exitCode === 0 && hash.stdout.toString().trim() === row.blob) {
      process.stdout.write(`${escaped(row.path)}\t${row.ts}\t${escaped(row.section)}\n`);
    }
  }
}

const [mode, ...args] = Bun.argv.slice(2);
if (mode === "escape") process.stdout.write(escaped(args[0] ?? ""));
else if (mode === "current") current(args[0]!, args[1]!);
else if (mode === "log") {
  const [ts, path, blob, section] = args;
  const row = readRow({ ts: Number(ts), path, blob, section });
  if (!row) throw new Error("invalid document read record");
  process.stdout.write(`${JSON.stringify(row)}\n`);
} else throw new Error("expected log, current or escape");
