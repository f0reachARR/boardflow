# Issue #73: boardflow-action entrypointをRustバイナリへ移行

## 経緯
- ユーザー要望5: シェルスクリプトベースのentrypointをRustバイナリに移行
- 既存Issue #73 (OPEN) がそのまま要望に合致

## ユーザー要望
- `action/entrypoint.sh` (bash ~530行) を Rust バイナリに移行

## Issue状態
- 既存Issue #73 がOPENで、内容は十分に詳細（Phase 1-4のスコープ、設計方針あり）
- 更新不要、そのまま処理対象とする

## 後続処理タイプ
`implementation_required`

## 残リスク
- Dockerfile内でのRustビルド時間
- Docker imageサイズへの影響
