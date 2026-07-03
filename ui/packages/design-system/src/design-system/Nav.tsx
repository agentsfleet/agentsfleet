import { type ComponentProps } from "react";
import { cn } from "../utils";

/*
 * Nav — navigation landmark primitive. Renders <nav> with a required
 * accessible name so every navigation landmark on a page is distinguishable
 * (Primary, Breadcrumbs, ...). Owns only the semantics; layout classes come
 * from the caller. RSC-safe.
 */
export type NavProps = ComponentProps<"nav"> & { "aria-label": string };

export function Nav({ className, ref, ...props }: NavProps) {
  return <nav ref={ref} className={cn(className)} {...props} />;
}

export default Nav;
