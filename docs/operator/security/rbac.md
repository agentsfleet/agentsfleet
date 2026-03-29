# Roles and access control

## Overview

UseZombie enforces role-based access control (RBAC) on all API operations. Roles are assigned per-workspace and checked server-side on every request. The client never makes authorization decisions.

## Roles

Three roles are defined, each a superset of the previous:

### user

The default role for workspace members. Covers normal day-to-day operations.

| Permission | Description |
|-----------|-------------|
| Submit specs | Create and submit specs for execution. |
| View runs | View run status, logs, scorecards, and PR output. |
| Manage workspaces | Add repositories, configure workspace settings. |
| View billing | View current plan, credit balance, and usage history. |

### operator

Elevated role for platform operators and workspace administrators.

| Permission | Description |
|-----------|-------------|
| All `user` permissions | Everything a user can do. |
| Harness management | Configure agent harnesses, source references, and compile constraints. |
| Skill-secret management | Manage workspace-scoped secrets available to agent skills. |
| Scoring views | Access detailed scoring breakdowns and agent performance analytics. |
| Workspace configuration | Advanced workspace settings (concurrency limits, timeout overrides). |

### admin

Full platform access for billing and administrative operations.

| Permission | Description |
|-----------|-------------|
| All `operator` permissions | Everything an operator can do. |
| Billing lifecycle | Change plans, manage payment methods, adjust credit budgets. |
| API key management | Create, rotate, and revoke API-key-backed admin access. |
| Workspace suspension | Pause or suspend workspaces. |

## Policy dimensions

Every API endpoint is protected by two policy dimensions:

### Minimum role

Each endpoint requires a minimum role. Requests from users with insufficient roles receive `403 Forbidden`.

| Endpoint pattern | Minimum role |
|-----------------|--------------|
| `POST /runs`, `GET /runs/*` | `user` |
| `POST /specs/*`, `GET /specs/*` | `user` |
| `GET /workspaces/*`, `PUT /workspaces/*` | `user` |
| `PUT /harness/*`, `PUT /secrets/*` | `operator` |
| `GET /scoring/*` | `operator` |
| `POST /billing/*`, `DELETE /api-keys/*` | `admin` |
| `POST /workspaces/*/pause` | `admin` |

### Credit policy

Some endpoints additionally require that the workspace has sufficient credits:

| Credit policy | Meaning | Applied to |
|--------------|---------|------------|
| `none` | No credit check — endpoint is always accessible. | Read operations, workspace management, billing views. |
| `execution_required` | Workspace must have a positive credit balance or be on a plan with included credits. | `POST /runs` — creating a new run. |

A request that passes the role check but fails the credit check receives `402 Payment Required`.

## Enforcement

Authorization is enforced entirely server-side in the API middleware. The flow:

1. Extract the Clerk JWT from the `Authorization` header.
2. Verify the JWT signature and expiration.
3. Look up the user's role for the target workspace in PostgreSQL.
4. Check the endpoint's minimum role requirement.
5. If the endpoint has a credit policy, check the workspace's credit balance.
6. Allow or deny the request.

<Info>
RBAC enforcement is covered by live HTTP integration tests that verify every role/endpoint combination. These tests run on every CI build.
</Info>

## Role assignment

Roles are assigned through the API:

```bash
# Assign operator role to a user in a workspace
zombiectl workspace role set --workspace <id> --user <user_id> --role operator
```

The first user to create a workspace is automatically assigned the `admin` role. Additional users are assigned the `user` role by default when they join.
