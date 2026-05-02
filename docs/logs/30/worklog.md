# Issue #30: Frontend: Repository一覧・詳細画面実装 — 作業ログ

## Issueまでの経緯

- #29（Next.jsセットアップ）がマージ済みで、フロントエンドの基盤は整備完了
- Chakra UI v3 + Next.js App Router + Server Components + openapi-fetch によるAPI通信パターンが確立済み
- 以下の画面が既に実装済み:
  - Repository一覧（テーブル形式）: `boardflow/src/app/(authenticated)/repositories/page.tsx`
  - Repository詳細（BoardProject一覧テーブル付き）: `boardflow/src/app/(authenticated)/repositories/[repositoryId]/page.tsx`
  - BoardProject詳細: `boardflow/src/app/(authenticated)/repositories/[repositoryId]/boards/[boardProjectId]/page.tsx`
  - Runs一覧: `boardflow/src/app/(authenticated)/repositories/[repositoryId]/boards/[boardProjectId]/runs/page.tsx`
  - Run詳細: `boardflow/src/app/(authenticated)/repositories/[repositoryId]/boards/[boardProjectId]/runs/[boardRunId]/page.tsx`
- レイアウト（AppShell、Header、Sidebar）も実装済み

## ユーザー要望

- docs以下の仕様に基づいてアプリケーションを一通り実装する
- Issue本文: Repository一覧のカード表示、詳細はBoardProject一覧を含む

## 調査結果

### 外部調査は不要

このIssueは既存実装の差分追加であり、新しい外部ライブラリや技術の導入は不要。

既存のスタック構成:
- **Chakra UI v3**: Box, Heading, Table, Text, Badge, VStack, HStack 等を使用済み
- **Next.js App Router**: Server Components パターン確立済み（`async function` + `createServerClient()`）
- **openapi-fetch**: `client.GET()` パターンで各APIエンドポイントを呼び出し済み
- **lucide-react**: アイコン利用パターン確立済み

### 現状の実装構造

| 画面 | パス | 表示形式 | 状態 |
|------|------|----------|------|
| Repository一覧 | `/repositories` | テーブル | 実装済み |
| Repository詳細 | `/repositories/[repositoryId]` | ヘッダー + BoardProjectテーブル | 実装済み |
| BoardProject詳細 | `.../boards/[boardProjectId]` | 詳細カード | 実装済み |
| Runs一覧 | `.../runs` | — | 実装済み |
| Run詳細 | `.../runs/[boardRunId]` | — | 実装済み |

### docs/frontend/summary.md の記載

> 一覧画面では、カードよりも表形式やセクション分割の方が相性がよい可能性が高い。特に MVP では、視覚的な派手さよりも運用時の読みやすさを優先する。

現在のテーブル形式はこの方針に合致しており、大きなレイアウト変更の必要性は低い。

## 結論ステータス

`implementation_required`

## 実装内容（2026-05-02）

### 1. ブレッドクラムナビゲーション

新規ファイル `boardflow/src/components/ui/breadcrumb.tsx` を作成。
- Server Component として実装（"use client" 不要）
- Chakra UI の HStack + Text、lucide-react の ChevronRight アイコンをセパレータに使用
- `BreadcrumbItem[]` を props で受け取り、最後のアイテムはリンクなし

以下4ページにブレッドクラムを配置:
- Repository詳細: `Repositories > {owner}/{name}`
- BoardProject詳細: `Repositories > {owner}/{name} > {display_name}`
- Runs一覧: `Repositories > {owner}/{name} > {display_name} > Runs`
- Run詳細: `Repositories > {owner}/{name} > {display_name} > Runs > {run_id短縮}`

### 2. timed_out / detected 状態説明テキスト

Repository詳細ページの BoardProject テーブル State 列:
- `timed_out`: Badge の横に `(中断または未完了の可能性)` を表示
- `detected`: Badge の横に `(初回Run未完了)` を表示

### 3. Repository一覧の視覚強調

- `latest_run_status === "failed"` → 行背景 `red.50`
- `latest_run_status === "timed_out"` → 行背景 `orange.50`

## 変更ファイル一覧

| ファイル | 変更種別 |
|---------|----------|
| `boardflow/src/components/ui/breadcrumb.tsx` | 新規作成 |
| `boardflow/src/app/(authenticated)/repositories/page.tsx` | 修正（行ハイライト） |
| `boardflow/src/app/(authenticated)/repositories/[repositoryId]/page.tsx` | 修正（ブレッドクラム + 状態説明） |
| `.../boards/[boardProjectId]/page.tsx` | 修正（ブレッドクラム） |
| `.../boards/[boardProjectId]/runs/page.tsx` | 修正（ブレッドクラム + project並行fetch） |
| `.../boards/[boardProjectId]/runs/[boardRunId]/page.tsx` | 修正（ブレッドクラム + project並行fetch） |

