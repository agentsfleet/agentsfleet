-- Vault read-protection for ops readonly principals.

REVOKE ALL ON vault.secrets FROM ops_readonly_human, ops_readonly_agent;
