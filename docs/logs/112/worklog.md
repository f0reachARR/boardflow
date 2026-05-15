# Issue #112: ArtifactViewer のタブ選択と KiCanvas fallback 判定を分離する

## 経緯

ArtifactViewerSection (~230行) に viewer tab の表示判定、default tab 判定、KiCanvas fallback 判定、URL refresh query、実際の描画が全て集中している。UI変更と viewer 選択ルールの変更が同じファイルに影響し、テストが難しい構造になっている。

## ユーザー要望

- viewer 選択ロジックを純粋関数として切り出し、テスト可能にする
- refresh query を hook として分離する
- renderViewerContent を小さな表示コンポーネントへ分ける
- 残っている console.log を削除する
- 挙動変更は避け、純粋なコード移動・抽出に留める

## 調査結果

### 現在のファイル構造
```
boardflow/src/components/artifact-viewer/
  artifact-viewer-section.tsx  (対象: ~230行)
  download-list.tsx
  ibom-viewer.tsx
  kicanvas-viewer.tsx
  pdf-viewer.tsx
  svg-viewer.tsx
  viewer-status-message.tsx
```

### 現在のコード分析

1. **TAB_DEFINITIONS** (L13-19): 5つのタブ定義定数
2. **useQuery** (L34-48): viewer sources の定期 refresh (4分間隔)
3. **visibleTabs** (L52-68): TAB_DEFINITIONS からの表示タブ計算 + KiCanvas fallback 判定
4. **defaultTab** (L85-89): 最初の available/partial タブ選択
5. **renderViewerContent** (L130-229): 4ケースの switch 文描画関数
6. **console.log** (L179): `console.log('kicanvasPcbSources', kicanvasPcbSources)` — 要削除

### 使用箇所
- `run-detail-content.tsx` が `ArtifactViewerSection` を import・使用 (props: viewers, expiresAt, boardRunId)

### 型依存
- `ViewerEntry` = `components['schemas']['ViewerStatus']` (downloads?, iframe_url?, primary?, sources?, status)
- `ViewerSource` = `components['schemas']['ViewerSource']` (artifact_id?, artifact_type?, kind?, name?, source_path?, url?)
- `ViewerSourceKind` = `'project' | 'schematic' | 'board'`

## 計画

### 実装計画 (2026-05-15)

**ステータス**: `implementation_required`

---

#### 目的

ArtifactViewerSection からタブ選択ロジック、データ取得フック、描画コンポーネントを分離し、各責務を独立したファイルに配置する。KiCanvas fallback 判定を関数名で仕様表現し、テスト可能にする。

#### 非目的

- 挙動変更（console.log 削除を除く）
- 新機能追加
- テストファイルの新規作成（typecheck/lint/build で担保）
- 既存コンポーネント (KiCanvasViewer, PdfViewer 等) の変更

#### 受け入れ条件

1. ArtifactViewerSection がタブ UI と構成に集中している（~60行以下）
2. KiCanvas fallback の仕様が `canUseKicanvasFallback`, `getKicanvasSources` 等の関数名で表現されている
3. `console.log('kicanvasPcbSources', ...)` が削除されている
4. `pnpm typecheck` が通る
5. `pnpm lint` が通る
6. `pnpm build` が通る
7. `run-detail-content.tsx` の import に変更なし（公開 API 維持）

#### 詳細要件

##### 1. viewer-selection.ts（新規: 純粋関数モジュール）

| export | 引数 | 戻り値 | 責務 |
|---|---|---|---|
| `TabDefinition` (type) | — | `{ key: string; label: string }` | タブ定義の型 |
| `TAB_DEFINITIONS` (const) | — | `TabDefinition[]` | 5つのタブ定義 |
| `getKicanvasSources` | `(allViewers: Record<string, ViewerEntry>, kind: 'schematic' \| 'board'): ViewerSource[]` | `ViewerSource[]` | kicanvas viewer から指定 kind のソースを取得。schematic の場合は kind='project' も含む |
| `canUseKicanvasFallback` | `(allViewers: Record<string, ViewerEntry>, kind: 'schematic' \| 'board'): boolean` | `boolean` | 指定 kind の KiCanvas ソースが存在するか判定 |
| `getVisibleViewerTabs` | `(viewers: Record<string, ViewerEntry>): TabDefinition[]` | `TabDefinition[]` | 表示すべきタブを TAB_DEFINITIONS から計算 |
| `getDefaultViewerTab` | `(visibleTabs: TabDefinition[], viewers: Record<string, ViewerEntry>): string` | `string` | デフォルト選択タブの key を返す |

