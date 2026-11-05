SELECT id::text FROM identity.workspaces WHERE id = $1::uuid FOR KEY SHARE
