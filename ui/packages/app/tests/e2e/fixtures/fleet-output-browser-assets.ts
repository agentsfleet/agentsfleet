// Playwright's Node workers compile the shipped styles for the component fixture.
import { readFile } from "node:fs/promises";
import path from "node:path";
import postcss from "postcss";
import tailwind from "@tailwindcss/postcss";

/** Embed font assets so relative fixture URLs cannot silently use fallback fonts. */
export async function applicationCss(appRoot: string): Promise<string> {
  const source = path.join(appRoot, "app/globals.css");
  const plugin = tailwind({base: appRoot});
  const processor = "postcssPlugin" in plugin ? postcss(plugin) : plugin;
  const result = await processor.process(await readFile(source, "utf8"), {from: source});
  let css = result.css;
  for (const [match, relative] of css.matchAll(/url\(['"]?([^)'"\s]+\.woff2?)['"]?\)/g)) {
    if (!relative) throw new Error("Font URL has no path");
    const font = await readFile(path.resolve(path.dirname(source), relative));
    const mime = relative.endsWith(".woff2") ? "font/woff2" : "font/woff";
    css = css.replaceAll(match, `url(data:${mime};base64,${font.toString("base64")})`);
  }
  return css;
}
