// How a model reads in a table: the name its provider markets, its context
// window and its per-million rates. The admin Model library and the workspace
// Models table both render through these, so one model cannot read two ways.
//
// The name is derived from the provider's id because the catalogue has no
// display field; the id itself stays on the row (hover, sort, accessible
// labels), since it is what a fleet config and a provider dashboard need.

import { nanosToUsdPerMtok } from "@/lib/api/admin-model-library-types";
import type { LibraryModel } from "@/lib/api/model-library-types";

export type RateFields = Pick<
  LibraryModel,
  "input_nanos_per_mtok" | "cached_input_nanos_per_mtok" | "output_nanos_per_mtok"
>;

/** What a cell shows when the value does not exist for the row. */
export const EMPTY_VALUE = "—";

export const CONTEXT_HEADER = "Context";
export const RATES_HEADER = "Rates $/1M (in / cached / out)";

const RATE_SEPARATOR = " / ";
const RATE_DECIMALS = 2;

/** Ids from a gateway or host carry a path; the model is its last segment. */
const PATH_SEPARATOR = "/";
const WORD_SEPARATOR = "-";
/** Anthropic's ids lead with the family's vendor word, which the name omits. */
const CLAUDE_PREFIX = "claude-";
/** Fireworks writes a version's point as `p`: `glm-5p3` is GLM 5.3. */
const FIREWORKS_POINT = /(\d)p(\d)/g;
/** A digit run short enough to be a version part, not a date. */
const VERSION_PART = /^\d{1,2}$/;
/** A version token: `6`, `6.1`, `v4.1`, `k3`. */
const VERSION_TOKEN = /^[a-z]?\d+(\.\d+)*$/i;
/** A parameter count: `120b`, `397B`. */
const SIZE_TOKEN = /^\d+(\.\d+)?[bmk]$/i;
/** OpenAI's version after `gpt`: `5`, `6.1`, `4o`. */
const GPT_VERSION = /^\d+(\.\d+)*[a-z]?$/i;
/** A release date an id ends with, `-YYYY-MM-DD`, kept whole as one word. */
const TRAILING_DATE = /-(\d{4}-\d{2}-\d{2})$/;
/** An id that is one plain word, which reads title-cased: `sonar` → `Sonar`. */
const PLAIN_WORD = /^[a-z]+$/i;

/** Words written in capitals, wherever they appear in an id. */
const ACRONYMS = new Set(["gpt", "glm", "oss", "ocr", "tee"]);
/** Brands whose casing a first-letter capital would get wrong. */
const BRANDS: Record<string, string> = { deepseek: "DeepSeek", minimax: "MiniMax" };
const GPT = "gpt";

function casedWord(word: string): string {
  const lower = word.toLowerCase();
  if (ACRONYMS.has(lower)) return lower.toUpperCase();
  const brand = BRANDS[lower];
  if (brand) return brand;
  if (SIZE_TOKEN.test(word) || VERSION_TOKEN.test(word)) return word.toUpperCase();
  return word.charAt(0).toUpperCase() + word.slice(1);
}

/** `["opus", "4", "8"]` → `["opus", "4.8"]`: Anthropic spells a point as a hyphen. */
function joinVersionParts(words: string[]): string[] {
  return words.reduce<string[]>((joined, word) => {
    const previous = joined[joined.length - 1];
    if (previous !== undefined && VERSION_PART.test(word) && /^\d{1,2}(\.\d{1,2})*$/.test(previous)) {
      joined[joined.length - 1] = `${previous}.${word}`;
    } else {
      joined.push(word);
    }
    return joined;
  }, []);
}

/**
 * The name a provider markets a model by, from its id.
 *
 * `claude-fable-5` → `Fable 5`, `claude-opus-5-5` → `Opus 5.5`,
 * `accounts/fireworks/models/glm-5p3-flash` → `GLM 5.3 Flash`,
 * `gpt-6.1-sol` → `GPT-6.1 Sol`, `gpt-4o-2024-08-06` → `GPT-4o 2024-08-06`,
 * `sonar` → `Sonar`. An id with no word structure (`syn:large:text`, `o3`)
 * comes back as its last path segment.
 */
export function modelLabel(modelId: string): string {
  const segment = modelId.slice(modelId.lastIndexOf(PATH_SEPARATOR) + 1);
  if (segment.includes(":")) return segment;
  if (!segment.includes(WORD_SEPARATOR)) return PLAIN_WORD.test(segment) ? casedWord(segment) : segment;
  const dated = TRAILING_DATE.exec(segment);
  if (!dated) return wordsLabel(segment);
  return `${wordsLabel(segment.slice(0, dated.index))} ${dated[1]}`;
}

/** The label of an id's hyphenated words, its release date already set aside. */
function wordsLabel(segment: string): string {
  const isClaude = segment.toLowerCase().startsWith(CLAUDE_PREFIX);
  const bare = (isClaude ? segment.slice(CLAUDE_PREFIX.length) : segment).replace(FIREWORKS_POINT, "$1.$2");
  const split = bare.split(WORD_SEPARATOR).filter(Boolean);
  const words = isClaude ? joinVersionParts(split) : split;
  const cased = words.map(casedWord);
  // OpenAI's names keep the hyphen between GPT and its version: "GPT-6.1 Sol".
  if (words[0]?.toLowerCase() === GPT && words[1] !== undefined && GPT_VERSION.test(words[1])) {
    return [`${cased[0]}${WORD_SEPARATOR}${cased[1]}`, ...cased.slice(2)].join(" ");
  }
  return cased.join(" ");
}

/** What a Model column sorts by: the name it shows, then the id, so two ids
 * that read alike keep a fixed order. */
export function modelSortKey(modelId: string): string {
  return `${modelLabel(modelId)} ${modelId}`;
}

/** A token count grouped in threes with commas, by a fixed rule so the server and the browser agree. */
export function formatContextTokens(tokens: number | undefined): string {
  if (tokens == null) return EMPTY_VALUE;
  return String(Math.trunc(tokens)).replace(/\B(?=(\d{3})+(?!\d))/g, ",");
}

/** Rates per million tokens as `in / cached / out`; the header carries the unit. */
export function formatRatesPerMtok(rates: RateFields | null): string {
  if (!rates) return EMPTY_VALUE;
  const usd = (nanos: number) => nanosToUsdPerMtok(nanos).toFixed(RATE_DECIMALS);
  return [
    usd(rates.input_nanos_per_mtok),
    usd(rates.cached_input_nanos_per_mtok),
    usd(rates.output_nanos_per_mtok),
  ].join(RATE_SEPARATOR);
}
