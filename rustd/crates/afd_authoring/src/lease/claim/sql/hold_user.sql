SELECT id::text FROM identity.users WHERE id = $1::uuid FOR SHARE NOWAIT
