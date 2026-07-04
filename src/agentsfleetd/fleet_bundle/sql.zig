//! Centralised SQL for the Fleet Library domain (M103). Every query against
//! `core.fleet_library` / `core.tenant_fleet_library` lives here so table names
//! are grep-able from one file and queries are testable in isolation.

pub const SELECT_PLATFORM_INSTALL =
    \\SELECT skill_markdown, trigger_markdown, content_hash
    \\  FROM core.fleet_library
    \\ WHERE id = $1 AND content_hash IS NOT NULL AND skill_markdown IS NOT NULL
;

pub const SELECT_TENANT_INSTALL =
    \\SELECT skill_markdown, trigger_markdown, content_hash
    \\  FROM core.tenant_fleet_library
    \\ WHERE id = $1::uuid AND workspace_id = $2::uuid
;

pub const INSERT_PLATFORM =
    \\INSERT INTO core.fleet_library
    \\  (id, name, description, source_repo, source_path, source_ref,
    \\   required_credentials, required_credentials_reasons, required_tools, network_hosts,
    \\   visibility, content_hash, skill_markdown, trigger_markdown, support_files_json,
    \\   created_at, updated_at)
    \\VALUES ($1, $2, $3, $4, $5, $6,
    \\        ($7::jsonb -> 'credentials'), $8::jsonb, ($7::jsonb -> 'tools'), ($7::jsonb -> 'network_hosts'),
    \\        $9, $10, $11, $12, $13::jsonb, $14, $14)
    \\ON CONFLICT (id) DO UPDATE SET
    \\   name = EXCLUDED.name,
    \\   description = EXCLUDED.description,
    \\   source_repo = EXCLUDED.source_repo,
    \\   source_ref = EXCLUDED.source_ref,
    \\   required_credentials = EXCLUDED.required_credentials,
    \\   required_tools = EXCLUDED.required_tools,
    \\   network_hosts = EXCLUDED.network_hosts,
    \\   content_hash = EXCLUDED.content_hash,
    \\   skill_markdown = EXCLUDED.skill_markdown,
    \\   trigger_markdown = EXCLUDED.trigger_markdown,
    \\   support_files_json = EXCLUDED.support_files_json,
    \\   updated_at = EXCLUDED.updated_at
    \\RETURNING id
;

pub const INSERT_TENANT =
    \\WITH inserted AS (
    \\  INSERT INTO core.tenant_fleet_library
    \\    (id, workspace_id, name, description, source_kind, source_ref, visibility,
    \\     content_hash, skill_markdown, trigger_markdown, support_files_json,
    \\     requirements_json, created_at, updated_at)
    \\  VALUES ($1::uuid, $2::uuid, $3, $4, $5, $6, $7, $8, $9, $10, $11::jsonb, $12::jsonb, $13, $13)
    \\  ON CONFLICT (workspace_id, content_hash) DO NOTHING
    \\  RETURNING id::text
    \\)
    \\SELECT id FROM inserted
    \\UNION ALL
    \\SELECT id::text FROM core.tenant_fleet_library
    \\WHERE workspace_id = $2::uuid AND content_hash = $8
    \\LIMIT 1
;

pub const SELECT_GALLERY_PLATFORM =
    \\SELECT id, name, description, source_repo,
    \\       required_credentials::text, required_tools::text, network_hosts::text,
    \\       required_credentials_reasons::text,
    \\       COALESCE(support_files_json::text, '[]'), (trigger_markdown IS NOT NULL)
    \\  FROM core.fleet_library
    \\ WHERE visibility = $1 AND content_hash IS NOT NULL
    \\ ORDER BY id
;

pub const SELECT_GALLERY_TENANT =
    \\SELECT id::text, name, description, source_ref,
    \\       requirements_json::text, support_files_json::text
    \\  FROM core.tenant_fleet_library
    \\ WHERE workspace_id = $1::uuid
    \\ ORDER BY created_at DESC
;

pub const SELECT_BUNDLES_LIST =
    \\SELECT id, name, description,
    \\       required_credentials::text, required_tools::text, network_hosts::text,
    \\       required_credentials_reasons::text
    \\  FROM core.fleet_library
    \\ WHERE visibility = $1
    \\ ORDER BY id
;
