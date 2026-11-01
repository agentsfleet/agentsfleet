import { resolve } from "node:path";
import { z } from "zod";
import { EvidenceError } from "./records.ts";
import { gradeBehavior } from "./grade.ts";
import { checkDevelopment } from "./development.ts";
import { REPORT_MAX_BYTES, checkHistory } from "./history.ts";

type CommandOptions = {root?: string; output?: (line: string) => void};
const argumentsSchema = z.tuple([z.literal("--report"), z.string().min(1), z.literal("--section"), z.enum(["grounding", "behavior", "development"])]);

export async function runEvidenceCommand(args: string[], options: CommandOptions = {}): Promise<number> {
  const output = options.output ?? console.log;
  const parsed = argumentsSchema.safeParse(args);
  if (!parsed.success) {output("usage: bun scripts/check-ci-drill-evidence.ts --report PATH --section grounding|behavior|development"); return 2;}
  const [, path, , section] = parsed.data;
  const root = options.root ?? resolve(import.meta.dirname, "../..");
  try {
    const report = await readJson(path, "report");
    const manifest = await readJson(resolve(root, "tests/fixtures/fleets/ci-investigation/cases.json"), "manifest");
    const graded = gradeBehavior(report, manifest);
    checkHistory(graded.report, root);
    if (section === "development") {checkDevelopment(graded); output("development=PASS"); return 0;}
    const count = section === "grounding" ? graded.grounding : graded.behavior;
    output(`${section}=${count}/8; ${section === "grounding" ? "critical_failures" : "worst_case_failures"}=${8 - count}`);
    return count === 8 ? 0 : 1;
  } catch (error) {
    if (error instanceof EvidenceError) {output(error.message); return error.status;}
    output("input: evidence could not be read or validated");
    return 2;
  }
}

async function readJson(path: string, field: string): Promise<unknown> {
  try {
    const file = Bun.file(path);
    if (file.size > REPORT_MAX_BYTES) throw new EvidenceError(field, "input exceeds 16 MiB", 2);
    const text = await file.slice(0, REPORT_MAX_BYTES + 1).text();
    if (Buffer.byteLength(text) > REPORT_MAX_BYTES) throw new EvidenceError(field, "input exceeds 16 MiB", 2);
    return JSON.parse(text);
  } catch (error) {
    if (error instanceof EvidenceError) throw error;
    throw new EvidenceError(field, "input is missing or malformed JSON", 2);
  }
}
