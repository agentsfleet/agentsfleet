"use client";

import { useTheme } from "next-themes";
import { MoonIcon, SunIcon } from "@phosphor-icons/react/ssr";
import { DropdownMenuItem } from "@agentsfleet/design-system";
import { NAV_ICON_SIZE } from "./sidebar-navigation-data";

export function AppearanceMenuItem() {
  const {resolvedTheme, setTheme} = useTheme();
  const light = resolvedTheme === "light";
  const Icon = light ? SunIcon : MoonIcon;
  return <DropdownMenuItem className="gap-lg px-xl" onSelect={event => {event.preventDefault(); setTheme(light ? "dark" : "light");}}
    aria-label={`Appearance: switch to ${light ? "dark" : "light"}`}>
    <Icon size={NAV_ICON_SIZE} className="shrink-0 text-muted-foreground" aria-hidden="true" />
    <span className="flex-1">Appearance</span><span className="text-eyebrow text-text-dim">{light ? "Light" : "Dark"}</span>
  </DropdownMenuItem>;
}
