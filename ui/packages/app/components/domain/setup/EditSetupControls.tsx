"use client";

import Link from "next/link";
import { NotePencilIcon } from "@phosphor-icons/react/ssr";
import { Button, DropdownMenuItem } from "@agentsfleet/design-system";
import { useFleetSetup, type SetupLocation } from "./FleetSetupContext";

export function EditSetupButton(props: SetupLocation | {inline: true}) {
  const setup = useFleetSetup();
  const inline = "inline" in props;
  const variant = inline ? "link" : "outline";
  const location = inline ? setup?.location : props;
  if (!location) return <>Edit setup</>;
  if (setup?.modelChanging) return <Button variant={variant} size={inline ? "sm" : "icon"} aria-label="Edit setup" disabled>{inline ? "Edit setup" : <NotePencilIcon size={16} aria-hidden="true" />}</Button>;
  return <Button variant={variant} size={inline ? "eyebrow" : "icon"} aria-label="Edit setup" title={inline ? undefined : "Edit setup"} asChild
    className={inline ? "h-auto px-0 align-baseline text-eyebrow" : undefined}>
    <Link href={editSetupHref(location)}>{inline ? "Edit setup" : <NotePencilIcon size={16} aria-hidden="true" />}</Link>
  </Button>;
}

export function EditSetupMenuItem(location: SetupLocation) {
  const disabled = useFleetSetup()?.modelChanging ?? false;
  return <DropdownMenuItem asChild disabled={disabled}><Link href={editSetupHref(location)}>
    <NotePencilIcon size={16} aria-hidden="true" />Edit setup
  </Link></DropdownMenuItem>;
}

function editSetupHref(location: SetupLocation): string {
  const query = new URLSearchParams({fleet: location.fleet, setup_revision: location.revision});
  return `/w/${location.workspace}/fleets/new?${query}`;
}
