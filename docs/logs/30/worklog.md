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
- BoardProject詳細/Runs/Run詳細ページのロジック変更（ブレッドクラム追加は本Issue内で実施）

### 受け入れ条件

1. `timed_out` 状態のBoardProjectに「中断または未完了の可能性」の説明テキストが表示される
2. `detected` 状態のBoardProjectに「初回Run未完了」の説明テキストが表示される
3. ブレッドクラムナビゲーションがネストページ（Repository詳細、BoardProject詳細、Runs一覧、Run詳細）に表示される
4. Repository一覧で failed/timed_out の行に視覚的な強調（背景色）が適用される

### 詳細要件

#### 1. 状態説明テキスト追加

Repository詳細ページの BoardProject 一覧テーブルで:
- `timed_out` Badge の横に `(中断または未完了の可能性)` テキストを表示
- `detected` Badge の横に `(初回Run未完了)` テキストを表示

#### 2. ブレッドクラムナビゲーション

共通コンポーネント `Breadcrumb` を作成し、各ネストページに配置:
- Repository詳細: `Repositories > {owner}/{name}`
- BoardProject詳細: `Repositories > {owner}/{name} > {display_name}`
- Runs一覧: `Repositories > {owner}/{name} > {display_name} > Runs`
- Run詳細: `Repositories > {owner}/{name} > {display_name} > Runs > {run_id短縮}`

#### 3. Repository一覧の視覚強調

現在の `Latest Status` カラムに加え、行背景色で視覚強調:
- `latest_run_status` が `failed` の場合: 行背景色 `red.50`
- `latest_run_status` が `timed_out` の場合: 行背景色 `orange.50`

> **スコープ変更**: 当初計画にあった「failed/timed_out のproject数テキスト表示」「Repository詳細での状態集計サマリ表示」は、現APIレスポンスに project 別状態集計フィールドがないため今回スコープ外とした。行ハイライトによる視覚強調に置き換えた。

### 影響範囲

| ファイル | 変更内容 |
|---------|---------|
| `boardflow/src/components/ui/breadcrumb.tsx` | **新規作成**: ブレッドクラムコンポーネント |
| `boardflow/src/app/(authenticated)/repositories/page.tsx` | Repository一覧: 行背景色による視覚強調（failed→red.50, timed_out→orange.50） |
| `boardflow/src/app/(authenticated)/repositories/[repositoryId]/page.tsx` | Repository詳細: ブレッドクラム追加 + timed_out/detected説明テキスト |
| `boardflow/src/app/(authenticated)/repositories/[repositoryId]/boards/[boardProjectId]/page.tsx` | BoardProject詳細: ブレッドクラム追加 |
| `boardflow/src/app/(authenticated)/repositories/[repositoryId]/boards/[boardProjectId]/runs/page.tsx` | Runs一覧: ブレッドクラム追加 |
| `boardflow/src/app/(authenticated)/repositories/[repositoryId]/boards/[boardProjectId]/runs/[boardRunId]/page.tsx` | Run詳細: ブレッドクラム追加 |

### 設計方針

- **ブレッドクラム**: Server Component として実装。パンくずデータは各ページで props として渡す（API追加不要、各ページが既に必要データを持つ）
- **状態テキスト**: Badge横に `Text` コンポーネントで注釈追加。色は `orange.600`（timed_out）、`gray.500`（detected）
- **視覚強調**: Repository一覧で `latest_run_status` が `failed`/`timed_out` の行に背景色を追加し、問題のあるリポジトリを即座に視認可能にする（API変更不要）

> **スコープ変更メモ**: 当初検討した「project別状態集計表示」は現APIに集計フィールドがないためスコープ外とした。

### テスト観点

- ブレッドクラムコンポーネントの単体テスト（リンク生成の正しさ）
- 各ページでブレッドクラムが正しくレンダリングされること（目視確認 or Playwright）
- timed_out/detected状態のBoardProjectで説明テキストが表示されること
- Repository一覧で latest_run_status が failed/timed_out のとき視覚強調があること
- MVP段階では Playwright smoke test を追加予定（後続 frontend test 導入 Issue で対応）

