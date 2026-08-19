"use client";

import type { UseFormReturn } from "react-hook-form";
import {
  FormControl,
  FormDescription,
  FormField,
  FormItem,
  FormLabel,
  FormMessage,
  Input,
  Tabs,
  TabsContent,
  TabsList,
  TabsTrigger,
  Textarea,
} from "@agentsfleet/design-system";
import { CircleHelpIcon } from "lucide-react";
import { SOURCE_KIND_GITHUB, SOURCE_KIND_UPLOAD } from "@/lib/types";
import {
  LIBRARY_AUTHORING_DOC_URL,
  SAMPLE_LIBRARY_REPO,
  SAMPLE_LIBRARY_REPO_URL,
} from "@/lib/fleet-library-source";
import { SKILL_FILE_NAME, TRIGGER_FILE_NAME } from "./bundle-files";
import { BundleFolderPicker } from "./BundleFolderPicker";
import type { LibrarySourceValues } from "./library-source-form";

const TAB_GITHUB = "GitHub";
const TAB_UPLOAD = "Upload from computer";

const REPOSITORY_LABEL = "Repository";
const REPOSITORY_PLACEHOLDER = "owner/repo";
const LEARN_MORE = "Learn more";
const LEARN_MORE_CONTEXT = " about authoring fleet libraries (opens in a new tab)";
const NEW_TAB_CONTEXT = " (opens in a new tab)";

const SKILL_PLACEHOLDER = "---\nname: my-fleet\n---";
const TRIGGER_PLACEHOLDER = "---\nname: my-fleet\nx-agentsfleet:\n  triggers:\n---";
const SKILL_DESCRIPTION =
  `The entry is named from this frontmatter, and ${TRIGGER_FILE_NAME} below must name the same fleet.`;
const TRIGGER_DESCRIPTION =
  "Declares the tools, credentials, hosts and approval gates this fleet runs under. Uploading without it yields a fleet that declares none of them.";

const MARKDOWN_ROWS = 8;
const LINK_CLASS = "text-pulse underline-offset-2 hover:underline focus-visible:underline";

type SourceForm = UseFormReturn<LibrarySourceValues>;

/**
 * The repository field, on its own.
 *
 * Exported because the platform catalog's Fetch-update path renders it without
 * the tabs around it: that path re-reads a row's stored source, so the source is
 * already decided and offering a second one would be offering to change it.
 */
export function GitHubSourceField({ form, readOnly = false }: { form: SourceForm; readOnly?: boolean }) {
  return (
    <FormField
      control={form.control}
      name="source_ref"
      render={({ field }) => (
        <FormItem>
          <FormLabel>{REPOSITORY_LABEL}</FormLabel>
          <FormControl>
            <Input
              placeholder={REPOSITORY_PLACEHOLDER}
              autoComplete="off"
              spellCheck={false}
              {...field}
              readOnly={readOnly}
            />
          </FormControl>
          <FormDescription className="space-y-1">
            <span className="block">
              Example:{" "}
              <a href={SAMPLE_LIBRARY_REPO_URL} target="_blank" rel="noopener noreferrer" className={LINK_CLASS}>
                {SAMPLE_LIBRARY_REPO}
                <span className="sr-only">{NEW_TAB_CONTEXT}</span>
              </a>
            </span>
            <a
              href={LIBRARY_AUTHORING_DOC_URL}
              target="_blank"
              rel="noopener noreferrer"
              className={`inline-flex items-center gap-1 ${LINK_CLASS}`}
            >
              <CircleHelpIcon size={13} aria-hidden="true" />
              {LEARN_MORE}
              <span className="sr-only">{LEARN_MORE_CONTEXT}</span>
            </a>
          </FormDescription>
          <FormMessage />
        </FormItem>
      )}
    />
  );
}

function MarkdownBodyField({
  form,
  name,
  label,
  placeholder,
  description,
}: {
  form: SourceForm;
  name: "skill_markdown" | "trigger_markdown";
  label: string;
  placeholder: string;
  description: string;
}) {
  return (
    <FormField
      control={form.control}
      name={name}
      render={({ field }) => (
        <FormItem>
          <FormLabel>{label}</FormLabel>
          <FormControl>
            <Textarea rows={MARKDOWN_ROWS} spellCheck={false} placeholder={placeholder} {...field} />
          </FormControl>
          <FormDescription>{description}</FormDescription>
          <FormMessage />
        </FormItem>
      )}
    />
  );
}

/**
 * Both sources a fleet-library entry can be created from, as one tabbed field set.
 *
 * The caller owns the form and the submit; this owns only what the operator picks
 * the bundle FROM, so the workspace dialog and the platform catalog dialog cannot
 * drift apart on which sources exist or what each one demands.
 */
export function LibrarySourceTabs({
  form,
  onSourceChange,
  disabled = false,
}: {
  form: SourceForm;
  /** Fires on a tab switch, so the caller can drop a server error the other tab earned. */
  onSourceChange?: () => void;
  /**
   * Locks the source while a submit is in flight.
   *
   * Without it the answer to an in-flight request lands against whichever tab the
   * operator has moved to since — so a name collision reported for a repository
   * could be confirmed with Replace while the form already holds an upload, and
   * the retry would send that upload instead.
   */
  disabled?: boolean;
}) {
  const sourceKind = form.watch("source_kind");

  // Switching source clears the other tab's error state, so a half-filled
  // GitHub ref does not keep the upload tab's submit button explaining itself.
  function handleSourceChange(next: string) {
    onSourceChange?.();
    form.clearErrors();
    form.setValue("source_kind", next === SOURCE_KIND_UPLOAD ? SOURCE_KIND_UPLOAD : SOURCE_KIND_GITHUB);
  }

  // A chosen folder fills the boxes rather than going straight to the wire.
  // Frontmatter is unforgiving here — a single apostrophe truncates the parse —
  // so the person uploading gets to read what leaves the browser.
  function handleBundleLoaded(skillMarkdown: string, triggerMarkdown: string) {
    form.setValue("skill_markdown", skillMarkdown);
    form.setValue("trigger_markdown", triggerMarkdown);
    form.clearErrors();
  }

  return (
    <Tabs value={sourceKind} onValueChange={handleSourceChange}>
      <TabsList>
        <TabsTrigger value={SOURCE_KIND_GITHUB} disabled={disabled}>{TAB_GITHUB}</TabsTrigger>
        <TabsTrigger value={SOURCE_KIND_UPLOAD} disabled={disabled}>{TAB_UPLOAD}</TabsTrigger>
      </TabsList>
      <TabsContent value={SOURCE_KIND_GITHUB}>
        <GitHubSourceField form={form} />
      </TabsContent>
      <TabsContent value={SOURCE_KIND_UPLOAD} className="space-y-4">
        <BundleFolderPicker onLoaded={handleBundleLoaded} />
        <MarkdownBodyField
          form={form}
          name="skill_markdown"
          label={SKILL_FILE_NAME}
          placeholder={SKILL_PLACEHOLDER}
          description={SKILL_DESCRIPTION}
        />
        <MarkdownBodyField
          form={form}
          name="trigger_markdown"
          label={TRIGGER_FILE_NAME}
          placeholder={TRIGGER_PLACEHOLDER}
          description={TRIGGER_DESCRIPTION}
        />
      </TabsContent>
    </Tabs>
  );
}
