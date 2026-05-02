# Issue #31: Frontend: BoardProject詳細・BoardRun一覧・詳細画面実装

## 経緯

- Issue #29, #30 がマージ済みで、基本画面（Repository一覧/詳細、BoardProject詳細、BoardRun一覧/詳細）は既に実装済み
- 本Issueでは不足している機能を差分実装する

## ユーザー要望

- docs以下の仕様に基づいてアプリケーションを一通り実装する
- 既存実装がある場合は差分のみ

## 調査結果

### 既存実装状況
- `/repositories` - Repository一覧 ✅
- `/repositories/[repositoryId]` - Repository詳細(BoardProject一覧含む) ✅
- `/repositories/[repositoryId]/boards/[boardProjectId]` - BoardProject詳細 ✅
- `/repositories/[repositoryId]/boards/[boardProjectId]/runs` - BoardRun一覧 ✅
- `/repositories/[repositoryId]/boards/[boardProjectId]/runs/[boardRunId]` - BoardRun詳細 ✅

### 不足機能（API仕様に対する差分）
1. **Findings一覧ページ** - `GET /api/v1/board-runs/{board_run_id}/checks/{check_kind}/findings` の消費画面がない
2. **Diff情報表示** - `GET /api/v1/board-runs/{board_run_id}/diff` の消費画面がない
3. **BoardProject詳細に最近のRun一覧** - 現在はリンクのみで、直近Runのインライン表示がない
4. **schema.d.ts** - findings, diff APIの型定義が未追加

## 計画 (2026-05-02 策定)

### 目的
- API仕様 (docs/backend/api.md) に対して不足しているフロントエンド画面を実装し、ERC/DRC findings の閲覧、Run間 diff の把握、BoardProject詳細での直近Run概要表示を可能にする。

### 非目的
- 新規バックエンドAPI実装（既にIssue #35, #36で実装済み）
- findings の個別詳細画面（MVP範囲外）
- BOM diffの詳細表示（summaryレベルのみ）
- ページネーションのinfinite scroll等（cursor-based paginationのLoad Moreボタンで十分）
- テスト自動化フレームワーク導入（MVP段階ではビルド通過で確認）

### 受け入れ条件
1. `schema.d.ts` に Findings API (`GET /api/v1/board-runs/{board_run_id}/checks/{check_kind}/findings`) と Diff API (`GET /api/v1/board-runs/{board_run_id}/diff`) の型定義が追加されている
2. BoardProject詳細ページに直近5件のRunサマリがインライン表示される
3. Run詳細ページのChecksセクションに各check kindのfindings一覧への「View Findings」リンクがある
4. Findings一覧ページ (`/repositories/[repositoryId]/boards/[boardProjectId]/runs/[boardRunId]/checks/[checkKind]`) が表示される
5. Findings一覧ページでseverityフィルタが動作する
6. Run詳細ページにDiffセクションが表示される（status別のUI表示）
7. `pnpm build` がエラーなく完了する

### 詳細要件

#### Task 1: schema.d.ts 型定義追加
- `paths` に以下を追加:
  - `"/api/v1/board-runs/{board_run_id}/checks/{check_kind}/findings"` (GET)
    - パスパラメータ: `board_run_id`, `check_kind`
    - クエリパラメータ: `limit?`, `cursor?`, `severity?`
    - レスポンス: `PaginatedResponse<Finding>`
  - `"/api/v1/board-runs/{board_run_id}/diff"` (GET)
    - パスパラメータ: `board_run_id`
    - レスポンス: `DiffResponse`
    - 404: APIError (diff未作成時)
- 新規interface:
  - `Finding` (id, severity, rule_code, title, message, subject_kind, subject_ref, sheet_path, pcb_layer, pos_mm)
  - `PosMm` ({x: number, y: number} | null)
  - `DiffResponse` (board_run_id, base_board_run_id, status, summary, metadata, error_message, created_at)
  - `DiffSummary` (file_changes, bom_changes, checks, artifacts)
  - `DiffFileChanges` (added, removed, changed, unchanged)
  - `DiffBomChanges` (added, removed, changed)
  - `DiffChecksChange` (status_change, error_delta, warning_delta)
  - `DiffArtifactsChange` (added, removed, changed)

