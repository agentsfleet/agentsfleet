"use client";

import type { ReactNode, RefObject } from "react";
import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState } from "react";
import nextDynamic from "next/dynamic";
import { useRouter } from "next/navigation";
import { XIcon } from "lucide-react";
import { Button, Section, Skeleton, cn } from "@agentsfleet/design-system";
import { SOURCE_KIND } from "@/lib/api/draft-types";
import { useCurrentUser } from "@/lib/auth/client";
import { FleetStreamProvider } from "../FleetStreamProvider";

type Editor = {open: () => void; close: () => void; enabled: boolean; editing: boolean; trigger: RefObject<HTMLButtonElement | null>};
const SetupContext = createContext<Editor | null>(null);
const SetupChat = nextDynamic(() => import("./WorkspaceChat").then(module => module.WorkspaceChat), {
  ssr: false, loading: () => <output className="contents" aria-label="Opening setup chat"><Skeleton className="min-h-0 flex-1" /></output>,
});

/** Server-rendered fleet content stays mounted while its private editor opens. */
export function FleetSetupWorkspace({account, workspace, fleet, revision, viewer, header, children}: {
  account: string | null; workspace: string; fleet: string; revision: string; viewer: string | null; header: ReactNode; children: ReactNode;
}) {
  const {userId, isLoaded} = useCurrentUser();
  if (isLoaded && !userId) return null;
  // Verified server identity keeps hydration from resetting an already open editor.
  const subject = isLoaded ? userId : viewer;
  return <FleetStreamProvider key={`${account}:${workspace}:${subject}`}>
    <FleetWorkspace account={account} workspace={workspace} fleet={fleet} revision={revision} header={header}>{children}</FleetWorkspace>
  </FleetStreamProvider>;
}

function FleetWorkspace({account, workspace, fleet, revision, header, children}: {
  account: string | null; workspace: string; fleet: string; revision: string; header: ReactNode; children: ReactNode;
}) {
  const [editing, setEditing] = useState(false);
  const [opened, setOpened] = useState(false);
  const trigger = useRef<HTMLButtonElement>(null);
  const router = useRouter();
  const open = useCallback(() => {setOpened(true); setEditing(true);}, []);
  const close = useCallback(() => {setEditing(false); trigger.current?.focus({preventScroll: true});}, []);
  const saved = useCallback(() => {close(); setOpened(false); router.refresh();}, [close, router]);
  const editor = useMemo(() => ({open, close, trigger, editing, enabled: Boolean(account)}), [open, close, account, editing]);
  return <SetupContext value={editor}>
    {header}
    <div className="relative flex min-h-0 flex-1 overflow-clip">
      <div className={cn("flex min-h-0 min-w-0 flex-1 flex-col", editing && "hidden md:flex")}>{children}</div>
      {opened && account ? <SetupPanel hidden={!editing} account={account} workspace={workspace} fleet={fleet} revision={revision} close={close} saved={saved} /> : null}
    </div>
  </SetupContext>;
}

export function EditSetupButton() {
  const editor = useContext(SetupContext);
  return <Button ref={editor?.trigger} variant="outline" size="sm" disabled={!editor?.enabled}
    title={editor?.enabled ? undefined : "Workspace access must be available to edit setup."}
    aria-expanded={editor?.editing ?? false} onClick={editor?.open}>Edit setup</Button>;
}

function SetupPanel({account, workspace, fleet, revision, close, saved, hidden}: {
  account: string; workspace: string; fleet: string; revision: string; close: () => void; saved: () => void; hidden: boolean;
}) {
  useEffect(() => {
    if (hidden) return;
    const escape = (event: KeyboardEvent): void => {if (event.key === "Escape" && !event.defaultPrevented) close();};
    window.addEventListener("keydown", escape);
    return () => window.removeEventListener("keydown", escape);
  }, [close, hidden]);
  return <Section asChild className={cn("ph-no-capture ph-no-record min-h-0 w-full shrink-0 flex-col border-l border-border bg-background md:max-w-form", hidden ? "hidden" : "flex")}>
    <aside aria-label="Edit fleet setup" hidden={hidden}>
      <div className="fleet-setup-toolbar flex min-h-11 shrink-0 items-center justify-between gap-md border-b border-border px-xl sm:px-chat-inset">
        <div className="flex min-w-0 flex-wrap items-baseline gap-x-md"><h2 className="text-body-sm font-semibold">Edit setup</h2><p className="text-eyebrow text-muted-foreground">Based on v{revision} · Private to you</p></div>
        <Button size="icon" variant="ghost" aria-label="Close setup editor" onClick={close}><XIcon className="size-4" aria-hidden="true" /></Button>
      </div>
      <SetupChat account={account} workspace={workspace} source={{kind: SOURCE_KIND.fleet, fleet_id: fleet, setup_revision: revision}} onSaved={saved} />
    </aside>
  </Section>;
}
