# Issue #111 作業ログ

## 経緯
- #109 DiffSummary パース共通化済み（`boardflow/src/lib/domain/diff-summary.ts` に抽出）
- #107 status/format 関数集約済み（`boardflow/src/lib/domain/status.ts`, `boardflow/src/lib/format.ts`）
- #111 は DiffContent を セクション単位に分割するリファクタリング Issue

## ユーザー要望
- 既存 Issue に従い、`boardflow/src/components/diff/diff-content.tsx` を分割する
- 挙動変更なし、純粋なコード移動・分割

## 調査フェーズ（2026-05-14）

### Issue #111 要件（GitHub Issue 本文）

**背景**: DiffContent が header、status別メッセージ、file changes、BOM changes、checks、artifact changes、preview links、metadata 解析を1ファイルに抱えている。

**対象**: `boardflow/src/components/diff/diff-content.tsx`

**やること**:
- diff feature 配下にセクションコンポーネントを切り出す
- 例: DiffHeader、DiffStatusMessage、FileChangesSection、BomChangesSection、ChecksSection、ArtifactChangesSection、PreviewLinksSection
- DiffContent はデータ取得とページ構成を中心に薄くする
- DiffSummary 共通化Issueの成果がある場合はそれを利用する

**完了条件**:
- diff-content.tsx のファイルサイズと責務が小さくなっている
- metadata 解析や型ガードが表示コンポーネントから過度に漏れていない
- 既存の表示挙動が変わっていない
- `pnpm typecheck` と `pnpm lint` が通る

### 現在の DiffContent コンポーネント構造

**ファイル**: `boardflow/src/components/diff/diff-content.tsx` （約470行、1ファイル）

| セクション | 行範囲 | 責務 |
|---|---|---|
| imports | 1-11 | 外部ライブラリ・内部モジュールのインポート |
| Props interface | 13-17 | コンポーネントのProps型定義 |
| `DiffContent` (export) | 19-137 | データ取得（useSuspenseQuery×2）、Breadcrumb、Header、Status別メッセージ、ready時のセクション委譲 |
| `FileChangesSection` (local) | 139-204 | ファイル変更の表示。metadata.file_hashes を利用 |
| `BomChangesSection` (local) | 206-246 | BOM変更の表示。metadata.bom_summary を利用 |
| `ChecksSection` (local) | 248-302 | ERC/DRC チェック結果の表示 |
| `ArtifactChangesSection` (local) | 304-368 | アーティファクト変更の表示。metadata.artifacts_summary を利用 |
| `PreviewLinksSection` (local) | 370-470 | プレビューリンクの表示。metadata.previews を利用 |

### 依存関係マップ

**DiffContent をインポートしているファイル**:
- `boardflow/src/app/(authenticated)/repositories/[repositoryId]/boards/[boardProjectId]/runs/[boardRunId]/diff/page.tsx`（唯一の使用箇所）

### barrel export パターン
- `boardflow/src/components/` 配下に `index.ts` は存在しない
- `run-detail/` ディレクトリも個別ファイル直接インポート方式（barrel なし）
- → 本 Issue でも barrel export は作成せず、各ファイルから直接 import する方針

---

## 計画フェーズ（2026-05-14）

### 実装要否

`implementation_required`

### 目的

`diff-content.tsx`（約470行）を7つの小ファイルに分割し、DiffContent コンポーネントをデータ取得とページ構成のみに薄くする。

### 非目的

- 表示挙動の変更
- 新機能追加
- Props 型の再設計（既存の inline 型を named 型に変更するのみ）
- metadata 解析ロジックの変更
- barrel export (index.ts) の導入

### 受け入れ条件

1. `diff-content.tsx` がデータ取得 + ページ構成のみに薄くなっている（実績: 95行）
2. 分割された各セクションコンポーネントが個別ファイルに存在する
3. 表示挙動が変わっていない（レンダリング結果が同一）
4. `pnpm typecheck` が通る
5. `pnpm lint` が通る
6. `pnpm build` が通る
7. 各コンポーネントに named export がある

### 詳細要件

#### ブランチ名

`refactor/issue-111-diff-content-split`

