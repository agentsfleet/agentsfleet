"use client";

import { useFieldArray, type Control } from "react-hook-form";
import { PlusIcon, Trash2Icon } from "lucide-react";
import {
  Button,
  FormControl,
  FormDescription,
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
  BIND_MODES,
  BIND_MODE_DESCRIPTIONS,
  BIND_MODE_LABELS,
  BIND_ROW_DEFAULT,
  MAX_EXTRA_BINDS,
} from "./policy-binds";
import type { PolicyFormValues } from "./PolicyFields";

// The repeatable bind list. Rows rather than a comma-separated string because
// `mode` is a security boundary: read-write lets tenant agent code write
// through to the host on every lease, and a mode hidden inside free text is a
// mode nobody reviewed. The select makes the widening an explicit choice.

export const BINDS_ASSIGNMENT_LABEL = "Extra sandbox binds (optional)";
export const BINDS_ASSIGNMENT_DESCRIPTION =
  "Host paths mounted into every lease's sandbox, in addition to the daemon-owned baseline. An assignment can only add paths — it can never drop or re-mode one the sandbox depends on.";
export const ADD_BIND_LABEL = "Add bind";
export const REMOVE_BIND_LABEL = "Remove bind";
export const BIND_PATH_PLACEHOLDER = "/srv/models";
export const BIND_NOTE_PLACEHOLDER = "why this host needs it (optional)";
export const NO_BINDS_DESCRIPTION = "No extra binds — the sandbox gets the daemon-owned baseline only.";

export function PolicyBindsField({ control }: { control: Control<PolicyFormValues> }) {
  const { fields, append, remove } = useFieldArray({ control, name: "extra_binds" });
  const atCap = fields.length >= MAX_EXTRA_BINDS;

  // A group heading, not a FormLabel: FormLabel resolves its htmlFor and error
  // state from a single FormField's context, and this group owns a list rather
  // than one control. Per-row labels below sit inside their own FormField.
  return (
    <div className="flex flex-col gap-md">
      <Label>{BINDS_ASSIGNMENT_LABEL}</Label>
      <p className="text-body-sm text-muted-foreground">{BINDS_ASSIGNMENT_DESCRIPTION}</p>

      {fields.length === 0 ? <p className="text-body-sm text-muted-foreground">{NO_BINDS_DESCRIPTION}</p> : null}

      <div className="flex flex-col gap-md">
        {fields.map((row, index) => (
          <div key={row.id} className="flex flex-col gap-sm sm:flex-row sm:items-start">
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
                  <Select value={field.value} onValueChange={field.onChange}>
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
                  <FormDescription>{BIND_MODE_DESCRIPTIONS[field.value]}</FormDescription>
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
    </div>
  );
}
