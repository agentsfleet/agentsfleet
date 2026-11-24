"use client";

import type { ReactNode } from "react";
import type { Definition, Review, Schedule } from "@/lib/api/draft-types";
import type { SetupNetworkChange } from "@/lib/setup-requirements";
import type { Icon as PhosphorIcon } from "@phosphor-icons/react";
import { useId, useState } from "react";
import Link from "next/link";
import { Button, DescriptionDetails, DescriptionList, DescriptionTerm, List, ListItem, Section, SectionLabel, Time, cn, sameCalendarDay } from "@agentsfleet/design-system";
import { ClockIcon, CpuIcon, GitBranchIcon, KeyIcon, TerminalWindowIcon, WalletIcon } from "@phosphor-icons/react/ssr";
import { SUMMARY_ICON, SUMMARY_ROW } from "@/components/domain/shared/summary/summary-row";
import { SecretValueMask } from "@/components/domain/shared/secrets/SecretValueMask";
import { hasProposedWakeup, setupRequirements } from "@/lib/setup-requirements";
import { SetupNetworkPolicy } from "./SetupNetworkPolicy";
import { formatDollars } from "@/lib/money";
import { nextFleetWakeup } from "@/lib/api/schedule-status";

type Props = {definition?: Definition; requirements?: Review["requirements"]; modelLabel?: string; schedules?: readonly Schedule[];
  ready?: boolean; scheduleRecovery?: ReactNode; disabled?: boolean; secretEditor?: ReactNode; editHref?: string;
  onNetworkChange?: (change: SetupNetworkChange) => void; onManage?: (kind: "repositories" | "credentials", trigger: HTMLButtonElement) => void};

const SHELL_TOOLS = new Set(["shell", "exec_command", "write_stdin"]);
const FILE_TOOLS = new Set(["apply_patch", "file_read", "file_read_hashed", "file_write", "file_edit", "file_edit_hashed", "file_append", "file_delete"]);

export function SetupSummary({definition, requirements, modelLabel, schedules = [], ready, scheduleRecovery, disabled, secretEditor, editHref, onNetworkChange, onManage}: Props) {
  const id = useId();
  const [today] = useState(() => new Date());
  if (!definition && !requirements && !ready) return <p className="text-label leading-label text-muted-foreground">Your setup will appear here as you chat.</p>;
  const setup = setupRequirements(definition, requirements);
  const network = setup?.network;
  const nextWakeup = nextFleetWakeup(schedules);
  const tools = setup?.tools.length ? toolSummary(setup.tools) : null;
  return <div className="ph-no-capture ph-no-record min-w-0 text-label leading-label [overflow-wrap:anywhere]">
    <DescriptionList className="space-y-0 border-b border-border pb-md text-label [&>div]:flex-nowrap [&>div]:items-center [&>div]:gap-md">
      <Meta icon={WalletIcon} label="Spending limit">{setup ? <>
        <BudgetAmount nanos={setup.budget.daily_nanos} period="day" />
        {setup.budget.monthly_nanos ? <BudgetAmount nanos={setup.budget.monthly_nanos} period="month" /> : null}
      </> : "Not reviewed"}</Meta>
      <Meta icon={ClockIcon} label="Next wake-up">{nextWakeup ? <Time value={nextWakeup} format={sameCalendarDay(nextWakeup, today) ? "minute" : "compact"} /> : wakeupStatus(schedules, definition, Boolean(setup))}</Meta>
      {modelLabel ? <Meta icon={CpuIcon} label="Model"><ScrollableValue label="Model name">{modelLabel}</ScrollableValue></Meta> : null}
    </DescriptionList>
    {scheduleRecovery ? <div className="mt-lg">{scheduleRecovery}</div> : null}
    <SummarySection title="Repositories" action={<Manage kind="repositories" href={editHref} disabled={disabled} onClick={onManage && (trigger => onManage("repositories", trigger))} />}>
      <SummaryResources items={setup?.repositories} kind="repositories" />
      {tools ? <p className={cn(SUMMARY_ROW, "min-h-8 py-xs text-eyebrow leading-summary-label text-muted-foreground")}>
        <TerminalWindowIcon className={SUMMARY_ICON} aria-hidden="true" />
        <span className="min-w-0"><span className="text-text-dim">Tools · </span><span>{tools}</span></span>
      </p> : null}
    </SummarySection>
    <SummarySection title="Public internet access">
      {setup ? <SetupNetworkPolicy publicInternet={network?.http_egress === "public"} packages={network?.package_manager_domains === true}
        editHref={editHref} disabled={disabled} onChange={onNetworkChange} /> : <p className="text-muted-foreground">Network policy not reviewed</p>}
    </SummarySection>
    <SummarySection title="Secrets" action={<Manage kind="secrets" href={editHref} disabled={disabled} expanded={Boolean(secretEditor)}
      controls={secretEditor ? `${id}-secrets` : undefined} onClick={onManage && (trigger => onManage("credentials", trigger))} />}>
      {secretEditor ? <div id={`${id}-secrets`}>{secretEditor}</div> : <SummaryResources items={setup?.credentials} kind="secrets" />}
    </SummarySection>
  </div>;
}