#### 作成・変更するファイル一覧

| # | ファイルパス | 操作 | 内容 |
|---|---|---|---|
| 1 | `boardflow/src/components/diff/diff-header.tsx` | 新規作成 | Header 部分（Badge + 比較リンク + 日時）を抽出 |
| 2 | `boardflow/src/components/diff/diff-status-message.tsx` | 新規作成 | status 別メッセージ（no_baseline / unavailable / failed）を抽出 |
| 3 | `boardflow/src/components/diff/file-changes-section.tsx` | 新規作成 | `FileChangesSection` を抽出 |
| 4 | `boardflow/src/components/diff/bom-changes-section.tsx` | 新規作成 | `BomChangesSection` を抽出 |
| 5 | `boardflow/src/components/diff/checks-section.tsx` | 新規作成 | `ChecksSection` を抽出 |
| 6 | `boardflow/src/components/diff/artifact-changes-section.tsx` | 新規作成 | `ArtifactChangesSection` を抽出 |
| 7 | `boardflow/src/components/diff/preview-links-section.tsx` | 新規作成 | `PreviewLinksSection` を抽出 |
| 8 | `boardflow/src/components/diff/diff-content.tsx` | 変更 | ローカル関数を削除し、新ファイルからの import に置換 |

#### 各ファイルの Props 型定義と import

**1. `diff-header.tsx`**
```tsx
'use client';

import { Badge, Box, Heading, HStack, Text } from '@chakra-ui/react';
import Link from 'next/link';
import { diffStatusColor } from '@/lib/domain/status';
import { formatDateTime, shortId } from '@/lib/format';
import { routes } from '@/lib/routes';

interface DiffHeaderProps {
  status: string;
  baseBoardRunId: string | null;
  boardRunId: string;
  repositoryId: string;
  boardProjectId: string;
  createdAt: string;
}

export function DiffHeader({ ... }: DiffHeaderProps) { ... }
```
- 元の `DiffContent` 内 `{/* Header */}` セクション（L63-90）を抽出
- `'use client'` は不要な可能性があるが、`Link` を使用しているため付与

**2. `diff-status-message.tsx`**
```tsx
'use client';

import { Box, Text } from '@chakra-ui/react';

interface DiffStatusMessageProps {
  status: string;
  errorMessage: string | null;
}

export function DiffStatusMessage({ status, errorMessage }: DiffStatusMessageProps) { ... }
```
- 元の L93-113 の status 分岐3つを1コンポーネントに集約
- status が `no_baseline` / `unavailable` / `failed` のいずれでもなければ `null` を返す

**3. `file-changes-section.tsx`**
```tsx
'use client';

import { Badge, Box, Heading, HStack, Text, VStack } from '@chakra-ui/react';
import type { ParsedDiffSummary } from '@/lib/domain/diff-summary';
import { isRecord } from '@/lib/domain/guards';

interface FileChangesSectionProps {
  summary: ParsedDiffSummary;
  metadata: Record<string, unknown> | null;
}

export function FileChangesSection({ summary, metadata }: FileChangesSectionProps) { ... }
```
- 元の L139-204 をそのまま移動

**4. `bom-changes-section.tsx`**
```tsx
'use client';

import { Badge, Box, Heading, HStack, Text } from '@chakra-ui/react';
import type { ParsedDiffSummary } from '@/lib/domain/diff-summary';

interface BomChangesSectionProps {
  summary: ParsedDiffSummary;
  metadata: Record<string, unknown> | null;
}

export function BomChangesSection({ summary, metadata }: BomChangesSectionProps) { ... }
```
- 元の L206-246 をそのまま移動

**5. `checks-section.tsx`**
```tsx
'use client';

import { Box, Heading, HStack, Text, VStack } from '@chakra-ui/react';
import type { ParsedDiffSummary } from '@/lib/domain/diff-summary';

interface ChecksSectionProps {
  summary: ParsedDiffSummary;
}

export function ChecksSection({ summary }: ChecksSectionProps) { ... }
```
- 元の L248-302 をそのまま移動

