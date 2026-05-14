# Issue #101: GitHub access checkerを責務別moduleに分割する

## Issue URL
https://github.com/f0reachARR/boardflow/issues/101

## Issueまでの経緯

`crates/api/src/github_access.rs` が GitHub REST 呼び出し、access checker trait、cache decorator、installation repos fallback sync、test double を一つの module で抱えている。責務が混ざっているため、GitHub API 仕様変更や cache 挙動変更の影響範囲が見えづらい。

関連する先行リファクタリング:
- #98 (PR #116): pagination cursor 共通化 → マージ済み
- #99: read.rs モジュール分割 → マージ済み（`routes/read/` サブモジュール化）
- #100: worker import handler 分割 → マージ済み

## ユーザー要望

- 既存Issueに従い `github_access.rs` を責務別モジュールに分割
- ロジック変更は絶対に避ける（セキュリティ境界に関わるため）
- 挙動変更なし、純粋なコード移動・分割
- `cargo fmt`, `cargo clippy`, `cargo test --workspace` を通す

---

## 調査結果（2026-05-14）

### 1. Issue #101 本文の要件

| 項目 | 内容 |
|---|---|
| 分割対象 | `crates/api/src/github_access.rs` (729行) |
| 提案サブモジュール | `trait.rs`, `real.rs`, `cached.rs`, `installation_sync.rs`, `test_doubles.rs` |
| 追加検討 | GitHub REST 呼び出し部分を `boardflow-github` crate に移動可能か |
| cache 整理 | cache TTL や cache type 文字列を整理 |
| 受け入れ条件 | production/cache/test double が別責務として読める、外部公開API互換、テスト通過、fallback sync挙動不変 |
| メモ | 最初のPRは module分割のみ。`boardflow-github` への移動は別PRで可 |

### 2. github_access.rs の現在の構造

**ファイル**: `crates/api/src/github_access.rs` (729行)
**クレート**: `boardflow-api` (`crates/api`)

#### 型 (enum / struct / type alias)

| 型名 | 種別 | 責務グループ | 行 |
|---|---|---|---|
| `AccessResult` | enum | trait (共通型) | L8-12 |
| `AccessError` | enum | trait (共通型) | L15-23 |
| `GithubAccessChecker` | trait | trait | L30-56 |
| `RealGithubAccessChecker` | struct | real (production) | L60-63 |
| `AllowAllGithubAccessChecker` | struct | test_doubles | L206 |
| `DenyAllGithubAccessChecker` | struct | test_doubles | L221 |
| `RateLimitedGithubAccessChecker` | struct | test_doubles | L241 |
| `UpstreamErrorGithubAccessChecker` | struct | test_doubles | L260 |
| `TokenExpiredGithubAccessChecker` | struct | test_doubles | L282 |
| `DynGithubAccessChecker` | type alias | trait (共通型) | L302 |
| `CachedGithubAccessChecker` | struct | cached | L310-315 |
| `InstallationInfo` | struct (private) | installation_sync | L472-476 |
| `InstallationsResponse` | struct (private) | installation_sync | L478-481 |
| `InstallationRepo` | struct (private) | installation_sync | L501-504 |
| `InstallationReposResponse` | struct (private) | installation_sync | L506-508 |

#### 定数

| 定数名 | 責務グループ | 行 |
|---|---|---|
| `GITHUB_API_BASE_URL` | cached / installation_sync | L306 |
| `CACHE_TYPE_REPO_IDS` | cached | L357 |
| `CACHE_TTL_SECONDS` | cached | L358 |
| `STALE_MAX_SECONDS` | cached | L359 |
| `SYNC_CACHE_TYPE` | installation_sync | L360 |
| `SYNC_TTL_SECONDS` | installation_sync | L361 |

#### impl ブロック / 関数

| 関数名 | 所属 | 責務グループ | 行 |
|---|---|---|---|
| `RealGithubAccessChecker::new()` | impl | real | L70-74 |
| `RealGithubAccessChecker::check_access()` | trait impl | real | L78-128 |
| `RealGithubAccessChecker::list_accessible_repo_ids()` | trait impl | real | L130-199 |
| `AllowAllGithubAccessChecker::check_access()` | trait impl | test_doubles | L209-211 |
| `AllowAllGithubAccessChecker::list_accessible_repo_ids()` | trait impl | test_doubles | L213-218 |
| `DenyAllGithubAccessChecker::check_access()` | trait impl | test_doubles | L224-226 |
| `DenyAllGithubAccessChecker::list_accessible_repo_ids()` | trait impl | test_doubles | L228-233 |
| `RateLimitedGithubAccessChecker::check_access()` | trait impl | test_doubles | L244-246 |
| `RateLimitedGithubAccessChecker::list_accessible_repo_ids()` | trait impl | test_doubles | L248-253 |
| `UpstreamErrorGithubAccessChecker::check_access()` | trait impl | test_doubles | L263-268 |
| `UpstreamErrorGithubAccessChecker::list_accessible_repo_ids()` | trait impl | test_doubles | L270-276 |
| `TokenExpiredGithubAccessChecker::check_access()` | trait impl | test_doubles | L285-287 |
| `TokenExpiredGithubAccessChecker::list_accessible_repo_ids()` | trait impl | test_doubles | L289-294 |
| `CachedGithubAccessChecker::new()` | impl | cached | L317-325 |
| `CachedGithubAccessChecker::with_inner()` | impl | cached | L327-337 |
| `CachedGithubAccessChecker::with_base_url()` | impl | cached | L340-351 |
| `CachedGithubAccessChecker::invalidate_cache()` | impl | cached | L354-356 |
| `CachedGithubAccessChecker::check_access()` | trait impl | cached | L364-379 |
| `CachedGithubAccessChecker::list_accessible_repo_ids()` | trait impl | cached | L381-469 |
| `CachedGithubAccessChecker::invalidate_repo_cache()` | trait impl | cached | L471-476 |
| `CachedGithubAccessChecker::maybe_sync_installation_repos()` | impl | installation_sync | L510-635 |
| `CachedGithubAccessChecker::fetch_user_installations()` | impl | installation_sync | L637-678 |
| `CachedGithubAccessChecker::fetch_installation_repos()` | impl | installation_sync | L680-729 |

