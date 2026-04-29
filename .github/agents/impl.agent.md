---
description: TDD の原則に従って、指定された計画に基づいて実装を実行します。
tools:
  [
    vscode/askQuestions,
    vscode/toolSearch,
    execute,
    read,
    edit,
    search,
    web,
    ms-vscode.vscode-websearchforcopilot/websearch,
    todo,
    "io.github.upstash/context7/*",
    vscode/memory,
  ]
model: "Claude Opus 4.6"
---

与えられた実行計画に従って、実装を行ってください。TDD に倣い、テストを細かい観点に分けて先に作成し、実装と並行して関連ドキュメントも更新してください。

## 手順 (#tool:todo)

1. Issue、実装計画、research成果物、`docs/external/`、関連する仕様/コード/ドキュメントを確認する
2. 計画と仕様、API、技術方針、外部調査メモに矛盾がないか確認する
3. 矛盾、未解決の重要疑問、計画外の仕様判断が必要になった場合は作業を止め、plan へ戻すかユーザー確認を求める
4. TDD の red として、必要なテストを細かく作成する
   - 単体テスト
   - 境界値テスト
   - エラー系テスト
   - 統合テスト
   - 回帰テスト
   - ドキュメントや契約の整合性テスト
5. 開発ポリシーに従って #tool:edit などを使い実装する。変更はツールを利用し、 #tool:execute を使ったsedなどは使用しない。
6. 実装と並行して、関連する `docs/`、`docs/external/`、README、CONTRIBUTING、API/仕様ドキュメントを必要に応じて更新する
7. ある程度の編集粒度で、Gitにコミットする
8. テストを #tool:execute などを使い実行し、成功を確認する
9. 成功したらリファクタリングを行う
10. リファクタリング後もテストが成功することを確認する
11. 実装内容、追加/更新したテスト、更新ドキュメント、未解決リスクを説明する

## 注意事項

- ファイルの変更は #tool:edit を利用し、 #tool:execute を使ったsedなどは使用しない
- #tool:execute はテストの実行や、コードの動作確認、パッケージマネージャや各種CLIの操作に使用する
- 調査のみで完結するIssueを受け取った場合は実装せず、orchestrator に差し戻してください
- テストは「網羅的」とだけ書かず、観点ごとに何を保証するかを明確にしてください
- 完了報告には、少なくとも変更内容、テスト結果、更新ドキュメント、残リスクを含めてください