**6. `artifact-changes-section.tsx`**
```tsx
'use client';

import { Badge, Box, Heading, HStack, Text, VStack } from '@chakra-ui/react';
import type { ParsedDiffSummary } from '@/lib/domain/diff-summary';
import { isRecord } from '@/lib/domain/guards';

interface ArtifactChangesSectionProps {
  summary: ParsedDiffSummary;
  metadata: Record<string, unknown> | null;
}

export function ArtifactChangesSection({ summary, metadata }: ArtifactChangesSectionProps) { ... }
```
- 元の L304-368 をそのまま移動

**7. `preview-links-section.tsx`**
```tsx
'use client';

import { Box, Heading, HStack, Text, VStack } from '@chakra-ui/react';
import Link from 'next/link';
import { isRecord } from '@/lib/domain/guards';
import { shortId } from '@/lib/format';
import { routes } from '@/lib/routes';

interface PreviewLinksSectionProps {
  metadata: Record<string, unknown> | null;
  repositoryId: string;
  boardProjectId: string;
  boardRunId: string;
  baseRunId: string | null;
}

export function PreviewLinksSection({ ... }: PreviewLinksSectionProps) { ... }
```
- 元の L370-470 をそのまま移動

**8. `diff-content.tsx`（変更後）**
```tsx
'use client';

import { Box, VStack } from '@chakra-ui/react';
import { Breadcrumb } from '@/components/ui/breadcrumb';
import { $api } from '@/lib/api/react-query';
import { parseDiffSummary } from '@/lib/domain/diff-summary';
import { shortId } from '@/lib/format';
import { routes } from '@/lib/routes';
import { ArtifactChangesSection } from './artifact-changes-section';
import { BomChangesSection } from './bom-changes-section';
import { ChecksSection } from './checks-section';
import { DiffHeader } from './diff-header';
import { DiffStatusMessage } from './diff-status-message';
import { FileChangesSection } from './file-changes-section';
import { PreviewLinksSection } from './preview-links-section';

// Props 型はそのまま維持
interface Props { ... }

export function DiffContent({ ... }: Props) {
  // useSuspenseQuery x2 はそのまま維持
  // Breadcrumb はそのまま維持
  // Header → <DiffHeader ... />
  // Status messages → <DiffStatusMessage ... />
  // ready セクション → 各セクションコンポーネントをそのまま呼び出し
}
```
- 約95行に縮小（データ取得 + Breadcrumb + セクション配置のみ）
- ローカル関数を全て削除し、import に置換
- `parseDiffSummary` の呼び出しは `DiffContent` に残す（データ取得の責務）

#### import/export の依存関係図

```
page.tsx
  └─ DiffContent (diff-content.tsx)  ← export 維持、import パスも変更なし
       ├─ DiffHeader (diff-header.tsx)
       ├─ DiffStatusMessage (diff-status-message.tsx)
       ├─ FileChangesSection (file-changes-section.tsx)
       ├─ BomChangesSection (bom-changes-section.tsx)
       ├─ ChecksSection (checks-section.tsx)
       ├─ ArtifactChangesSection (artifact-changes-section.tsx)
       └─ PreviewLinksSection (preview-links-section.tsx)
```

外部使用者 (`page.tsx`) の import パスは変更不要。

### 影響範囲

- `boardflow/src/components/diff/` ディレクトリ内のみ
- `page.tsx` は変更不要（`DiffContent` の export は同じファイルパス・同じ名前のまま）
- ドメインライブラリ（`diff-summary.ts`, `guards.ts`, `status.ts`, `format.ts`, `routes.ts`）は変更なし

### 設計方針

1. **純粋なコード移動**: ローカル関数を named export 付き個別ファイルに移動。ロジック変更なし。
2. **Props 型の明示化**: inline 型パラメータ `{ summary: ParsedDiffSummary; metadata: ... }` を named interface に変更。
3. **`'use client'` ディレクティブ**: 全ファイルに付与（Chakra UI コンポーネントを使用するため）。
4. **barrel export なし**: 既存プロジェクト慣例に従い、`index.ts` は作成しない。
5. **DiffStatusMessage の集約**: 3つの status 分岐を1コンポーネントに集約し、`DiffContent` を更に薄くする。

