"use client";

import { useId } from "react";
import { Button, Dialog, DialogClose, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle, DialogTrigger, Label, Switch, TooltipButton, cn } from "@agentsfleet/design-system";
import { GlobeIcon, InfoIcon, PackageIcon } from "@phosphor-icons/react/ssr";
import { SUMMARY_ICON, SUMMARY_ROW } from "@/components/domain/shared/summary/summary-row";

export type NetworkChange = {http_egress: "public" | "restricted"} | {package_domains: boolean};
type Props = {
  publicInternet: boolean;
  packages: boolean;
  disabled: boolean;
  onChange: (change: NetworkChange) => void;
};
const PACKAGE_LABEL = "Packages";
const PACKAGE_HELP = "Allow standard package registries while public internet access is restricted.";

/** The fleet's two network switches. Both start off; the caller owns what a switch is confirmed as. */
export function FleetNetworkPolicy({publicInternet, packages, disabled, onChange}: Props) {
  const id = useId();
  return <>
    <div className={cn(SUMMARY_ROW, "min-h-9 pointer-coarse:min-h-12")}>
      <GlobeIcon className={SUMMARY_ICON} aria-hidden="true" />
      <Label htmlFor={`${id}-public`} className="flex-1 font-normal">Allow all</Label>
      <Switch id={`${id}-public`} aria-label="Allow all public internet access" checked={publicInternet} disabled={disabled}
        onCheckedChange={checked => onChange({http_egress: checked ? "public" : "restricted"})} />
    </div>
    <div className={cn(SUMMARY_ROW, "min-h-9 pointer-coarse:min-h-12")}>
      <PackageIcon className={SUMMARY_ICON} aria-hidden="true" />
      <div className="flex min-w-0 flex-1 items-center gap-xs">
        <Label htmlFor={`${id}-packages`} className={cn("whitespace-nowrap font-normal", publicInternet && "text-text-dim")}>{PACKAGE_LABEL}</Label>
        <PackageManagerDomainsHelp />
      </div>
      <Switch id={`${id}-packages`} checked={packages} disabled={disabled || publicInternet}
        onCheckedChange={checked => onChange({package_domains: checked})} />
    </div>
  </>;
}

function PackageManagerDomainsHelp() {
  return <Dialog>
    <DialogTrigger asChild>
      <TooltipButton tooltip={PACKAGE_HELP} variant="ghost" size="icon-compact" className="pointer-coarse:min-h-11 pointer-coarse:min-w-9" aria-label="About package manager domains">
        <InfoIcon size={16} aria-hidden="true" />
      </TooltipButton>
    </DialogTrigger>
    <DialogContent>
      <DialogHeader><DialogTitle>Package manager domains</DialogTitle><DialogDescription>{PACKAGE_HELP}</DialogDescription></DialogHeader>
      <p className="text-label leading-label text-muted-foreground">This option is disabled when Allow all is on. Its selection is kept when Allow all is switched off.</p>
      <DialogFooter><DialogClose asChild><Button>Done</Button></DialogClose></DialogFooter>
    </DialogContent>
  </Dialog>;
}
