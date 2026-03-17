// Tests split into focused files. Shared helpers live in proposals_test_support.zig.
//   proposals_trigger_reporting_test.zig   — trigger + reporting tests
//   proposals_lifecycle_autoapply_test.zig — auto-apply reconcile + listOpen
//   proposals_lifecycle_manual_test.zig    — manual approve/reject/veto/expiry
//   proposals_revert_validation_test.zig   — revert + entitlement/validation
//   proposals_idempotent_test.zig          — duplicate/idempotent/boundary tests
// All of the above are registered in the scoring.zig test block.
// This file is retained only as a breadcrumb; do not add new tests here.
