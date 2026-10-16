// The authenticated scope and identifiers the fleet command suites share, so
// two copies cannot drift the moment one suite changes a fixture.

import { withAuthedStateDir } from "./helpers-cli-state.ts";
import { withMockApi, jsonResponse, type MockRoutes } from "./helpers-mock-api.ts";

export const WS_ID = "01900000-0000-7000-8000-000000c1a170";
export const FLEET_ID = "01900000-0000-7000-8000-000000c1a171";

export const authedScope = <T>(fn: (stateDir: string) => Promise<T>): Promise<T> =>
  withAuthedStateDir({ workspaceId: WS_ID, sessionId: "sess_install" }, fn);

export { withMockApi, jsonResponse, type MockRoutes };