### テスト観点

1. `pnpm typecheck` — 全 Props 型が正しく定義され、型エラーがないこと
2. `pnpm lint` — Biome ルールに準拠（import 順序、未使用 import なし）
3. `pnpm build` — Next.js ビルドが成功すること
4. 手動確認: diff ページの表示が変わっていないこと（受け入れ条件の一部）

### ドキュメント更新対象

- `docs/logs/111/worklog.md` — 本計画の記録と実装結果の追記

### 実装の順序（依存関係を考慮）

以下の順序で実装する。ステップ 1-7 は互いに独立しているため並行作成可能。

1. `diff-header.tsx` を新規作成
2. `diff-status-message.tsx` を新規作成
3. `file-changes-section.tsx` を新規作成
4. `bom-changes-section.tsx` を新規作成
5. `checks-section.tsx` を新規作成
6. `artifact-changes-section.tsx` を新規作成
7. `preview-links-section.tsx` を新規作成
8. `diff-content.tsx` を編集（ローカル関数削除 + 新ファイルの import 追加）
9. `pnpm typecheck && pnpm lint && pnpm build` で検証
10. コミット & プッシュ

### barrel export (index.ts) の扱い

作成しない。既存プロジェクトの慣例（`run-detail/`, `checks/` 等）に従い、直接パス import を使用する。

### 未解決の疑問

なし。調査により全ての情報が揃っている。

### 残リスク

- **表示崩れ**: コード移動のみだが、import 漏れや Props 渡し漏れの可能性 → typecheck で検出可能
- **Biome import 順序**: 新ファイル追加により import 順序が Biome ルールに合わない可能性 → `pnpm lint` で検出・自動修正可能

**DiffContent が依存しているモジュール**:
- `@chakra-ui/react`: Box, Badge, Heading, HStack, Text, VStack
- `next/link`: Link
- `@/components/ui/breadcrumb`: Breadcrumb
- `@/lib/api/react-query`: $api
- `@/lib/domain/diff-summary`: ParsedDiffSummary, parseDiffSummary（#109 成果）
- `@/lib/domain/guards`: isRecord（#109 成果）
- `@/lib/domain/status`: diffStatusColor（#107 成果）
- `@/lib/format`: formatDateTime, shortId
- `@/lib/routes`: routes

### #109/#107 の成果物で活用すべきもの

| 成果物 | ファイル | 活用方法 |
|---|---|---|
| `parseDiffSummary` | `boardflow/src/lib/domain/diff-summary.ts` | DiffContent本体でパースし、各セクションに渡す（現状通り） |
| `ParsedDiffSummary` 型 | 同上 | 各セクションコンポーネントのProps型で利用 |
| `isRecord` ガード | `boardflow/src/lib/domain/guards.ts` | FileChangesSection, ArtifactChangesSection, PreviewLinksSection で利用 |
| `diffStatusColor` | `boardflow/src/lib/domain/status.ts` | DiffHeader で利用 |
| `formatDateTime`, `shortId` | `boardflow/src/lib/format.ts` | DiffHeader, PreviewLinksSection で利用 |

### 分割対象セクション一覧と分割先ファイル名の提案

| 分割コンポーネント | 分割先ファイル | 元の行範囲 | 主な依存 |
|---|---|---|---|
| `DiffHeader` (新規) | `boardflow/src/components/diff/diff-header.tsx` | 59-89 (Header部分) | Badge, diffStatusColor, formatDateTime, shortId, routes, Link |
| `DiffStatusMessage` (新規) | `boardflow/src/components/diff/diff-status-message.tsx` | 91-115 (Status別メッセージ) | Box, Text |
| `FileChangesSection` | `boardflow/src/components/diff/file-changes-section.tsx` | 139-204 | Badge, isRecord, ParsedDiffSummary |
| `BomChangesSection` | `boardflow/src/components/diff/bom-changes-section.tsx` | 206-246 | Badge, ParsedDiffSummary |
| `ChecksSection` | `boardflow/src/components/diff/checks-section.tsx` | 248-302 | ParsedDiffSummary |
| `ArtifactChangesSection` | `boardflow/src/components/diff/artifact-changes-section.tsx` | 304-368 | Badge, isRecord, ParsedDiffSummary |
| `PreviewLinksSection` | `boardflow/src/components/diff/preview-links-section.tsx` | 370-470 | isRecord, routes, shortId, Link |

