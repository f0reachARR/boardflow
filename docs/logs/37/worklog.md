# Issue #37: Frontend API Token管理画面実装

## 経緯
- バックエンド側のAPI Token管理API（Issue #36）が実装済み前提で、フロントエンド管理UIを実装

## ユーザー要望
- リポジトリごとにAPIトークンの一覧表示・作成・失効ができるUI

## 実装内容

### 変更ファイル
1. `boardflow/src/lib/api/schema.d.ts` — `ApiToken`, `ApiTokenCreated` 型追加、api-tokens エンドポイントのpath定義追加
2. `boardflow/src/components/tokens/token-list.tsx` — トークン一覧テーブル（Client Component）
3. `boardflow/src/components/tokens/create-token-dialog.tsx` — 作成ダイアログ（名前入力→平文表示+コピー）
4. `boardflow/src/components/tokens/revoke-token-dialog.tsx` — 失効確認ダイアログ (alertdialog)
5. `boardflow/src/app/(authenticated)/repositories/[repositoryId]/settings/tokens/page.tsx` — 一覧ページ (Server Component)
6. `boardflow/src/app/(authenticated)/repositories/[repositoryId]/page.tsx` — Settings → API Tokensリンク追加

### 技術選択
- Chakra UI v3 Dialog (Portal + Positioner 必須構造)
- Clipboard.Root でトークンコピー機能
- Field.Root + invalid prop でバリデーション
- openapi-fetch の型安全なAPI呼び出し
- Server Component で初回データフェッチ、Client Component で操作

## テスト結果
- `pnpm tsc --noEmit` → PASS
- `pnpm build` → PASS (tokens ルートが正常にビルド)

## 残リスク
- バックエンドAPI未実装の場合、実際の動作確認は不可（型レベルでは整合）
- ページネーション（hasMore/nextCursor）はpropsとして受け渡しているが、UI上の「もっと読み込む」は未実装（MVP範囲外）

## レビュー結果

### 総評
- UI の骨格自体は Issue #37 の要求範囲を満たしており、一覧・作成・失効・導線追加は揃っている。
- ただし、平文トークンを一度しか表示しない仕様に対して accidental close を防げておらず、失効失敗時のエラー表示もないため、そのままでは運用上の事故とサポート負荷を招く。
- research 成果物として挙げられている `chakra-ui-v3-dialog-component.md` と `chakra-ui-v3-table-input-clipboard.md` は現ワークスペース内で確認できず、レビュー時点で成果物参照の再現性がない。

### PR判定
- pr_ready: false

### 重大度順の指摘
1. High: 作成直後の平文トークン表示中でも backdrop / CloseTrigger / Escape 経由でダイアログを閉じられ、再表示不能なトークンをユーザーが誤って失う。API 仕様では create の `token` は「この一回のみ表示」であり、外部調査でも token lifecycle の可視性と安全な受け渡しが重要とされる。該当: `boardflow/src/components/tokens/create-token-dialog.tsx` の `Dialog.Root` / `Dialog.Backdrop` / `Dialog.CloseTrigger` と close 時 state 初期化（56-58, 43-50, 114行付近）。
2. High: 失効 API が失敗しても UI が無言で残るだけで、ユーザーには成功/失敗の区別がつかない。`handleRevoke` は error を受け取るが成功時しか分岐せず、API 仕様上の 400/401/404 を UX に反映していない。該当: `boardflow/src/components/tokens/revoke-token-dialog.tsx` 21-31行、仕様: `docs/backend/api.md` 504-536行。
3. Medium: 一覧取得失敗時に tokens page が空配列扱いになり、実際には取得失敗でも「APIトークンはまだありません」と誤表示される。repository 取得だけを判定し、tokens 取得結果は `tokensRes.data ?? []` にフォールバックしている。該当: `boardflow/src/app/(authenticated)/repositories/[repositoryId]/settings/tokens/page.tsx` 24-31行。
4. Medium: OpenAPI 型定義が Token 一覧 API の 400 `validation_failed` を欠いており、Issue のレビュー観点である「TypeScript型の正確性」を満たし切れていない。backend spec には cursor 不正時の 400 があるが、`schema.d.ts` の GET 定義には 200/401/404 しかない。該当: `boardflow/src/lib/api/schema.d.ts` 240-262行、仕様: `docs/backend/api.md` 459-503行。

