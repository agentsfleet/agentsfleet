"use client";

import { Alert, Button, Label, Switch } from "@agentsfleet/design-system";
import { CATALOGUE_STATUS } from "./catalogue-status";
import { useModelCatalogue } from "./ModelCatalogueProvider";

type Props = { id: string; advanced: boolean; onAdvancedChange: (value: boolean) => void };

export function CatalogueBrowseControls({ id, advanced, onAdvancedChange }: Props) {
  const { preloadAdvanced } = useModelCatalogue();
  const changeAdvanced = (checked: boolean) => {
    onAdvancedChange(checked);
    if (checked) preloadAdvanced();
  };
  return <div className="space-y-sm">
    <div className="flex items-center gap-sm">
      <Switch id={id} checked={advanced} onCheckedChange={changeAdvanced} />
      <Label htmlFor={id}>Show advanced models</Label>
    </div>
    <CatalogueReadControls includeAdvanced={advanced} />
  </div>;
}

/** Paging and recovery also serve read-only model evidence, without picker filters. */
export function CatalogueReadControls({includeAdvanced, label = "Load more models"}: {includeAdvanced: boolean; label?: string}) {
  const {status, error, nextCursor, advancedNextCursor, loadMore, retry} = useModelCatalogue();
  const more = nextCursor !== null || (includeAdvanced && advancedNextCursor !== null);
  return <div className="space-y-sm">
    {status === CATALOGUE_STATUS.error && error ? <Alert variant="warning">
      {error}
      <Button type="button" variant="ghost" onClick={retry}>Try again</Button>
    </Alert> : null}
    {more ? <Button type="button" variant="secondary" disabled={status === CATALOGUE_STATUS.loading}
      onClick={() => loadMore(includeAdvanced)}>{label}</Button> : null}
  </div>;
}
