"use client";

import { ArrowUpRightIcon, BrowserIcon, CaretRightIcon, PaperclipIcon, TerminalWindowIcon, UsersIcon } from "@phosphor-icons/react/ssr";
import { cn } from "@agentsfleet/design-system";
import { WorkSection } from "../../shared/summary/WorkSection";
import type { RunResource } from "./recorded-run-resources";


export function PreviewOutputs({links}: {links: readonly string[]}) {
  if (!links.length) return null;
  return <>{links.map(href =>
    <a key={href} href={href} target="_blank" rel="noopener noreferrer" className="fleet-resource-row">
      <BrowserIcon size={16} aria-hidden="true" /><span className="min-w-0 flex-1 font-medium">Launch preview</span><ArrowUpRightIcon size={16} aria-hidden="true" />
    </a>)}</>;
}

export function RunResources({rows}: {rows: readonly RunResource[]}) {
  if (!rows.length) return null;
  return <WorkSection title="Agents & processes">{rows.map(row => {
    const Icon = row.kind === "agent" ? UsersIcon : TerminalWindowIcon;
    return <details key={row.key} className="min-w-0">
      <summary className="fleet-resource-row cursor-pointer list-none [&::-webkit-details-marker]:hidden">
        <Icon size={16} className="shrink-0 text-muted-foreground" aria-hidden="true" /><span className="min-w-0 flex-1 break-words">{row.label}</span>
        <span className={cn("text-eyebrow leading-eyebrow", row.state === "Running" ? "text-success" : "text-muted-foreground")}>{row.state === "Last seen running" ? "Was running" : row.state}</span>
      </summary>
      <pre className="fleet-resource-detail">{row.state === "Last seen running" ? "Last reported running during this run.\n" : ""}{row.detail || row.state}</pre>
    </details>;
  })}</WorkSection>;
}

export function AttachedSources({files}: {files: readonly {id: string; name: string}[]}) {
  return <details className="group min-w-0"><summary className="fleet-resource-row cursor-pointer list-none [&::-webkit-details-marker]:hidden">
    <PaperclipIcon size={16} className="shrink-0 text-muted-foreground" aria-hidden="true" /><span className="flex-1 font-medium">{files.length} attached {files.length === 1 ? "file" : "files"}</span><CaretRightIcon size={16} className="group-open:rotate-90" aria-hidden="true" />
  </summary><ul className="space-y-xs border-l border-border pl-md text-label text-muted-foreground">{files.map(file => <li key={file.id} className="break-all">{file.name}</li>)}</ul></details>;
}