> **注**: 現時点では frontend テスト基盤（Vitest / Playwright）が未導入のため、検証は `tsc --noEmit` と `next build` に留まっている。自動テスト追加は後続Issueで対応する。

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
5. Repository一覧: latest_run_status 視覚強調（行ハイライト）
6. 動作確認 + コミット

> **注**: 当初計画にあった「Repository詳細での状態集計サマリ表示」は現APIの制約によりスコープ外とした。

## 残リスク

- failed/timed_out のproject数の一覧表示は現APIでは直接サポートしていない（将来のAPI拡張で対応可能）
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

---

## ドキュメント再確認結果 (2026-05-02)

### 総評

- 前回 docs review で指摘された 3 点の必須修正は反映されている
- ただし、`docs/logs/30/worklog.md` 内には今回修正対象とは別の内部不整合が残っており、現時点では docs review を完了扱いにできない
- 実装そのものと大きく食い違う README / `docs/frontend/summary.md` の問題は見当たらないが、作業ログ単体で読んだときにスコープ判断と実装結果の説明が衝突している

### docs_ready

`docs_ready: false`

### 必須修正

1. 実装計画の「影響範囲」で `boardflow/src/app/(authenticated)/repositories/page.tsx` が「Repository一覧: 情報密度向上（failed/timed_out表示）」のままで、同じ作業ログ内の受け入れ条件 4 と実装内容 3 にある「行背景色の視覚強調」と揃っていない。表現を実装済み内容に合わせて統一すること。
2. 実装計画の「設計方針」にある `案A + 案Bの組み合わせを採用: 一覧では既存Badge強調、詳細では集計サマリテキスト表示` が、同じ節のスコープ変更注記 `状態集計サマリ表示は今回スコープ外` と矛盾している。採用判断を現行スコープに合わせて修正すること。
3. 末尾側の「残リスク」に `failed/timed_out のproject数の一覧表示は現APIでは直接サポートしていないため、Repository詳細レベルでの集計に留める` とあるが、今回の実装概要とスコープ外判断では Repository詳細の集計表示は実装していない。未実装の挙動を書かない形に修正すること。

### 任意改善

1. 「テスト観点」には後続 frontend test 導入 Issue への言及が追加されており方向性は明確になった。可能なら参照先 Issue 番号を明記すると追跡しやすい。

### 不整合のあるドキュメント

- `docs/logs/30/worklog.md`

### 不足しているドキュメント

- なし。今回の差分範囲では README や `docs/frontend/summary.md` の追加更新は不要と判断する。

### 外部調査メモに関する指摘

- なし。この Issue は既存 UI 改修の範囲であり、外部調査不要という整理と矛盾は見当たらない。

### PR/完了結果

- docs_ready: false

### 残リスク

- worklog 修正後に、受け入れ条件・実装内容・影響範囲・残リスクの4箇所で同じスコープ説明に揃っているかを再確認する必要がある

### plan / research / docs との不整合

1. 実装計画に記載されていた breadcrumb コンポーネントの単体テスト、各ページでのレンダリング確認を自動テストとしては満たしていない。
2. Issue 本文には「Repository 一覧はカード表示」とあるが、仕様書と実装計画ではテーブル維持に合意済みであり、今回の実装はその合意に沿っている。

---

## ドキュメント確認 (2026-05-02, docs review)

### 対象Issue

- #30 Frontend: Repository一覧・詳細画面実装

### 総評

- 実装コードは `docs/frontend/summary.md` の「一覧は表形式優先」「timed_out 状態の説明表示」「主要導線の可視化」を外していない。
- `docs/spec.md` と README に、今回の UI 品質改善だけを理由に追記が必要な項目は見当たらない。
- ただし `docs/logs/30/worklog.md` 内に、計画時点の記述が実装確定内容と食い違ったまま残っており、単一Issueの記録としては一貫性が不足している。

### 判定

- docs_ready: false

### 必須修正