function ScrollableValue({label, children, className, textClassName}: {label: string; children: ReactNode; className?: string; textClassName?: string}) {
  return <Section aria-label={label} tabIndex={0} className={cn("summary-scroll-label", className)}><span className={textClassName}>{children}</span></Section>;
}

function BudgetAmount({nanos, period}: {nanos: string; period: "day" | "month"}) {
  return <ScrollableValue label={`Spending limit per ${period}`} textClassName="text-eyebrow font-title leading-label tabular-nums">
    {formatDollars(nanos, 9)} <small className="text-eyebrow font-medium text-text-dim">/ {period}</small>
  </ScrollableValue>;
}

function Meta({icon: Icon, label, children}: {icon: PhosphorIcon; label: string; children: ReactNode}) {
  return <div className="min-h-7 min-w-0">
    <DescriptionTerm className={cn(SUMMARY_ROW, "shrink-0 text-eyebrow leading-summary-label text-text-dim")}>
      <Icon className={SUMMARY_ICON} aria-hidden="true" /><span>{label}</span>
    </DescriptionTerm>
    <DescriptionDetails className="min-w-0 flex-1 break-words font-medium leading-label">{children}</DescriptionDetails>
  </div>;
}
function SummarySection({title, action, children}: {title: string; action?: ReactNode; children: ReactNode}) {
  return <Section aria-label={title} className="block space-y-sm pt-lg">
    <div className="flex items-center justify-between gap-sm">
      <SectionLabel as="h3" variant="summary" className="font-medium">{title}</SectionLabel>
      {action}
    </div>
    {children}
  </Section>;
}
function SummaryResources({items, kind}: {items?: readonly string[]; kind: "repositories" | "secrets"}) {
  if (!items?.length) return <p className="text-muted-foreground">{items ? `No ${kind} selected` : "Not reviewed"}</p>;
  const Icon = kind === "repositories" ? GitBranchIcon : KeyIcon;
  return <List variant="plain" className="space-y-0 text-label leading-label">{items.map(name => <ListItem key={name} className={cn(SUMMARY_ROW, "min-h-8 py-xs")}>
    <Icon className={SUMMARY_ICON} aria-hidden="true" />
    <ScrollableValue label={kind === "secrets" ? "Secret name" : "Repository name"} className="flex-1 font-medium">{name}</ScrollableValue>
    {kind === "secrets" ? <span className="shrink-0 text-muted-foreground"><SecretValueMask /></span> : null}
  </ListItem>)}</List>;
}
function Manage({kind, href, onClick, disabled, expanded, controls}: {kind: "secrets" | "repositories"; href?: string; onClick?: (trigger: HTMLButtonElement) => void; disabled?: boolean; expanded?: boolean; controls?: string}) {
  if (!href && !onClick) return null;
  return <Button asChild={Boolean(href)} variant="ghost" size="compact" className="min-h-7 px-0 text-eyebrow leading-eyebrow pointer-coarse:min-h-11"
    aria-label={`Manage ${kind}`} aria-expanded={href ? undefined : expanded} aria-controls={controls} disabled={disabled}
    onClick={onClick ? event => onClick(event.currentTarget) : undefined}>
    {href ? <Link href={`${href}#${kind}`}>Manage</Link> : "Manage"}
  </Button>;
}
function wakeupStatus(schedules: readonly Schedule[], definition: Definition | undefined, known: boolean): string {
  if (schedules.some(schedule => schedule.status === "active")) return "Unavailable";
  if (schedules.some(schedule => schedule.status === "paused")) return "Paused";
  if (hasProposedWakeup(definition) && !schedules.length) return "Pending save";
  return known ? "Not set" : "Not reviewed";
}

function toolSummary(tools: readonly string[]): string {
  const categories = new Set(tools.map(tool => SHELL_TOOLS.has(tool) ? "Shell" : FILE_TOOLS.has(tool) ? "Files" : tool.replaceAll("_", " ")));
  const [first, second] = categories;
  return [first, categories.size > 2 ? `+${categories.size - 1} more` : second].filter(Boolean).join(" · ");
}