#### Task 2: BoardProject詳細ページにRecent Runs追加
- ファイル: `boardflow/src/app/(authenticated)/repositories/[repositoryId]/boards/[boardProjectId]/page.tsx`
- 変更内容:
  - `board-runs` API を `limit=5` で呼び出し（既存の board-projects API と並行）
  - 「View All Runs」ボタンの上に直近Run一覧テーブルを表示
  - 表示カラム: Status(badge), Commit(7文字), Branch, ERC, DRC, Created
  - 各行はRun詳細へのリンク
  - runs が0件の場合は「No runs yet.」表示

#### Task 3: Run詳細ページのChecksに「View Findings」リンク追加
- ファイル: `boardflow/src/app/(authenticated)/repositories/[repositoryId]/boards/[boardProjectId]/runs/[boardRunId]/page.tsx`
- 変更内容:
  - 各check cardの下部に「View Findings →」リンクを追加
  - リンク先: `/repositories/${repositoryId}/boards/${boardProjectId}/runs/${boardRunId}/checks/${check.kind}`
  - error_count + warning_count > 0 の場合のみリンク表示

#### Task 4: Findings一覧ページ新規作成
- ファイル: `boardflow/src/app/(authenticated)/repositories/[repositoryId]/boards/[boardProjectId]/runs/[boardRunId]/checks/[checkKind]/page.tsx`
- 内容:
  - Server Component
  - Breadcrumb: Repositories > owner/name > project > Runs > runId短縮 > checkKind(大文字)
  - APIリクエスト: `GET /api/v1/board-runs/{board_run_id}/checks/{check_kind}/findings?limit=50`
  - 表示:
    - ヘッダー: 「ERC Findings」または「DRC Findings」
    - Severity フィルタ (All / Error / Warning / Notice) - URLクエリパラメータ `severity` で制御
    - Findingsテーブル: Severity(badge), Rule Code, Title, Message, Location(sheet_path or pcb_layer)
    - 空の場合: 「No findings.」メッセージ
  - cursor pagination: has_more が true の場合「Load More」ボタン（Client Componentで実装するか、リンクで次ページ表示）
  - MVP: 初回ロードの50件表示のみ（has_moreの場合に「More results available」テキスト表示）

#### Task 5: Run詳細ページにDiffセクション追加
- ファイル: `boardflow/src/app/(authenticated)/repositories/[repositoryId]/boards/[boardProjectId]/runs/[boardRunId]/page.tsx`
- 変更内容:
  - Diff API を呼び出し（404の場合はセクション非表示）
  - Checksセクションの直後にDiffセクション挿入
  - status別の表示:
    - `ready`: サマリ情報をカード形式で表示（File Changes, BOM Changes, Checks Delta, Artifacts）
    - `no_baseline`: 「This is the first run. No baseline available for comparison.」
    - `unavailable`: 「Diff data is unavailable for this run.」
    - `failed`: 「Diff generation failed: {error_message}」
  - base_board_run_id がある場合は比較元Runへのリンクを表示

### 影響範囲
- `boardflow/src/lib/api/schema.d.ts` - 型追加
- `boardflow/src/app/(authenticated)/repositories/[repositoryId]/boards/[boardProjectId]/page.tsx` - Recent Runs追加
- `boardflow/src/app/(authenticated)/repositories/[repositoryId]/boards/[boardProjectId]/runs/[boardRunId]/page.tsx` - Findings link + Diff section
- `boardflow/src/app/(authenticated)/repositories/[repositoryId]/boards/[boardProjectId]/runs/[boardRunId]/checks/[checkKind]/page.tsx` - 新規

