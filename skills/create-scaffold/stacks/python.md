# Python Backend

## Reference Repos

- `$HOME/Projects/marketplace_api` — if missing, clone `git@awakeninggit.e2enetworks.net:cloud/marketplace-api.git`
- `$HOME/Projects/cache_access_layer` — if missing, clone `git@awakeninggit.e2enetworks.net:cloud/cache_access_layer.git`

## Stack Inputs

- Framework (`django` or `fastapi`)

## Quality Commands

```bash
ruff check .
ruff format --check .
pytest
```

## Notes

- Use `pyproject.toml` for project metadata.
- Prefer `uv` or `pip-compile` for dependency locking.
