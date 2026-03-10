pub const initial_sql = @embedFile("001_initial.sql");
pub const vault_sql = @embedFile("002_vault_schema.sql");
pub const request_correlation_sql = @embedFile("003_request_correlation.sql");
pub const side_effect_ledger_sql = @embedFile("004_side_effect_ledger.sql");
pub const side_effect_outbox_sql = @embedFile("005_side_effect_outbox.sql");
pub const harness_control_plane_sql = @embedFile("006_harness_control_plane.sql");
pub const rls_tenant_isolation_sql = @embedFile("007_rls_tenant_isolation.sql");
