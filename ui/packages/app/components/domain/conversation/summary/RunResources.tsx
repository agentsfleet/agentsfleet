"use client";

import { ArrowUpRightIcon, CaretRightIcon, GlobeIcon, PaperclipIcon, TerminalWindowIcon, UsersIcon } from "@phosphor-icons/react/ssr";
import { List, ListItem, cn } from "@agentsfleet/design-system";
import { SUMMARY_ICON, SUMMARY_ROW } from "@/components/domain/shared/summary/summary-row";
import { WorkSection } from "../../shared/summary/WorkSection";
import type { RunResource } from "./recorded-run-resources";

const RESOURCE_ROW = `${SUMMARY_ROW} min-h-8 rounded-sm py-sm text-label leading-label hover:text-foreground focus-visible:outline-2 focus-visible:outline-ring pointer-coarse:min-h-11`;
const RESOURCE_DISCLOSURE = `${RESOURCE_ROW} cursor-pointer list-none [&::-webkit-details-marker]:hidden`;

export function PreviewOutputs({links}: {links: readonly string[]}) {
  if (!links.length) return null;
  return <>{links.map(href =>
    <a key={href} href={href} target="_blank" rel="noopener noreferrer" className={RESOURCE_ROW}>
      <GlobeIcon className={SUMMARY_ICON} aria-hidden="true" /><span className="min-w-0 flex-1 font-medium">Launch preview</span><ArrowUpRightIcon className={SUMMARY_ICON} aria-hidden="true" />
    </a>)}</>;
}

export function RunResources({rows}: {rows: readonly RunResource[]}) {
  if (!rows.length) return null;
  return <WorkSection title="Agents & processes">{rows.map(row => {
    const Icon = row.kind === "agent" ? UsersIcon : TerminalWindowIcon;
    return <details key={row.key} className="min-w-0">
      <summary className={RESOURCE_DISCLOSURE}>
        <Icon className={SUMMARY_ICON} aria-hidden="true" /><span className="min-w-0 flex-1 break-words">{row.label}</span>
        <span className={cn("text-eyebrow leading-eyebrow", row.state === "Running" ? "text-success" : "text-muted-foreground")}>{row.state === "Last seen running" ? "Was running" : row.state}</span>
      </summary>
      <pre className="max-h-40 overflow-auto whitespace-pre-wrap break-words border-l border-border pl-md text-eyebrow text-muted-foreground">{row.state === "Last seen running" ? "Last reported running during this run.\n" : ""}{row.detail || row.state}</pre>
    </details>;
  })}</WorkSection>;
}

export function AttachedSources({files}: {files: readonly {id: string; name: string}[]}) {
  return <details className="group min-w-0"><summary className={RESOURCE_DISCLOSURE}>
    <PaperclipIcon className={SUMMARY_ICON} aria-hidden="true" /><span className="min-w-0 flex-1 font-medium">{files.length} attached {files.length === 1 ? "file" : "files"}</span><CaretRightIcon className={cn(SUMMARY_ICON, "group-open:rotate-90")} aria-hidden="true" />
  </summary><List variant="plain" className="space-y-xs pl-6 text-label leading-label text-muted-foreground">
    {files.map(file => <ListItem key={file.id} className="break-all">{file.name}</ListItem>)}
  </List></details>;
}
