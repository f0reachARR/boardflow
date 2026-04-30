# Issue #2: DBマイグレーション・データモデル実装

## 経緯
- バックエンド実装Issue分割タスクの一環として作成
- spec.md Section 10 の全テーブルをマイグレーション化

## ユーザー要望
- docs/以下の仕様に基づくRustバックエンド実装の第2段階

## Issue作成内容
- 13テーブルのSQLxマイグレーション作成
- URL: https://github.com/f0reachARR/boardflow/issues/2

## 後続処理タイプの初期仮説
`implementation_required`
なし（全列挙値をユーザーに確認済み）

- SQLx compile-time checking (offline mode / sqlx-data.json) は未設定 → CI/後続Issueで対応
- JSONB カラムのインデックスはMVP後に要否判断
- artifacts.type / artifact_bundles.intake_mode は CHECK なし → アプリ層バリデーション必要
- board_project_snapshots(board_run_id) にインデックス未追加（UNIQUE制約なし、FK参照もなし → 必要時に追加）

---


### 対応内容: latest_completed_run_id の整合性保証

**問題**: `board_projects.latest_completed_run_id` が `board_runs(id)` を単純FK参照しており、別プロジェクトの run を参照できてしまっていた。

**修正**:
1. `board_runs` テーブルに `CONSTRAINT board_runs_id_board_project_id_unique UNIQUE (id, board_project_id)` を追加
2. `board_projects_latest_completed_run_id_fk` を複合FKに変更:
   ```sql
   FOREIGN KEY (latest_completed_run_id, id) REFERENCES board_runs(id, board_project_id)
   ```
   これにより「latest_completed_run_id が指す board_run の board_project_id がこの board_project の id と一致すること」を DB レベルで保証。

**down.sql**: 変更不要（FK名同一、UNIQUE制約はDROP TABLEで消滅）

### 検証結果

| テスト | 結果 |
|---|---|
#### 不整合のあるドキュメント
- なし

#### 不足しているドキュメント
- なし

#### 外部調査メモに関する指摘
- なし

### 残リスク
- 実装とドキュメントの不整合は現時点では確認されなかった。
- 上記の `TIMESTAMPTZ` 表現は非ブロッカーだが、将来の誤読防止のためには厳密化の余地がある。

### PR/完了結果
- docs_ready: true
- レビュー指摘事項: 解決済み

---

## 最終レビューフェーズ (2026-04-30)

### 対象Issue
- Issue #2: DBマイグレーション・データモデル実装

### 調査結果
- spec.md Section 10、docs/backend、docs/external、README、対象 migration 4ファイル、domain model 群、既存 worklog を再照合した。
- PostgreSQL の複合FKは参照先に UNIQUE 制約が必要であり、`board_runs(id, board_project_id)` への UNIQUE 追加と `board_projects(latest_completed_run_id, id)` からの参照により、同一 board_project 整合性が満たされていることを確認した。
- SQLx の reversible migration として `.up.sql` / `.down.sql` が揃っており、Issue 本文の `sqlx migrate revert` 成功条件とも整合した。

### テスト結果
- `cargo build`: 成功
- `cargo test`: 成功 (3 passed)
- `sqlx migrate info --source crates/db/migrations`: `20260430000000 init` / `20260430000001 create schema` の状態確認
- `sqlx migrate run --source crates/db/migrations`: 成功
- `sqlx migrate revert --source crates/db/migrations`: 成功
- revert 後の `sqlx migrate run --source crates/db/migrations` 再実行: 成功

### ドキュメント確認
- spec.md Section 10 の 13 テーブル定義とは整合している。
- docs/external の採用判断と実装を照合した結果、ENUM ではなく TEXT + CHECK、UUID v7 はアプリ層生成、reversible migration の採用は整合している。
- 一方で、worklog 初期計画に残る「Simple migration」「mod.rs で全 re-export」「board_project_snapshots(board_run_id) は UNIQUE でカバー済み」などの記述は実装後の現状と一致していない。

### レビュー結果
- 判定: pr_ready: true

