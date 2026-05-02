# Issue #32: Frontend Artifact Viewer 実装 - 調査ログ

## 基本情報

- **Issue ID**: #32
- **Title**: Frontend: Artifact Viewer実装（PDF/SVG/iBOM/Download）
- **担当フェーズ**: research
- **日時**: 2026-05-02

## Issueまでの経緯

- Artifact Proxy API (#18) が実装済み前提
- Backend の viewer-sources API がviewer単位でURL（短命トークン付き）を返す仕様
- iBOMはartifact domain上で配信し、app domainとは分離する方針
- KiCanvasは将来対応。今回はPDF/SVG/iBOM/Downloadに集中

## ユーザー要望

docs以下の仕様に基づいてArtifact Viewerフロントエンドを一通り実装する。

## 調査対象

1. iframe sandbox属性のベストプラクティス（iBOM HTML向け）
2. Next.js App Router での Client Component 内 iframe パターン

## 調査結果

### 1. iframe sandbox属性（iBOM HTML向け）

**結論**: `sandbox="allow-scripts allow-same-origin"` を採用。

- iBOM HTMLは自己完結型で外部通信なし。JavaScript必須（Canvas描画、BOM操作）
- localStorage を使用するが graceful degradation 実装済み（利用不可でも動作する）
- `allow-scripts` + `allow-same-origin` の組み合わせは同一オリジンでは危険（sandbox除去攻撃）だが、BoardFlowではクロスオリジン配信（app domain ≠ artifact domain）のためリスク軽減
- サーバー側 CSP ヘッダで多層防御を推奨
- 詳細: `docs/external/iframe-sandbox-ibom.md`

### 2. Next.js App Router iframe パターン

**結論**: Server Component → Client Component props渡し + Route Handler 再取得パターン。

- Server Component で viewer-sources API を呼び出し（cookie認証転送）
- 結果を Client Component に props で渡す
- Client Component で iframe 表示、タブ切り替え、URL期限管理を担当
- URL 再取得は Next.js Route Handler 経由でプロキシ
- 各 viewer type (PDF/SVG/iBOM/Download) ごとに表示コンポーネントを分離
- viewer status (available/partial/missing/failed/skipped) に応じたUI分岐が必要
- 詳細: `docs/external/nextjs-iframe-artifact-viewer.md`

## 結論ステータス

**`implementation_required`**

調査は完了。2つの外部トピックについて十分な情報が得られた。実装に進むべき。

---

## 実装計画（Plan フェーズ）

**日時**: 2026-05-02  
**担当フェーズ**: plan

### 目的

Run詳細ページのViewersセクションを拡張し、成果物のインラインプレビュー（PDF/SVG/iBOM）とダウンロード機能を実装する。

### 非目的

- KiCanvas viewer の実装（将来Issue）
- artifact proxy の CSP ヘッダ変更（バックエンド側、既存 #18 で対応済み前提）
- PDF/SVG の画像比較UI
- E2E テスト（本Issueではコンポーネント構造とインタラクションの正しさに集中）

### 受け入れ条件

1. Run詳細ページで schematic viewer (PDF) が iframe によりインラインプレビューされる
2. PDF表示の横にダウンロードリンクが常に表示される
3. pcb_preview (SVG) が Tab UI（Top/Bottom）で切替表示できる
4. iBOM viewer が `sandbox="allow-scripts allow-same-origin"` 付き iframe で表示される
5. fabrication / bom の downloads がダウンロードリンク一覧で表示される
6. viewer status が available 以外の場合、適切なフォールバックメッセージが表示される
7. URL 期限切れ前にバックグラウンドで再取得する仕組みがある
8. TypeScript 型エラーなし、lint pass

### 詳細要件

#### Viewer 種別ごとの表示仕様

| Viewer | status=available | status=partial | status=missing/failed/skipped |
|---|---|---|---|
| schematic | iframe (PDF) + Download link | Download link only (primary欠損時) | メッセージ表示 |
| pcb_preview | Tab(Top/Bottom) iframe (SVG) + Download links | 利用可能な source のみ Tab 表示 | メッセージ表示 |
| ibom | sandboxed iframe | — | メッセージ表示 |
| bom | ダウンロードリンク一覧 | 利用可能なもののみ表示 | メッセージ表示 |
| fabrication | ダウンロードリンク一覧 | 利用可能なもののみ + 欠損警告 | メッセージ表示 |
| kicanvas | (今回スキップ) | — | — |

#### iframe 属性

- PDF: `sandbox` なし（ブラウザ内蔵ビューア）
- SVG: `sandbox=""` (スクリプト完全ブロック)
- iBOM: `sandbox="allow-scripts allow-same-origin"`

#### URL 期限管理

- `expires_at` の5分前に Route Handler 経由で再取得
- 再取得失敗時はリロード促進メッセージ

### 影響範囲

- `boardflow/src/app/(authenticated)/.../runs/[boardRunId]/page.tsx` — Viewers セクションを Client Component に差し替え
- `boardflow/src/components/artifact-viewer/` — 新規ディレクトリ、コンポーネント群
- `boardflow/src/app/api/viewer-sources/[boardRunId]/route.ts` — 新規 Route Handler
- `boardflow/src/lib/api/schema.d.ts` — 型は既存で十分（変更不要）

### 設計方針

#### コンポーネント構成

```
boardflow/src/
├── app/
│   ├── api/
│   │   └── viewer-sources/
│   │       └── [boardRunId]/
│   │           └── route.ts          # Route Handler: Client→Backend proxy
│   └── (authenticated)/.../runs/[boardRunId]/
│       └── page.tsx                   # Server Component (既存、Viewers部分を差し替え)
└── components/
    └── artifact-viewer/
        ├── ArtifactViewerSection.tsx   # "use client" - 全体コンテナ、URL期限管理
        ├── PdfViewer.tsx              # PDF iframe + download link
        ├── SvgViewer.tsx              # SVG Tab(Top/Bottom) + download links
        ├── IbomViewer.tsx             # iBOM sandboxed iframe
        ├── DownloadList.tsx           # ダウンロードリンク一覧
        └── ViewerStatusMessage.tsx    # status != available 時のフォールバック表示
```

#### Server/Client 境界

- **Server Component (page.tsx)**: viewer-sources API を server-side で呼び出し、結果を props として Client Component に渡す（既存のパターンを維持）
- **Client Component (ArtifactViewerSection)**: タブ切替、iframe管理、URL期限管理を担当

#### データフロー

```
page.tsx (Server)
  → viewer-sources API 呼出し (cookie認証転送)
  → viewers, expires_at を ArtifactViewerSection に props で渡す
    → ArtifactViewerSection (Client)
      → useState で viewers / expires_at を保持
      → useEffect で expires_at - 5min にタイマー設定
      → タイマー発火 → /api/viewer-sources/[boardRunId] (Route Handler) を fetch
      → 新しいURLで state 更新 → iframe src が自動更新
```

### 作成/変更ファイル一覧

| ファイル | 種別 | 役割 |
|---|---|---|
| `boardflow/src/components/artifact-viewer/ArtifactViewerSection.tsx` | 新規 | Client Component。viewer全体のコンテナ。URL期限管理、viewer切替 |
| `boardflow/src/components/artifact-viewer/PdfViewer.tsx` | 新規 | PDF iframe表示 + ダウンロードリンク |
| `boardflow/src/components/artifact-viewer/SvgViewer.tsx` | 新規 | SVG Tab切替（Top/Bottom）iframe表示 + ダウンロードリンク |
| `boardflow/src/components/artifact-viewer/IbomViewer.tsx` | 新規 | iBOM sandboxed iframe表示 |
| `boardflow/src/components/artifact-viewer/DownloadList.tsx` | 新規 | ダウンロードリンク群の表示 |
| `boardflow/src/components/artifact-viewer/ViewerStatusMessage.tsx` | 新規 | viewer unavailable 時のフォールバックメッセージ |
| `boardflow/src/app/api/viewer-sources/[boardRunId]/route.ts` | 新規 | Route Handler。Client→Backend proxy |
| `boardflow/src/app/(authenticated)/.../runs/[boardRunId]/page.tsx` | 変更 | Viewersセクションを ArtifactViewerSection に差し替え |

### 実装ステップ（順序と依存関係）

1. **Route Handler 作成** (`app/api/viewer-sources/[boardRunId]/route.ts`)
   - 依存: なし
   - Client Component からの URL 再取得用プロキシ

2. **ViewerStatusMessage 作成** (`components/artifact-viewer/ViewerStatusMessage.tsx`)
   - 依存: なし
   - status に応じたメッセージ表示

3. **PdfViewer 作成** (`components/artifact-viewer/PdfViewer.tsx`)
   - 依存: なし
   - iframe + ダウンロードリンク

4. **SvgViewer 作成** (`components/artifact-viewer/SvgViewer.tsx`)
   - 依存: なし
   - Chakra UI Tabs で Top/Bottom 切替

5. **IbomViewer 作成** (`components/artifact-viewer/IbomViewer.tsx`)
   - 依存: なし
   - sandboxed iframe

6. **DownloadList 作成** (`components/artifact-viewer/DownloadList.tsx`)
   - 依存: なし
   - ダウンロードリンク一覧

7. **ArtifactViewerSection 作成** (`components/artifact-viewer/ArtifactViewerSection.tsx`)
   - 依存: Step 1-6 すべて
   - 全体コンテナ、URL期限管理、各viewer呼び出し

8. **page.tsx 変更** (Run詳細ページ)
   - 依存: Step 7
   - 既存のViewersセクションを ArtifactViewerSection に差し替え

9. **TypeScript 型チェック & lint**
   - 依存: Step 8

### テスト観点

1. **型安全性**: `pnpm typecheck` でエラーなし
2. **lint**: `pnpm lint` pass
3. **コンポーネント表示分岐**:
   - viewer status = available → 各種プレビュー表示
   - viewer status = missing/failed/skipped → フォールバックメッセージ
   - viewer status = partial → 利用可能分のみ表示
4. **セキュリティ**:
   - SVG iframe に `sandbox=""` が付与されていること
   - iBOM iframe に `sandbox="allow-scripts allow-same-origin"` が付与されていること
   - Route Handler が cookie を正しく転送すること
5. **URL期限管理**:
   - expires_at 前にタイマーが発火し再取得されること（手動テスト）
6. **レスポンシブ/レイアウト**: iframe の高さ・幅が適切であること（手動確認）

※ E2E テスト (Playwright) は本 Issue の scope 外。将来 Issue で追加。

### ドキュメント更新対象

- `docs/logs/32/worklog.md` — 本ファイル（計画・実装・結果を追記）
- `docs/frontend/summary.md` — 必要に応じて Artifact Viewer の実装状況を追記（実装後）

### 実装要否

**`implementation_required`**

### 未解決の疑問

なし。調査完了済み、仕様は spec.md と research 成果物で十分に明確。

### 残リスク

1. Safari/iOS での PDF iframe 表示互換性（ダウンロードリンク併設で緩和）
2. expires_at 期限内に iframe 読み込みが完了しない超低速回線環境（エッジケース、MVP では許容）
3. Chakra UI v3 の Tabs API が変更された場合の互換性（package.json でバージョン固定済み）

---

## レビュー結果（Review フェーズ）

**日時**: 2026-05-02  
**担当フェーズ**: review

### 総評

- PDF/SVG/iBOM の iframe 分離、SVG の `sandbox=""`、iBOM の `sandbox="allow-scripts allow-same-origin"`、短命 URL の再取得導線など、Issue #32 の主目的に沿った構成にはなっている。
- 一方で、既存の viewer-sources 契約に含まれる `kicanvas` viewer を UI 側が処理できておらず、仕様との不整合と表示退行がある。
- また、URL 再取得失敗時のユーザー通知が未実装で、期限切れ後に壊れた iframe / download link だけが残る経路を防げていない。

### 調査結果

- backend 契約では `viewer-sources` に `kicanvas` を含める仕様で、API テストでも `available` が明示されている。
- frontend 方針では `Schematic / PCB Preview では KiCanvas を第一候補にしつつ、PDF/SVG fallback を必ず残す` とされている。
- 実装では `ArtifactViewerSection` が未知 viewer を generic download viewer として扱うため、`kicanvas` の `sources` を無視して `missing` 相当の表示に落ちる。
- URL 再取得ロジックはあるが、`!res.ok` と `catch` がサイレント失敗で、計画にある「再取得失敗時はリロード促進メッセージ」がない。

### レビュー指摘

#### 必須修正

1. **`kicanvas` viewer が `available` のときに誤表示になる**
   - `ArtifactViewerSection` は全 viewer を描画対象にしているが、`renderViewer` に `kicanvas` 分岐がないため、backend が返す `sources` を無視して default 分岐へ落ちる。
   - その結果、`kicanvas` が `available` でも「利用不可」相当のメッセージになるか、少なくとも viewer contract に整合しない UI になる。
   - これは frontend 方針・spec・backend 契約のいずれとも不整合。

2. **viewer URL の再取得失敗がユーザーに一切見えない**
   - `refreshViewerSources()` は `!res.ok` で即 return、`catch` も握りつぶしており、失敗状態を state に保持しない。
   - 期限切れ後は iframe と download link が stale URL のまま残るため、ユーザーは壊れた表示だけを見せられる。
   - Issue 計画の「再取得失敗時はリロード促進メッセージ」と未整合。

#### 任意改善

1. Route Handler が backend の error payload を潰して generic error だけ返しているため、client 側で `unauthorized` / `forbidden` / `not_found` を出し分けにくい。
2. `partial` status の扱いが viewer ごとに弱く、`pcb_preview` で片面だけ available の場合や `schematic` で preview 不可だが download は可能な場合に、限定表示であることの説明が薄い。
3. URL 更新時に iframe `src` が差し替わるため、iBOM のスクロール位置や UI 状態が token 更新のたびに失われうる。現状でも要件未達ではないが、UX リスクとして明示した方がよい。

### テスト結果

- 確認済み: `pnpm typecheck` pass、`pnpm lint` pass
- 未確認: component test、Playwright smoke test、viewer status 切替テスト、URL 再取得失敗時の UI テスト

### ドキュメント確認

- `docs/frontend/summary.md`: KiCanvas first + PDF/SVG fallback 方針あり
- `docs/spec.md`: KiCanvas interactive preview と fallback の要件あり
- `docs/backend/api.md`: `viewer-sources` に `kicanvas` を含む契約あり
- 今回の実装説明・plan は「KiCanvas は今回スキップ」としているが、既存仕様との差分整理が不足している

### PR/完了結果

- `pr_ready: false`
- 理由: 既存 viewer contract (`kicanvas`) の表示退行と、URL 再取得失敗時の回復導線欠如があるため

### 残リスク

- `kicanvas` の仕様整理なしに本 PR を出すと、backend 契約と UI の責務分担が曖昧なまま固定化される
- 自動再取得失敗時の回復導線がないままでは、短命 URL 前提の viewer UX が本番で不安定になる

## 実装時の注意事項

1. **artifact domain の分離を維持**: sandbox="allow-scripts allow-same-origin" の安全性はクロスオリジン前提
2. **サーバー側CSPヘッダ**: artifact proxy で `frame-ancestors`, `default-src 'none'`, `script-src 'unsafe-inline'` を付与
3. **URL期限管理**: expires_at の5分前にバックグラウンド再取得。iframe src 更新時の状態ロスに注意
4. **PDF表示のフォールバック**: Safari/iOS制限あり。常にダウンロードリンクを併設
5. **SVGのsandbox**: KiCad SVGにスクリプトが含まれる可能性は低いが、`sandbox=""` で完全ブロックを検討
6. **viewer status 分岐**: available のみプレビュー表示。missing/failed/skipped はメッセージ表示

## 参照URL

- InteractiveHtmlBom: https://github.com/openscopeproject/InteractiveHtmlBom
- MDN iframe sandbox: https://developer.mozilla.org/en-US/docs/Web/HTML/Element/iframe#sandbox
- MDN CSP sandbox: https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers/Content-Security-Policy/sandbox
- Next.js Server Components: https://nextjs.org/docs/app/building-your-application/rendering/server-components
- Next.js Route Handlers: https://nextjs.org/docs/app/building-your-application/routing/route-handlers

---

## 実装フェーズ

**日時**: 2026-05-02  
**担当フェーズ**: impl

### 作成したファイル

| ファイル | 役割 |
|---|---|
| `boardflow/src/app/api/viewer-sources/[boardRunId]/route.ts` | Route Handler: Client→Backend proxy (cookie認証転送) |
| `boardflow/src/components/artifact-viewer/artifact-viewer-section.tsx` | Client Component コンテナ。URL期限管理 + viewer切替 |
| `boardflow/src/components/artifact-viewer/pdf-viewer.tsx` | PDF iframe + ダウンロードリンク |
| `boardflow/src/components/artifact-viewer/svg-viewer.tsx` | SVG Tab(Top/Bottom) + sandbox="" + ダウンロードリンク |
| `boardflow/src/components/artifact-viewer/ibom-viewer.tsx` | iBOM sandboxed iframe (allow-scripts allow-same-origin) |
| `boardflow/src/components/artifact-viewer/download-list.tsx` | ダウンロードリンク一覧 (available/unavailable 分岐) |
| `boardflow/src/components/artifact-viewer/viewer-status-message.tsx` | viewer unavailable時のフォールバックメッセージ |

### 変更したファイル

| ファイル | 変更内容 |
|---|---|
| `boardflow/src/app/(authenticated)/.../runs/[boardRunId]/page.tsx` | Viewersセクションを `ArtifactViewerSection` に差し替え。未使用の `viewerStatusColor` 関数を削除。 |

### 実装ポイント

1. **iframe**: Chakra UI v3の`Box as="iframe"`はiframe属性の型定義がないため、native `<iframe>` を `Box` で囲むパターンを採用
2. **URL期限管理**: `useEffect` + `setTimeout` で `expires_at - 5分` にRoute Handler経由で再取得
3. **セキュリティ**:
   - SVG: `sandbox=""` でスクリプト完全ブロック
   - iBOM: `sandbox="allow-scripts allow-same-origin"` (クロスオリジン配信前提)
   - 全ダウンロードリンク: `rel="noopener noreferrer"` 付与
   - Route Handler: cookie認証チェック後にバックエンドへプロキシ
4. **lucide-react `Image`**: ESLint jsx-a11y/alt-text と名前衝突するため `ImageIcon` にリネーム

### テスト結果

- `pnpm typecheck`: ✅ 0 errors
- `pnpm lint`: ✅ 0 warnings, 0 errors

### 残リスク

1. Safari/iOS での PDF iframe 表示互換性（ダウンロードリンク併設で緩和）
2. Chakra UI v3 Tabs の API 安定性（package.json でバージョン固定済み）
3. 期限切れ後の再取得失敗時にユーザーへ通知なし（サイレント失敗）— 将来的にトースト通知を検討
4. E2Eテスト未実装（本Issue scope外、将来Issue）

## 更新ファイル

- `docs/external/iframe-sandbox-ibom.md` (新規作成)
- `docs/external/nextjs-iframe-artifact-viewer.md` (新規作成)
- `docs/logs/32/worklog.md` (本ファイル)

## 残リスク

- artifact domain と app domain が同一になった場合、sandbox のセキュリティモデルが破綻する
- iBOM が将来バージョンで外部リソース読み込みを行うようになった場合、CSP の調整が必要
- Safari/iOS の iframe 内 PDF 表示の互換性は実機テストで確認が必要

---

## レビュー指摘修正フェーズ

**日時**: 2026-05-02  
**担当フェーズ**: impl (review fix)

### 修正内容

| 指摘 | 種別 | 対応 |
|---|---|---|
| kicanvas viewer のハンドリング | Major | `kicanvas` を明示的にスキップし「KiCanvas (coming soon)」placeholder を表示。静的viewer (schematic/pcb_preview) が別途表示されるため情報欠損なし |
| URL再取得失敗時のUX | Major | `refreshError` state を追加。失敗時に「Viewer URLs have expired. Please reload the page.」メッセージ + Reload ボタンを表示 |
| Route Handler のエラーpassthrough | Minor | backend の JSON body をそのまま透過。`res.json()` を先に取得し、`!res.ok` 時はそのまま返却 |
| partial status の表示 | Minor | `hasPartial` チェックを追加し「Some sources are unavailable. Showing limited preview.」テキストを表示 |

### 変更ファイル

| ファイル | 変更内容 |
|---|---|
| `boardflow/src/app/api/viewer-sources/[boardRunId]/route.ts` | backend error payload passthrough |
| `boardflow/src/components/artifact-viewer/artifact-viewer-section.tsx` | kicanvas placeholder, refreshError state & UI, partial message |

### テスト結果

- `pnpm typecheck`: ✅ 0 errors
- `pnpm lint`: ✅ 0 warnings, 0 errors

### 残リスク

- kicanvas の完全実装は別 Issue で対応必要
- refreshError 表示後にネットワーク復旧しても自動リカバリなし（ユーザーが手動でリロード）
- iBOM iframe の URL 更新時スクロール状態ロスは未解決（UX 改善として別 Issue 候補）
