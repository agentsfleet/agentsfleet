import type { LibraryModel } from "@/lib/api/model-library-types";

/** Display names keyed by the model identifier a ledger, fleet or setup records. */
export type ModelLabels = Readonly<Record<string, string>>;

export const NO_MODEL_LABELS: ModelLabels = Object.freeze({});

/**
 * One label per identifier, and only when it is unambiguous: every provider
 * listing the identifier names it the same, and that name is not blank.
 * Two providers that disagree leave the identifier unlabelled.
 */
export function modelLabelsOf(models: readonly Pick<LibraryModel, "id" | "display_name">[]): ModelLabels {
  const names = new Map<string, string | null>();
  for (const model of models) {
    const seen = names.get(model.id);
    const agrees = seen === undefined || seen === model.display_name;
    names.set(model.id, agrees && model.display_name.trim() !== "" ? model.display_name : null);
  }
  return Object.fromEntries([...names].filter((entry): entry is [string, string] => entry[1] !== null));
}

/** The label, or the recorded identifier when none is known. Own keys only, so `constructor` stays a model. */
export function modelLabel(labels: ModelLabels, model: string): string {
  const label = Object.hasOwn(labels, model) ? labels[model] : undefined;
  return label ?? model;
}
