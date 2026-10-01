import { ACCOUNT_ROLE } from "@/lib/api/workspaces";

// A listed workspace's account and the caller's role in it when the caller owns
// it: every workspace in a solo account's list has this shape. Spread it into a
// fixture beside the workspace's id and name.
export const OWN_ACCOUNT = {
  account: { tenant_id: "tenant_own", owner_name: "You" },
  role: ACCOUNT_ROLE.owner,
};
