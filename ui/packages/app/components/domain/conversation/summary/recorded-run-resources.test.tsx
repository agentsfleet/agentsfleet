import {cleanup, render, screen} from "@testing-library/react";
import {afterEach, expect, it} from "vitest";
import type {FleetToolCall} from "@/lib/streaming/fleet-stream-row";
import {ev} from "@/tests/fleet-thread/harness";
import {recordedRunResources} from "./recorded-run-resources";
import {AttachedSources, PreviewOutputs, RunResources} from "./RunResources";

afterEach(cleanup);
const href = "https://preview.example.test/launch";
const call = (name: string, args: FleetToolCall["args"], outputHead?: string, extra?: Partial<FleetToolCall>): FleetToolCall =>
  ({name, args, done: true, startedAtMs: 1, ms: 1, status: "succeeded", outputHead, ...extra});
const spawn = (task?: string) => call("spawn", {task: task ?? ""}, '{"child_id":7}');
const wait = (outputHead: string) => call("wait_agent", {child_id: 7}, outputHead);
const preview = (detail: string) => call("exec_command", {cmd: "bun run preview"}, detail, {outputTail: "Process running with session ID 12"});

it("correlates observed children and sessions with their own run, and links a preview reported in both places", () => {
  const event = ev({actor:"operator",role:"user",status: "received", reply: '[Launch preview](' + href + ')', tools: [
    spawn("UI reviewer"), wait('{"status":"done","answer":"Reviewed desktop and mobile"}'), preview(href),
  ]});
  const saved = recordedRunResources(event);
  expect(recordedRunResources(event)).toBe(saved);
  expect(saved.previews).toEqual([href]);
  expect(saved.rows.map(row => [row.label, row.state])).toEqual([["UI reviewer", "Done"], ["Preview server", "Running"]]);
  const view = render(<><RunResources rows={saved.rows} /><PreviewOutputs links={saved.previews} /><AttachedSources files={[{id:"brief",name:"brief.md"},{id:"ref",name:"mobile.png"}]} /></>);
  expect(screen.getByRole("link", {name:"Launch preview"}).getAttribute("href")).toBe(href);
  for (const label of ["UI reviewer","Preview server","2 attached files"]) screen.getByText(label).closest("summary")!.click();
  expect(screen.getByText("Reviewed desktop and mobile")).toBeTruthy();
  expect(screen.getByText("mobile.png")).toBeTruthy();
  view.rerender(<><RunResources rows={[]} /><PreviewOutputs links={[]} /><AttachedSources files={[{id:"one",name:"brief.md"}]} /></>);
  expect(screen.getByText("1 attached file")).toBeTruthy();
  expect(screen.queryByRole("region",{name:"Agents & processes"})).toBeNull();
  expect(recordedRunResources(ev({actor:"operator",role:"user",tools:[wait('{"status":"done","answer":"Unrelated"}')]})).rows).toEqual([]);
});

