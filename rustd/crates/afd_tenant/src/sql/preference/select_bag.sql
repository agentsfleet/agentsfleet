SELECT name, value
FROM identity.user_preferences
WHERE user_id = $1::uuid AND workspace_id = $2::uuid
ORDER BY name
