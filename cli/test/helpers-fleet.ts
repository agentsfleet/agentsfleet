// The `Fleet` that `GET F` and `PATCH F` answer, shared by the suites that read
// or change one fleet.

import type { Fleet } from "../src/commands/fleet/read.ts";

export const FLEET_ID = "0199a801-1000-7000-8000-000000000004";
export const FLEET: Fleet = {
  id: FLEET_ID, name: "argo", status: "active", etag: '"fleet-1"',
  model_ref: "owner/runtime", http_egress: "restricted", package_domains: false,
};