it("preserves unknown states, ignores malformed results, and distinguishes terminal results from the last running observation", () => {
  const tools = [
    call("unrelated", {}), call("spawn", {}, "", {status: "failed"}), call("spawn", {}, undefined),
    ...['null','[]','{','2','{"child_id":-1}','{}'].map(output => call("spawn", {}, output)),
    spawn(), wait('{"status":false}'), wait('{"status":"new-status"}'), wait('{"status":"running"}'),
    call("exec_command", {}, "Process running with session ID 1"), call("exec_command",{cmd:""},"Process running with session ID 4"),
    call("exec_command",{cmd:"task\nsecond"},"Process running with session ID 2"), call("write_stdin",{session_id:3},""),
    call("write_stdin",{session_id:2},""), call("write_stdin",{},""), call("write_stdin",undefined,""), call("write_stdin",{session_id:"2"},"",{exitCode:0}), call("write_stdin",{session_id:2},"",{status:"interrupted"}), call("write_stdin",{session_id:2},"",{status:"failed",exitCode:1}),
    preview("Ready"), call("write_stdin",{session_id:12},"",{exitCode:0}),
  ];
  expect(recordedRunResources(ev({actor:"operator",role:"user",})).rows).toEqual([]);
  const event=ev({actor:"operator",role:"user",status:"processed",tools});
  const rows=recordedRunResources(event).rows;
  expect(rows.map(row=>[row.label,row.state])).toEqual([["Agent 7","Last seen running"],["Command","Last seen running"],["task","Failed"],["Preview server","Done"]]);
  const view=render(<RunResources rows={rows} />);
  expect(view.getAllByText("Was running").length).toBeGreaterThan(0);
  expect(recordedRunResources({...event,tools:[spawn("Check"),wait('{"status":"failed","detail":"Stopped"}')]}).rows[0]).toMatchObject({state:"Failed",detail:"Stopped"});
  expect(recordedRunResources({...event,tools:[spawn("Check"),wait('{"status":"interrupted"}')]}).rows[0]).toMatchObject({state:"Interrupted",detail:"Check"});
  expect(recordedRunResources({...event,tools:[call("spawn",{},"{\"child_id\":2}")]}).rows[0]?.label).toBe("Agent 2");
});

it("rejects uncorroborated URLs, malformed hosts and credentials", () => {
  const urls=["https://[","https://name@preview.example.test","https://:password@preview.example.test","http://localhost:4173","http://127.0.0.1","https://unconfirmed.example.test"];
  const event=ev({actor:"operator",role:"user",status:"processed",reply:urls.map(url=>'[Open preview]('+url+')').join('\n'),tools:[preview(urls.slice(0,-1).join('\n'))]});
  expect(recordedRunResources(event).previews).toEqual([]);
});

it("accepts a preview URL printed by a later poll of the same session", () => {
  const run = ev({actor: "operator", role: "user", reply: "[Launch preview](" + href + ")"});
  const poll = (session_id: number) => call("write_stdin", {session_id}, href);
  expect(recordedRunResources({...run, tools: [preview("Starting"), poll(6)]}).previews).toEqual([]);
  expect(recordedRunResources({...run, tools: [preview("Starting"), poll(12), poll(12)]}).previews).toEqual([href]);
});

it("recognizes the leading native status when a child's long answer is clipped across output edges", () => {
  for (const [status, field, state] of [["done", "answer", "Done"], ["failed", "detail", "Failed"]] as const) {
    const raw = JSON.stringify({status, [field]: "Evidence ".repeat(300)});
    const poll = {...wait(raw.slice(0,1024)), outputTail: raw.slice(-1024)};
    const run = ev({actor: "operator", role: "user", tools: [spawn("UI reviewer"), poll]});
    expect(recordedRunResources(run).rows[0]).toMatchObject({state, detail: expect.stringContaining("shortened")});
    for (const broken of [undefined, '{"answer":"user supplied status']) {
      expect(recordedRunResources({...run, tools: [spawn("UI reviewer"), {...poll, outputHead: broken}]}).rows[0]?.state).toBe("Last seen running");
    }
  }
});

it("records explicit process endings without an exit code and keeps an unknown poll failure inconclusive", () => {
  const run = ev({actor: "operator", role: "user", status: "received"});
  for (const [ending, state] of [
    ["Process timed out", "Failed"], ["Process interrupted before its ending arrived", "Interrupted"], ["Session is busy", "Running"], [undefined, "Running"],
  ] as const) {
    const close = call("write_stdin", {session_id: 12}, ending, {status: "failed"});
    expect(recordedRunResources({...run, tools: [preview("Starting"), close]}).rows[0]?.state).toBe(state);
  }
  const tail = call("write_stdin", {session_id: 12}, "Earlier output", {status: "failed", outputTail: "Output\nProcess timed out"});
  expect(recordedRunResources({...run, tools: [preview("Starting"), tail]}).rows[0]?.detail).toContain("Process timed out");
});