##### 2. use-viewer-sources.ts（新規: カスタムフック）

| export | 引数 | 戻り値 | 責務 |
|---|---|---|---|
| `useViewerSources` | `(initialViewers, initialExpiresAt, boardRunId)` | `{ viewers, isRefreshError }` | viewer sources の定期 refresh (4分間隔) |

##### 3. viewer-content.tsx（新規: 描画コンポーネント群）

| export | props | 責務 |
|---|---|---|
| `ViewerContent` | `{ name: string; viewer: ViewerEntry; allViewers: Record<string, ViewerEntry> }` | name に応じた描画ディスパッチ |
| `SchematicContent` | `{ viewer: ViewerEntry; allViewers: Record<string, ViewerEntry> }` | schematic タブの描画 (KiCanvas fallback + PDF + Download) |
| `PcbPreviewContent` | `{ viewer: ViewerEntry; allViewers: Record<string, ViewerEntry> }` | pcb_preview タブの描画 (KiCanvas + SVG)。console.log なし |
| `IbomContent` | `{ viewer: ViewerEntry }` | ibom タブの描画 |
| `GenericDownloadContent` | `{ name: string; viewer: ViewerEntry }` | bom/fabrication 等の汎用ダウンロード描画 |

##### 4. artifact-viewer-section.tsx（変更: UIシェルに集中）

残すもの:
- `ArtifactViewerSectionProps` interface
- `ArtifactViewerSection` コンポーネント（タブ UI のみ）

削除するもの:
- `TAB_DEFINITIONS` → viewer-selection.ts へ
- `useQuery` ブロック → use-viewer-sources.ts へ
- `visibleTabs` 計算 → `getVisibleViewerTabs()` 呼び出しに
- `defaultTab` 計算 → `getDefaultViewerTab()` 呼び出しに
- `renderViewerContent` 関数 → viewer-content.tsx の `ViewerContent` へ
- `console.log` → 削除
- 不要になった import (useQuery, 各 viewer コンポーネント等)

#### 影響範囲

- `boardflow/src/components/artifact-viewer/` 配下のみ
- `run-detail-content.tsx` の import は変更なし（ArtifactViewerSection の公開インターフェース不変）
- 挙動変更なし

#### 設計方針

- **純粋関数分離**: viewer-selection.ts は React に依存しない。将来的にユニットテスト追加が容易。
- **フック分離**: useViewerSources は TanStack Query の詳細を隠蔽し、コンポーネントはデータの取得方法を知らない。
- **コンポーネント分離**: 各 viewer の描画ロジックは ViewerContent 経由でディスパッチ。個々のコンテンツコンポーネントは独立。
- **'use client' ディレクティブ**: use-viewer-sources.ts と viewer-content.tsx に付与。viewer-selection.ts は純粋関数のため不要。

#### テスト観点

- `pnpm typecheck` — 型エラーなし
- `pnpm lint` — Biome lint パス（console.log 削除確認含む）
- `pnpm build` — ビルド成功
- ブラウザ動作確認は CI / 手動テストで担保（本 Issue のスコープ外）

#### ドキュメント更新対象

- なし（内部リファクタリングのため）

#### 作業順序

