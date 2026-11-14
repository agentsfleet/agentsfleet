"use client";

import type { ReactNode } from "react";
import type { Definition, Review } from "@/lib/api/draft-types";
import type { SetupNetworkChange } from "@/lib/setup-requirements";
import { useId } from "react";
import { Button, Switch, TooltipButton, cn } from "@agentsfleet/design-system";
import { GitBranchIcon, InfoIcon, KeyIcon } from "@phosphor-icons/react/ssr";
import { setupRequirements } from "@/lib/setup-requirements";
import { formatDollars } from "@/lib/money";

type Props = {definition?: Definition; requirements?: Review["requirements"]; modelLabel?: string; sourceLabel?: string;
  ready?: boolean; scheduleRecovery?: ReactNode; disabled?: boolean;
  onNetworkChange?: (change: SetupNetworkChange) => void; onManage?: (kind: "repositories" | "credentials") => void};

export function SetupSummary({definition, requirements, modelLabel, sourceLabel, ready, scheduleRecovery, disabled, onNetworkChange, onManage}: Props) {
  const id = useId();
  if (!definition && !requirements && !ready) return <p className="text-label leading-label text-muted-foreground">Your setup will appear here as you chat.</p>;
  const setup = setupRequirements(definition, requirements);
  const network = setup?.network;
  const networkKnown = Boolean(network && (network.http_egress !== undefined || network.allow !== undefined || network.package_manager_domains !== undefined));
  const publicInternet = network?.http_egress === "public";
  const selected = (items: readonly string[] | undefined, empty: string, kind: "repositories" | "credentials") => items
    ? items.length ? <ul className="space-y-sm">{items.map(name => <li key={name} className="flex min-w-0 items-center gap-md py-xs">
      {kind === "repositories" ? <GitBranchIcon size={16} className="shrink-0 text-muted-foreground" aria-hidden="true" /> : <KeyIcon size={16} className="shrink-0 text-muted-foreground" aria-hidden="true" />}
      <span className="min-w-0 break-words">{name}</span></li>)}</ul> : <p className="text-muted-foreground">{empty}</p>
    : <p className="text-muted-foreground">Not reviewed</p>;
  return <div className="min-w-0 space-y-lg text-label leading-label [overflow-wrap:anywhere]">
    <dl className="space-y-md">
      <Meta label="Model">{modelLabel ?? "Not reviewed"}</Meta>
      <Meta label="Spending limit">{setup ? <>{formatDollars(setup.budget.daily_nanos, 9)} / day{setup.budget.monthly_nanos ? <span className="block">{formatDollars(setup.budget.monthly_nanos, 9)} / month</span> : null}</> : "Not reviewed"}</Meta>
    </dl>
    {scheduleRecovery}
    {sourceLabel ? <p className="text-eyebrow leading-eyebrow text-text-dim">From {sourceLabel}</p> : null}
    <SummarySection title="Repositories" action={onManage ? <Manage kind="repositories" disabled={disabled} onClick={() => onManage("repositories")} /> : undefined}>
      {selected(setup?.repositories, "No repositories selected", "repositories")}
      <p className="text-eyebrow leading-eyebrow text-muted-foreground"><span className="mr-sm text-text-dim">Tools</span>{setup ? setup.tools.length ? setup.tools.join(", ") : "None selected" : "Not reviewed"}</p>
    </SummarySection>
    <SummarySection title="Public internet access">
      {networkKnown ? <div className="space-y-xs">
        <div className="flex min-h-11 flex-wrap items-center justify-between gap-x-md gap-y-xs sm:min-h-9"><label htmlFor={`${id}-public`}>Allow all</label>
          <Switch id={`${id}-public`} aria-label="Allow all public internet access" checked={publicInternet} disabled={disabled || !onNetworkChange}
            onCheckedChange={checked => onNetworkChange?.({http_egress: checked ? "public" : "restricted"})} /></div>
        <div className="flex min-h-11 flex-wrap items-center justify-between gap-x-md gap-y-xs sm:min-h-9"><div className="flex max-w-full shrink-0 flex-wrap items-center gap-xs">
          <label htmlFor={`${id}-packages`} className={cn("whitespace-nowrap", publicInternet && "text-text-dim")}>Enable package manager domains</label>
          <TooltipButton tooltip="Allow standard package registries while public internet access is restricted." variant="ghost" size="icon-compact" aria-label="About package manager domains"><InfoIcon size={16} aria-hidden="true" /></TooltipButton>
        </div><Switch id={`${id}-packages`} className="ml-auto" checked={network?.package_manager_domains === true} disabled={disabled || publicInternet || !onNetworkChange}
          onCheckedChange={checked => onNetworkChange?.({package_manager_domains: checked})} /></div>
      </div> : <p className="text-muted-foreground">Network policy not reviewed</p>}
    </SummarySection>
    <SummarySection title="Secrets" action={onManage ? <Manage kind="secrets" disabled={disabled} onClick={() => onManage("credentials")} /> : undefined}>
      {selected(setup?.credentials, "No secrets selected", "credentials")}
    </SummarySection>
  </div>;
}

function Meta({label, children}: {label: string; children: ReactNode}) {
  return <div className="grid grid-cols-[5.25rem_minmax(0,1fr)] items-baseline gap-md"><dt className="text-eyebrow leading-eyebrow text-text-dim">{label}</dt><dd className="min-w-0 break-words">{children}</dd></div>;
}
function SummarySection({title, action, children}: {title: string; action?: ReactNode; children: ReactNode}) {
  return <section aria-label={title} className="space-y-sm border-t border-border pt-md"><div className="flex items-center justify-between gap-sm"><h3 className="text-eyebrow leading-eyebrow font-normal text-text-dim">{title}</h3>{action}</div>{children}</section>;
}
function Manage({kind, onClick, disabled}: {kind: string; onClick: () => void; disabled?: boolean}) {
  return <Button variant="ghost" size="compact" className="px-0 text-eyebrow text-text-dim" aria-label={`Manage ${kind}`} disabled={disabled} onClick={onClick}>Manage</Button>;
}