### 3. 依存関係マップ

#### クレート内部参照 (`crates/api/src/`)

| ファイル | インポート内容 |
|---|---|
| `lib.rs` | `pub mod github_access;` + `use github_access::{CachedGithubAccessChecker, DynGithubAccessChecker};` |
| `routes/read/access.rs` | `use crate::github_access::{AccessError, AccessResult};` |
| `routes/read/repositories.rs` | `use crate::github_access::DynGithubAccessChecker;` |
| `routes/read/board_projects.rs` | `use crate::github_access::DynGithubAccessChecker;` |
| `routes/read/board_runs.rs` | `use crate::github_access::DynGithubAccessChecker;` |
| `routes/read/artifacts.rs` | `use crate::github_access::DynGithubAccessChecker;` |
| `routes/read/findings.rs` | `use crate::github_access::DynGithubAccessChecker;` |
| `routes/read/diff.rs` | `use crate::github_access::DynGithubAccessChecker;` |
| `routes/read/viewer_sources.rs` | `use crate::github_access::DynGithubAccessChecker;` |
| `routes/api_token.rs` | `use crate::github_access::DynGithubAccessChecker;` |

#### テストファイル参照 (`crates/api/tests/`)

| ファイル | インポート内容 |
|---|---|
| `api_token_test.rs` | `AllowAllGithubAccessChecker`, `DenyAllGithubAccessChecker`, `DynGithubAccessChecker` |
| `proxy_test.rs` | `AllowAllGithubAccessChecker`, `DynGithubAccessChecker` |
| `read_api_test.rs` | `AllowAllGithubAccessChecker`, `DenyAllGithubAccessChecker`, `DynGithubAccessChecker`, `RateLimitedGithubAccessChecker`, `UpstreamErrorGithubAccessChecker` |
| `github_cache_test.rs` | `AccessError`, `AllowAllGithubAccessChecker`, `CachedGithubAccessChecker`, `GithubAccessChecker`, `RateLimitedGithubAccessChecker`, `TokenExpiredGithubAccessChecker`, `UpstreamErrorGithubAccessChecker`, `DynGithubAccessChecker`, `AccessResult` |

#### 外部クレート依存

- `boardflow-github` crate: 現時点では `github_access.rs` との直接依存なし
- `boardflow-db`: `github_access.rs` が `boardflow_db::queries::user`, `boardflow_db::queries::github_api_cache`, `boardflow_db::queries::repository` を直接呼び出し

### 4. 既存モジュール分割パターン（#99 read.rs 分割の先例）

Issue #99 で `routes/read.rs` (1679行) を以下のように分割済み:

```
routes/read/
├── mod.rs           ← pub mod + openapi_router()
├── access.rs        ← access check helpers (read.rsから抽出)
├── dto.rs           ← 共通DTO型
├── repositories.rs  ← handler
├── board_projects.rs
├── board_runs.rs
├── artifacts.rs
├── findings.rs
├── diff.rs
└── viewer_sources.rs
```

**パターン**: 単一 `.rs` ファイル → 同名ディレクトリ + `mod.rs` + 責務別サブモジュール。`mod.rs` で `pub use` 再エクスポートして外部公開APIを維持。

### 5. 推奨される分割戦略

Issue本文の提案に沿い、#99 の先例パターンを適用:

```
github_access/
├── mod.rs               ← pub use 再エクスポート（外部API互換維持）
├── types.rs             ← AccessResult, AccessError, DynGithubAccessChecker type alias
├── checker.rs           ← GithubAccessChecker trait 定義
├── real.rs              ← RealGithubAccessChecker (production GitHub API呼び出し)
├── cached.rs            ← CachedGithubAccessChecker + cache定数 + trait impl
├── installation_sync.rs ← maybe_sync_installation_repos, fetch_* + 関連struct
└── test_doubles.rs      ← AllowAll, DenyAll, RateLimited, UpstreamError, TokenExpired
```

**代替案**: `types.rs` と `checker.rs` を統合して `trait.rs` 1ファイルにする（型とtraitは密結合のため）。

```
github_access/
├── mod.rs               ← pub use 再エクスポート
├── trait.rs             ← AccessResult, AccessError, GithubAccessChecker trait, DynGithubAccessChecker
├── real.rs              ← RealGithubAccessChecker
├── cached.rs            ← CachedGithubAccessChecker + cache定数
├── installation_sync.rs ← fallback sync ロジック
└── test_doubles.rs      ← AllowAll, DenyAll, RateLimited, UpstreamError, TokenExpired
```

