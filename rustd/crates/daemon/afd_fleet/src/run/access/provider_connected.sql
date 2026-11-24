-- Whether a workspace has connected a provider's account: the routing row its connect flow
-- wrote. Any account answers; which one a delivery or a mint names is checked where it is used.
--
-- $1 workspace · $2 provider
SELECT EXISTS (
    SELECT 1 FROM connector.installs WHERE workspace_id = $1 AND provider = $2
)