1. `docs/logs/30/worklog.md` の「非目的」に「BoardProject詳細/Runs/Run詳細ページの変更（別Issueで対応済み）」とあるが、実装概要と実コードではこれらのページに breadcrumb を追加している。スコープ記述を実装済み内容に合わせて修正する必要がある。
2. `docs/logs/30/worklog.md` の受け入れ条件・詳細要件では detected 状態の説明を「初回実行待ち」としている一方、実装内容と実コードは「初回Run未完了」と記録している。どちらを正式文言にするか統一が必要である。
3. `docs/logs/30/worklog.md` の詳細要件と実装順序には「Repository一覧の failed/timed_out 件数表示」「Repository詳細での状態集計サマリ表示」が残っているが、最終実装は Repository 一覧の行ハイライトと状態説明追加に留まっている。未実装要件として残すのか、今回スコープ外として整理するのかを明記すべきである。

### 任意改善

1. `docs/logs/30/worklog.md` の「テスト観点」に書かれている component test / Playwright smoke test は未実施のため、次の frontend test 導入Issueへ参照先を付けると追跡しやすい。

### 不整合のあるドキュメント

- `docs/logs/30/worklog.md`

### 不足しているドキュメント

- なし。`docs/frontend/summary.md`、`docs/spec.md`、README に今回必須の追記は不要。

### 外部調査メモに関する指摘

- なし。Issue #30 は外部調査不要という前提と実装内容が一致している。

### PR/完了結果

- docs_ready: false
- ドキュメント面のブロッカーは `docs/logs/30/worklog.md` の内部不整合のみであり、仕様書本体や README の更新漏れは確認できなかった。

### 残リスク

- 実装自体ではなく記録面の不整合が残るため、このまま PR を作成すると「何を採用し、何を見送ったか」が Issue #30 単体のログから追いにくい。

### 残リスク

1. Runs 一覧と Run 詳細は breadcrumb 表示のために board-project API を追加取得しているが、BoardRun API に親メタデータが含まれていないため現状は妥当。性能影響は限定的。

---

## 再レビュー結果 (2026-05-02)

### 総評

- Issue #30 の前回必須修正だった breadcrumb のアクセシビリティ対応はコード上で反映されている
- `nav aria-label="Breadcrumb"`、`ol/li`、現在位置の `aria-current="page"`、区切りアイコンの `aria-hidden="true"` を確認した
- `pnpm exec tsc --noEmit` と `pnpm exec next build` を再実行し、いずれも成功した
- 追加で PR を止める実装欠陥は見当たらない

### PR/完了結果

- pr_ready: true

### 必須修正

- なし

### 任意改善

1. Repository 一覧の行ハイライトと breadcrumb 内テキスト色は Chakra の固定色トークンに依存しているため、将来テーマ拡張やダークモード対応を行うなら semantic token 化を検討したい。
2. `boardflow/src/components/ui/breadcrumb.tsx` は見た目のためにインライン style を多用している。再利用箇所が増えるなら Chakra コンポーネントか共有 style への寄せ方を検討してよい。

### テスト不足

1. breadcrumb、状態説明テキスト、Repository 一覧の行ハイライトに対する自動テストは引き続き未整備。
2. ただし今回の差分については型チェックと production build は通過している。

### ドキュメント確認

- `docs/frontend/summary.md` の一覧性重視、timed_out 状態の説明表示、主要導線のテスト方針との整合を確認した
- `docs/spec.md`、README を確認し、今回の UI 改善に対する追加の利用者向けドキュメント更新は必須ではないと判断した
- `CONTRIBUTING.md` は引き続きリポジトリ内に存在しなかった

### plan / research / docs との不整合

1. 作業ログ内の計画には Repository 詳細の「状態集計サマリ表示」が残っているが、実装は行ごとの状態説明に留まっている。仕様逸脱ではないが、計画記述としては古くなっている。

### 残リスク

1. frontend の自動テスト基盤が未整備のため、今後 UI 差分が増える場合は主要導線の smoke test を早めに追加したい。
