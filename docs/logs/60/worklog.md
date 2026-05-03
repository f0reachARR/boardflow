# Issue #60: GitHub Actions向けDocker Action (boardflow-action) 実装

## Issueまでの経緯

- docs/spec.md §2.2でDocker Actionとしてboardflow-actionを提供する方針が定義済み
- Issue #10でKiCad CLI/iBOMのDocker内ヘッドレス利用方法の調査が完了（CLOSED）
- docs/external/kicad-docker-cli.mdに詳細な手順・Dockerfile参考例が文書化済み
- Issue #47でGitHub Actions CIセットアップは完了（CLOSED）だが、ユーザー向けDocker Actionは未実装
- 既存のaction.yml、Dockerfile、entrypointスクリプトは一切存在しない

## ユーザー要望

GitHub Actions向けのDocker Workflowやそこで動かすKiCadのラッパーが存在せず、ActionsからBoardflowを呼び出せない。これを実装する必要がある。

## Issue作成内容

- Issue #60として新規作成
- labels: infrastructure, docker, kicad
- Docker container action (action.yml + Dockerfile + entrypoint) の実装

## 後続処理タイプの初期仮説

`implementation_required`

## 残リスク

- Docker imageサイズが大きい場合のCI実行時間への影響
- GHCRへの事前publishフローの設計が必要
- spec.mdの詳細なフロー（Plan API→KiCad CLI→Import API）の実装複雑度
