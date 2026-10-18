SELECT 1 FROM identity.workspaces
WHERE id = $1::uuid
FOR NO KEY UPDATE
