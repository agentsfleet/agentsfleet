SELECT name, value
FROM identity.user_preferences
WHERE user_id = $1 AND workspace_id = $2
ORDER BY name
