import type { Case, Report } from "./schema.ts";
import type { checkCase } from "./cases.ts";
import { Records, canonical, requireProof } from "./records.ts";
import { freezeSchema, identitySchema, settingsSchema } from "./native.ts";

export function checkModel(subject: Case, report: Report, native: ReturnType<typeof checkCase>, records: Records, field: string): string {
  const freeze = records.json(subject.model_freeze, freezeSchema, `${field}.model_freeze`);
  requireProof(freeze.evaluation_id === report.evaluation_id, field, "model freeze belongs to another campaign");
  const {event, fleet} = native;
  requireProof(Date.parse(freeze.agreed_at) <= Date.parse(event.created_at) && Date.parse(freeze.at) >= Date.parse(event.updated_at), field, "model freeze does not cover the event");
  const before = records.json(subject.settings_before, settingsSchema, `${field}.settings_before`);
  const after = records.json(subject.settings_after, settingsSchema, `${field}.settings_after`);
  requireProof(Date.parse(before.observed_at) <= Date.parse(event.created_at)
    && Date.parse(after.observed_at) >= Date.parse(event.updated_at), field, "settings capture does not bracket settlement");
  const project = (settings: typeof before) => {
    requireProof(settings.catalogue.read_url === "https://api-dev.agentsfleet.net/v1/admin/models"
      && settings.selection.read_url === `https://api-dev.agentsfleet.net/v1/workspaces/${subject.workspace_id}/models`, field, "model settings address differs");
    const selected = settings.selection.response.items.filter(item => item.ref === fleet.definition.config["x-agentsfleet"].model);
    requireProof(selected.length === 1 && selected[0], field, "selected model reference is unavailable or ambiguous");
    const selection = selected[0];
    const matches = settings.catalogue.response.models.filter(item => item.provider === selection.provider && item.model_id === selection.model);
    requireProof(matches.length === 1 && matches[0], field, "catalogue identity is unavailable or ambiguous");
    const model = matches[0];
    requireProof(model.id === freeze.model_record_id && model.provider === report.model.provider && model.model_id === report.model.id, field, "registered model identity differs");
    requireProof((model.version ?? null) === report.model.version, field, "provider model version differs or is unavailable");
    requireProof(model.inference && canonical(model.inference) === canonical(report.model.settings.inference), field, "native inference settings differ or are unavailable");
    requireProof(model.metadata.context_cap_tokens === report.model.settings.context_cap_tokens
      && selection.context_window_tokens === model.metadata.context_cap_tokens
      && selection.max_output_tokens === model.inference.max_output_tokens, field, "model capacity differs");
    requireProof(canonical(fleet.definition.config["x-agentsfleet"].context ?? null) === canonical(report.model.settings.context), field, "accepted fleet context differs");
    return canonical({selection, model});
  };
  const selected = project(before);
  requireProof(selected === project(after), field, "selected model changed during the event");
  checkLeases(subject, report, records, event, field);
  return selected;
}

function checkLeases(subject: Case, report: Report, records: Records, event: ReturnType<typeof checkCase>["event"], field: string): void {
  const identity = records.json(subject.model_identity, identitySchema, `${field}.model_identity`);
  requireProof(identity.workspace_id === subject.workspace_id && identity.fleet_id === subject.fleet_id
    && identity.event_id === subject.event_id, field, "lease projection belongs to another event");
  requireProof(Date.parse(identity.captured_at) >= Date.parse(event.updated_at), field, "lease identity predates settlement");
  requireProof(new Set(identity.leases.map(lease => lease.lease_id)).size === identity.leases.length, field, "duplicate lease identity");
  requireProof(identity.leases.every(lease => lease.provider === report.model.provider && lease.model === report.model.id), field, "event used another model");
  requireProof(identity.leases.some(lease => lease.status === "reported" && Date.parse(lease.updated_at) >= Date.parse(lease.created_at)), field, "no matching settled lease");
}