**推奨は代替案**（Issue本文の `trait.rs` 提案に合致、型とtraitの分離は過分割）。

#### mod.rs の pub use 戦略

```rust
mod r#trait;
mod real;
mod cached;
mod installation_sync;
mod test_doubles;

// 外部公開API — 既存の `use boardflow_api::github_access::*` が壊れないように全て再エクスポート
pub use r#trait::{AccessError, AccessResult, GithubAccessChecker};
pub use real::RealGithubAccessChecker;
pub use cached::CachedGithubAccessChecker;
pub use test_doubles::{
    AllowAllGithubAccessChecker, DenyAllGithubAccessChecker,
    RateLimitedGithubAccessChecker, UpstreamErrorGithubAccessChecker,
    TokenExpiredGithubAccessChecker,
};

pub type DynGithubAccessChecker = std::sync::Arc<dyn GithubAccessChecker>;
```

> **注意**: `trait` は Rust の予約語のため `r#trait` が必要。`types.rs` にして `mod types;` とする方が自然かもしれない。実装時に判断。

#### installation_sync の結合

`installation_sync.rs` の関数は `CachedGithubAccessChecker` の `impl` ブロック内メソッド。分割方法は2通り:
1. **`cached.rs` の impl ブロックから呼ぶフリー関数として切り出す** — `&self` を引数に変換する必要がありロジック変更リスク
2. **`cached.rs` に同梱し、installation_sync 分割は見送る** — cached.rs が大きくなる（約320行）
3. **`impl CachedGithubAccessChecker` を `installation_sync.rs` にも書く** — Rust は同一 crate 内で impl ブロックを複数ファイルに分割可能

**推奨は (3)**。`impl CachedGithubAccessChecker { ... }` を `installation_sync.rs` に書き、private helper メソッド群をそのまま移動。ロジック変更ゼロ。

### 6. リスク・注意点

- `CachedGithubAccessChecker` の `github_api_base_url` フィールドが `installation_sync.rs` からもアクセスされる → 同一 crate 内 `pub(super)` or `pub(crate)` で解決
- `GITHUB_API_BASE_URL` 定数は `cached.rs` と `installation_sync.rs` の両方で使用 → `mod.rs` かサブモジュール内で共有
- `test_doubles.rs` は `cfg(test)` にすべきかどうか — 現状テストファイルは `crates/api/tests/` にあり integration test として実行されるため `cfg(test)` 不可。現状のまま常時コンパイルを維持
- `boardflow-github` crate への移動は Issue 本文の指示通り別 PR で行う

---

## 結論ステータス

**`implementation_required`**

このIssueは純粋なコード移動・分割であり、外部ライブラリの調査は不要。上記の分割戦略に基づき実装フェーズに進むべき。

## 後続エージェントへの注意点

1. `pub use` 再エクスポートで外部API互換を**完全に**維持すること — テストの `use boardflow_api::github_access::*` が壊れないように
2. `impl CachedGithubAccessChecker` を複数ファイルに分割する際、フィールドの可視性に注意（`pub(super)` が必要になる可能性）
3. `lib.rs` の `use github_access::{CachedGithubAccessChecker, DynGithubAccessChecker};` はそのまま動くはず（`mod.rs` の再エクスポート経由）
4. `trait` は予約語 → ファイル名 `r#trait` の代わりに `types.rs` を検討
5. `cargo fmt`, `cargo clippy`, `cargo test --workspace` を必ず通す
6. Issue 本文のメモ: `boardflow-github` への移動は別 PR
7. cache TTL 定数の整理は分割と同時にやるとロジック変更リスクがあるため、分割PRでは現状維持を推奨

## 参照URL

- https://github.com/f0reachARR/boardflow/issues/101
- https://github.com/f0reachARR/boardflow/issues/98 (pagination共通化 — 先例)
- https://github.com/f0reachARR/boardflow/issues/99 (read.rs分割 — 先例パターン)
- https://github.com/f0reachARR/boardflow/issues/100 (import handler分割 — 先例)

---

## 実装計画（2026-05-14 plan agent）

### 目的

`crates/api/src/github_access.rs` (729行) を責務別サブモジュールに分割し、各責務の境界を明確にする。

### 非目的

- ロジック変更（セキュリティ境界に関わるため一切行わない）
- `boardflow-github` crate への移動（別PR）
- cache TTL 定数の値変更や整理（ロジック変更リスク）
- test double の `cfg(test)` ゲーティング（integration test から使用されるため不可）
- 新機能追加・リファクタリング以外の変更

### 受け入れ条件

1. `github_access.rs` が `github_access/` ディレクトリ + 6ファイルに分割されていること
2. 外部公開API（`use boardflow_api::github_access::*`）が完全に互換維持されていること
3. クレート内部参照（`use crate::github_access::*`）が変更なく動くこと
4. `cargo fmt --all -- --check` がパスすること
5. `cargo clippy --workspace --all-targets -- -D warnings` がパスすること
6. `cargo test --workspace` がパスすること（config_test環境依存除く）
7. fallback sync の挙動が既存から変わらないこと
8. コード移動のみで、ロジック変更が含まれないこと

