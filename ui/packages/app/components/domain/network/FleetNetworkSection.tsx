"use client";

import { HTTP_EGRESS } from "@/lib/api/fleet-detail";
import { Alert } from "@agentsfleet/design-system";
import { WorkSection } from "../shared/summary/WorkSection";
import { useFleetSettings } from "../fleet/FleetSettings";
import { FleetNetworkPolicy } from "./FleetNetworkPolicy";

/** The fleet's two network switches, each one `PATCH F`; a switch moves only when the daemon confirms it. */
export function FleetNetworkSection() {
  const settings = useFleetSettings();
  if (!settings) return null;
  const {value, pending, error, change} = settings;
  return <WorkSection title="Network">
    <FleetNetworkPolicy publicInternet={value.http_egress === HTTP_EGRESS.public} packages={value.package_domains}
      disabled={pending !== null} onChange={next => {void change("network", next);}} />
    {error?.kind === "network" ? <Alert variant="destructive" className="mt-md wrap-anywhere px-lg py-md">{error.message}</Alert> : null}
  </WorkSection>;
}
