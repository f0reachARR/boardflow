# Repository Guidelines

## Project Structure & Module Organization

`crates/` contains the Rust workspace. Core packages include `api` (HTTP server), `worker`, `db` (SQLx migrations in `crates/db/migrations`), and shared libraries such as `domain`, `config`, `github`, and `kicad`. Most Rust crates keep implementation in `src/` and crate-level tests in `tests/`.

`boardflow/` contains the Next.js frontend. Route code lives under `boardflow/src/app`, reusable UI under `boardflow/src/components`, and API client code under `boardflow/src/lib/api`. Static assets belong in `boardflow/public`. Supporting design and backend notes live in `docs/`.

## Build, Test, and Development Commands

Run backend commands from the repository root:

- `mise exec -- cargo run -p boardflow-api` starts the API on port `3000`.
- `cargo fmt --all -- --check` verifies Rust formatting.
- `cargo clippy --workspace --all-targets -- -D warnings` runs the CI lint gate.
- `cargo test --workspace` runs unit tests.
- `cargo test --workspace -- --ignored` runs DB-backed integration tests.

Run frontend commands from `boardflow/`:

- `pnpm install` installs dependencies.
- `pnpm dev --port 3001` starts the frontend without colliding with the API.
- `pnpm lint`, `pnpm typecheck`, and `pnpm build` match CI checks.
- `pnpm generate:api` regenerates `src/lib/api/schema.d.ts` from the local OpenAPI endpoint.

## Coding Style & Naming Conventions

Rust should remain `rustfmt`-clean and `clippy`-clean. Follow the existing crate split and prefer descriptive snake_case module and file names. Frontend code uses Biome with spaces, 100-column width, and single quotes; run `pnpm lint` before opening a PR. Keep generated files such as `boardflow/src/lib/api/schema.d.ts` out of manual edits.

## Testing Guidelines

Rust tests use the standard test harness, with files named `*_test.rs` under each crate’s `tests/` directory. Database changes must include matching `.up.sql` and `.down.sql` migrations with timestamp prefixes. If API shapes change, update the OpenAPI snapshot in `crates/api/tests/snapshots/` after reviewing the diff with `cargo insta review`.

## Commit & Pull Request Guidelines

Recent history favors short, imperative subjects such as `fix: remove unused yaml feature from insta` or `ibom fix`. Keep commits focused and reference issue numbers when relevant. PRs should describe the behavioral change, note any migration or schema impact, and include screenshots for UI changes. Before requesting review, run the Rust and frontend checks listed above.
