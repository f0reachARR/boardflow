# Issue #62: OpenAPI定義からschema.d.ts生成の運用整備

## Issueまでの経緯

- boardflow/package.jsonにgenerate:apiスクリプトが既に定義済み（ポート3001指定）
- openapi-typescript v7とopenapi-fetchは依存に含まれ、schema.d.tsも存在する
- Issue #29 (Frontend: Next.jsプロジェクトセットアップ) で基盤構築済み（CLOSED）
- 実際のAPIサーバデフォルトポートは3000（api/config.rsのapi_port）
- schema.d.tsが最新のAPI定義と同期されているか不明確
- 開発時のワークフローとして型生成手順が明文化されていない

## ユーザー要望

APIサーバ側のOpenAPI定義が適切に活用されていない。APIサーバを起動してOpenAPI定義を取得し、schema.d.tsを生成する必要がある。.envは用意済み。

## Issue作成内容

- Issue #62として新規作成
- labels: frontend, infrastructure
- ポート番号修正、schema.d.ts再生成、手順ドキュメント化

## 後続処理タイプの初期仮説

`implementation_required`

## 残リスク

- APIサーバ起動が必要な型生成フローはCI上での自動化が複雑
- ポート番号はenvで変更可能なため、固定値の是非
