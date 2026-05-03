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
