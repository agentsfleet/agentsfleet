import type { Development } from "./schema.ts";
import type { Graded } from "./grade.ts";
import { z } from "zod";
import { instant } from "./schema.ts";
import { canonical, requireProof } from "./records.ts";
import { callSchema, eventSchema, eventUrl } from "./native.ts";
import { checkCalls } from "./cases.ts";
import { checkInvestigationReads } from "./investigation.ts";

const storeArguments = z.object({key: z.string().min(1), content: z.string().min(1),
  category: z.string().optional(), visibility: z.enum(["fleet", "workspace"]).optional()});
const recallArguments = z.object({query: z.string()}).passthrough();
const entrySchema = z.strictObject({observed_at: instant, read_url: z.url(), response: z.object({items: z.array(z.object({
  key: z.string(), content: z.string(), writer_fleet_id: z.string(), category: z.string(),
  visibility: z.string(), updated_at: instant}).passthrough())}).passthrough()});

export function checkMemory(graded: Graded, development: Development): void {
  const {records, report} = graded;
  const memory = development.memory;
  const field = "development.memory";
  requireProof(memory.review.grade === "pass", field, "memory review failed or is incomplete");
  const saved = records.json(memory.store_event.record, eventSchema, `${field}.store_event`);
  const recalled = records.json(memory.recall_event.record, eventSchema, `${field}.recall_event`);
  const diagnosis = report.cases.find(subject => subject.case_id === development.case_links.responder);
  requireProof(diagnosis && diagnosis.event_id === saved.event_id && diagnosis.event_detail.read_url === memory.store_event.read_url,
    field, "memory save belongs to another diagnosis");
  requireProof(canonical(diagnosis.event_detail.record) === canonical(memory.store_event.record)
    && diagnosis.tool_records.some(call => call.read_url === memory.store_call.read_url
      && canonical(call.record) === canonical(memory.store_call.record)), field, "memory save uses different native witnesses");
  requireProof(memory.store_event.read_url === eventUrl(saved.workspace_id, saved.fleet_id, saved.event_id)
    && memory.recall_event.read_url === eventUrl(recalled.workspace_id, recalled.fleet_id, recalled.event_id), field, "memory event address differs");
  requireProof(saved.workspace_id === recalled.workspace_id && saved.fleet_id === recalled.fleet_id && saved.event_id !== recalled.event_id
    && saved.status === "processed" && recalled.status === "processed" && Date.parse(saved.updated_at) <= Date.parse(recalled.created_at), field, "memory was not recalled by a later settled event of the same fleet");
  const storedCall = records.json(memory.store_call.record, callSchema, `${field}.store_call`);
  const recalledCall = records.json(memory.recall_call.record, callSchema, `${field}.recall_call`);
  const savedTool = saved.tool_calls?.calls.find(call => `${memory.store_event.read_url}/tool-calls/${encodeURIComponent(call.call_id)}` === memory.store_call.read_url);
  const recalledTool = recalled.tool_calls?.calls.find(call => `${memory.recall_event.read_url}/tool-calls/${encodeURIComponent(call.call_id)}` === memory.recall_call.read_url);
  requireProof(savedTool?.name === "memory_store" && recalledTool?.name === "memory_recall" && savedTool.status === "succeeded"
    && recalledTool.status === "succeeded", field, "native memory tool identities or outcomes differ");
  const outputs = checkCalls({tool_records: memory.recall_tools, outcome: "diagnosis"}, recalled, records, memory.recall_event.read_url, field);
  requireProof(outputs.get(recalledTool.call_id) === recalledCall.response.output
    && memory.recall_tools.some(call => call.call_id === recalledTool.call_id && call.read_url === memory.recall_call.read_url
      && canonical(call.record) === canonical(memory.recall_call.record))
    && storedCall.read_url === memory.store_call.read_url && recalledCall.read_url === memory.recall_call.read_url, field, "memory tool detail differs");
  const stored = storeArguments.safeParse(storedCall.response.arguments);
  const query = recallArguments.safeParse(recalledCall.response.arguments);
  requireProof(stored.success && query.success && !storedCall.response.truncated && !storedCall.response.truncated_arguments
    && !storedCall.response.output.startsWith("["), field, "memory tool arguments or result are incomplete");
  const entries = records.json(memory.entry.record, entrySchema, `${field}.entry`);
  const path = `/v1/workspaces/${saved.workspace_id}/fleets/${saved.fleet_id}/memories`;
  requireProof(entries.read_url === memory.entry.read_url && new URL(entries.read_url).origin === "https://api-dev.agentsfleet.net"
    && new URL(entries.read_url).pathname === path, field, "persisted memory address differs");
  requireProof(entries.response.items.some(entry => entry.key === stored.data.key && entry.content === stored.data.content
    && entry.writer_fleet_id === saved.fleet_id && entry.visibility === (stored.data.visibility ?? "fleet")
    && entry.category === (stored.data.category ?? "core") && Date.parse(entry.updated_at) >= Date.parse(saved.created_at)
    && Date.parse(entry.updated_at) <= Date.parse(recalled.created_at)), field, "stored memory is missing, predates its save or differs");
  requireProof(Date.parse(entries.observed_at) >= Date.parse(saved.updated_at)
    && Date.parse(entries.observed_at) <= Date.parse(memory.review.at), field, "persisted entry observation is not after the save");
  requireProof(recalledCall.response.output.includes(stored.data.key) && recalledCall.response.output.includes(stored.data.content), field, "later recall did not return the persisted finding");
  const prior = new Set(recalled.tool_calls?.calls.slice(0, recalled.tool_calls.calls.indexOf(recalledTool)).map(call => call.call_id));
  checkInvestigationReads(development.source, {commit: development.regression.before.commit, name: development.pull_request.base},
    memory.recall_tools.filter(call => prior.has(call.call_id)), recalled, records, field);
  requireProof(Date.parse(memory.review.at) >= Date.parse(recalled.updated_at), field, "memory review predates recall settlement");
}