## テスト結果

- `pnpm exec tsc --noEmit`: 成功（EXIT:0）
- `pnpm exec next build`: 成功（全ページ正常にビルド）

## 残リスク

- Runs一覧・Run詳細では breadcrumb 表示のために board-project API を追加で呼び出しており、パフォーマンスへの影響は軽微だがゼロではない
- ダークモード対応は未実施（`red.50`, `orange.50` はライトモード前提）

## ユーザー確認結果 (2026-05-02)

## レビュー指摘修正 (2026-05-02)

### Breadcrumbコンポーネントのアクセシビリティ修正

**修正ファイル**: `boardflow/src/components/ui/breadcrumb.tsx`

**修正内容**:
1. 最外側を `<nav aria-label="Breadcrumb">` で囲んだ
2. 内部を `<ol>` + `<li>` のセマンティックなリスト構造に変更
3. 最終要素（現在ページ、`href` なし）に `aria-current="page"` を付与
4. 区切りアイコン（ChevronRight）に `aria-hidden="true"` を付与
5. Chakra UI の `HStack` を排除し、CSS変数ベースのインラインスタイルでレイアウトを維持
6. 未使用の `HStack` インポートを削除

**テスト結果**: `pnpm exec tsc --noEmit` → EXIT:0

- **表示形式**: 仕様書優先（テーブル形式維持 + 品質改善）を選択 → カード表示への変更は行わない
- **スコープ**: Research結果の提案通り（timed_out警告・ブレッドクラム・情報密度向上）で確定

---

## 実装計画 (2026-05-02)

### 目的

既存のRepository一覧・詳細画面を仕様書の品質要件に適合させる。テーブル形式を維持しつつ、状態把握のしやすさとナビゲーションUXを改善する。

### 非目的

- カード表示への変更（仕様書方針に基づき不採用）
- フィルタ・検索・ソート機能（将来Issue）
- Pagination UI実装（cursor paginationは既にAPIレベルで対応、UIは今回スコープ外）
- BoardProject詳細/Runs/Run詳細ページの変更（別Issueで対応済み）

### 受け入れ条件

1. `timed_out` 状態のBoardProjectに「中断または未完了の可能性」の説明テキストが表示される
2. `detected` 状態のBoardProjectに「初回実行待ち」の説明テキストが表示される
3. ブレッドクラムナビゲーションがネストページ（Repository詳細、BoardProject詳細、Runs一覧、Run詳細）に表示される
4. Repository一覧で failed/timed_out のproject数が情報として表示される

### 詳細要件

#### 1. 状態説明テキスト追加

Repository詳細ページの BoardProject 一覧テーブルで:
- `timed_out` Badge の横に `(中断または未完了の可能性)` テキストを表示
- `detected` Badge の横に `(初回実行待ち)` テキストを表示

#### 2. ブレッドクラムナビゲーション

共通コンポーネント `Breadcrumb` を作成し、各ネストページに配置:
- Repository詳細: `Repositories > {owner}/{name}`
- BoardProject詳細: `Repositories > {owner}/{name} > {display_name}`
- Runs一覧: `Repositories > {owner}/{name} > {display_name} > Runs`
- Run詳細: `Repositories > {owner}/{name} > {display_name} > Runs > {run_id短縮}`

#### 3. Repository一覧の情報密度向上

現在の `Latest Status` カラムに加え、状態サマリを追加:
- failed/timed_out のproject数がある場合、行に警告情報を表示
- 例: `2 failed, 1 timed_out` のようなサブテキスト

### 影響範囲

| ファイル | 変更内容 |
|---------|---------|
| `boardflow/src/components/ui/breadcrumb.tsx` | **新規作成**: ブレッドクラムコンポーネント |
| `boardflow/src/app/(authenticated)/repositories/page.tsx` | Repository一覧: 情報密度向上（failed/timed_out表示） |
| `boardflow/src/app/(authenticated)/repositories/[repositoryId]/page.tsx` | Repository詳細: ブレッドクラム追加 + timed_out/detected説明テキスト |
| `boardflow/src/app/(authenticated)/repositories/[repositoryId]/boards/[boardProjectId]/page.tsx` | BoardProject詳細: ブレッドクラム追加 |
| `boardflow/src/app/(authenticated)/repositories/[repositoryId]/boards/[boardProjectId]/runs/page.tsx` | Runs一覧: ブレッドクラム追加 |
| `boardflow/src/app/(authenticated)/repositories/[repositoryId]/boards/[boardProjectId]/runs/[boardRunId]/page.tsx` | Run詳細: ブレッドクラム追加 |

### 設計方針