**DiffContent（本体）** は以下の責務のみに縮小:
- データ取得（useSuspenseQuery × 2）
- Breadcrumb
- DiffHeader, DiffStatusMessage の配置
- ready 時の各セクション委譲（parseDiffSummary 呼び出し + セクション配置）

### 制約・注意点
- `PreviewLinksSection` は repositoryId/boardProjectId/boardRunId/baseRunId を受け取るため、Props が他セクションより多い
- `DiffHeader` は Breadcrumb を含めるか別にするか検討の余地あり（Issue本文では DiffHeader として言及）
- `DiffStatusMessage` は status ごとの分岐のみで比較的小さい。DiffContent 本体に残す選択肢もあるが、Issue 本文の例に従い分割する
- Biome lint / typecheck / build を通すこと

---

## 実装フェーズ（2026-05-14）

### 実施内容

計画通り、`boardflow/src/components/diff/` に7ファイルを新規作成し、`diff-content.tsx` を修正。

| ファイル | 操作 | 行数 |
|---|---|---|
| `diff-header.tsx` | 新規作成 | 56行 |
| `diff-status-message.tsx` | 新規作成 | 43行 |
| `file-changes-section.tsx` | 新規作成 | 79行 |
| `bom-changes-section.tsx` | 新規作成 | 53行 |
| `checks-section.tsx` | 新規作成 | 75行 |
| `artifact-changes-section.tsx` | 新規作成 | 78行 |
| `preview-links-section.tsx` | 新規作成 | 85行 |
| `diff-content.tsx` | 変更 | 470行 → 95行 |

### 実装上の判断

- `DiffStatusMessage` は3つの status 分岐を1コンポーネントに集約。該当しない status の場合は `null` を返す
- Props interface は全て `export` 付き named interface として定義（`DiffHeaderProps` 等）
- Biome formatter が `DiffStatusMessage` の Props を1行に収める形式を要求 → 修正対応

### テスト結果

| チェック | 結果 |
|---|---|
| `pnpm typecheck` | ✅ パス |
| `pnpm lint` | ✅ パス（Biome format 修正1件対応） |
| `pnpm build` | ✅ パス（全10ルート正常ビルド） |

### コミット

- `f8a590b`: `refactor: split DiffContent into dedicated section components (#111)`

### 更新ドキュメント

- `docs/logs/111/worklog.md` — 本ログ

### 残リスク

- なし（純粋なコード移動のみ、挙動変更なし）

## 結論ステータス

**`implementation_required`** — 外部ライブラリの調査は不要。純粋なコード移動・分割のリファクタリング。

## 残リスク
- なし（挙動変更なし、型安全な移動のみ）

---

## レビューフェーズ（2026-05-14）

### レビュー結果

- 対象 Issue: #111
- 判定: `pr_ready: true`
- 総評: 7つの抽出コンポーネントへの分割は、main の `diff-content.tsx` にあった表示ロジックをそのまま移送しており、`DiffContent` 本体はデータ取得・Breadcrumb・セクション配置に責務が整理されている。`DiffContent` の外部利用箇所も diff page の1箇所のままで、import/export の互換性は維持されている。

### 確認内容

- `git diff main...refactor/issue-111-diff-content-split` で差分を確認
- main 側の `diff-content.tsx` と、分割後の 8 ファイルを比較し、各セクションの JSX と条件分岐が一致することを確認
- `DiffContent` の利用箇所が diff page のみであることを確認
- `pnpm typecheck`
- `pnpm lint`
- `pnpm build`

### 指摘事項

- `suggestion`: 各抽出先ファイルの `'use client'` は動作上問題ないが、`DiffContent` 自体がすでにクライアント境界なので、現状の利用形態では冗長。Next.js の `use client` は Server Component から直接 import されるエントリにだけ必要であり、子コンポーネント側まで境界を増やす必要はない。将来これらを Server Component から再利用しない前提が続くなら、`diff-content.tsx` を境界として他 7 ファイルの directive を外す余地がある。

