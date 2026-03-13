# Rust

## Reference Repos

- `$HOME/Projects/sre/e2e-logging-platform/rust` (if missing, clone `awakeninggit.e2enetworks.net/infra/e2e-logging-platform.git`)
- `$HOME/Projects/manager/cache-kit.rs`

## Stack Inputs

- Project type (`library` or `api`)

## Quality Commands

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all
cargo build --release
```
