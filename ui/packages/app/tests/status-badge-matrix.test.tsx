import React from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render } from "@testing-library/react";

vi.mock("next/link", () => ({
  default: ({ href, children, ...rest }: React.PropsWithChildren<{ href: string }>) =>
    React.createElement("a", { href, ...rest }, children),
}));

// The tile's only live dependency. A stub pins the connection state so the
// `active` row can be read at its best case — a fully live stream — rather than
// at whatever a real EventSource happened to be doing.
const streamMock = vi.fn();
vi.mock("@/components/domain/useWorkspaceStream", () => ({
  useWorkspaceFleetStream: (...args: unknown[]) => streamMock(...args),
}));

// The header's two lifecycle controls stand in as markers. What the matrix
// records is WHICH element the header gives a status, not how the control
// behaves, and rendering the real ones drags server actions into a render test.
const HEADER_MARKER = {
  killSwitch: "kill-switch",
  fleetConfig: "fleet-config",
} as const;
vi.mock("@/app/(dashboard)/w/[workspaceId]/fleets/[id]/components/KillSwitch", () => ({
  default: () => React.createElement("div", { "data-testid": HEADER_MARKER.killSwitch }),
}));
vi.mock("@/app/(dashboard)/w/[workspaceId]/fleets/[id]/components/FleetConfig", () => ({
  default: () => React.createElement("div", { "data-testid": HEADER_MARKER.fleetConfig }),
}));

import FleetTile from "@/app/(dashboard)/w/[workspaceId]/fleets/components/FleetTile";
import { FleetHeader } from "@/app/(dashboard)/w/[workspaceId]/fleets/[id]/components/FleetHeader";
import RunMetricsStrip from "@/app/(dashboard)/w/[workspaceId]/fleets/[id]/components/RunMetricsStrip";
import { AGENTSFLEET_STATUS, type FleetStatus } from "@/lib/api/fleets-types";
import { CONNECTION_STATUS } from "@/lib/streaming/fleet-stream-registry";
import type { Fleet } from "@/lib/api/fleets";
import type { FleetDetail } from "@/lib/types";

/**
 * What the three fleet-status surfaces render, per status, read off real
 * renders rather than off a reading of the source.
 *
 * The surfaces do not agree, and the disagreements are the point of the table:
 * the wall tile colours a dot, the chat strip tints a dot AND its label, and
 * the detail header gives four of five statuses no status element at all —
 * it renders a lifecycle control in that slot instead. Nothing here is a
 * proposal; the spec that picks one mapping quotes this table as the "before".
 */

// Dot fills the wall tile chooses between. `bg-current` on the chat strip means
// the dot inherits its tint from the label, so the strip has one tone, not two.
const TILE_DOT = {
  info: "bg-info",
  pulse: "bg-pulse",
  muted: "bg-muted-foreground",
} as const;

// The chat strip's only status-dependent decision.
const STRIP_TONE = {
  pulse: "text-pulse",
  inherited: "inherited",
} as const;

const HEADER_ELEMENT = {
  badgeCyan: "badge:cyan",
  killSwitch: `control:${HEADER_MARKER.killSwitch}`,
  fleetConfig: `control:${HEADER_MARKER.fleetConfig}`,
  badgeDefault: "badge:default",
} as const;

type MatrixRow = {
  wallTileDot: (typeof TILE_DOT)[keyof typeof TILE_DOT];
  chatStripTone: (typeof STRIP_TONE)[keyof typeof STRIP_TONE];
  detailHeader: (typeof HEADER_ELEMENT)[keyof typeof HEADER_ELEMENT];
};

const STATUS_MATRIX: Record<FleetStatus, MatrixRow> = {
  [AGENTSFLEET_STATUS.ACTIVE]: {
    wallTileDot: TILE_DOT.pulse,
    chatStripTone: STRIP_TONE.pulse,
    detailHeader: HEADER_ELEMENT.killSwitch,
  },
  [AGENTSFLEET_STATUS.INSTALLING]: {
    wallTileDot: TILE_DOT.info,
    chatStripTone: STRIP_TONE.inherited,
    detailHeader: HEADER_ELEMENT.badgeCyan,
  },
  [AGENTSFLEET_STATUS.PAUSED]: {
    wallTileDot: TILE_DOT.muted,
    chatStripTone: STRIP_TONE.inherited,
    detailHeader: HEADER_ELEMENT.killSwitch,
  },
  [AGENTSFLEET_STATUS.STOPPED]: {
    wallTileDot: TILE_DOT.muted,
    chatStripTone: STRIP_TONE.inherited,
    detailHeader: HEADER_ELEMENT.killSwitch,
  },
  [AGENTSFLEET_STATUS.KILLED]: {
    wallTileDot: TILE_DOT.muted,
    chatStripTone: STRIP_TONE.inherited,
    detailHeader: HEADER_ELEMENT.fleetConfig,
  },
};

const WORKSPACE_ID = "ws_1";
const FLEET_ID = "agt_1";
const DOT_SELECTOR = "span.rounded-full";
const HEADER_STATUS_SELECTOR = '[aria-label^="Fleet status:"]';
const HEADER_INSTALLING_LABEL = "Fleet status: installing";

function fleetWith(status: FleetStatus): Fleet {
  return {
    id: FLEET_ID,
    workspace_id: WORKSPACE_ID,
    name: "matrix-fleet",
    status,
    created_at: 1_700_000_000_000,
    updated_at: 1_700_000_000_000,
  } as unknown as Fleet;
}

