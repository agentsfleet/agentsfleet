import type { Case } from "@ci-evidence/schema";
import { describe, expect, test } from "bun:test";
import { checkDevelopment } from "@ci-evidence/development";
import { gradeBehavior } from "@ci-evidence/grade";
import { callSchema, eventSchema } from "@ci-evidence/native";
import { EvidenceFixture } from "./fixture.ts";
import { JOBS_URL, SOURCE_JOB } from "./source-fixture.ts";

function check(f: EvidenceFixture): void {checkDevelopment(gradeBehavior(f.report, f.manifest));}

function pagedFixture(terminal = false) {
  const f = new EvidenceFixture();
  f.development().source.jobs_url = JOBS_URL.replace("per_page=100", "per_page=1");
  const pages: Case["tool_records"] = [];
  const memory = f.development().memory;
  const recalled = {...eventSchema.parse(f.json(memory.recall_event.record)),
    tool_records: memory.recall_tools, event_detail: memory.recall_event};
  for (const subject of [f.subject(), f.subject(6), recalled]) {
    const first = subject.tool_records.find(call => call.call_id === "1:3");
    if (!first) throw new Error("fixture first job page missing");
    f.edit(first.record, ["response", "arguments", "url"], f.development().source.jobs_url);
    f.edit(first.record, ["response", "output"], output({total_count: 2, jobs: [SOURCE_JOB]}));
    const next = new URL(f.development().source.jobs_url);
    next.searchParams.set("page", "2");
    const second = f.call(subject, "1:7", "http_request", {url: next.href},
      output({total_count: 2, jobs: [{...SOURCE_JOB, id: 89, conclusion: "success", steps: []}]}));
    const event = eventSchema.parse(f.json(subject.event_detail.record));
    const added = [second];
    if (terminal) {
      next.searchParams.set("page", "3");
      added.push(f.call(subject, "1:10", "http_request", {url: next.href}, output({total_count: 2, jobs: []})));
    }
    const calls = event.tool_calls?.calls ?? [];
    calls.splice(6, 0, ...added.map(call => ({...call.inventory, status: "succeeded" as const})));
    f.edit(subject.event_detail.record, ["tool_calls", "calls"], calls);
    subject.tool_records.splice(6, 0, ...added.map(call => call.detail));
    pages.push(...added.map(call => call.detail));
  }
  return {f, pages};
}

function output(value: unknown): string {return `Status: 200\nContent-Type: application/json\n\nResponse Body:\n${JSON.stringify(value)}`;}

describe("complete selected-attempt job pagination", () => {
  test("accepts two independently read pages in both fleet investigations", () => {
    expect(() => check(pagedFixture().f)).not.toThrow();
  });

  test("accepts a verified empty terminal page after an exact multiple of the page size", () => {
    expect(() => check(pagedFixture(true).f)).not.toThrow();
  });

  test.each([{total_count: 3, jobs: []}, {total_count: 2, jobs: [SOURCE_JOB]}])("rejects contradictory terminal pages %#", response => {
    const {f, pages} = pagedFixture(true);
    const terminal = pages[1]; if (!terminal) throw new Error("fixture terminal page missing");
    f.edit(terminal.record, ["response", "output"], output(response));
    expect(() => check(f)).toThrow("development.source");
  });

  test.each([
    {total_count: 3, jobs: [{...SOURCE_JOB, id: 89, conclusion: "success"}]},
    {total_count: 2, jobs: [SOURCE_JOB]},
    {total_count: 2, jobs: [{...SOURCE_JOB, id: 89, run_attempt: 2}]},
    {total_count: 2, jobs: [{...SOURCE_JOB, id: 89, run_id: 78}]},
    {total_count: 2, jobs: []},
    {total_count: 2, jobs: [{...SOURCE_JOB, id: 89}]},
  ])("rejects incomplete, duplicate, foreign or unread failed jobs %#", response => {
    const {f, pages} = pagedFixture();
    const page = pages[0]; if (!page) throw new Error("fixture second page missing");
    f.edit(page.record, ["response", "output"], output(response));
    expect(() => check(f)).toThrow("development.source");
  });

  test.each(["?per_page=0&page=1", "?per_page=101&page=1", "?page=2", "?page=1&page=2", "?filter=latest",
    "?per_page=2&page=2", "?per_page=1&page=3", "?per_page=1&page=2&unexpected=1"]) ("rejects ambiguous or missing pages %s", query => {
    const {f, pages} = pagedFixture();
    const page = pages[0]; if (!page) throw new Error("fixture second page missing");
    const args = callSchema.parse(f.json(page.record)).response.arguments;
    if (typeof args !== "object" || args === null || !("url" in args) || typeof args.url !== "string") throw new Error("fixture URL missing");
    f.edit(page.record, ["response", "arguments", "url"], args.url.split("?")[0] + query);
    expect(() => check(f)).toThrow("development.source");
  });
});
