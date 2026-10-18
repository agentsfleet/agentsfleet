"use client";

import type { CataloguePrice } from "@/lib/api/catalogue-types";
import { z } from "zod";
import { Accordion, AccordionContent, AccordionItem, AccordionTrigger, Button,
  DescriptionDetails, DescriptionList, DescriptionTerm, Time } from "@agentsfleet/design-system";
import { formatContextTokens, formatRatePerMtok, PRICE_UNKNOWN } from "@/lib/models/display";

const RATES = [
  ["input_nanos_per_mtok", "Input"], ["cached_input_nanos_per_mtok", "Cached input"],
  ["cache_write_nanos_per_mtok", "Cache write"], ["cache_write_5m_nanos_per_mtok", "Cache write, 5-minute"],
  ["cache_write_1h_nanos_per_mtok", "Cache write, 1-hour"], ["output_nanos_per_mtok", "Output"],
] as const;
const EVIDENCE_URL = z.url({ protocol: /^https?$/ });

export function CatalogueEvidence({ source, verifiedAt }: { source: string; verifiedAt: string }) {
  const url = EVIDENCE_URL.safeParse(source);
  return <div className="flex flex-wrap items-center gap-sm text-body-sm text-muted-foreground">
    {url.success ? <Button asChild variant="ghost" size="sm"><a href={url.data} target="_blank" rel="noreferrer">Source</a></Button> : <span>{source}</span>}
    <span>Verified <Time value={verifiedAt} format="datetime" label={verifiedAt} /></span>
  </div>;
}

export function ModelPriceSchedule({ prices }: { prices: CataloguePrice[] }) {
  if (prices.length === 0) return <span>{PRICE_UNKNOWN}</span>;
  return <Accordion type="single" collapsible>
    <AccordionItem value="prices">
      <AccordionTrigger>Price schedule ({prices.length})</AccordionTrigger>
      <AccordionContent>
        <div className="space-y-lg text-left">
          {prices.map((price) => <PriceDetails key={`${price.prompt_tokens_above}:${price.effective_from}`} price={price} />)}
        </div>
      </AccordionContent>
    </AccordionItem>
  </Accordion>;
}

function PriceDetails({ price }: { price: CataloguePrice }) {
  return <div className="space-y-sm">
    <DescriptionList>
      <div><DescriptionTerm>Prompt tier</DescriptionTerm><DescriptionDetails>
        {price.prompt_tokens_above === 0 ? "Base tier" : `Whole prompt above ${formatContextTokens(price.prompt_tokens_above)} tokens`}
      </DescriptionDetails></div>
      <div><DescriptionTerm>Effective from</DescriptionTerm><DescriptionDetails><Time value={price.effective_from} format="datetime" label={price.effective_from} /></DescriptionDetails></div>
      <div><DescriptionTerm>Effective until (exclusive)</DescriptionTerm><DescriptionDetails>
        {price.effective_until === null ? "Open-ended" : <Time value={price.effective_until} format="datetime" label={price.effective_until} />}
      </DescriptionDetails></div>
      {RATES.map(([field, label]) => <div key={field}><DescriptionTerm>{label} ($/1M)</DescriptionTerm>
        <DescriptionDetails mono>{formatRatePerMtok(price[field])}</DescriptionDetails></div>)}
      <div><DescriptionTerm>Conditions</DescriptionTerm><DescriptionDetails>{price.conditions || "None recorded"}</DescriptionDetails></div>
    </DescriptionList>
    <CatalogueEvidence source={price.source_url} verifiedAt={price.verified_at} />
  </div>;
}
