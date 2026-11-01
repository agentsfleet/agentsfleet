import type { Manifest, Report } from "./schema.ts";
import { CASE_EXPECTATIONS, manifestSchema, reportSchema } from "./schema.ts";
import { Records, canonical, decode, hash, requireProof } from "./records.ts";
import { checkCase } from "./cases.ts";
import { checkModel } from "./models.ts";

export type Graded = ReturnType<typeof gradeBehavior>;

export function gradeBehavior(reportInput: unknown, manifestInput: unknown) {
  const report = decode(reportSchema, reportInput, "report");
  const manifest = decode(manifestSchema, manifestInput, "manifest");
  canonical(report);
  requireProof(hash(canonical(manifest)) === report.corpus_sha256, "corpus_sha256", "manifest digest differs");
  checkManifest(manifest);
  requireProof(new Set(report.cases.map(subject => subject.case_id)).size === 8, "cases", "duplicate or missing case");
  requireProof(new Set(report.cases.map(subject => subject.event_id)).size === 8, "cases", "native events are mixed or reused");
  const records = new Records(report.records);
  const native = report.cases.map((subject, index) => {
    const field = `cases[${index}]`;
    const expected = manifest.cases.find(item => item.case_id === subject.case_id);
    requireProof(expected && expected.variant_id === subject.variant_id && expected.policy_sha256 === subject.policy_sha256
      && expected.role === subject.role && expected.expected_outcome === subject.outcome, field, "case does not match the pinned manifest");
    requireProof(expected.input === records.text(subject.prompt, field), field, "manifest input differs from the exact composed prompt");
    return checkCase(subject, report, records, field);
  });
  const models = report.cases.map((subject, index) => {
    const record = native[index];
    requireProof(record, "cases", "native record is missing");
    return checkModel(subject, report, record, records, `cases[${index}]`);
  });
  requireProof(new Set(models).size === 1, "model", "campaign mixes selected model settings");
  const grounding = report.cases.filter(subject => subject.review.all_claims_reviewed && subject.review.grounding === "pass"
    && subject.review.stopping_scope === "pass" && subject.review.findings.length === 0).length;
  const behavior = report.cases.filter(subject => subject.review.all_claims_reviewed && subject.review.grounding === "pass"
    && subject.review.stopping_scope === "pass" && subject.review.usefulness === "pass" && subject.review.findings.length === 0).length;
  return {report, manifest, records, native, grounding, behavior};
}

function checkManifest(manifest: Manifest): void {
  requireProof(new Set(manifest.cases.map(subject => subject.case_id)).size === 8, "manifest.cases", "duplicate or missing case");
  const variants = new Map<string, string>();
  for (const subject of manifest.cases) {
    const [role, outcome] = CASE_EXPECTATIONS[subject.case_id];
    requireProof(role === subject.role && outcome === subject.expected_outcome, "manifest.cases", "role or outcome weakens a fixed case");
    const key = `${subject.role}:${subject.variant_id}`;
    const existing = variants.get(key);
    requireProof(existing === undefined || existing === subject.policy_sha256, "manifest.cases", "variant has conflicting policies");
    variants.set(key, subject.policy_sha256);
  }
}

export function campaignChanged(before: Report, after: Report): boolean {
  return before.corpus_sha256 !== after.corpus_sha256 || canonical(before.instructions_sha256) !== canonical(after.instructions_sha256)
    || canonical(before.model) !== canonical(after.model);
}
