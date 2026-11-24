"use client";

import type { SetupNetworkChange } from "@/lib/setup-requirements";
import { useId } from "react";
import Link from "next/link";
import { Button, Dialog, DialogClose, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle, DialogTrigger, Label, Switch, SwitchIndicator, TooltipButton, cn } from "@agentsfleet/design-system";
import { GlobeIcon, InfoIcon, PackageIcon } from "@phosphor-icons/react/ssr";
import { SUMMARY_ICON, SUMMARY_ROW } from "@/components/domain/shared/summary/summary-row";

type Props = {
  publicInternet: boolean;
  packages: boolean;
  disabled?: boolean;
  editHref?: string;
  onChange?: (change: SetupNetworkChange) => void;
};
const PACKAGE_HELP = "Allow standard package registries while public internet access is restricted.";

export function SetupNetworkPolicy({publicInternet, packages, disabled, editHref, onChange}: Props) {
  const id = useId();
  return <>
    <div className={cn(SUMMARY_ROW, "min-h-11")}>
      <GlobeIcon className={SUMMARY_ICON} aria-hidden="true" />
      {editHref ? <span className="flex-1">Allow all</span> : <Label htmlFor={`${id}-public`} className="flex-1 font-normal">Allow all</Label>}
      {editHref ? <NetworkEditLink href={editHref} checked={publicInternet} label="Allow all public internet access" />
        : <Switch id={`${id}-public`} aria-label="Allow all public internet access" checked={publicInternet} disabled={disabled || !onChange}
          onCheckedChange={checked => onChange?.({http_egress: checked ? "public" : "restricted"})} />}
    </div>
    <div className={cn(SUMMARY_ROW, "min-h-11 border-t border-border-subtle pt-md")}>
      <PackageIcon className={SUMMARY_ICON} aria-hidden="true" />
      <div className="flex min-w-0 flex-1 items-center gap-xs">
        {editHref && !publicInternet ? <span>Enable package manager domains</span>
          : <Label htmlFor={`${id}-packages`} className={cn("min-w-0 font-normal", publicInternet && "text-text-dim")}>Enable package manager domains</Label>}
        <PackageManagerDomainsHelp />
      </div>
      {editHref && !publicInternet ? <NetworkEditLink href={editHref} checked={packages} label="Enable package manager domains" />
        : <Switch id={`${id}-packages`} checked={packages} disabled={disabled || publicInternet || !onChange}
          onCheckedChange={checked => onChange?.({package_manager_domains: checked})} />}
    </div>
  </>;
}

function NetworkEditLink({href, checked, label}: {href: string; checked: boolean; label: string}) {
  return <Link href={href} aria-label={`${label} (${checked ? "on" : "off"}). Edit setup`}
    className="inline-flex min-h-11 shrink-0 items-center rounded-control focus-visible:outline-2 focus-visible:outline-ring">
    <SwitchIndicator checked={checked} />
  </Link>;
}

function PackageManagerDomainsHelp() {
  return <Dialog>
    <DialogTrigger asChild>
      <TooltipButton tooltip={PACKAGE_HELP} variant="ghost" size="icon-compact" aria-label="About package manager domains">
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
