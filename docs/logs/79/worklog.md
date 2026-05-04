# Issue #79: TanStack Formへのフォームリファクタリング

## 経緯
- ユーザー要望8: すべてのフォームをTanStack Formに移行

## ユーザー要望
- フォームの状態管理をTanStack Formに統一

## 調査結果
- 現在のフォーム: Token作成ダイアログ (`create-token-dialog.tsx`) で `useState` による手動管理
- バリデーションは手動（文字列長チェック等）
- ローディング/エラー状態も `useState` で個別管理

## Issue作成内容
- タイトル: TanStack Formへのフォームリファクタリング
- ラベル: enhancement, frontend
- 新規作成

## 後続処理タイプ
`implementation_required`

## 残リスク
- TanStack Form + Chakra UI v3 の統合パターンの調査が必要
- Issue #78 (TanStack Query mutation) との連携