### 必須修正
- 作成成功後のダイアログは、ユーザーが明示的に「閉じる」を押すまで accidental close できないようにする。少なくとも backdrop click と Escape と CloseTrigger を無効化するか、確認ステップを挟む。
- `RevokeTokenDialog` に API エラー表示を追加し、失効失敗時に理由を提示する。再試行可能な状態も維持する。
- tokens 一覧取得失敗時は empty state ではなく error state を出し分ける。
- `schema.d.ts` の Token 一覧 GET に 400 レスポンスを追加し、仕様と一致させる。

### 任意改善
- 一覧の `hasMore` / `nextCursor` は props で渡しているが未使用なので、MVP で使わないなら props を削るか TODO を残して意図を明示したい。
- 一覧件数表示 `items.length tokens` は revoke 済みも含むため、必要なら Active / Revoked の内訳を分けると運用しやすい。

### テスト不足
- create 成功後に accidental close を防ぐ E2E/コンポーネントテストがない。
- revoke 失敗時にエラーメッセージが表示されることを確認する UI テストがない。
- tokens 一覧取得失敗時に empty state ではなく error state を出すテストがない。

### ドキュメント確認
- `docs/backend/api.md` の Token Management API 仕様とは概ね整合するが、frontend schema の 400 欠落が残る。
- `docs/spec.md` の「Webでの表示・管理」を満たす方向ではあるが、運用上重要な error visibility が不足している。
- `README.md` には本 Issue で追加更新すべき明示的項目は見当たらなかった。

### plan / research / docs との不整合
- 計画の 1-6 はファイル配置レベルでは概ね実施済み。
- Research 成果物として記載された 2 ファイルはワークスペース内に存在せず、レビュー根拠として追跡できない。実際に参照した文書が別名なら worklog か docs/external に揃えるべき。

### 残リスク
- 現状のままマージすると、create 直後の token 紛失問い合わせと revoke 失敗時の問い合わせが起きやすい。
- network failure / backend transient error を empty state と誤認させるため、障害検知も遅れる。

### PR/完了結果
- Issue #37 review 完了。
- pr_ready: false

## レビュー指摘修正 (2回目実装)

### 修正内容

1. **[High] 平文トークン表示後の accidental close 防止** (`create-token-dialog.tsx`)
   - `Dialog.Root` に `closeOnInteractOutside={!createdToken}` と `closeOnEscape={!createdToken}` を追加
   - `createdToken` セット後は `Dialog.CloseTrigger` を非表示にし、明示的な「閉じる」ボタンのみで閉じる

2. **[High] revoke 失敗時のエラー表示** (`revoke-token-dialog.tsx`)
   - `error` state を追加
   - API失敗時に `apiError.error.message` をセット
   - `Dialog.Body` 内にエラーテキストを赤色で表示

3. **[Medium] token 一覧取得失敗時の error state** (`tokens/page.tsx`, `token-list.tsx`)
   - `tokensRes.error` 時に `fetchError` 文字列を生成
   - `TokenList` に `fetchError` prop を追加
   - error 時は「トークン一覧の取得に失敗しました」を表示（空配列フォールバックではなく明示的エラー）

4. **[Medium] schema.d.ts に GET 400 追加** (`schema.d.ts`)
   - GET `/api/v1/repositories/{github_repository_id}/api-tokens` の responses に `400: { content: { "application/json": ApiError } }` を追加

### テスト結果
- `pnpm tsc --noEmit` → PASS
- `pnpm build` → PASS

### 残リスク
- E2E/コンポーネントテストは未追加（UIテスト基盤が未整備のため現時点ではスキップ）
- ページネーション UI は引き続き未実装（MVP範囲外）