1. `feature/issue-112-viewer-selection-split` ブランチ作成
2. `viewer-selection.ts` 作成 — TAB_DEFINITIONS, getKicanvasSources, canUseKicanvasFallback, getVisibleViewerTabs, getDefaultViewerTab
3. `use-viewer-sources.ts` 作成 — useViewerSources フック
4. `viewer-content.tsx` 作成 — ViewerContent, SchematicContent, PcbPreviewContent, IbomContent, GenericDownloadContent
5. `artifact-viewer-section.tsx` 変更 — 新モジュールからの import に切り替え、旧コード削除、console.log 削除
6. `pnpm typecheck && pnpm lint && pnpm build` 実行
7. 修正があれば対応
8. コミット・PR 作成

#### 実装要否

`implementation_required`

#### 未解決の疑問

なし。Issue の要件は明確であり、ユーザー判断が必要な曖昧点はない。

---

## 実装 (2026-05-15)

### 作業ブランチ
`refactor/issue-112-viewer-selection-split` (main から作成)

### 新規ファイル
1. **`viewer-selection.ts`** — 純粋関数モジュール (88行)
   - `TabDefinition` 型、`TAB_DEFINITIONS` 定数
   - `getKicanvasSources()` — KiCanvas ソースのフィルタリング
   - `canUseKicanvasFallback()` — KiCanvas fallback 判定
   - `getVisibleViewerTabs()` — 表示タブのフィルタリング
   - `getDefaultViewerTab()` — デフォルトタブ選択

2. **`use-viewer-sources.ts`** — カスタムフック (35行)
   - `useViewerSources()` — useQuery によるビューアソースの取得・リフレッシュ

3. **`viewer-content.tsx`** — ビューアコンテンツコンポーネント (112行)
   - `ViewerContent` — name で dispatch するメインコンポーネント
   - `SchematicContent` — schematic 描画 (KiCanvas fallback 対応)
   - `PcbPreviewContent` — PCB 描画 (KiCanvas fallback 対応、console.log 削除)
   - `IbomContent` — iBOM iframe 描画
   - `GenericDownloadContent` — ダウンロード系汎用描画

### 変更ファイル
4. **`artifact-viewer-section.tsx`** — 108行 → 約60行に縮小
   - TAB_DEFINITIONS, renderViewerContent, useQuery 直接利用、個別ビューアimportを削除
   - `getVisibleViewerTabs()`, `getDefaultViewerTab()`, `useViewerSources()`, `<ViewerContent>` に委譲

### テスト結果
- `pnpm typecheck`: OK
- `pnpm lint`: OK (Biome import sort / format 準拠)
- `pnpm build`: OK

### 受け入れ条件チェック
1. ✅ ArtifactViewerSection がタブ UI と構成に集中 (~60行)
2. ✅ KiCanvas fallback が `canUseKicanvasFallback`, `getKicanvasSources` で表現
3. ✅ `console.log('kicanvasPcbSources', ...)` 削除済み
4. ✅ `pnpm typecheck` パス
5. ✅ `pnpm lint` パス
6. ✅ `pnpm build` パス
7. ✅ `run-detail-content.tsx` の import 変更なし

### 残リスク
- `viewer-selection.ts` の純粋関数にユニットテストは未追加（将来追加の余地あり）
- ブラウザでの挙動確認は手動/E2E テストで別途実施が必要

#### 残リスク

- renderViewerContent 内の描画ロジックは現状テストがないため、移動時の見落としリスクはあるが、typecheck + build で型レベルの整合性は担保される。

## レビュー結果 (2026-05-15)

### 実施内容

- Issue #112 の対象差分 (`boardflow/src/components/artifact-viewer/`) を main 比較でレビュー
- `docs/spec.md`, `docs/frontend/summary.md`, `docs/backend/api.md`, `docs/external/kicanvas.md`, `docs/external/kicanvas-embed-api.md` を確認
- `pnpm typecheck`, `pnpm lint` を再実行して受け入れ条件を検証

### 総評

責務分離自体は概ね狙いどおりで、`ArtifactViewerSection` は UI シェルに集中しており、`console.log` も除去されている。一方で、KiCanvas fallback 判定を共通関数へ寄せた際に、viewer tab の表示条件が main から変わっている箇所が 1 件あるため、このままでは「純粋なコード移動・抽出」とは言えない。

### 必須修正

