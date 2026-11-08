-- Only a subject-checked OAuth callback may establish a Slack person's binding.
-- Runtime authorization also requires the current install and workspace membership.
CREATE TABLE connector.person_links (
    workspace_id UUID NOT NULL REFERENCES identity.workspaces(id) ON DELETE CASCADE,
    install_id UUID NOT NULL REFERENCES connector.installs(id) ON DELETE CASCADE,
    team_id TEXT NOT NULL CHECK (team_id ~ '^T[A-Z0-9]{1,63}$'),
    external_user_id TEXT NOT NULL CHECK (external_user_id ~ '^[UW][A-Z0-9]{1,63}$'),
    user_id UUID NOT NULL REFERENCES identity.users(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ(3) NOT NULL,
    updated_at TIMESTAMPTZ(3) NOT NULL,
    PRIMARY KEY (workspace_id, team_id, external_user_id)
);

CREATE INDEX idx_person_links_user_id ON connector.person_links (user_id);

CREATE INDEX idx_person_links_install_id ON connector.person_links (install_id);

GRANT SELECT, INSERT, UPDATE, DELETE ON connector.person_links TO api_runtime;

CREATE FUNCTION connector.slack_person_allowed(workspace UUID, actor TEXT, provider TEXT, actor_pattern TEXT, separator TEXT)
RETURNS BOOLEAN LANGUAGE SQL STABLE SET search_path = pg_catalog AS $$
    SELECT $2 ~ $4 AND EXISTS (
        SELECT 1 FROM connector.person_links l
        JOIN connector.installs i ON i.id = l.install_id AND i.provider = $3 AND i.external_account_id = l.team_id
            AND i.workspace_id = l.workspace_id
        JOIN identity.workspaces w ON w.id = l.workspace_id
        JOIN identity.users u ON u.id = l.user_id
        WHERE l.workspace_id = $1
          AND l.team_id = split_part($2, $5, 2)
          AND l.external_user_id = split_part($2, $5, 3)
          AND (u.tenant_id = w.tenant_id OR EXISTS (
              SELECT 1 FROM identity.memberships m WHERE m.tenant_id = w.tenant_id AND m.user_id = u.id
          ))
    )
$$;

GRANT EXECUTE ON FUNCTION connector.slack_person_allowed(UUID, TEXT, TEXT, TEXT, TEXT) TO api_runtime;

-- This is consulted by the existing lease proof, including after admission cleanup.
-- A quoted message never enters these identities; all rows came from native binding.
CREATE FUNCTION fleet.task_person_allowed(task UUID, source_producer TEXT, provider TEXT, actor_pattern TEXT, separator TEXT)
RETURNS BOOLEAN LANGUAGE SQL STABLE SET search_path = pg_catalog AS $$
    WITH target AS MATERIALIZED (
        SELECT id, workspace_id, fleet_id, originating_task_item_id, covered_input_sequence
        FROM fleet.tasks WHERE id = $1
    ), senders AS MATERIALIZED (
        -- Repeated messages by one person require one current membership check.
        -- UNION also deduplicates the origin against incorporated input sources.
        SELECT p.source_actor FROM target t
        JOIN fleet.task_items origin ON origin.id = t.originating_task_item_id
        JOIN fleet.task_interpretations p ON p.admission_id = origin.admission_id
            AND p.fleet_id = t.fleet_id AND p.workspace_id = t.workspace_id
        WHERE p.source_producer = $2
        UNION
        SELECT applied.source_actor FROM target t
        JOIN fleet.task_inputs n ON n.task_id = t.id AND n.sequence <= t.covered_input_sequence
        JOIN fleet.task_items i ON i.id = n.task_item_id
        JOIN fleet.task_interpretations applied ON applied.admission_id = i.admission_id
            AND applied.fleet_id = t.fleet_id AND applied.workspace_id = t.workspace_id
        WHERE applied.source_producer = $2
    )
    SELECT EXISTS (
        SELECT 1 FROM target t WHERE NOT EXISTS (
            SELECT 1 FROM senders s
            WHERE NOT COALESCE(connector.slack_person_allowed(t.workspace_id, s.source_actor, $3, $4, $5), FALSE)
        )
    )
$$;

GRANT EXECUTE ON FUNCTION fleet.task_person_allowed(UUID, TEXT, TEXT, TEXT, TEXT) TO api_runtime;
