"use client";

import { useState } from "react";
import { Button, EmptyState, Input } from "@agentsfleet/design-system";
import type { FleetTemplate } from "@/lib/types";
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

// Gallery-first source selector (approved Variant D):
// curated templates lead as the primary choice; importing a public GitHub repo
// and pasting a SKILL.md sit in a secondary "or start from source" strip.
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
    <div className="space-y-6">
      <div className="space-y-1">
        <h2 className="font-mono text-heading text-foreground">Start from a template</h2>
        <p className="text-sm text-muted-foreground">
          Curated Fleets, pinned and ready. Connect what they need, then create.
        </p>
      </div>

      {templates.length > 0 ? (
        <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
          {templates.map((template) => (
            <TemplateCard
              key={template.id}
              template={template}
              action={
                <Button type="button" size="sm" onClick={() => onUseTemplate(template)}>
                  Use template
                </Button>
              }
            />
          ))}
        </div>
      ) : (
        <EmptyState
          title="No templates available yet"
          description="Import from a public GitHub repo or paste a SKILL.md below."
        />
      )}

      <div className="space-y-3 border-t border-border pt-5">
        <p className="font-mono text-xs uppercase tracking-label text-muted-foreground">
          or start from source
        </p>
        <div className="flex flex-wrap items-start gap-2">
          <div className="flex-1 space-y-1">
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
            size="sm"
            disabled={importPending}
            aria-busy={importPending}
            onClick={() => onImport(repo)}
          >
            {importPending ? "Importing…" : "Import from GitHub"}
          </Button>
        </div>
        <Button type="button" variant="ghost" size="sm" onClick={onPaste}>
          Paste SKILL.md instead
        </Button>
      </div>
    </div>
  );
}
