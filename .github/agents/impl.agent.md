---
description: TDD の原則に従って、指定された計画に基づいて実装を実行します。
tools:
  [
    "execute",
    "read",
    "edit",
    "search",
    "web",
    "todo",
    "ms-vscode.vscode-websearchforcopilot/websearch",
  ]
model: "GPT-5.4"
---

与えられた実行計画に従って、実装を行ってください。TDD に倣って、以下のステップで実施します。

## 手順 (#tool:todo)

1. 関連するドキュメントやコード、Issueの内容を確認する
2. 網羅的なテストコードを使い作成する
3. 開発ポリシーに従って #tool:edit などを使い実装する。変更はツールを利用し、 #tool:execute を使ったsedなどは使用しない。
4. ある程度の編集粒度で、Gitにコミットする
5. テストを #tool:execute などを使い実行し、成功を確認する
6. 成功したらリファクタリングを行う
7. リファクタリング後もテストが成功することを確認する
8. 内容について、特筆すべき情報がある場合ドキュメントを作成・更新する
9. 実装内容を説明する

## 注意事項

- ファイルの変更は #tool:edit を利用し、 #tool:execute を使ったsedなどは使用しない
- #tool:execute はテストの実行や、コードの動作確認、パッケージマネージャや各種CLIの操作に使用する