1. `getVisibleViewerTabs()` の KiCanvas fallback 条件を main と同じに戻す
   - 現状の `canUseKicanvasFallback()` は `kicanvas.status !== 'missing'` かつ relevant source があれば `true` になりうるため、`kicanvas` viewer が `partial` / `failed` / `skipped` でも schematic / pcb_preview タブが表示される可能性がある
   - main では「`kicanvas` viewer が `available` のときだけ、static viewer が `missing` / `failed` でも tab を表示する」実装だったため、抽出後に表示条件が広がっている
   - backend でも `kicanvas` viewer は `partial` / `failed` / `skipped` を取りうるため、実現しない状態ではない

### 任意改善

- `viewer-selection.ts` の純粋関数は今回の主目的なので、少なくとも `getVisibleViewerTabs()` と `getDefaultViewerTab()` の単体テストを追加すると、今後の仕様変更で同じズレを防ぎやすい

### テスト結果

- `pnpm typecheck`: OK
- `pnpm lint`: OK
- `pnpm build`: 未再実行（実装者報告では OK）

### ドキュメント確認

- `docs/frontend/summary.md` と `docs/backend/api.md` の「KiCanvas は補助 preview、静的 preview は fallback を維持」という方針とは整合
- ただし Issue 本文の「挙動変更は避ける」という要件とは、上記 tab 表示条件の変更が不整合

### PR/完了結果

- `pr_ready: false`

### 残リスク

- pure function 化されたロジックに回帰テストがないため、今回のような条件差分が将来も混入しやすい

## 再レビュー結果 (2026-05-15)

### 実施内容

