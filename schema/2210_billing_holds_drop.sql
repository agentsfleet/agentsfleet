-- Billing sets no money aside (M240_002): a funds check reads the balance less
-- the payer's unsettled usage lines, and what parallel work spends past it is
-- a debt the next grant repays. Holds had no reader left, so the table goes.
-- Owner's approval for the drop: "Remove holds in this PR" (Indy, Nov 05, 2026).

DROP TABLE billing.holds;
