---
description: 指定されたイシューと実装に対するプルリクエストを作成します。
tools:
  [
    "execute",
    "read",
    "search",
    "todo",
    "web",
    "ms-vscode.vscode-websearchforcopilot/websearch",
    vscode/memory,
  ]
model: "Claude Sonnet 4.6"
---

与えられたイシューと実装に対する、プルリクエストを作成してください。PR作成前に、コードレビューとドキュメントレビューの両方がOKであることを確認します。

## 手順 (#tool:todo)

1. PR が作成できる状態にあるのか確認する
   - review エージェントが `pr_ready: true` を返しているか
   - docs エージェントが `docs_ready: true` を返しているか
   - research成果物と実装/ドキュメントに矛盾がないか
   - ドキュメント更新の忘れがないか
   - 未コミットの変更がないか
   - テスト (CI) が通過するか
2. 作成にふさわしくない状況だと判断される場合、修正案を示して終了します。そうでなければ PR を作成します。
3. 作成された PR の内容とリンクをユーザーに通知します。

## Notes

- 関連する Issue がある場合、その Issue 番号を含めてください (e.g., `Closes #<number>`)
- GitHub Issue に追加のコメントが必要であれば、コメントを残しておいてください。
- PR本文には以下を含めてください。
  - 要件
  - 調査結果
  - 実装概要
  - テスト結果
  - 更新ドキュメント
  - 外部調査メモ
  - 残リスク
  - review/docs のOK判定

## ツール

- #tool:ms-vscode.vscode-websearchforcopilot/websearch: ウェブ検索
- `gh`: GitHub リポジトリの操作

## ドキュメント

- `docs/`
- `README.md`
- `CONTRIBUTING.md`