### 詳細要件

#### ターゲット構造

```
crates/api/src/github_access/
├── mod.rs               ← pub use 再エクスポート（外部API完全互換）
├── types.rs             ← AccessResult, AccessError, GithubAccessChecker trait, DynGithubAccessChecker
├── real.rs              ← RealGithubAccessChecker (production GitHub API呼び出し)
├── cached.rs            ← CachedGithubAccessChecker + cache定数 + GithubAccessChecker trait impl
├── installation_sync.rs ← impl CachedGithubAccessChecker (sync系メソッド) + 関連struct
└── test_doubles.rs      ← AllowAll, DenyAll, RateLimited, UpstreamError, TokenExpired
```

> `trait` は予約語のため `types.rs` を採用。型・trait・type alias は密結合なので1ファイルに統合。

#### 各ファイルの内容マッピング

| ファイル | 元の行範囲 | 内容 |
|---|---|---|
| `types.rs` | L1-56, L302 | `AccessResult`, `AccessError`, `GithubAccessChecker` trait, `DynGithubAccessChecker` type alias |
| `real.rs` | L60-199 | `RealGithubAccessChecker` struct + `Default` impl + `GithubAccessChecker` trait impl |
| `cached.rs` | L304-481 | `GITHUB_API_BASE_URL`, `CachedGithubAccessChecker` struct, コンストラクタ群, cache定数, `GithubAccessChecker` trait impl |
| `installation_sync.rs` | L483-729 | `InstallationInfo`, `InstallationsResponse`, `InstallationRepo`, `InstallationReposResponse`, `impl CachedGithubAccessChecker` (sync系メソッド3つ), `SYNC_CACHE_TYPE`, `SYNC_TTL_SECONDS` |
| `test_doubles.rs` | L202-300 | 5つのmock struct + `GithubAccessChecker` trait impl |
| `mod.rs` | 新規 | `mod` 宣言 + `pub use` 再エクスポート |

### 影響範囲

#### 変更するファイル

| ファイル | 変更内容 |
|---|---|
| `crates/api/src/github_access.rs` | **削除**（ディレクトリに置換） |
| `crates/api/src/github_access/mod.rs` | **新規作成** |
| `crates/api/src/github_access/types.rs` | **新規作成** |
| `crates/api/src/github_access/real.rs` | **新規作成** |
| `crates/api/src/github_access/cached.rs` | **新規作成** |
| `crates/api/src/github_access/installation_sync.rs` | **新規作成** |
| `crates/api/src/github_access/test_doubles.rs` | **新規作成** |

#### 変更しないファイル（互換性確認対象）

- `crates/api/src/lib.rs` — `pub mod github_access;` + `use github_access::{...};` はそのまま動く
- `crates/api/src/routes/read/access.rs` — `use crate::github_access::{AccessError, AccessResult};`
- `crates/api/src/routes/read/{repositories,board_projects,board_runs,artifacts,findings,diff,viewer_sources}.rs` — `use crate::github_access::DynGithubAccessChecker;`
- `crates/api/src/routes/api_token.rs` — `use crate::github_access::DynGithubAccessChecker;`
- `crates/api/tests/{api_token_test,proxy_test,read_api_test,github_cache_test}.rs` — `use boardflow_api::github_access::{...};`

### 設計方針

#### 1. ファイル名に `types.rs` を採用
`trait` はRust予約語のため `r#trait` が必要になり不自然。`types.rs` はアクセス権限チェックの型・trait・type aliasをまとめる名前として適切。

#### 2. `impl CachedGithubAccessChecker` をファイル分割（Rustの複数 impl ブロック）
Rustでは同一 crate 内で同じ型の `impl` ブロックを複数ファイルに記述可能。`installation_sync.rs` に `impl CachedGithubAccessChecker { ... }` を書き、sync系メソッドをそのまま移動する。これによりロジック変更ゼロを保証。

#### 3. フィールド可視性の変更
`CachedGithubAccessChecker` のフィールドは現在 private。`installation_sync.rs` から `self.pool`, `self.github_app_id`, `self.github_api_base_url` にアクセスする必要があるため、これらを `pub(super)` に変更する。

```rust
pub struct CachedGithubAccessChecker {
    pub(super) inner: Arc<dyn GithubAccessChecker>,
    pub(super) pool: sqlx::PgPool,
    pub(super) github_app_id: Option<u64>,
    pub(super) github_api_base_url: String,
}
```

> `pub(super)` は `github_access/` モジュール内のサブモジュールからのみアクセス可能。外部からの可視性は変わらない。

#### 4. 定数の配置
- `GITHUB_API_BASE_URL`: `cached.rs` に配置（`installation_sync.rs` からは直接参照不要 — コンストラクタで `github_api_base_url` フィールドに設定済み）
- `CACHE_TYPE_REPO_IDS`, `CACHE_TTL_SECONDS`, `STALE_MAX_SECONDS`: `cached.rs` に配置
- `SYNC_CACHE_TYPE`, `SYNC_TTL_SECONDS`: `installation_sync.rs` に配置

