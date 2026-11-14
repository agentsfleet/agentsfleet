"use client";

import type { Plugin, PluginCard } from "@/lib/api/draft-plugin";
import { useCallback, useEffect, useState } from "react";
import nextDynamic from "next/dynamic";
import { CaretDownIcon, NotePencilIcon, PencilSimpleIcon } from "@phosphor-icons/react/ssr";
import { Button, Collapsible, CollapsibleContent, CollapsibleTrigger, Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle, Spinner, UnavailableState } from "@agentsfleet/design-system";
import { inspectPlugin } from "@/lib/api/draft-plugin";
import { losesAccess } from "@/lib/api/draft-request";
import { pluginUnavailableDescription } from "@/lib/plugin-copy";

const PluginHistory = nextDynamic(() => import("./PluginHistory").then(module => module.PluginHistory));
const PluginRemoval = nextDynamic(() => import("./PluginRemoval").then(module => module.PluginRemoval));
const FleetSetupDetails = nextDynamic(() => import("../setup/summary/FleetSetupDetails").then(module => module.FleetSetupDetails));

type Props = {workspace: string; card: PluginCard; onClose: () => void; onRemoved: () => void; onAccessLost: (cause: unknown) => void;
  onUse: (entry: Plugin, intent: "edit" | "new") => void};

export function PluginInspector({workspace, card, onClose, onRemoved, onAccessLost, onUse}: Props) {
  const [revision, setRevision] = useState<string | undefined>();
  const [attempt, setAttempt] = useState(0);
  const [entry, setEntry] = useState<Plugin | null>(null);
  const [error, setError] = useState<string | null>(null);
  const failed = useCallback((cause: unknown) => {
    if (losesAccess(cause)) onAccessLost(cause);
    else setError(pluginUnavailableDescription(undefined, true));
  }, [onAccessLost]);
  useEffect(() => {
    const controller = new AbortController();setEntry(null);setError(null);
    void inspectPlugin(workspace, card, controller.signal, revision).then(result => {
      if (!controller.signal.aborted) setEntry(result);
    }).catch(cause => {if (!controller.signal.aborted) failed(cause);});
    return () => controller.abort();
  }, [workspace, card, revision, attempt, failed]);
  return <Dialog open onOpenChange={onClose}><DialogContent>
    <DialogHeader><DialogTitle>{card.name}</DialogTitle><DialogDescription>{card.description}</DialogDescription></DialogHeader>
    {error ? <UnavailableState title="This plugin could not be loaded" description={error}
      actions={<Button variant="outline" onClick={() => setAttempt(value => value + 1)}>Retry plugin</Button>} /> : null}
    <Collapsible><CollapsibleTrigger asChild><Button variant="ghost" className="px-0">Choose a saved version<CaretDownIcon size={16} aria-hidden="true" /></Button></CollapsibleTrigger>
      <CollapsibleContent><PluginHistory workspace={workspace} plugin={card.id} onError={failed} onSelect={id => {setRevision(id);setAttempt(value => value + 1);}} /></CollapsibleContent>
    </Collapsible>
    {!entry && !error ? <Spinner label="Loading selected revision" /> : null}
    {entry && !error ? <div className="mt-md space-y-md">
      <p className="text-eyebrow text-text-dim">Version {entry.head.revision}</p>
      <Button variant="outline" className="min-h-11 w-full justify-start gap-md" onClick={() => onUse(entry, "edit")}><PencilSimpleIcon size={16} /><span>Edit setup</span></Button>
      <Button variant="outline" className="min-h-11 w-full justify-start gap-md" onClick={() => onUse(entry, "new")}><NotePencilIcon size={16} /><span>Use for new fleet</span></Button>
      <Collapsible><CollapsibleTrigger asChild><Button variant="ghost" className="px-0">Inspect setup<CaretDownIcon size={16} aria-hidden="true" /></Button></CollapsibleTrigger>
        <CollapsibleContent className="pt-lg"><FleetSetupDetails review={entry.review} /></CollapsibleContent></Collapsible>
    </div> : null}
    <div className="pt-md"><PluginRemoval workspace={workspace} card={card} onRemoved={onRemoved} /></div>
  </DialogContent></Dialog>;
}
