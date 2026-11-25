import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { TooltipProvider } from "@agentsfleet/design-system";
import { FleetDetailSchema } from "@/lib/api/fleet-detail";
import { FLEET_DETAIL, WORKSPACE } from "@/tests/helpers/fleet-fixtures";
import { FleetHeader } from "./FleetHeader";

const memory = vi.hoisted(() => ({canGrant: [] as boolean[]}));
vi.mock("./MemoryMenu", () => ({MemoryMenu: ({canGrant}: {canGrant: boolean}) => {memory.canGrant.push(canGrant); return null;}}));
vi.mock("./FleetActions", () => ({FleetActions: ({children}: {children: React.ReactNode}) => <div>{children}</div>}));
vi.mock("next/navigation", () => ({useRouter: () => ({refresh: vi.fn()})}));
afterEach(() => {cleanup(); memory.canGrant = [];});

it.each([[undefined, false], [true, true]] as const)("offers memory sharing only with grant scope (%s), and no setup action", (canGrant, offered) => {
  render(<TooltipProvider><FleetHeader workspaceId={WORKSPACE} fleet={FleetDetailSchema.parse(FLEET_DETAIL)} {...(canGrant === undefined ? {} : {canGrant})} /></TooltipProvider>);
  expect(screen.getByRole("heading", {name: FLEET_DETAIL.name})).toBeTruthy();
  expect(memory.canGrant).toEqual([offered]);
  expect(screen.queryByText(/setup/i)).toBeNull();
});
