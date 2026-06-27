"use client";

import { useState } from "react";
import {
  Button,
  DashboardPanel,
  DashboardPanelContent,
  DashboardPanelDescription,
  DashboardPanelHeader,
  DashboardPanelTitle,
  EmptyState,
  Input,
  SectionLabel,
} from "@agentsfleet/design-system";
import type { FleetTemplate } from "@/lib/types";
import { InstallFlowGuide } from "./InstallFlowGuide";
import { TemplateCard } from "./TemplateCard";

const GITHUB_PLACEHOLDER = "owner/repo";

type Props = {
  templates: FleetTemplate[];
  onUseTemplate: (template: FleetTemplate) => void;
  onImport: (sourceRef: string) => void;
  importPending: boolean;
  importError: string | null;
  onPaste: () => void;
};

// Minimal, one-experience source selector: curated templates lead as the
// primary choice; an `owner/repo` import is the secondary path; pasting a
// SKILL.md is a quiet tertiary link. Picking any source proceeds inline to the
// live install states — there is no review page.
export function InstallSourceSelector({
  templates,
  onUseTemplate,
  onImport,
  importPending,
  importError,
  onPaste,
}: Props) {
  const [repo, setRepo] = useState("");
  return (
    <div className="grid grid-cols-1 gap-lg lg:grid-cols-3">
      <DashboardPanel padding="compact" className="lg:col-span-2">
        <DashboardPanelHeader>
          <div className="space-y-2">
            <SectionLabel>Next step</SectionLabel>
            <DashboardPanelTitle>Start your fleet</DashboardPanelTitle>
            <DashboardPanelDescription className="max-w-prose">
              Pick a template, import GitHub, or paste SKILL.md. Install runs inline.
            </DashboardPanelDescription>
          </div>
        </DashboardPanelHeader>

        <DashboardPanelContent>
          <div className="space-y-lg">
            <div className="space-y-sm">
              <SectionLabel>Start from a template</SectionLabel>
              {templates.length > 0 ? (
                <div className="grid grid-cols-1 gap-md sm:grid-cols-2">
                  {templates.map((template) => (
                    <TemplateCard
                      key={template.id}
                      template={template}
                      action={
                        <Button type="button" onClick={() => onUseTemplate(template)}>
                          Use template
                        </Button>
                      }
                    />
                  ))}
                </div>
              ) : (
                <EmptyState
                  title="No templates available yet"
                  description="Import GitHub or paste SKILL.md."
                />
              )}
            </div>

            <div className="space-y-3 border-t border-border pt-lg">
              <SectionLabel>Or start from source</SectionLabel>
              <div className="flex flex-wrap items-start gap-md">
                <div className="min-w-64 flex-1 space-y-1">
                  <Input
                    value={repo}
                    onChange={(event) => setRepo(event.target.value)}
                    placeholder={GITHUB_PLACEHOLDER}
                    aria-label="GitHub owner/repo"
                    autoComplete="off"
                    spellCheck={false}
                    className="font-mono text-sm"
                  />
                  {importError ? <p className="text-sm text-destructive">{importError}</p> : null}
                </div>
                <Button
                  type="button"
                  variant="outline"
                  disabled={importPending}
                  aria-busy={importPending}
                  onClick={() => onImport(repo)}
                >
                  {importPending ? "Importing…" : "Import from GitHub"}
                </Button>
              </div>
              <Button type="button" variant="link" onClick={onPaste}>
                Paste SKILL.md instead
              </Button>
            </div>
          </div>
        </DashboardPanelContent>
      </DashboardPanel>

      <InstallFlowGuide />
    </div>
  );
}
