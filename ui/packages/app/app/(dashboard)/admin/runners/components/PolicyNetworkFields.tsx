"use client";

import { useId, useState } from "react";
import { useController, type Control } from "react-hook-form";
import { PencilIcon } from "lucide-react";
import { Button, Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle,
  Label, Switch, Textarea } from "@agentsfleet/design-system";
import { parseRegistryAllowlist } from "@/lib/api/runners-types";
import type { PolicyFormValues } from "./PolicyFields";
import { runnerNetworkAssignment, runnerNetworkForm } from "./runner-network-form";

export function PolicyNetworkFields({ control }: { control: Control<PolicyFormValues> }) {
  const network = useController({control, name: "network_policy"});
  const registry = useController({control, name: "registry_allowlist"});
  const state = runnerNetworkForm(network.field.value, registry.field.value);
  const [open, setOpen] = useState(false);
  const [draft, setDraft] = useState("");
  const [error, setError] = useState<string | null>(null);
  const publicId = useId();
  const packagesId = useId();
  const domainsId = useId();
  const update = (publicInternet: boolean, packages: boolean, domains: readonly string[]) => {
    const assignment = runnerNetworkAssignment(publicInternet, packages, domains);
    const valid = parseRegistryAllowlist(assignment.registry_allowlist);
    if (valid.error) {setError(valid.error.replace("registries", "domains")); return false;}
    setError(null);
    network.field.onChange(assignment.network_policy);
    registry.field.onChange(assignment.registry_allowlist);
    return true;
  };
  const saveDomains = () => {
    const parsed = parseRegistryAllowlist(draft.replaceAll(/\r?\n/g, ","));
    if (parsed.error) {setError(parsed.error.replace("Registry", "Domain").replace("registries", "domains")); return;}
    // Viewing and saving dormant hosts must not broaden a deny-all assignment.
    if (parsed.hosts.join(",") === state.domains.join(",")) {setError(null); setOpen(false); return;}
    if (update(state.publicInternet, state.packages, parsed.hosts)) setOpen(false);
  };
  return <>
    <div className="divide-y divide-border rounded-xl border border-border bg-muted/30 px-lg">
      <div className="flex min-h-12 items-center justify-between gap-lg py-md">
        <Label htmlFor={publicId}>Enable all public internet access</Label>
        <Switch id={publicId} checked={state.publicInternet} onCheckedChange={checked => update(checked, checked && state.packages, state.domains)} onBlur={network.field.onBlur} ref={network.field.ref} />
      </div>
      <div className="flex min-h-12 items-center justify-between gap-lg py-md">
        <Label htmlFor={packagesId} className={state.publicInternet ? "text-muted-foreground" : undefined}>Enable package manager domains</Label>
        <Switch id={packagesId} checked={!state.publicInternet && state.packages} disabled={state.publicInternet}
          onCheckedChange={checked => update(false, checked, state.domains)} title={state.publicInternet ? "Public internet access already includes package managers" : "Allow the standard package registry hosts"} />
      </div>
      <div className="flex min-h-12 items-center justify-between gap-md py-md">
        <span className="text-label">Additional allowed domains</span>
        <Button type="button" variant="ghost" size="sm" className="min-w-0 gap-sm text-muted-foreground" aria-label="Edit additional allowed domains"
          onClick={() => {setDraft(state.domains.join("\n")); setError(null); setOpen(true);}}>
          <span className="truncate">{state.domains.length ? `${state.domains.length} ${state.domains.length === 1 ? "domain" : "domains"}` : "No additional domains"}{network.field.value === "deny_all_egress" && state.domains.length ? " · inactive" : ""}</span>
          <PencilIcon size={14} aria-hidden="true" />
        </Button>
      </div>
    </div>
    <p className="text-eyebrow text-muted-foreground">{state.publicInternet ? "Outbound connections are allowed. Each fleet keeps its own access rules."
      : state.packages || state.domains.length ? "Only approved destinations are allowed. Requires network enforcement on the host." : "Outbound network connections are blocked."}</p>
    {!open && error ? <p role="alert" className="text-label text-destructive">{error}</p> : null}
    {network.field.value === "deny_all_egress" && state.domains.length ? <p className="text-eyebrow text-muted-foreground">Saved domains are inactive while outbound access is blocked.</p> : null}
    <Dialog open={open} onOpenChange={value => {setOpen(value); setError(null);}}>
      <DialogContent className="max-w-lg">
        <DialogHeader><DialogTitle>Additional allowed domains</DialogTitle><DialogDescription>Enter host names separated by commas or new lines. Use exact hosts; wildcard domains are not supported.</DialogDescription></DialogHeader>
        <Label htmlFor={domainsId} className="sr-only">Additional allowed domains</Label>
        <Textarea id={domainsId} value={draft} onChange={event => {setDraft(event.target.value); setError(null);}} placeholder="example.com" rows={3} aria-invalid={Boolean(error)} aria-describedby={error ? `${domainsId}-error` : undefined} />
        {error ? <p id={`${domainsId}-error`} role="alert" className="text-label text-destructive">{error}</p> : null}
        <DialogFooter><Button type="button" variant="ghost" onClick={() => {setError(null); setOpen(false);}}>Cancel</Button><Button type="button" onClick={saveDomains}>Save</Button></DialogFooter>
      </DialogContent>
    </Dialog>
  </>;
}