### 設計方針
- 全ページ Server Components（Next.js App Router）
- API呼び出しは `openapi-fetch` の `createServerClient()` 経由
- スタイリングは Chakra UI v3 のコンポーネント使用（既存パターン踏襲）
- severity フィルタは `searchParams` で受け取り、Server Componentでフィルタ済みデータ取得
- Diff API の404はtry-catchで捕捉し、セクション非表示にするだけ（エラーにしない）
- 新しいClient Componentは作らない（MVPのため全部Server Component + <Link>）

### テスト観点
- `pnpm build` でTypeScriptコンパイル成功（型整合性確認）
- `pnpm lint` でESLintエラーなし
- 各ページが正しいファイルパスに配置されている（Next.js routing確認）
- schema.d.tsの型がAPI仕様と一致（手動レビュー）

### ドキュメント更新対象
- `docs/logs/31/worklog.md` - 本計画・実装経過
- `docs/frontend/summary.md` - 実装完了後にページ一覧更新（必要に応じて）

### 実装要否
- `implementation_required`

### 実装順序
1. Task 1: schema.d.ts 型定義追加（他タスクの前提）
2. Task 2: BoardProject詳細ページ修正
3. Task 3: Run詳細ページ修正（Findings link + Diff section）
4. Task 4: Findings一覧ページ新規作成
5. 全体ビルド確認 (`pnpm build`)

### 未解決の疑問
- なし（既存コードパターンとAPI仕様から十分に判断可能）

### 残リスク
- Diff APIが404を返す場合のServer Component内でのハンドリング: fetch失敗時にコンポーネント全体がエラーにならないよう、個別にtry-catchする設計
- Findings の cursor pagination: MVP では初回50件のみ表示し、Load More は今後対応可能な構造にしておく

### ブランチ
- `feat/31-board-detail-run-pages` (作成済み、mainから分岐)

## 実装内容 (2026-05-02)

### Task 1: schema.d.ts 型定義追加
- `Finding` 型追加（severity, rule_code, title, message, location情報）
- `DiffResponse` 型追加（status, summary, error_message）
- `DiffSummary` 型追加（file_changes, bom_changes, checks, artifacts）
- パス定義追加: `/api/v1/board-runs/{board_run_id}/checks/{check_kind}/findings`, `/api/v1/board-runs/{board_run_id}/diff`

### Task 2: BoardProject詳細ページ拡充
- `board-runs` APIを並列呼び出しで直近5件取得
- 「View All Runs」ボタンの前に Recent Runs テーブル追加（Status, Commit, Branch, ERC, DRC, Created列）
- statusColor, checkBadge ヘルパー関数追加

### Task 3: BoardRun詳細ページ拡充
- 各CheckカードにFindingsページへの「View N findings」リンク追加（error+warning > 0 の場合のみ）
- Diff API呼び出し追加（Promise.allで並列、errorの場合はnull）
- 「Changes from Baseline」セクション追加（ready/no_baseline/failed/unavailable表示分岐）

### Task 4: Findings一覧ページ新規作成
- ファイル: `boardflow/src/app/(authenticated)/repositories/[repositoryId]/boards/[boardProjectId]/runs/[boardRunId]/checks/[checkKind]/page.tsx`
- Breadcrumb, severity filter (All/Errors/Warnings/Notices), テーブル表示
- severityColor, locationText ヘルパー関数

### Task 5: ビルド確認
- `pnpm build` 成功（全ルート含む新規 checks/[checkKind] ルート確認済み）

## テスト結果
- Next.js ビルド成功（TypeScript型チェック・ESLint含む）
- テスト自動化は本Issue対象外（Issue指示による）

## レビュー結果
(レビュー後に追記)

## ドキュメント確認
- API型定義は docs/backend/api.md の仕様と整合
- フロントエンドページ構成は docs/frontend/summary.md の想定と整合

## PR/完了結果
(PR後に追記)

## 残リスク
- Findings API / Diff API がバックエンド未実装の場合、画面はエラー/非表示のgraceful degradation（設計通り）
- Diff の checks 表示は status_change 文字列をそのまま表示（フォーマット調整は後続で可能）
- Findings の cursor pagination: MVP では全件表示（limit指定なし）、Load More は今後対応可能な構造

## 残リスク
(実装後に追記)