- **ブレッドクラム**: Server Component として実装。パンくずデータは各ページで props として渡す（API追加不要、各ページが既に必要データを持つ）
- **状態テキスト**: Badge横に `Text` コンポーネントで注釈追加。色は `orange.600`（timed_out）、`gray.500`（detected）
- **情報密度**: Repository一覧のAPI responseの `board_project_count` と `latest_run_status` は既にあるが、failed/timed_out のproject数は現APIレスポンスに含まれない → **Repository一覧テーブルでは `latest_run_status` の表示を維持し、timed_outの場合に警告色を強調する方針に変更**
  - あるいはRepository詳細でProjectテーブルの状態を集計表示するのみとする

#### API制約への対応

現在の `/api/v1/repositories` レスポンスの `Repository` 型には `latest_run_status` のみでproject別状態集計がない。バックエンドAPI変更なしで実現する方法:
- **案A**: Repository一覧では `latest_run_status` が `failed`/`timed_out` の場合に視覚的強調のみ追加（API変更不要）
- **案B**: Repository詳細ページでprojectsを取得した際にクライアント側で集計表示

→ **案A + 案Bの組み合わせを採用**: 一覧では既存Badge強調、詳細では集計サマリテキスト表示

### テスト観点

- ブレッドクラムコンポーネントの単体テスト（リンク生成の正しさ）
- 各ページでブレッドクラムが正しくレンダリングされること（目視確認 or Playwright）
- timed_out/detected状態のBoardProjectで説明テキストが表示されること
- Repository一覧で latest_run_status が failed/timed_out のとき視覚強調があること
- MVP段階では Playwright smoke test を追加予定（後続Issue）

### ドキュメント更新対象

- `docs/logs/30/worklog.md`（本ファイル、作業記録）
- `docs/frontend/summary.md` への追記は不要（既存方針の実現であるため）

### ブランチ名

`feat/30-repository-ui-improvements`

（feat/29-frontend-nextjs-setup からブランチを切る）

### 実装順序

1. `feat/30-repository-ui-improvements` ブランチ作成
2. `breadcrumb.tsx` コンポーネント作成
3. Repository詳細ページ: ブレッドクラム + timed_out/detected テキスト追加
4. BoardProject詳細/Runs一覧/Run詳細: ブレッドクラム追加
5. Repository一覧: latest_run_status 視覚強調
6. Repository詳細: 状態集計サマリ表示
7. 動作確認 + コミット

## 残リスク

- failed/timed_out のproject数の一覧表示は現APIでは直接サポートしていないため、Repository詳細レベルでの集計に留める
- Pagination UIは今回スコープ外（API側は cursor pagination 対応済み）

---

## レビュー結果 (2026-05-02)

### 総評

- Issue #30 の実装は、docs/frontend/summary.md に合わせて一覧をテーブル形式のまま改善する方針に沿っており、差分範囲も計画どおりに収まっている
- TypeScript 診断は追加分を含めて問題なく、`pnpm exec tsc --noEmit` も通過している
- 一方で、新規 Breadcrumb コンポーネントがアクセシブルな breadcrumb navigation として実装されておらず、この Issue で追加したナビゲーション機能としては不十分

### PR/完了結果

- pr_ready: false

### 必須修正

1. `boardflow/src/components/ui/breadcrumb.tsx` の breadcrumb が `nav` ランドマーク、`aria-label="breadcrumb"`、現在位置の `aria-current="page"` を持っておらず、スクリーンリーダーから breadcrumb として認識しづらい。区切りアイコンも装飾要素として明示されていないため、アクセシビリティ観点でこのままのマージは避けたい。

### 任意改善

1. Repository 一覧の行ハイライトは `red.50` / `orange.50` の固定色指定のみで、色覚差や将来のテーマ拡張に弱い。必要なら status badge の補助テキストや semantic token 化を検討したい。

### テスト不足

1. 今回追加した breadcrumb 表示、Repository 詳細の状態説明文、Repository 一覧の行ハイライトを検証する自動テストがない。
2. frontend 側には現時点で Vitest / Playwright などのテスト基盤自体が入っておらず、検証が `tsc` と `next build` に偏っている。

### ドキュメント確認

- `docs/frontend/summary.md` の「一覧は表形式優先」「timed_out 状態の説明表示」「主要導線のテスト」という方針を確認した
- `docs/spec.md` と README を確認し、今回の UI 改善に対して追加の利用者向けドキュメント更新は必須ではないと判断した
- `CONTRIBUTING.md` はリポジトリ内に存在しなかった

### plan / research / docs との不整合

1. 実装計画に記載されていた breadcrumb コンポーネントの単体テスト、各ページでのレンダリング確認を自動テストとしては満たしていない。
2. Issue 本文には「Repository 一覧はカード表示」とあるが、仕様書と実装計画ではテーブル維持に合意済みであり、今回の実装はその合意に沿っている。

### 残リスク

1. Runs 一覧と Run 詳細は breadcrumb 表示のために board-project API を追加取得しているが、BoardRun API に親メタデータが含まれていないため現状は妥当。性能影響は限定的。