### 必須修正

- なし

### 任意改善

- `diff-header.tsx` など抽出先 7 ファイルの `'use client'` を削除し、クライアント境界を `diff-content.tsx` に集約する

### テスト結果

- `pnpm typecheck`: pass
- `pnpm lint`: pass
- `pnpm build`: pass

### ドキュメント確認

- `docs/logs/111/worklog.md` の計画・実装内容・テスト結果は、実際の差分と概ね整合している
- ただし、計画フェーズの受け入れ条件にある「`diff-content.tsx` が ～60行以下」は実装結果の 90 行台と一致していない。プロダクト上の問題ではないが、計画値としては未達なので、必要なら受け入れ条件の表現を「本体を十分に薄くする」などに修正した方が記録としては正確

### 残リスク

- なし。確認できた範囲では挙動差分は見当たらない

### PR/完了結果

- `pr_ready: true`

---

## ドキュメント確認フェーズ（2026-05-14）

### 対象 Issue

- Issue ID: `#111`
- ブランチ: `refactor/issue-111-diff-content-split`

### 確認対象

- `docs/logs/111/worklog.md`
- `docs/frontend/summary.md`
- `AGENTS.md`
- `docs/spec.md`
- `README.md`

### ドキュメント確認結果

- `docs/logs/111/worklog.md` は、実装した分割対象ファイル、責務分離方針、検証コマンドの記録は実装と整合している
- ただし、計画フェーズの受け入れ条件にある「`diff-content.tsx` がデータ取得 + ページ構成のみ（～60行以下）」は、実装結果の `diff-content.tsx` が約90行である現状と一致していない
- `docs/frontend/summary.md` は frontend の方針文書であり、今回のコンポーネント分割リファクタリングによる更新は不要
- `docs/spec.md` は差分レビュー機能の仕様を記述しており、今回の UI 内部構造変更で更新すべき箇所はない
- `README.md` はセットアップ・開発手順中心のため、今回の変更による更新は不要
- `AGENTS.md` の frontend 配置・検証コマンド方針とは整合している

### 判定

- `docs_ready: false`

### 必須修正

- `docs/logs/111/worklog.md` の計画フェーズにある受け入れ条件「～60行以下」を、実装結果に合わせて「約90行」へ修正するか、「データ取得とページ構成のみに薄くする」のような責務ベースの表現へ修正する
- 同ログ内の変更後 `diff-content.tsx` 説明にある「約60行に縮小」も、実装結果に合わせて修正する

### 任意改善

- レビューフェーズの「ドキュメント確認」で既に触れている行数不一致を、計画フェーズ側にも反映してログ全体の自己整合性を上げる

### 不整合のあるドキュメント

- `docs/logs/111/worklog.md`

### 不足しているドキュメント

- なし

### 外部調査メモに関する指摘

- なし。Issue #111 は既存 UI コンポーネントの分割リファクタリングであり、`docs/external/` の追加確認は不要

### 残リスク

- 実装自体ではなく記録精度の問題に留まるが、受け入れ条件の数値が未修正のままだと、後続レビュー時に「計画未達」か「記録誤り」かの判断コストが残る

---

## ドキュメント修正フェーズ（2026-05-14）

### 修正内容

レビュー・ドキュメント確認フェーズで指摘された worklog 内の行数不整合を修正。

| 箇所 | 修正前 | 修正後 |
|---|---|---|
| 計画フェーズ 受け入れ条件 #1 | `～60行以下` | `データ取得 + ページ構成のみに薄くなっている（実績: 95行）` |
| 計画フェーズ 変更後 diff-content.tsx 説明 | `約60行に縮小` | `約95行に縮小（データ取得 + Breadcrumb + セクション配置のみ）` |
| 実装フェーズ テーブル | `470行 → 90行` | `470行 → 95行` |

### 根拠

- `wc -l boardflow/src/components/diff/diff-content.tsx` の結果: **95行**
- コード変更は不要（レビューで `pr_ready: true` 判定済み）

### 残リスク

- なし
