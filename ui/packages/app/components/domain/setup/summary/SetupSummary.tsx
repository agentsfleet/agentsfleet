"use client";

import type { ReactNode } from "react";
import type { Definition, Review, Schedule } from "@/lib/api/draft-types";
import type { SetupNetworkChange } from "@/lib/setup-requirements";
import { useId, useState } from "react";
import Link from "next/link";
import { Button, Section, SectionLabel, Time, cn, sameCalendarDay } from "@agentsfleet/design-system";
import { GitBranchIcon, KeyIcon, PuzzlePieceIcon } from "@phosphor-icons/react/ssr";
import { hasProposedWakeup, setupRequirements } from "@/lib/setup-requirements";
import { SetupNetworkPolicy } from "./SetupNetworkPolicy";
import { formatDollars } from "@/lib/money";
import { nextFleetWakeup } from "@/lib/api/schedule-status";

type Props = {definition?: Definition; requirements?: Review["requirements"]; modelLabel?: string; sourceLabel?: string; schedules?: readonly Schedule[];
  ready?: boolean; scheduleRecovery?: ReactNode; disabled?: boolean; secretEditor?: ReactNode; editHref?: string;
  onNetworkChange?: (change: SetupNetworkChange) => void; onManage?: (kind: "repositories" | "credentials", trigger: HTMLButtonElement) => void};

const SHELL_TOOLS = new Set(["shell", "exec_command", "write_stdin"]);
const FILE_TOOLS = new Set(["apply_patch", "file_read", "file_read_hashed", "file_write", "file_edit", "file_edit_hashed", "file_append", "file_delete"]);

export function SetupSummary({definition, requirements, modelLabel, sourceLabel, schedules = [], ready, scheduleRecovery, disabled, secretEditor, editHref, onNetworkChange, onManage}: Props) {
  const id = useId();
  const [today] = useState(() => new Date());
  if (!definition && !requirements && !ready) return <p className="text-label leading-label text-muted-foreground">Your setup will appear here as you chat.</p>;
  const setup = setupRequirements(definition, requirements);
  const network = setup?.network;
  const nextWakeup = nextFleetWakeup(schedules);
  const tools = setup?.tools.length ? toolSummary(setup.tools) : null;
  return <div className="ph-no-capture ph-no-record min-w-0 text-label leading-label [overflow-wrap:anywhere]">
    <dl className="space-y-md">
      <Meta label="Model">{modelLabel ?? "Not reviewed"}</Meta>
      <Meta label="Spending limit" emphasize>{setup ? <>{formatDollars(setup.budget.daily_nanos, 9)} / day{setup.budget.monthly_nanos ? <span className="block">{formatDollars(setup.budget.monthly_nanos, 9)} / month</span> : null}</> : "Not reviewed"}</Meta>
      <Meta label="Next wake-up" emphasize>{nextWakeup ? <Time value={nextWakeup} format={sameCalendarDay(nextWakeup, today) ? "minute" : "compact"} /> : wakeupStatus(schedules, definition, Boolean(setup))}</Meta>
    </dl>
    {sourceLabel ? <p className="mt-lg flex items-center gap-sm text-eyebrow leading-eyebrow text-text-dim"><PuzzlePieceIcon size={14} className="shrink-0" aria-hidden="true" />From {sourceLabel}</p> : null}
    {scheduleRecovery ? <div className="mt-lg">{scheduleRecovery}</div> : null}
    <SummarySection title="Repositories" className="mt-lg" action={<Manage kind="repositories" href={editHref} disabled={disabled} onClick={onManage && (trigger => onManage("repositories", trigger))} />}>
      <SummaryResources items={setup?.repositories} kind="repositories" />
      {tools ? <p className="mt-sm text-eyebrow leading-summary-label text-muted-foreground"><span className="mr-md text-text-dim">Tools</span>{tools}</p> : null}
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

function Meta({label, emphasize, children}: {label: string; emphasize?: boolean; children: ReactNode}) {
  return <div className="grid grid-cols-[5.25rem_minmax(0,1fr)] items-baseline gap-md">
    <dt className="text-eyebrow leading-summary-label text-text-dim">{label}</dt>
    <dd className={cn("min-w-0 break-words", emphasize && "font-medium")}>{children}</dd>
  </div>;
}
function SummarySection({title, action, children, className}: {title: string; action?: ReactNode; children: ReactNode; className?: string}) {
  return <Section aria-label={title} variant="divided" className={cn("space-y-sm py-lg last:pb-0", className)}>
    <div className="flex items-center justify-between gap-sm">
      <SectionLabel as="h3" variant="summary">{title}</SectionLabel>
      {action}
    </div>
    {children}
  </Section>;
}
function SummaryResources({items, kind}: {items?: readonly string[]; kind: "repositories" | "secrets"}) {
  if (!items?.length) return <p className="text-muted-foreground">{items ? `No ${kind} selected` : "Not reviewed"}</p>;
  const Icon = kind === "repositories" ? GitBranchIcon : KeyIcon;
  return <ul>{items.map(name => <li key={name} className="flex min-h-8 min-w-0 items-center gap-md py-xs">
    <Icon size={16} className="shrink-0 text-muted-foreground" aria-hidden="true" />
    <span className="min-w-0 break-words font-medium">{name}</span>
  </li>)}</ul>;
}
function Manage({kind, href, onClick, disabled, expanded, controls}: {kind: "secrets" | "repositories"; href?: string; onClick?: (trigger: HTMLButtonElement) => void; disabled?: boolean; expanded?: boolean; controls?: string}) {
  if (!href && !onClick) return null;
  return <Button asChild={Boolean(href)} variant="ghost" size="compact" className="px-0 text-eyebrow"
    aria-label={`Manage ${kind}`} aria-expanded={href ? undefined : expanded} aria-controls={controls} disabled={disabled}
    onClick={onClick ? event => onClick(event.currentTarget) : undefined}>
    {href ? <Link href={`${href}#${kind}`}>Manage</Link> : "Manage"}
  </Button>;
}
function wakeupStatus(schedules: readonly Schedule[], definition: Definition | undefined, known: boolean): string {
  if (schedules.some(schedule => schedule.status === "active")) return "Unavailable";
  if (schedules.some(schedule => schedule.status === "paused")) return "Paused";
  if (hasProposedWakeup(definition) && !schedules.length) return "Pending save";
  return known ? "Not scheduled" : "Not reviewed";
}

function toolSummary(tools: readonly string[]): string {
  const categories = new Set(tools.map(tool => SHELL_TOOLS.has(tool) ? "Shell" : FILE_TOOLS.has(tool) ? "Files" : tool.replaceAll("_", " ")));
  const [first, second] = categories;
  return [first, categories.size > 2 ? `+${categories.size - 1} more` : second].filter(Boolean).join(" · ");
}
