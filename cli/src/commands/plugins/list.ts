import type { PluginPageOptions } from "../plugins-read.ts";
import { pluginEffectFromOptions } from "./plugin.ts";

export const pluginListEffectFromOptions = (options: PluginPageOptions) =>
  pluginEffectFromOptions(options);