/** The tile read at its best case: a fully live stream for the given status. */
function wallTileDotFor(status: FleetStatus): string {
  streamMock.mockReturnValue({
    events: [],
    connectionStatus: CONNECTION_STATUS.LIVE,
    helloReceived: true,
    isLive: true,
    catchingUp: false,
  });
  const { container } = render(
    React.createElement(FleetTile, { fleet: fleetWith(status), workspaceId: WORKSPACE_ID }),
  );
  const dot = container.querySelector(DOT_SELECTOR);
  const fills = Object.values(TILE_DOT).filter((fill) => dot?.classList.contains(fill));
  // Exactly one, not "the first": a dot carrying two known fills means the
  // tile grew a state the matrix has no row for, and that must fail loudly.
  expect(fills, `exactly one known dot fill for ${status}`).toHaveLength(1);
  const [fill] = fills;
  if (fill === undefined) throw new Error(`no known dot fill rendered for ${status}`);
  return fill;
}

function chatStripToneFor(status: FleetStatus): string {
  const { container } = render(
    React.createElement(RunMetricsStrip, {
      status,
      latest: null,
      pendingApprovals: 0,
      approvalsHref: `/w/${WORKSPACE_ID}/approvals`,
      summaryAvailable: false,
    }),
  );
  const dot = container.querySelector(DOT_SELECTOR);
  const label = dot?.parentElement;
  return label?.classList.contains(STRIP_TONE.pulse)
    ? STRIP_TONE.pulse
    : STRIP_TONE.inherited;
}

function fleetDetailWith(status: FleetStatus): FleetDetail {
  return { ...fleetWith(status), triggers: [] } as unknown as FleetDetail;
}

function detailHeaderElementFor(status: FleetStatus): string {
  const { container } = render(
    React.createElement(FleetHeader, {
      workspaceId: WORKSPACE_ID,
      fleet: fleetDetailWith(status),
    }),
  );
  if (container.querySelector(`[data-testid="${HEADER_MARKER.killSwitch}"]`)) {
    return HEADER_ELEMENT.killSwitch;
  }
  if (container.querySelector(`[data-testid="${HEADER_MARKER.fleetConfig}"]`)) {
    return HEADER_ELEMENT.fleetConfig;
  }
  const badge = container.querySelector(HEADER_STATUS_SELECTOR);
  expect(badge, `a status element for ${status}`).not.toBeNull();
  return badge?.getAttribute("aria-label") === HEADER_INSTALLING_LABEL
    ? HEADER_ELEMENT.badgeCyan
    : HEADER_ELEMENT.badgeDefault;
}

describe("fleet status rendering matrix", () => {
  afterEach(() => cleanup());

  it("every fleet status has a matrix row", () => {
    const declared = Object.values(AGENTSFLEET_STATUS).sort();
    const charted = Object.keys(STATUS_MATRIX).sort();
    expect(charted, "a status with no matrix row").toEqual(declared);
  });

  it("the status matrix is what each surface renders", () => {
    for (const status of Object.values(AGENTSFLEET_STATUS)) {
      const row = STATUS_MATRIX[status];
      expect(wallTileDotFor(status), `wall tile dot for ${status}`).toBe(row.wallTileDot);
      cleanup();
      expect(chatStripToneFor(status), `chat strip tone for ${status}`).toBe(row.chatStripTone);
      cleanup();
      expect(detailHeaderElementFor(status), `detail header element for ${status}`).toBe(
        row.detailHeader,
      );
      cleanup();
    }
  });

  it("the wall tile and the chat strip disagree on every drained status", () => {
    // Named because it is the finding, not an accident: the tile greys a
    // stopped fleet against the workspace background while the strip leaves it
    // on the label's own colour. One surface says "inert", the other says
    // nothing at all.
    const drained: FleetStatus[] = [
      AGENTSFLEET_STATUS.PAUSED,
      AGENTSFLEET_STATUS.STOPPED,
      AGENTSFLEET_STATUS.KILLED,
    ];
    for (const status of drained) {
      expect(STATUS_MATRIX[status].wallTileDot).toBe(TILE_DOT.muted);
      expect(STATUS_MATRIX[status].chatStripTone).toBe(STRIP_TONE.inherited);
    }
  });

  it("the detail header gives exactly one status a status element", () => {
    // Four of five statuses get a lifecycle control where the other surfaces
    // put a status indicator, so "is ACTIVE the same colour everywhere?" has no
    // answer on this surface — it renders no colour for ACTIVE at all.
    const withBadge = Object.values(AGENTSFLEET_STATUS).filter(
      (status) => STATUS_MATRIX[status].detailHeader === HEADER_ELEMENT.badgeCyan,
    );
    expect(withBadge).toEqual([AGENTSFLEET_STATUS.INSTALLING]);
  });

  it("no status reaches the header's default badge", () => {
    // `FleetHeader`'s final `else` renders `<Badge>{status}</Badge>`, but the
    // three branches above it already cover all five union members. Recorded
    // rather than removed: deleting a branch is a behaviour change, and this
    // spec measures. The assertion fails the day a sixth status makes it live.
    const reachingDefault = Object.values(AGENTSFLEET_STATUS).filter(
      (status) => detailHeaderElementFor(status) === HEADER_ELEMENT.badgeDefault,
    );
    expect(reachingDefault).toEqual([]);
  });
});