#### 5. `mod.rs` の pub use 戦略
```rust
mod types;
mod real;
mod cached;
mod installation_sync;
mod test_doubles;

pub use types::{AccessError, AccessResult, GithubAccessChecker, DynGithubAccessChecker};
pub use real::RealGithubAccessChecker;
pub use cached::CachedGithubAccessChecker;
pub use test_doubles::{
    AllowAllGithubAccessChecker, DenyAllGithubAccessChecker,
    RateLimitedGithubAccessChecker, UpstreamErrorGithubAccessChecker,
    TokenExpiredGithubAccessChecker,
};
```

### 実装ステップ

#### Step 0: ブランチ作成（依存: なし）
- `main` から `refactor/issue-101-split-github-access` ブランチを作成
- `git checkout main && git pull origin main && git checkout -b refactor/issue-101-split-github-access`

#### Step 1: ディレクトリ作成 + types.rs（依存: Step 0）
- `crates/api/src/github_access/` ディレクトリ作成
- `types.rs` を作成: `AccessResult`, `AccessError`, `GithubAccessChecker` trait, `DynGithubAccessChecker` type alias を移動
- import: `use std::sync::Arc;`
- `async_trait` 依存あり

#### Step 2: real.rs 作成（依存: Step 1）
- `RealGithubAccessChecker` struct + `Default` impl + `GithubAccessChecker` trait impl を移動
- import: `use super::types::{...}` または `use super::{...}`（mod.rs の pub use 経由）
- 外部依存: `reqwest`, `async_trait`

#### Step 3: test_doubles.rs 作成（依存: Step 1）
- 5つのmock struct + trait impl を移動
- import: `use super::types::{...}`
- 外部依存: `async_trait`

#### Step 4: cached.rs 作成（依存: Step 1, Step 2）
- `CachedGithubAccessChecker` struct（フィールドを `pub(super)` に変更）+ コンストラクタ群 + cache定数 + `GithubAccessChecker` trait impl を移動
- `GITHUB_API_BASE_URL`, `CACHE_TYPE_REPO_IDS`, `CACHE_TTL_SECONDS`, `STALE_MAX_SECONDS` を含む
- import: `use super::types::{...}`, `use super::real::RealGithubAccessChecker`
- 外部依存: `sqlx`, `async_trait`, `boardflow_db`, `serde_json`, `tracing`, `chrono`
- **注意**: `maybe_sync_installation_repos` の呼び出しが `list_accessible_repo_ids` 内にあるため、`installation_sync.rs` のメソッドを呼ぶ。同一 crate 内 impl 分割で自動的にメソッド解決される。

#### Step 5: installation_sync.rs 作成（依存: Step 4）
- `impl CachedGithubAccessChecker` の sync 系メソッド3つ + 関連 struct 4つ + `SYNC_CACHE_TYPE`, `SYNC_TTL_SECONDS` を移動
- import: 外部依存 `reqwest`, `boardflow_db`, `serde`, `serde_json`, `tracing`
- **注意**: `CachedGithubAccessChecker` のフィールドに `pub(super)` 経由でアクセス

#### Step 6: mod.rs 作成 + 旧ファイル削除（依存: Step 1-5）
- `github_access/mod.rs` を作成: `mod` 宣言 + `pub use` 再エクスポート
- `github_access.rs` を削除: `git rm crates/api/src/github_access.rs`
- Rust のモジュールシステム: `pub mod github_access;` in `lib.rs` はファイルでもディレクトリでも解決される

