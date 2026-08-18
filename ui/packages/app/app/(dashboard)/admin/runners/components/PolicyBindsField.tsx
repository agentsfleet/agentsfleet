"use client";

import { useState } from "react";
import { useFieldArray, type Control } from "react-hook-form";
import { CircleHelpIcon, PlusIcon, Trash2Icon } from "lucide-react";
import {
  Accordion,
  AccordionContent,
  AccordionItem,
  AccordionTrigger,
  Button,
  FormControl,
  FormField,
  FormItem,
  FormMessage,
  Input,
  Label,
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@agentsfleet/design-system";
import {
  BASELINE_RO_PATHS,
  BIND_MODES,
  BIND_MODE_LABELS,
  BIND_ROW_DEFAULT,
  MAX_EXTRA_BINDS,
} from "./policy-binds";
import type { PolicyFormValues } from "./PolicyFields";

// The repeatable bind list. Rows rather than a comma-separated string because
// `mode` is a security boundary: read-write lets tenant agent code write
// through to the host on every lease, and a mode hidden inside free text is a
// mode nobody reviewed. The select makes the widening an explicit choice.

export const BINDS_ASSIGNMENT_LABEL = "Sandbox mounts (optional)";
export const BINDS_ASSIGNMENT_DESCRIPTION = "Paths mounted into every leased sandbox.";
export const ADD_BIND_LABEL = "Add mount";
export const REMOVE_BIND_LABEL = "Remove mount";
export const BIND_PATH_PLACEHOLDER = "/srv/models";
export const BIND_NOTE_PLACEHOLDER = "why this host needs it (optional)";
export const DEFAULT_MOUNTS_LABEL = "Default mounts";
export const DEFAULT_MOUNTS_NOTE = "Read-only in every leased sandbox; an assignment can only add to them.";

/// The disclosure's single item value; also what the trigger toggles against.
const BINDS_SECTION_VALUE = "extra-binds";

export function PolicyBindsField({
  control,
}: {
  control: Control<PolicyFormValues>;
}) {
  const { fields, append, remove } = useFieldArray({
    control,
    name: "extra_binds",
  });
  const atCap = fields.length >= MAX_EXTRA_BINDS;
  // Open when the runner already carries binds, collapsed when it does not.
  // This is the one unbounded surface in the assignment form — up to
  // MAX_EXTRA_BINDS rows of three inputs — so leaving it expanded made the
  // dialog grow to the full viewport for the common edit, which changes
  // nothing here. An operator who has binds sees them; one who does not gets a
  // one-line row instead of an empty editor.
  const [open, setOpen] = useState(fields.length > 0);

  // A group heading, not a FormLabel: FormLabel resolves its htmlFor and error
  // state from a single FormField's context, and this group owns a list rather
  // than one control. Per-row labels below sit inside their own FormField.
  return (
    <Accordion
      type="single"
      collapsible
      value={open ? BINDS_SECTION_VALUE : ""}
      onValueChange={(value) => setOpen(value === BINDS_SECTION_VALUE)}
    >
      <AccordionItem value={BINDS_SECTION_VALUE} className="border-0">
        {/* The tooltip's affordance is a real button and a SIBLING of the
            accordion trigger — a focusable element nested inside the trigger
            button would be invalid markup and unreachable by keyboard. */}
        <div className="flex items-center gap-sm">
          <AccordionTrigger className="py-xs hover:no-underline">
            <span className="flex flex-col items-start gap-2xs text-left">
              <Label asChild>
                <span>{BINDS_ASSIGNMENT_LABEL}</span>
              </Label>
              {fields.length > 0 ? (
                <span className="text-body-sm text-muted-foreground">{`${fields.length} assigned`}</span>
              ) : null}
            </span>
          </AccordionTrigger>
          {/* The daemon-owned baseline lives behind a hover, not in the body:
              an operator deciding what to add can see what is already mounted
              without the list crowding the form. */}
          <TooltipProvider>
            <Tooltip>
              <TooltipTrigger asChild>
                <button
                  type="button"
                  aria-label={DEFAULT_MOUNTS_LABEL}
                  className="text-muted-foreground"
                >
                  <CircleHelpIcon size={14} aria-hidden="true" />
                </button>
              </TooltipTrigger>
              <TooltipContent>
                <span className="flex flex-col gap-2xs text-left">
                  <span className="text-label uppercase">{DEFAULT_MOUNTS_LABEL}</span>
                  <span className="text-body-sm text-muted-foreground">{DEFAULT_MOUNTS_NOTE}</span>
                  <span className="flex flex-col font-mono text-body-sm">
                    {BASELINE_RO_PATHS.map((path) => (
                      <span key={path}>{path}</span>
                    ))}
                  </span>
                </span>
              </TooltipContent>
            </Tooltip>
          </TooltipProvider>
        </div>
        <AccordionContent className="flex flex-col gap-md">
          <p className="text-body-sm text-muted-foreground">
            {BINDS_ASSIGNMENT_DESCRIPTION}
          </p>

          <div className="flex flex-col gap-md">
            {fields.map((row, index) => (
              <div
                key={row.id}
                className="flex flex-col gap-sm sm:flex-row sm:items-start"
              >
                <FormField
                  control={control}
                  name={`extra_binds.${index}.path`}
                  render={({ field }) => (
                    <FormItem className="grow">
                      <FormControl>
                        <Input
                          placeholder={BIND_PATH_PLACEHOLDER}
                          autoComplete="off"
                          aria-label={`Mount path ${index + 1}`}
                          {...field}
                        />
                      </FormControl>
                      <FormMessage />
                    </FormItem>
                  )}
                />
                <FormField
                  control={control}
                  name={`extra_binds.${index}.mode`}
                  render={({ field }) => (
                    <FormItem>
                      <Select
                        value={field.value}
                        onValueChange={field.onChange}
                      >
                        <FormControl>
                          <SelectTrigger aria-label={`Mount mode ${index + 1}`}>
                            <SelectValue />
                          </SelectTrigger>
                        </FormControl>
                        <SelectContent>
                          {BIND_MODES.map((m) => (
                            <SelectItem key={m} value={m}>
                              {BIND_MODE_LABELS[m]}
                            </SelectItem>
                          ))}
                        </SelectContent>
                      </Select>
                      <FormMessage />
                    </FormItem>
                  )}
                />
                <FormField
                  control={control}
                  name={`extra_binds.${index}.note`}
                  render={({ field }) => (
                    <FormItem className="grow">
                      <FormControl>
                        <Input
                          placeholder={BIND_NOTE_PLACEHOLDER}
                          autoComplete="off"
                          aria-label={`Mount note ${index + 1}`}
                          {...field}
                        />
                      </FormControl>
                      <FormMessage />
                    </FormItem>
                  )}
                />
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  aria-label={`${REMOVE_BIND_LABEL} ${index + 1}`}
                  onClick={() => remove(index)}
                >
                  <Trash2Icon size={14} aria-hidden="true" />
                </Button>
              </div>
            ))}
          </div>

          <Button
            type="button"
            variant="outline"
            size="sm"
            disabled={atCap}
            onClick={() => append({ ...BIND_ROW_DEFAULT })}
          >
            <PlusIcon size={14} aria-hidden="true" /> {ADD_BIND_LABEL}
          </Button>
          {atCap ? (
            <p className="text-body-sm text-muted-foreground">{`At most ${MAX_EXTRA_BINDS} mounts per runner.`}</p>
          ) : null}
        </AccordionContent>
      </AccordionItem>
    </Accordion>
  );
}
