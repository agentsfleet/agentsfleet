import { runDevDashboard } from "./dev-dashboard.ts";

export {runDevDashboard};
if (import.meta.main) await runDevDashboard(Bun.argv[2]);