- Issue #112 の再修正として `getVisibleViewerTabs()` の現実装を確認
- current の [boardflow/src/components/artifact-viewer/viewer-selection.ts](boardflow/src/components/artifact-viewer/viewer-selection.ts#L50) と main の元実装を比較
- [docs/spec.md](docs/spec.md), [docs/frontend/summary.md](docs/frontend/summary.md#L88), [docs/logs/112/worklog.md](docs/logs/112/worklog.md) を再確認
- `pnpm lint`, `pnpm typecheck`, `pnpm build` をローカルで再実行

### 総評

今回の修正は「前回レビューで指摘された条件」を反映しているが、main の元実装には戻っていない。結果として、`schematic` / `pcb_preview` が `missing` / `failed` かつ KiCanvas が `available` でないケースで、元コードではタブを表示して状態メッセージを見せていたのに、現在はタブ自体を非表示にしてしまう。これは「挙動変更は避ける」という Issue 要件に反する。

### 必須修正

1. `getVisibleViewerTabs()` の `return false` を除去し、main と同じフォールスルー挙動へ戻す
   - current: [boardflow/src/components/artifact-viewer/viewer-selection.ts](boardflow/src/components/artifact-viewer/viewer-selection.ts#L58) は、`schematic` / `pcb_preview` が `missing` / `failed` のとき、`kicanvas.status === 'available'` かつ relevant source がない限り `false` を返している
   - main: 元実装は `kicanvas.status === 'available'` の場合だけ relevant source を確認し、それ以外は [artifact-viewer-section.tsx](boardflow/src/components/artifact-viewer/artifact-viewer-section.tsx#L24) の流れに戻って最終的に `true` になる
   - そのため current は、`missing` / `failed` タブに付いていた状態表示と導線を失わせている

2. タブ表示条件と描画条件の不一致を解消する
   - [boardflow/src/components/artifact-viewer/viewer-selection.ts](boardflow/src/components/artifact-viewer/viewer-selection.ts#L19) の `getKicanvasSources()` は元どおり `status === 'missing'` だけを除外しており、`failed` / `skipped` / `partial` でも source があれば描画側では KiCanvas を使いうる
   - 一方で [boardflow/src/components/artifact-viewer/viewer-selection.ts](boardflow/src/components/artifact-viewer/viewer-selection.ts#L58) のタブ表示側は `status === 'available'` を要求しており、表示判定と描画判定がさらに乖離している
   - 元実装では、少なくとも `missing` / `failed` タブは表示され、内部で状態表示または fallback 描画に進めていた

### 任意改善

- [boardflow/src/components/artifact-viewer/viewer-selection.ts](boardflow/src/components/artifact-viewer/viewer-selection.ts#L50) に対して、少なくとも以下のケースを固定する単体テストを追加したい
  - `schematic.status = 'missing'`, `kicanvas.status = 'missing'` でもタブは残る
  - `schematic.status = 'failed'`, `kicanvas.status = 'failed'`, board/schematic source ありでも元挙動どおりタブが残る
  - `kicanvas.status = 'available'` だが relevant source なしなら該当タブは消える

### テスト結果

- `pnpm lint`: OK
- `pnpm typecheck`: OK
- `pnpm build`: OK
- build 時に Next.js の `middleware` 廃止予定警告は出るが、本 Issue の差分とは無関係

### ドキュメント確認

- [docs/frontend/summary.md](docs/frontend/summary.md#L88) の「viewer 単位の status に応じて表示、fallback、理由表示を切り替える」という方針と比べると、current は `missing` / `failed` 理由表示の導線を削っている
- Issue #112 の「挙動変更は避ける」という要件とも不整合

### PR/完了結果

- `pr_ready: false`

### 残リスク

- 前回レビューの論点を反映した修正が、そのまま main との差分を固定化しているため、単体テストがない限り同種の認識ずれが再発しやすい

## 3回目レビュー結果 (2026-05-15)

### 実施内容

- current の [boardflow/src/components/artifact-viewer/viewer-selection.ts](boardflow/src/components/artifact-viewer/viewer-selection.ts#L50) を main の元実装と再比較
- [boardflow/src/components/artifact-viewer/viewer-content.tsx](boardflow/src/components/artifact-viewer/viewer-content.tsx#L1) と [boardflow/src/components/artifact-viewer/artifact-viewer-section.tsx](boardflow/src/components/artifact-viewer/artifact-viewer-section.tsx#L1) を確認し、分離後の表示条件が元ロジックと一致するかを確認
- [docs/spec.md](docs/spec.md), [docs/frontend/summary.md](docs/frontend/summary.md#L88), [docs/backend/api.md](docs/backend/api.md#L912) と整合を確認
- `console.log` の残存を `boardflow/src/components/artifact-viewer/` 配下と `boardflow/src/` 配下で確認

### 総評

前回の blocking 指摘だった `getVisibleViewerTabs()` の挙動差分は解消されている。現行の [boardflow/src/components/artifact-viewer/viewer-selection.ts](boardflow/src/components/artifact-viewer/viewer-selection.ts#L50) は、main の元実装と同じフォールスルーを保持しており、`schematic` / `pcb_preview` が `missing` / `failed` かつ `kicanvas` が `available` でない場合でもタブを残して状態表示へ進める。今回確認した範囲では、Issue #112 の「挙動変更なしで責務分離」という要件を満たしている。

### レビュー結果

- blocking finding なし
- `pr_ready: true`

### 確認結果

1. 前回指摘の挙動差分
   - 解消済み
   - [boardflow/src/components/artifact-viewer/viewer-selection.ts](boardflow/src/components/artifact-viewer/viewer-selection.ts#L50) は、main の旧 [boardflow/src/components/artifact-viewer/artifact-viewer-section.tsx](boardflow/src/components/artifact-viewer/artifact-viewer-section.tsx#L24) と同じ条件分岐になっている

2. console.log の残存
   - Issue 対象の frontend source では残っていない
   - `boardflow/src/components/artifact-viewer/` 配下では検出なし
   - `boardflow/src/` 全体でも今回の実装コードに該当する `console.log` は検出なし
   - `boardflow/public/vendor/kicanvas/kicanvas.js` 内には vendor 由来の `console.log` が残るが、Issue #112 の変更対象外

### 任意改善

- [boardflow/src/components/artifact-viewer/viewer-selection.ts](boardflow/src/components/artifact-viewer/viewer-selection.ts#L1) は純粋関数として切り出されたので、`getVisibleViewerTabs()` と `getDefaultViewerTab()` の単体テストを追加すると今後の回帰防止に有効

### テスト確認

- 実装者報告の `pnpm typecheck`, `pnpm lint`, `pnpm build` はいずれも成功
- このレビューでは追加の実行はしていない

### ドキュメント確認

- [docs/frontend/summary.md](docs/frontend/summary.md#L88) の fallback 方針と整合
- [docs/backend/api.md](docs/backend/api.md#L912) の「`kicanvas` が `missing` / `failed` / `skipped` でも静的 fallback を提供する」方針と矛盾なし
- Issue #112 の「挙動変更は避ける」という要件とも整合

### 残リスク

- 抽出した pure function 群に専用テストがないため、将来の条件変更で同種の回帰が起きる余地は残る

## ドキュメント確認結果 (2026-05-15)

### 実施内容

- [boardflow/src/components/artifact-viewer/artifact-viewer-section.tsx](boardflow/src/components/artifact-viewer/artifact-viewer-section.tsx#L1) と新規分離された [boardflow/src/components/artifact-viewer/viewer-selection.ts](boardflow/src/components/artifact-viewer/viewer-selection.ts#L1), [boardflow/src/components/artifact-viewer/use-viewer-sources.ts](boardflow/src/components/artifact-viewer/use-viewer-sources.ts#L1), [boardflow/src/components/artifact-viewer/viewer-content.tsx](boardflow/src/components/artifact-viewer/viewer-content.tsx#L1) を確認
- [docs/frontend/summary.md](docs/frontend/summary.md#L86), [docs/spec.md](docs/spec.md#L1091), [docs/technology.md](docs/technology.md#L51) の ArtifactViewer / viewer-sources / KiCanvas fallback 関連記述を確認
- Issue #112 の変更が公開仕様ではなく内部リファクタリングに留まっているかを `main...HEAD` 差分で確認

### 総評

Issue #112 は ArtifactViewer の責務を `artifact-viewer-section.tsx` から純粋関数、カスタムフック、表示コンポーネントへ分離した内部リファクタリングであり、公開 API、画面仕様、viewer-sources の契約、KiCanvas fallback 方針には変更がない。確認した範囲では、既存ドキュメントは新しいファイル構造と矛盾しておらず、追加更新は不要。

### docs_ready

- `docs_ready: true`

### 確認結果

1. [docs/frontend/summary.md](docs/frontend/summary.md#L86)
   - ArtifactViewer について記載しているのは viewer-sources API、KiCanvas fallback、status ごとの表示方針といった振る舞いレベルのみ
   - `artifact-viewer-section.tsx` にロジックが集約されている前提や旧ファイル構造への言及はなく、今回の分離後も整合している

2. [docs/spec.md](docs/spec.md#L1091)
   - viewer-sources API が返す契約、KiCanvas を補助 preview とし PDF/SVG/iBOM fallback を維持する方針は不変
   - 今回の実装は UI 内部の責務整理のみで、仕様変更に当たらないため更新不要

3. [docs/technology.md](docs/technology.md#L51)
   - 技術方針は `viewer-sources API + KiCanvas + PDF/SVG fallback` という採用構成を示すレベルで、コンポーネント分割の詳細は扱っていない
   - 新しいファイル構造と矛盾なし

4. その他のドキュメント
   - README / CONTRIBUTING に今回の内部リファクタリングで更新すべき利用手順や開発手順の変更はない
   - `docs/external/` は本 Issue で追加参照が必要な外部トピックはなく、更新必須の調査メモもなし

### 必須修正

- なし

### 任意改善

- なし

### 不整合のあるドキュメント

- なし

### 不足しているドキュメント

- なし

### 外部調査メモに関する指摘

- なし（本 Issue は外部仕様調査を伴わない内部リファクタリング）

### PR/完了結果

- `docs_ready: true`

### 残リスク

- ドキュメント上の残課題はないが、実装面では pure function 群に専用テストがないため将来の条件変更時に回帰余地は残る
