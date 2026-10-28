# Python

This standard covers Python modules and scripts: parsing structured input, owning resources, validating at boundaries and handling exceptions.
A coding agent reads it before editing a `*.py` file, and a reviewer judges a diff against it.

## Input and resources

### PY-PARSE — Structured input is read with the standard-library parser for its format before any third-party package.
- **Applies to:** `**/*.py`
- **Checked by:** review
- **Bad:** `import toml` to read `pyproject.toml`
- **Good:** `with open(path, "rb") as file: config = tomllib.load(file)`
- **Why:** the standard library ships with the interpreter, so there is no dependency to pin, audit or install.

The standard parsers include `json`, `tomllib`, `csv`, `configparser`, `urllib.parse` and `email`.

### PY-WITH — A file, lock, socket or temporary directory is owned by a `with` block.
- **Applies to:** `**/*.py`
- **Checked by:** review
- **Bad:** `f = open(path); data = f.read()`
- **Good:** `with open(path, encoding="utf-8") as f: data = f.read()`
- **Why:** the context manager releases the resource on every exit path, including an exception.

### PY-BOUNDARY — Data is validated once, where it enters, into a typed object, and inner code takes that object rather than a raw `dict`.
- **Applies to:** `**/*.py`
- **Checked by:** review
- **Bad:** `payload["project_id"]` read from a raw `dict` five calls deep
- **Good:** `request = ProjectRequest.from_json(payload)` at the boundary, raising on a missing or malformed field
- **Why:** validation deferred to business logic accepts bad input at the edge and fails far from it.

## Exceptions

### PY-EXCEPT — Code catches the specific exception it can handle, and chains the cause when it raises a new one.
- **Applies to:** `**/*.py`
- **Checked by:** review
- **Bad:** `except Exception: return None`
- **Good:** `except FileNotFoundError: return None`, or `raise ConfigError(path) from error`
- **Why:** a broad catch hides the failure the caller needed to see, and an unchained raise loses its cause.
