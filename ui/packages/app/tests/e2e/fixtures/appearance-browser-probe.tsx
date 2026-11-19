import { createRoot } from "react-dom/client";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuTrigger,
} from "@agentsfleet/design-system";
import { AppearanceMenuItem } from "@/components/layout/AppearanceMenuItem";
import { ThemeProvider } from "@/components/layout/ThemeProvider";

const root = document.querySelector("#appearance-probe");
if (!root) throw new Error("Missing appearance probe");

createRoot(root).render(
  <ThemeProvider>
    <main className="app-dashboard-canvas" aria-label="Dashboard">
      <DropdownMenu modal={false}>
        <DropdownMenuTrigger>Workspace and account menu</DropdownMenuTrigger>
        <DropdownMenuContent>
          <AppearanceMenuItem />
        </DropdownMenuContent>
      </DropdownMenu>
    </main>
  </ThemeProvider>,
);