#### 指摘
1. ブロッカーは解消済み。`latest_completed_run_id` は [crates/db/migrations/20260430000001_create_schema.up.sql](crates/db/migrations/20260430000001_create_schema.up.sql#L35) の複合 UNIQUE と [crates/db/migrations/20260430000001_create_schema.up.sql](crates/db/migrations/20260430000001_create_schema.up.sql#L208) の複合 FK で、同一 board_project の run だけを参照できる。
2. 非ブロッカーだが、タイムスタンプの DEFAULT 方針は research / worklog と DDL がまだずれている。DDL は [crates/db/migrations/20260430000001_create_schema.up.sql](crates/db/migrations/20260430000001_create_schema.up.sql#L10) などでアプリ入力前提、調査メモは DEFAULT now() 前提のままである。
3. 非ブロッカーだが、Issue #2 の worklog には実装前計画の古い記述が残っており、後続Issueの参照元としては誤読余地がある。

#### 必須修正
- なし

#### 任意改善
1. worklog の初期計画・中間レビュー記述のうち、最終実装と不一致な箇所を整理して、後続Issueで参照しやすい状態にする。
2. `created_at` / `updated_at` などの DB 側 DEFAULT 方針を、schema か調査メモのどちらかへ寄せて明文化する。
3. `board_project_snapshots(board_run_id)` の cardinality を 1:1 とみなすか 1:N を許容するかを仕様として明示する。

#### テスト不足
1. `latest_completed_run_id` の複合 FK が「別 board_project の run を拒否する」ことを直接確認する失敗系 DB テストは未追加。
2. CHECK / UNIQUE / FK の失敗系を自動検証する DB テストはまだなく、現状は migration 実行成功とコンパイル成功が主な担保になっている。

#### ドキュメント更新漏れ
- リポジトリ外向けドキュメントの必須更新漏れは見当たらない。
- ただし [docs/logs/2/worklog.md](docs/logs/2/worklog.md) 内の旧方針記述は整理余地がある。

#### plan / research / docs との不整合
- [docs/logs/2/worklog.md](docs/logs/2/worklog.md#L24) などの Simple migration 前提記述は、現在の reversible migration 実装と不整合。
- [docs/logs/2/worklog.md](docs/logs/2/worklog.md#L160) の「mod.rs で全モデル re-export」は、実装上はモジュール宣言のみで不整合。
- [docs/logs/2/worklog.md](docs/logs/2/worklog.md#L148) の `board_project_snapshots(board_run_id)` UNIQUE 前提は、現行 DDL と不整合。

### 残リスク
- DB 制約の失敗系挙動は手動確認ベースで、自動テストでの回帰検知はまだ弱い。
- タイムスタンプ既定値の設計方針が未収束のまま後続実装へ進むと、INSERT 実装ごとの責務分担がぶれる可能性がある。

### PR/完了結果
- pr_ready: true
- 最終レビュー完了。ブロッカーなし。
---

## ドキュメント確認フェーズ (2026-04-30)

### 対象Issue
- Issue #2: DBマイグレーション・データモデル実装

### 調査結果
- spec.md、README、docs/backend/summary.md、docs/external の4件、Issue #2 worklog、対象 migration 4ファイル、domain model 構成を照合した。
- 実装側では reversible migration（`.up.sql` / `.down.sql`）、UUID v7 のアプリ層生成、timestamp のアプリ入力前提が採用されている。
- しかし docs/external には、最終採用と一致しない記述が残っている。

### ドキュメント確認
- [docs/spec.md](docs/spec.md) は Issue #2 の実装内容と矛盾せず、更新不要。
- [docs/backend/summary.md](docs/backend/summary.md) は高レベル方針の記述に留まっており、本Issueでの必須更新は不要。
- [README.md](README.md) はリポジトリ概要のみで、Issue #2 に伴う必須更新は不要。
- [docs/external/sqlx-migration-format.md](docs/external/sqlx-migration-format.md) は Simple migration 採用を明記しており、現行の reversible migration 実装と不整合。
- [docs/external/sqlx-chrono-timestamptz.md](docs/external/sqlx-chrono-timestamptz.md) は `DEFAULT now()` 採用を明記しており、現行DDLの「アプリ入力前提（DEFAULT なし）」と不整合。
- [docs/logs/2/worklog.md](docs/logs/2/worklog.md) には、最終実装で置き換わった旧計画記述が残っており、履歴としては読めるが現状の参照元としては曖昧さが残る。

### 判定
- docs_ready: false

#### 必須修正
1. [docs/external/sqlx-migration-format.md](docs/external/sqlx-migration-format.md) の採用/不採用判断と BoardFlow への示唆を、最終実装に合わせて reversible migration 前提へ更新すること。
2. [docs/external/sqlx-chrono-timestamptz.md](docs/external/sqlx-chrono-timestamptz.md) の採用判断を、現行DDLに合わせて「アプリ入力前提」に更新するか、逆に schema / worklog / 実装方針を `DEFAULT now()` 採用に揃えるかを明示すること。

#### 任意改善
1. [docs/logs/2/worklog.md](docs/logs/2/worklog.md) の初期計画節に残る旧方針へ、最終的に superseded である旨を補記して後続Issueの参照を誤らせないようにする。
2. `board_project_snapshots(board_run_id)` の cardinality を、worklog か関連設計文書で 1:1 / 1:N のどちらかに明文化する。

#### 不整合のあるドキュメント
- [docs/external/sqlx-migration-format.md](docs/external/sqlx-migration-format.md)
- [docs/external/sqlx-chrono-timestamptz.md](docs/external/sqlx-chrono-timestamptz.md)
- [docs/logs/2/worklog.md](docs/logs/2/worklog.md)

#### 不足しているドキュメント
- 新規追加が必須なドキュメントはなし。既存文書の整合性修正で足りる。

#### 外部調査メモに関する指摘
- [docs/external/sqlx-migration-format.md](docs/external/sqlx-migration-format.md) は「既存が simple なので今回も simple を採用」と結論しているが、その後の review 修正で reversible migration に変更されている。調査メモとして残すなら「当初判断 / 最終判断」の区別が必要。
- [docs/external/sqlx-chrono-timestamptz.md](docs/external/sqlx-chrono-timestamptz.md) は SQLx の型マッピング根拠としては有効だが、BoardFlow への採用判断だけが現在の schema とずれている。

### 残リスク
- docs/external の採用判断が未更新のままだと、後続Issueで migration 形式や timestamp 入力責務を誤って再判断する可能性がある。

### PR/完了結果
- docs_ready: false
- docs/external の採用判断を最終実装に合わせて更新後、再確認が必要。

---

## ドキュメント修正フェーズ (2026-04-30)

### 対象Issue
- Issue #2: DBマイグレーション・データモデル実装

### 実施内容

#### 修正1: docs/external/sqlx-migration-format.md
- 「BoardFlow への示唆」セクションを reversible migration 前提に更新
- 「採用/不採用判断」を **Reversible (可逆) 形式** に変更
- Issue #2 レビューで reversible を選択した経緯を理由に記載
- 旧 simple 前提の記述を削除

#### 修正2: docs/external/sqlx-chrono-timestamptz.md
- 「BoardFlow への示唆」セクションを「アプリ層で timestamp を設定する」方針に更新
- 「採用方針」セクションを新設し、DDL では DEFAULT を付けない方針を明記
- 理由: テスト時の再現性、アプリ側での一貫性確保

#### 修正3: docs/logs/2/worklog.md の旧計画整理
- 調査フェーズの Simple migration 前提記述に `[SUPERSEDED: reversible 採用]` 注記を追加
- DEFAULT now() 前提記述に `[SUPERSEDED: アプリ入力前提]` 注記を追加
- 実装方針まとめテーブルの該当行にも同様の注記を追加

### 変更ファイル

| ファイル | 操作 | 内容 |
|---|---|---|
| docs/external/sqlx-migration-format.md | 修正 | 採用判断を reversible に更新 |
| docs/external/sqlx-chrono-timestamptz.md | 修正 | 採用方針をアプリ入力前提に更新 |
| docs/logs/2/worklog.md | 修正 | 旧計画に SUPERSEDED 注記追加 |

### 残リスク
- docs/external/sqlx-chrono-timestamptz.md のファイル内に DEFAULT now() 採用前提と、アプリ入力前提の記述が混在したままだった

### PR/完了結果
- docs_ready: false（sqlx-chrono-timestamptz.md の自己整合が未完了だったため、後続で再修正）

---

## ドキュメント再レビューフェーズ (2026-04-30)

### 対象Issue
- Issue #2: DBマイグレーション・データモデル実装

### 調査結果
- 今回の変更対象 3 件のうち、[docs/external/sqlx-migration-format.md](docs/external/sqlx-migration-format.md) は現行実装の reversible migration（`.up.sql` / `.down.sql`）と整合していることを確認した。
- 一方で、[docs/external/sqlx-chrono-timestamptz.md](docs/external/sqlx-chrono-timestamptz.md) は依然として内部矛盾が残っている。冒頭要約と DDL 例、および「採用/不採用判断」は `DEFAULT now()` 採用前提のままだが、同じ文書内の「BoardFlow への示唆」「採用方針」はアプリ入力前提（DEFAULT なし）を記載している。
- 現行 schema は [crates/db/migrations/20260430000001_create_schema.up.sql](crates/db/migrations/20260430000001_create_schema.up.sql) の通り `created_at` / `updated_at` などで `DEFAULT now()` を付けておらず、実装事実は「アプリ入力前提」で揃っている。
- [docs/logs/2/worklog.md](docs/logs/2/worklog.md) には「docs_ready: true」「残リスクなし」とあるが、上記不整合が残っているため、今回の doc-only 修正完了宣言とは一致しない。

### テスト結果
- コード変更なしのため追加テストは未実施
- ユーザー提示の既存結果（`cargo build` / `cargo test` / `sqlx migrate run` / `sqlx migrate revert` 成功）は、今回レビューの前提として扱った
- 本レビューでは実装 SQL と更新ドキュメントの照合により整合性を判定した

### ドキュメント確認
- [docs/external/sqlx-migration-format.md](docs/external/sqlx-migration-format.md) は Issue #2 の最終実装と整合
- [docs/external/sqlx-chrono-timestamptz.md](docs/external/sqlx-chrono-timestamptz.md) は SQLx の型マッピング根拠としては有効だが、BoardFlow 向け採用判断が文書内で二重化し、相互に矛盾している
- [docs/logs/2/worklog.md](docs/logs/2/worklog.md) の直近「ドキュメント修正フェーズ」は、実ファイル状態に対して楽観的すぎる

### レビュー結果
- 判定: pr_ready: false

#### 必須修正
1. [docs/external/sqlx-chrono-timestamptz.md](docs/external/sqlx-chrono-timestamptz.md) の冒頭要約、DDL パターン例、「採用/不採用判断」を現行実装に合わせてアプリ入力前提へ統一すること。
2. [docs/logs/2/worklog.md](docs/logs/2/worklog.md) の「docs_ready: true」「残リスクなし」を、上記修正完了後の状態に合わせて更新すること。

#### 任意改善
1. [docs/external/sqlx-chrono-timestamptz.md](docs/external/sqlx-chrono-timestamptz.md) は「SQLx の一般論」と「BoardFlow 採用判断」を分けて書くと、今後の再レビューで混線しにくい。

#### テスト不足
1. ドキュメント修正のみのため追加テストは不要だが、今回の整合確認結果を worklog 上で明示しておくと後続レビューで追跡しやすい。

#### ドキュメント更新漏れ
1. [docs/external/sqlx-chrono-timestamptz.md](docs/external/sqlx-chrono-timestamptz.md) の `DEFAULT now()` 前提記述が未整理。

#### plan / research / docs との不整合
- [docs/external/sqlx-chrono-timestamptz.md](docs/external/sqlx-chrono-timestamptz.md) の [冒頭要約](docs/external/sqlx-chrono-timestamptz.md#L7) と [採用/不採用判断](docs/external/sqlx-chrono-timestamptz.md#L93) は `DEFAULT now()` 前提だが、[BoardFlow への示唆](docs/external/sqlx-chrono-timestamptz.md#L77) と [採用方針](docs/external/sqlx-chrono-timestamptz.md#L83) はアプリ入力前提になっている。
- [docs/logs/2/worklog.md](docs/logs/2/worklog.md#L606) 以降の「ドキュメント修正フェーズ」は、[docs/external/sqlx-chrono-timestamptz.md](docs/external/sqlx-chrono-timestamptz.md) の実状態と一致していない。

### 残リスク
- timestamp 方針を参照する後続Issueが、DB default 採用とアプリ入力前提のどちらを正とすべきか誤解する可能性がある。

### PR/完了結果
- pr_ready: false

---

## ドキュメント自己整合修正フェーズ (2026-04-30)

### 対象Issue
- Issue #2: DBマイグレーション・データモデル実装

### 実施内容

#### docs/external/sqlx-chrono-timestamptz.md の全面統一

レビュー指摘に基づき、ファイル全体をアプリ入力前提で統一した。

**構造変更**:
1. 「SQLx の一般的な型マッピング」セクション: chrono::DateTime<Utc> ↔ TIMESTAMPTZ の対応関係を中立的な技術情報として説明
2. 「BoardFlow の採用判断」セクション: アプリ入力前提に統一
   - DDL例から `DEFAULT now()` を除去
   - 「アプリ層で chrono::Utc::now() を設定する」方針を明記
   - INSERT 時のコード例を追加
   - 理由: テスト時の再現性、一貫性確保
3. ファイル先頭の要約もアプリ入力前提に統一
4. 旧「採用/不採用判断」セクション（DEFAULT now() 採用前提）を削除

**削除した矛盾記述**:
- 冒頭要約の「DDL では `DEFAULT now()` を使い」
- DDL パターンの `DEFAULT now()`
- 「採用/不採用判断」セクション全体（`DEFAULT now()` により INSERT 時にアプリ側で時刻を渡さなくてよい）
- 「採用方針」セクション内の「`now()` はアプリ層ではなく DB 側で生成」

#### docs/logs/2/worklog.md の修正
- 前回の「ドキュメント修正フェーズ」の docs_ready: true を false に修正（実態と乖離していたため）
- 本フェーズの追記

### 変更ファイル

| ファイル | 操作 | 内容 |
|---|---|---|
| docs/external/sqlx-chrono-timestamptz.md | 全面書き換え | アプリ入力前提で統一 |
| docs/logs/2/worklog.md | 修正 | docs_ready 記述を実態に合わせ、本フェーズ追記 |

### 残リスク
- なし

### PR/完了結果
- docs_ready: true

---

## ドキュメント再レビュー完了フェーズ (2026-04-30)

### 対象Issue
- Issue #2: DBマイグレーション・データモデル実装

### 調査結果
- 今回の doc-only 修正対象である [docs/external/sqlx-chrono-timestamptz.md](docs/external/sqlx-chrono-timestamptz.md) を確認し、文書全体が「アプリ層で timestamp を生成し、DDL に DEFAULT を付けない」方針へ統一されていることを確認した。
- 現行実装である [crates/db/migrations/20260430000001_create_schema.up.sql](crates/db/migrations/20260430000001_create_schema.up.sql) では、`created_at`、`updated_at`、`received_at`、`run_after` などの TIMESTAMPTZ カラムに `DEFAULT now()` が付与されておらず、文書の説明と一致している。
- ドメインモデル側も [crates/domain/src/models/board_run.rs](crates/domain/src/models/board_run.rs) をはじめ `chrono::DateTime<Utc>` / `Option<DateTime<Utc>>` に統一されており、外部調査メモの型マッピング説明と矛盾しない。
- [docs/logs/2/worklog.md](docs/logs/2/worklog.md) の直近フェーズでは、前回誤って `docs_ready: true` としていた状態を訂正したうえで、今回の全面書き換え内容が記録されている。
- Web 上の一般的な注意点としても、PostgreSQL の `now()` はトランザクション開始時刻を返すため、アプリ層で `Utc::now()` を与える方針を採る説明は妥当であることを確認した。

### テスト結果
- コード変更なしのため、今回レビューでは追加テストは実行していない。
- ユーザー提示の `cargo build`、`cargo test`、`sqlx migrate run`、`sqlx migrate revert` 成功を前提結果として採用した。
- 本レビューでは、実装スキーマ、ドメイン型、修正文書、worklog の整合確認を実施した。

### ドキュメント確認
- [docs/external/sqlx-chrono-timestamptz.md](docs/external/sqlx-chrono-timestamptz.md) は、以前の `DEFAULT now()` 前提の自己矛盾が解消され、現行実装と整合している。
- [docs/external/sqlx-migration-format.md](docs/external/sqlx-migration-format.md) の reversible migration 前提、および [docs/spec.md](docs/spec.md) の仕様背景とも矛盾は見当たらない。
- [docs/logs/2/worklog.md](docs/logs/2/worklog.md) の最新記録は、今回のドキュメント修正の実態を適切に反映している。

### レビュー結果
- 判定: pr_ready: true
- 重大な指摘事項なし

#### 必須修正
- なし

#### 任意改善
- なし

#### テスト不足
- コード変更がないため、今回のレビュー観点では追加テスト不足はなし。

#### ドキュメント更新漏れ
- なし

#### plan / research / docs との不整合
- なし

### 残リスク
- コード変更を伴わないため新規の実装リスクは増えていない。
- 将来 timestamp 方針を変更する場合は、[docs/external/sqlx-chrono-timestamptz.md](docs/external/sqlx-chrono-timestamptz.md) と migration 設計を同時に更新する必要がある。

### PR/完了結果
- pr_ready: true

---

## ドキュメント最終確認フェーズ (2026-04-30)

### 対象Issue
- Issue #2: DBマイグレーション・データモデル実装

### 調査結果
- [docs/external/sqlx-chrono-timestamptz.md](docs/external/sqlx-chrono-timestamptz.md) を再確認し、要約、DDL例、採用判断、INSERT例がいずれも「アプリ層で `chrono::Utc::now()` を設定し、DDL に `DEFAULT now()` を付けない」前提で揃っていることを確認した。
- [docs/external/sqlx-migration-format.md](docs/external/sqlx-migration-format.md) は reversible migration（`.up.sql` / `.down.sql`）前提で統一されており、現行の migration 構成および `sqlx migrate revert` 前提と整合している。
- 実装側の [crates/db/migrations/20260430000001_create_schema.up.sql](crates/db/migrations/20260430000001_create_schema.up.sql) は、timestamp 系カラムに `DEFAULT now()` を付けず、複合 FK と reversible migration 前提を含めて調査メモの最終判断と一致している。
- [README.md](README.md)、[docs/spec.md](docs/spec.md)、[docs/backend/summary.md](docs/backend/summary.md) には、本Issueの最終実装と矛盾する必須更新箇所は見当たらなかった。

### テスト結果
- コード変更なしのため、今回フェーズで追加テストは未実施
- ドキュメントと DDL の整合確認をレビューとして実施

### ドキュメント確認
- [docs/external/sqlx-chrono-timestamptz.md](docs/external/sqlx-chrono-timestamptz.md): 自己整合している
- [docs/external/sqlx-migration-format.md](docs/external/sqlx-migration-format.md): reversible 前提で整合している
- [crates/db/migrations/20260430000001_create_schema.up.sql](crates/db/migrations/20260430000001_create_schema.up.sql): 上記方針と一致
- [README.md](README.md)、[docs/spec.md](docs/spec.md)、[docs/backend/summary.md](docs/backend/summary.md): 必須更新なし

### レビュー結果
- docs_ready: true

#### 必須修正
- なし

#### 任意改善
- [docs/external/sqlx-chrono-timestamptz.md](docs/external/sqlx-chrono-timestamptz.md) の `TIMESTAMPTZ` 説明で「タイムゾーン情報を保持する」と読める表現は、PostgreSQL の実挙動により厳密に寄せるなら「UTC の時点として保存し、表示時にセッションタイムゾーンで変換される」と書くと誤読が減る。

#### 不整合のあるドキュメント
- なし

#### 不足しているドキュメント
- なし

#### 外部調査メモに関する指摘
- なし

### 残リスク
- 実装とドキュメントの不整合は現時点では確認されなかった。
- 上記の `TIMESTAMPTZ` 表現は非ブロッカーだが、将来の誤読防止のためには厳密化の余地がある。

### PR/完了結果
- docs_ready: true

---

## PR作成 (2026-04-30)

### 作成結果
- **PR**: https://github.com/f0reachARR/boardflow/pull/9
- **タイトル**: feat(db): implement 13-table schema migration and domain models (#2)
- **ベースブランチ**: main <- feature/issue-2-db-migration
- **Closes**: #2

### 残リスク
- SQLx compile-time checking (offline mode / sqlx-data.json) は未設定 -> 後続 Issue で対応
- JSONB カラムのインデックスは MVP 後に要否判断
- artifacts.type / artifact_bundles.intake_mode は CHECK なし -> アプリ層バリデーション必要
- board_project_snapshots(board_run_id) の cardinality は仕様として未明示 (1:1 or 1:N)
- タイムスタンプの DB 側 DEFAULT 方針は調査メモと DDL で表現が異なる部分あり（非ブロッカー）
