import { afterEach, beforeEach, expect, it } from "vitest";
import { act, cleanup, render, screen } from "@testing-library/react";
import { renderToString } from "react-dom/server";
import { FleetApprovalStatus } from "./FleetApprovalStatus";
import { FleetStreamProvider } from "./FleetStreamProvider";
import { useFleetEventStream } from "./useFleetEventStream";
import { FleetStreams } from "@/lib/streaming/fleet-stream-registry";
import { FRAME_KIND } from "@/lib/api/events-types";
import { FakeEventSource } from "@/tests/helpers/fake-event-source";
import { row } from "@/tests/helpers/fleet-stream-fixtures";

const PROPS = {workspaceId: "workspace", fleetId: "fleet", initial: 2};
const INITIAL = [row({event_id: "recorded", status: "processed", response_text: "Retained answer"})];
let streams: FleetStreams;
beforeEach(() => {FakeEventSource.install(); streams = new FleetStreams();});
afterEach(() => {cleanup(); streams.close(); FakeEventSource.uninstall();});

function Timeline() {
  const stream = useFleetEventStream(PROPS.workspaceId, PROPS.fleetId, INITIAL);
  return <p>{stream.events.map(event => event.reply).join(" ")}</p>;
}

it("keeps the server's verified attention outside a live stream provider", () => {
  const view = render(<FleetApprovalStatus {...PROPS} />);
  expect(screen.getByText("Needs approval")).toBeTruthy();
  view.rerender(<FleetApprovalStatus {...PROPS} initial={0} />);
  expect(screen.queryByText("Needs approval")).toBeNull();
  expect(FakeEventSource.instances).toHaveLength(0);
});

it("shares one native stream with a later timeline subscriber without losing initial rows", () => {
  const view = render(<FleetStreamProvider streams={streams}><FleetApprovalStatus {...PROPS} /><Timeline /></FleetStreamProvider>);
  expect(FakeEventSource.instances).toHaveLength(1);
  expect(screen.getByText("Retained answer")).toBeTruthy();
  expect(screen.getByText("Needs approval")).toBeTruthy();
  act(() => streams.reconcileServerFacts(PROPS.fleetId, {status: "active", pendingApprovals: 0}));
  expect(screen.queryByText("Needs approval")).toBeNull();
  act(() => streams.reconcileServerFacts(PROPS.fleetId, {status: "active", pendingApprovals: 1}));
  expect(screen.getByText("Needs approval")).toBeTruthy();
  view.unmount();
  expect(FakeEventSource.instances[0]?.closed).toBe(true);
});

it("uses the server's count for a server render without opening a stream", () => {
  expect(renderToString(<FleetStreamProvider streams={streams}><FleetApprovalStatus {...PROPS} /></FleetStreamProvider>)).toContain("Needs approval");
  expect(FakeEventSource.instances).toHaveLength(0);
});

it("clears retained approval attention immediately when the existing stream revokes access", () => {
  render(<FleetStreamProvider streams={streams}><FleetApprovalStatus {...PROPS} /></FleetStreamProvider>);
  act(() => streams.reconcileServerFacts(PROPS.fleetId, {status: "active", pendingApprovals: 2}));
  expect(screen.getByText("Needs approval")).toBeTruthy();
  act(() => FakeEventSource.instances[0]?.emitNamed(FRAME_KIND.ACCESS_REVOKED, JSON.stringify({kind: FRAME_KIND.ACCESS_REVOKED})));
  expect(screen.queryByText("Needs approval")).toBeNull();
  expect(FakeEventSource.instances).toHaveLength(1);
  expect(FakeEventSource.instances[0]?.closed).toBe(true);
});
