import type { Definition } from "@/lib/api/draft-types";
import { afterEach, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { SetupSummary } from "@/components/domain/chat/SetupSummary";
import { REVIEW } from "@/tests/helpers/draft-fixtures";

const PURPOSE = "Investigate failures";
const POLICY = "x-agentsfleet";
const MANUAL = /No automatic triggers/;
const UNKNOWN = /Automatic triggers could not be confirmed/;
const ACCESS_UNKNOWN = /Access and spending limits could not be summarized/;
const EMPTY_REQUIREMENTS = {credentials: [], repositories: [], tools: [], network: {allow: []}, budget: {daily_nanos: "1000000000"}};
const UTC = "UTC";
const SCHEDULE = "0 9 * * *";
const SOURCE = "github";

afterEach(() => {cleanup(); vi.useRealTimers();});

function definition(triggers?: unknown): Definition {
  return {...REVIEW.effective.definition, config: {[POLICY]: triggers === undefined ? {} : {triggers}}};
}

it("distinguishes a manual fleet from one with automatic triggers", () => {
  render(<SetupSummary definition={definition([])} />);
  expect(screen.getByText(PURPOSE)).toBeTruthy();
  expect(screen.getByText(MANUAL).textContent).toMatch(/When you send it a message/);
  expect(screen.queryByText(/automatically:/)).toBeNull();
});

it("treats an omitted triggers list as manual, matching runtime defaults", () => {
  render(<SetupSummary definition={definition()} />);
  expect(screen.getByText(MANUAL)).toBeTruthy();
});

it("does not infer manual operation when the saved definition is unavailable", () => {
  render(<SetupSummary />);
  expect(screen.getByText(/Open the fleet to see its saved instructions/)).toBeTruthy();
  expect(screen.getByText(UNKNOWN)).toBeTruthy();
  expect(screen.queryByText(MANUAL)).toBeNull();
});

it.each([
  {triggers: "not a list"},
  {triggers: [{type: "unknown-source"}]},
  {triggers: [{type: "webhook"}]},
  {triggers: [{type: "mention", source: SOURCE, channels: null}]},
])("keeps malformed trigger data unknown: $triggers", ({triggers}) => {
  render(<SetupSummary definition={definition(triggers)} />);
  expect(screen.getByText(UNKNOWN)).toBeTruthy();
  expect(screen.queryByText(MANUAL)).toBeNull();
});

it("explains event and mention sources without implying that delivery is connected", () => {
  render(<SetupSummary definition={definition([
    {type: "webhook", source: SOURCE, events: ["push", "pull_request"]},
    {type: "webhook", source: "linear"},
    {type: "webhook", source: "slack", events: []},
    {type: "mention", source: "slack", channels: ["C123"]},
  ])} />);
  expect(screen.getByText(/When github sends push, pull_request/).textContent).toMatch(/Connect the source/);
  expect(screen.getByText(/When linear sends an event/)).toBeTruthy();
  expect(screen.getByText(/When slack sends an event/)).toBeTruthy();
  expect(screen.getByText(/When mentioned in slack: C123/).textContent).toMatch(/Connect the source/);
  expect(screen.queryByText(MANUAL)).toBeNull();
});

it.each([
  {schedule: SCHEDULE, timezone: UTC},
  {schedule: "invalid schedule", timezone: "Unknown/Timezone"},
])("shows schedule intent without predicting scheduler behavior: $schedule", (trigger) => {
  render(<SetupSummary definition={definition([{type: "cron", ...trigger}])} />);
  expect(screen.getByText("Schedule requested. Save the fleet to activate it.")).toBeTruthy();
  expect(screen.queryByText(/Next scheduled time|Timing could not be previewed/)).toBeNull();
});

it("makes absent access and network permissions explicit and describes a ceiling rather than a fee", () => {
  render(<SetupSummary definition={definition([])} requirements={EMPTY_REQUIREMENTS} />);
  expect(screen.getByText(/No tools requested/)).toBeTruthy();
  expect(screen.getAllByText(/None requested/)).toHaveLength(2);
  expect(screen.getByText(/HTTP requests are blocked/)).toBeTruthy();
  expect(screen.getByText(/\$1\.00 per day; no monthly limit/)).toBeTruthy();
  expect(screen.getByText(/limits, not a fixed charge/)).toBeTruthy();
});

it("shows declared access and exact daily and monthly limits without rounding large amounts", () => {
  render(<SetupSummary requirements={{credentials: [SOURCE], repositories: ["owner/repo"], tools: ["read_file"],
    network: {allow: ["api.github.com"], read_only: true},
    budget: {daily_nanos: "1", monthly_nanos: "9223372036854775807"}}} />);
  expect(screen.getByText(/Connected accounts:/).closest("li")?.textContent).toContain(SOURCE);
  expect(screen.getByText(/Repositories:/).closest("li")?.textContent).toContain("owner/repo");
  expect(screen.getByText(/Tools:/).closest("li")?.textContent).toContain("read_file");
  expect(screen.getByText(/HTTP egress:/).closest("li")?.textContent).toContain("Read-only policy enabled.");
  // Pin test: exact decimal limits must survive values beyond Number.MAX_SAFE_INTEGER.
  expect(screen.getByText(/Spending limit:/).closest("li")?.textContent).toContain("$0.000000001 per day; $9223372036.854775807 per month.");
});

it("does not infer a read-only policy for an allowed host", () => {
  render(<SetupSummary requirements={{...EMPTY_REQUIREMENTS, network: {allow: ["example.com"]}}} />);
  expect(screen.getByText(/Requests to example.com/)).toBeTruthy();
  expect(screen.queryByText(/Read-only policy enabled/)).toBeNull();
});

it.each([undefined, {}])("does not invent network restrictions when the allow list is absent: %s", (network) => {
  render(<SetupSummary requirements={{...EMPTY_REQUIREMENTS, network}} />);
  expect(screen.getByText(/No explicit host limit shown here/)).toBeTruthy();
  expect(screen.queryByText(/HTTP requests are blocked/)).toBeNull();
});

it.each([
  {network: {http_egress: "public"}, description: "Public HTTPS destinations."},
  {network: {http_egress: "public", allow: [], read_only: true}, description: "Public HTTPS destinations. Read-only policy enabled."},
  {network: {http_egress: "restricted", allow: []}, description: "HTTP requests are blocked."},
  {network: {package_manager_domains: true}, description: "Standard package registry domains."},
  {network: {package_manager_domains: true, allow: []}, description: "Standard package registry domains."},
  {network: {package_manager_domains: true, allow: ["example.com"], read_only: true},
    description: "Standard package registry domains. Additional domains: example.com. Read-only policy enabled."},
  {network: {http_egress: "public", package_manager_domains: true, allow: ["example.com"]}, description: "Public HTTPS destinations."},
])("describes reviewed HTTP destination scope: $network", ({network, description}) => {
  render(<SetupSummary requirements={{...EMPTY_REQUIREMENTS, network}} />);
  expect(screen.getByText(/HTTP egress:/).closest("li")?.textContent).toBe(`HTTP egress: ${description}`);
});

it.each([
  {},
  {...EMPTY_REQUIREMENTS, credentials: null},
  {...EMPTY_REQUIREMENTS, budget: {daily_nanos: "0"}},
  {...EMPTY_REQUIREMENTS, budget: {daily_nanos: "-1"}},
  {...EMPTY_REQUIREMENTS, budget: {daily_nanos: "01"}},
  {...EMPTY_REQUIREMENTS, budget: {daily_nanos: "invalid"}},
  {...EMPTY_REQUIREMENTS, network: {http_egress: "unknown", allow: []}},
  {...EMPTY_REQUIREMENTS, network: {package_manager_domains: "true", allow: []}},
])("does not describe unvalidated requirements as safe or free: %s", (requirements) => {
  render(<SetupSummary requirements={requirements} />);
  expect(screen.getByText(ACCESS_UNKNOWN)).toBeTruthy();
  expect(screen.queryByText(/No tools requested/)).toBeNull();
  expect(screen.queryByText(/Spending limit:/)).toBeNull();
});
