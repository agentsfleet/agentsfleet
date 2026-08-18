"use client";

import { useState } from "react";
import { useFieldArray, type Control } from "react-hook-form";
import { PlusIcon, Trash2Icon } from "lucide-react";
import {
  Accordion,
  AccordionContent,
  AccordionItem,
  AccordionTrigger,
  Badge,
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

export const BINDS_ASSIGNMENT_LABEL = "Sandbox binds";
export const BINDS_ASSIGNMENT_DESCRIPTION =
  "Paths mounted into every lease's sandbox — additions only; the baseline can't be dropped or re-moded.";
export const ADD_BIND_LABEL = "Add bind";
export const REMOVE_BIND_LABEL = "Remove bind";
export const BIND_PATH_PLACEHOLDER = "/srv/models";
export const BIND_NOTE_PLACEHOLDER = "why this host needs it (optional)";
export const NO_BINDS_DESCRIPTION = "No extra binds — baseline only.";
export const BASELINE_HEADING = "Baseline (always bound)";

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
        <AccordionTrigger className="py-xs hover:no-underline">
          <span className="flex flex-col items-start gap-2xs text-left">
            <Label asChild>
              <span>{BINDS_ASSIGNMENT_LABEL}</span>
            </Label>
            <span className="text-body-sm text-muted-foreground">
              {fields.length === 0
                ? NO_BINDS_DESCRIPTION
                : `${fields.length} assigned`}
            </span>
          </span>
        </AccordionTrigger>
        <AccordionContent className="flex flex-col gap-md">
          <p className="text-body-sm text-muted-foreground">
            {BINDS_ASSIGNMENT_DESCRIPTION}
          </p>

          {/* The daemon-owned baseline, shown disabled: an operator deciding
              what to add must see what is already bound — and that none of it
              is editable from here. */}
          <div className="flex flex-col gap-2xs">
            <span className="text-label uppercase text-text-subtle">{BASELINE_HEADING}</span>
            <ul className="flex flex-col gap-2xs" aria-label={BASELINE_HEADING}>
              {BASELINE_RO_PATHS.map((path) => (
                <li
                  key={path}
                  className="flex items-baseline gap-sm font-mono text-body-sm text-muted-foreground"
                >
                  <span>{path}</span>
                  <Badge variant="default">{BIND_MODE_LABELS.read_only}</Badge>
                </li>
              ))}
            </ul>
          </div>

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
                          aria-label={`Bind path ${index + 1}`}
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
                          <SelectTrigger aria-label={`Bind mode ${index + 1}`}>
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
                          aria-label={`Bind note ${index + 1}`}
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
            <p className="text-body-sm text-muted-foreground">{`At most ${MAX_EXTRA_BINDS} binds per runner.`}</p>
          ) : null}
        </AccordionContent>
      </AccordionItem>
    </Accordion>
  );
}
