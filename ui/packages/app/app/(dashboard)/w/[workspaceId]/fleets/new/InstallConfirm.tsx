"use client";

import { useState } from "react";
import {
  Button,
  DashboardPanel,
  DashboardPanelContent,
  DashboardPanelDescription,
  DashboardPanelHeader,
  DashboardPanelTitle,
  Input,
  SectionLabel,
} from "@agentsfleet/design-system";
import type { FleetLibraryGalleryEntry } from "@/lib/types";

type Props = {
  entry: FleetLibraryGalleryEntry;
  onInstall: (name: string) => void;
  onBack: () => void;
};

// Confirm step between picking a library entry and the live install states.
// Lets the operator optionally name the fleet so one library entry can back
// several fleets in a workspace (parity with `agentsfleet install --library
// <id> --name`). A blank name falls back to the entry's SKILL.md `name:`.
export function InstallConfirm({ entry, onInstall, onBack }: Props) {
  const [name, setName] = useState("");
  return (
    <DashboardPanel padding="compact" className="max-w-prose">
      <DashboardPanelHeader>
        <div className="space-y-2">
          <SectionLabel>Install</SectionLabel>
          <DashboardPanelTitle>{entry.name}</DashboardPanelTitle>
          {entry.description ? (
            <DashboardPanelDescription>{entry.description}</DashboardPanelDescription>
          ) : null}
        </div>
      </DashboardPanelHeader>
      <DashboardPanelContent>
        <form
          className="space-y-md"
          onSubmit={(event) => {
            event.preventDefault();
            onInstall(name);
          }}
        >
          <div className="space-y-1">
            <SectionLabel>Fleet name (optional)</SectionLabel>
            <Input
              value={name}
              onChange={(event) => setName(event.target.value)}
              placeholder={entry.name}
              aria-label="Fleet name"
              autoComplete="off"
              spellCheck={false}
            />
            <p className="text-body-sm leading-body-sm text-muted-foreground">
              Leave blank to use the template name. Set a name to run more than one
              fleet from this template.
            </p>
          </div>
          <div className="flex flex-wrap items-center gap-2">
            <Button type="submit">Install</Button>
            <Button type="button" variant="ghost" onClick={onBack}>
              Back
            </Button>
          </div>
        </form>
      </DashboardPanelContent>
    </DashboardPanel>
  );
}