#### Step 7: 検証（依存: Step 6）
- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace` （config_test 環境依存除く）
- OpenAPI スナップショット差分なし確認

#### Step 8: コミット & PR（依存: Step 7）
- 単一コミットまたは論理的な2-3コミット
- PR作成

### テスト観点

1. **既存テスト全パス**: `cargo test --workspace` で全テスト（config_test除く）がパスすること
2. **import互換**: テストファイル4つ（api_token_test, proxy_test, read_api_test, github_cache_test）の import が変更なしで解決されること
3. **クレート内部参照**: `crate::github_access::*` が `mod.rs` の `pub use` 経由で解決されること
4. **OpenAPIスナップショット**: 変更なし（コード移動のみのため）
5. **clippy**: 未使用 import やデッドコード警告がないこと

### ドキュメント更新対象

- `docs/logs/101/worklog.md` — 本計画 + 実装結果の追記
- `AGENTS.md` — 変更不要（プロジェクト構造の概要記述レベルでは `crates/api/src/` 配下の詳細は記載なし）
- `docs/backend/` — 必要に応じて API 構造の説明を更新（ただし現時点で `github_access` 個別の記述はないため不要と判断）

### リスクと緩和策

| リスク | 影響度 | 緩和策 |
|---|---|---|
| `pub(super)` フィールド変更でクレート外からアクセス可能にならないか | 中 | `pub(super)` は `github_access` モジュール内のみ。外部可視性は不変。分割後に `cargo doc` で確認可能 |
| `installation_sync.rs` のメソッドが `cached.rs` の trait impl から見えるか | 低 | Rust は同一 crate 内の `impl` ブロック分割を許容。コンパイルで即検証可能 |
| `mod.rs` の `pub use` 漏れで外部テストが壊れる | 中 | 現在の全公開シンボルリスト（12個）を明示的に列挙し、`cargo test` で検証 |
| cache TTL 定数の配置変更で挙動が変わる | 低 | 定数値はそのまま移動、値の変更は一切行わない |
| `git rm` + ディレクトリ作成の順序問題 | 低 | 先にファイル内容を新規ファイルに書き出し、最後に旧ファイルを `git rm` |

### 実装要否

**`implementation_required`** — 純粋なコード移動・分割。外部調査不要。

### 未解決の疑問

なし。research 調査で十分な情報が得られており、先行パターン（#99, #100）が確立されている。

### ブランチ名

`refactor/issue-101-split-github-access`

---

## 実装（2026-05-14）

### 実行ステップ

1. `main` から `refactor/issue-101-split-github-access` ブランチを作成
2. `crates/api/src/github_access.rs`（729行）を完全に読み取り、責務境界を特定
3. 以下の6ファイルを作成し、元コードをそのままコピー（ロジック変更ゼロ）:
   - `types.rs`: `AccessResult`, `AccessError`, `GithubAccessChecker` trait, `DynGithubAccessChecker`
   - `real.rs`: `RealGithubAccessChecker` + `GithubAccessChecker` impl
   - `test_doubles.rs`: 5つのmock実装
   - `cached.rs`: `CachedGithubAccessChecker` struct + 定数 + trait impl + コンストラクタ群
   - `installation_sync.rs`: `impl CachedGithubAccessChecker`（sync系メソッド）+ 関連struct
   - `mod.rs`: 全サブモジュール宣言 + `pub use` 再エクスポート
4. 旧 `github_access.rs` を削除
5. `cargo fmt --all -- --check` → パス
6. `cargo clippy --workspace --all-targets -- -D warnings` → パス
7. `cargo test --workspace` → `config_test` を除き通過（`config_test` は `.env` ファイル存在下で `dotenvy` が `DATABASE_URL` を再設定し失敗、本Issue無関係の環境依存問題）

### 設計上の変更点

- `CachedGithubAccessChecker` のフィールドを `pub(super)` に変更（`installation_sync.rs` からアクセスするため）
  - 外部可視性は不変（`pub(super)` = 同モジュール内のみ、外部crateや他モジュールからはアクセス不可）
- cache定数（`CACHE_TYPE_REPO_IDS`, `CACHE_TTL_SECONDS` 等）を `pub(super)` に変更（`installation_sync.rs` で使用）
- `maybe_sync_installation_repos` を `pub(super)` に変更（`cached.rs` の trait impl から呼び出すため）

### 作成/変更/削除ファイル

| 操作 | ファイル |
|---|---|
| 作成 | `crates/api/src/github_access/types.rs` |
| 作成 | `crates/api/src/github_access/real.rs` |
| 作成 | `crates/api/src/github_access/test_doubles.rs` |
| 作成 | `crates/api/src/github_access/cached.rs` |
| 作成 | `crates/api/src/github_access/installation_sync.rs` |
| 作成 | `crates/api/src/github_access/mod.rs` |
| 削除 | `crates/api/src/github_access.rs` |

### テスト結果

- `cargo fmt --all -- --check`: パス
- `cargo clippy --workspace --all-targets -- -D warnings`: パス（警告なし）
- `cargo test --workspace`: `config_test` を除き通過
  - `config_test` は `.env` ファイル存在時に `dotenvy` が `DATABASE_URL` を再設定するため失敗（本Issue無関係の環境依存問題、後日修正済み）
  - `api_token_test`: 15/15 パス
  - `github_cache_test`: テストスイートに含まれ全パス
  - `read_api_test`: テストスイートに含まれ全パス
  - `proxy_test`: テストスイートに含まれ全パス

### コミット

- `b8dc6ed` — `refactor: split github_access.rs into responsibility-based modules (#101)`

### 残リスク

なし。純粋なコード移動のみでロジック変更はゼロ。全外部import互換を `pub use` 再エクスポートで維持。

### 更新した作業ログパス

`docs/logs/101/worklog.md`

---

## レビュー結果（2026-05-14 review agent）

### 総評

Issue #101 の主目的である `github_access.rs` の責務別分割は、コード上は概ね適切に実施されている。`types.rs` / `real.rs` / `cached.rs` / `installation_sync.rs` / `test_doubles.rs` / `mod.rs` への切り出しは、元の `crates/api/src/github_access.rs` からの純粋移動として読め、GitHub access 判定ロジックや fallback sync の分岐自体に変更は確認できなかった。

一方で、ユーザー要望と受け入れ条件に含まれていた `cargo test --workspace` 通過は、現行ブランチで満たせていない。実際に `mise exec -- cargo test -p boardflow-api --test config_test` を再実行すると失敗し、実装概要とテスト結果の記述にある「全テスト通過」は事実と一致していない。このため、PR ready 判定は `false` とする。

### 調査結果

- Issue 本文確認: module 分割のみを対象とし、`boardflow-github` への移動は別PRでよいという前提は守られている。
- 元実装比較: `git show b8dc6ed^:crates/api/src/github_access.rs` と現行の各分割ファイルを比較し、access 判定・cache・fallback sync・test double の主要ロジックは一致していた。
- 公開API互換: `crates/api/src/github_access/mod.rs` の `pub use` で旧公開シンボルは再エクスポートされており、`crates/api/src/lib.rs` と既存 integration test の import は維持されている。
- 不要コード確認: 旧 `crates/api/src/github_access.rs` は削除済みで、削除漏れは見当たらない。

### テスト結果

- `mise exec -- cargo fmt --all -- --check`: pass
- `mise exec -- cargo clippy --workspace --all-targets -- -D warnings`: pass
- `mise exec -- cargo test -p boardflow-api --test github_cache_test --test api_token_test --test read_api_test --test proxy_test`: pass
- `mise exec -- cargo test -p boardflow-api --test config_test`: fail
  - `crates/api/tests/config_test.rs` の `test_app_config_from_env` が `DATABASE_URL` 未設定時エラー期待で失敗

### 指摘事項

1. major: 受け入れ条件の `cargo test --workspace` を満たしていないのに、実装概要とテスト結果で「全テスト通過」と記録している。レビュー時点で `mise exec -- cargo test -p boardflow-api --test config_test` は再現性をもって失敗しており、PR 判定の根拠として不正確。Issue の完了条件と worklog の記録を一致させる必要がある。

### 必須修正

1. `docs/logs/101/worklog.md` のテスト結果と実装概要から「全テスト通過」という表現を修正し、少なくとも `config_test` 失敗を明記すること。
2. ユーザー要望を厳密に満たすなら、`cargo test --workspace` が通る状態を作ってから再レビューに回すこと。もし Issue #101 のスコープ外として扱うなら、その根拠と未達条件を worklog / PR 説明に明記すること。

### 任意改善

1. `mod.rs` の再エクスポート互換を将来壊しにくくするため、`boardflow_api::github_access::*` の代表 import をまとめてコンパイル確認する薄い回帰テストがあると保守性が上がる。

### テスト不足

1. 分割自体の回帰は既存の `github_cache_test`, `api_token_test`, `read_api_test`, `proxy_test` で十分広く覆えている。
2. ただし Issue 受け入れ条件としては workspace 全体成功が未確認ではなく、実測で未達。

### ドキュメント確認

- `docs/spec.md`: 本Issueは内部 module 分割であり、仕様変更は不要。整合している。
- `README.md`: `github_access` のファイル構造に依存する記述はなく、更新不要。
- `docs/external/` と過去 worklog に旧パス記載は残るが、これは歴史的記録であり今回の修正対象ではない。

### plan / research / docs との不整合

1. 計画・実装概要では `cargo test --workspace` を完了条件としていたが、レビュー実測では未達。
2. 実装概要には `config_test` を「既知失敗で無視」と記しているが、ユーザー要望では無視ではなく通過が要求されている。
3. 計画表では `installation_sync.rs` に sync 定数を置く想定だったが、実装は `cached.rs` から `pub(super)` 定数を参照する形になっている。これは挙動差ではなく設計上の軽微な差分で、問題なし。

### PR/完了結果

- `pr_ready: false`
- 理由: コード分割自体に重大なロジック変更は見当たらないが、Issue に明記された検証条件 `cargo test --workspace` を現行ブランチが満たしておらず、worklog の成功主張も不正確なため。

### 残リスク

- `config_test` 失敗を「既知別件」として扱う運用が続くと、今後も Issue の完了条件と検証記録が乖離しやすい。
- 今回の github_access 分割自体は安全に見えるが、最終マージ判定で必要なのはコード品質だけでなく、記録された検証結果の正確性である。

---

## レビュー指摘修正（2026-05-14）

### 修正内容

レビューで指摘された `config_test` 失敗の修正とworklog記述の正確化を実施。

#### 1. `crates/api/tests/config_test.rs` の修正

**原因**: テストは `std::env::remove_var("DATABASE_URL")` で環境変数を削除するが、`AppConfig::from_env()` 内部で `dotenvy::dotenv()` がプロジェクトルートの `.env` ファイルから `DATABASE_URL` を再読み込みしてしまい、「未設定時はエラー」のアサーションが失敗していた。

**修正**: テスト開始時にカレントディレクトリを `std::env::temp_dir()` に変更し、`dotenvy::dotenv()` が `.env` を見つけられないようにした。テスト終了時に元のディレクトリに復帰。`serial_test::serial` による直列実行のため、ディレクトリ変更は安全。ロジック変更なし。

#### 2. worklog の記述修正

- 実装概要の「全テスト通過（config_testのみ環境変数依存の既知失敗）」→ 正確な表現に修正
- テスト結果の「全テスト通過」→ `config_test` 失敗を明記する表現に修正

### テスト結果

- `cargo fmt --all -- --check`: パス
- `cargo clippy --workspace --all-targets -- -D warnings`: パス
- `cargo test --workspace`: **全テスト通過**（`config_test` 含む）
- `cargo test -p boardflow-api --test config_test`: 1 passed, 0 failed

### 残リスク

- なし。`config_test` の `.env` 読み込み問題は解消済み。

---

## 再レビュー結果（2026-05-14 review agent, follow-up）

### 総評

前回指摘の2点は解消を確認した。`crates/api/tests/config_test.rs` の修正により、プロジェクトルートの `.env` が `AppConfig::from_env()` に再注入される問題は再現しなくなり、`cargo test --workspace` も実測で全パスした。`docs/logs/101/worklog.md` の記述も現状の実測と整合している。

### 調査結果

- `AppConfig::from_env()` は `boardflow_config::load_dotenv()` を通じて `dotenvy::dotenv()` を呼び、カレントディレクトリから親方向に `.env` を探索する実装である。
- `config_test` は開始時にカレントディレクトリを `std::env::temp_dir()` へ移し、終了時に元のディレクトリへ戻しているため、ワークスペース直下の `.env` に影響されない。
- `config_test` は integration test バイナリ内の単一テストであり、`#[serial]` も付いている。今回の `set_current_dir` は process-global ではあるが、現状のテスト構成では他テストへの実害は確認できなかった。

### テスト結果

- `mise exec -- cargo fmt --all -- --check`: pass
- `mise exec -- cargo clippy --workspace --all-targets -- -D warnings`: pass
- `export DATABASE_URL=postgres://boardflow:boardflow@localhost:5432/boardflow && mise exec -- cargo test --workspace`: pass
- `crates/api/tests/config_test.rs`: 1 passed, 0 failed

### レビュー結果

- `pr_ready: true`

### 指摘事項

- なし

### 任意改善

- 将来 `config_test.rs` に複数テストを増やす場合は、カレントディレクトリ復元を panic-safe にする小さな guard を入れると保守性は上がる。ただし現時点では PR blocker ではない。

### テスト不足

- なし。Issue の受け入れ条件として要求された `fmt` / `clippy` / `cargo test --workspace` は再実測で満たした。

### ドキュメント確認

- `docs/spec.md`: 本Issueは内部リファクタリングであり、仕様変更は不要。整合している。
- `docs/logs/101/worklog.md`: 現在のテスト結果と記述の不整合は解消済み。

### plan / research / docs との不整合

- なし。最終状態は「ロジック変更なしの純粋なコード移動・分割」と一致している。

### PR/完了結果

- PR作成可

### 残リスク

- `std::env::set_current_dir` は process-global state なので、将来同一 test binary 内で同種のテストが増えた場合は guard 導入や補助関数化を検討したほうがよい。

---

## ドキュメント確認（2026-05-14 docs agent）

### 総評

Issue #101 は `github_access.rs` の責務別分割に限定された内部リファクタリングであり、仕様・API 契約・運用手順に変更はない。`AGENTS.md`、`docs/spec.md`、`docs/backend/api.md`、`docs/backend/summary.md`、`README.md` の関連記述を確認した範囲では、今回の実装内容と矛盾する記述は見当たらなかった。

### 判定

- `docs_ready: true`

### 必須修正

- なし

### 任意改善

- `docs/external/github-app-octocrab.md` に `crates/api/src/github_access.rs` への旧パス参照が 1 箇所残っている。今回の PR blocker ではないが、次回この外部調査メモを更新する際に `crates/api/src/github_access/mod.rs` もしくは `crates/api/src/github_access/` へ表現を直すと正確性が上がる。

### 不整合のあるドキュメント

- `docs/external/github-app-octocrab.md` — 外部調査メモ内の説明文に旧ファイルパス `crates/api/src/github_access.rs` が残っている。

### 不足しているドキュメント

- なし。内部モジュール分割のみで、公開仕様・利用手順・プロジェクト構造説明の更新は不要。

### 外部調査メモに関する指摘

- 今回の Issue 自体に追加の外部調査は不要。
- 既存メモ `docs/external/github-app-octocrab.md` の旧パス参照は歴史的文脈としては読めるが、現行構成との厳密な一致はしていない。

### 確認結果

- `AGENTS.md`: crate / frontend 単位の構造説明であり、`github_access` の単一ファイル構成には依存していないため更新不要。
- `docs/backend/api.md`: installation repos fallback sync や read API 認可の説明は実装責務と一致しており、分割後も更新不要。
- `docs/backend/summary.md`: `CachedGithubAccessChecker` による可視性判定という責務説明は維持されており、ファイル分割の影響なし。
- `docs/spec.md`: プロダクト仕様書であり、内部モジュール構成に依存する記述なし。
- `README.md`: `github_access` の実装ファイル配置に依存する案内なし。
- `docs/logs/101/worklog.md`: 実装概要、テスト結果、再レビュー結果と整合しており、Issue #101 の経緯として十分な記録になっている。

### PR/完了結果

- docs 観点では PR 作成可

### 更新した作業ログパス

- `docs/logs/101/worklog.md`

---

## PR/完了結果（2026-05-14 pr agent）

### PR作成結果

- **PR URL**: https://github.com/f0reachARR/boardflow/pull/122
- **PR番号**: #122
- **タイトル**: `refactor: split github_access.rs into responsibility-based modules (#101)`
- **ベースブランチ**: `main`
- **ヘッドブランチ**: `refactor/issue-101-split-github-access`
- **Closes**: #101

### 最終コミット履歴

| コミット | 内容 |
|---|---|
| `b8dc6ed` | `refactor: split github_access.rs into responsibility-based modules (#101)` |
| `d025c75` | `docs: update worklog for #101 implementation` |
| `febb167` | `fix: config_test が .env ファイル存在時に失敗する問題を修正` |
| `1fa61fb` | `docs: update worklog #101 with review and docs agent results` |

### 残リスク

- `std::env::set_current_dir` は process-global state なので、将来同一 test binary 内で同種のテストが増えた場合は guard 導入を検討。
- `docs/external/github-app-octocrab.md` の旧パス参照は次回更新時に修正予定。
