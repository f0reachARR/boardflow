# Frontend 技術方針サマリ

BoardFlow の frontend は、GitHub Actions が生成した成果物を人が見て判断しやすくするための UI を担当する。MVP では、装飾よりも一覧性、状態把握、成果物への到達しやすさを優先する。

## 1. 役割

frontend の責務は以下に絞る。

- Repository / BoardProject / BoardRun の一覧・詳細表示
- 成果物プレビューとダウンロード導線の提供
- 期待 artifact の生成済み / 欠損 / 失敗 / 対象外状態の表示
- 差分や最新状態を把握しやすい画面構成
- GitHub 権限に沿った閲覧体験の提供

KiCad 実行、成果物生成、GitHub Issue 更新などの副作用は backend に寄せる。

## 2. 採用スタック

| 領域 | 採用方針 | 理由 |
|---|---|---|
| フレームワーク | Next.js App Router + TypeScript | 読み取り中心画面を Server Components で素直に組みやすい |
| UI | Chakra UI | SaaS 管理画面を短期間で組みやすく、アクセシビリティも確保しやすい |
| アイコン | lucide-react | 軽量で一覧画面や状態表示に合わせやすい |
| API 型 | `openapi-typescript` などの生成型 | OpenAPI と UI の齟齬を減らせる |
| E2E | Playwright | 主要導線の smoke test に向く |

## 3. 画面アーキテクチャ

基本は Server Components を優先し、ブラウザ状態が必要な部分だけ Client Components に切り出す。

### Server Components に寄せる画面

- Repository 一覧
- Repository 詳細
- BoardProject 詳細
- Run 一覧
- Run 詳細の静的な情報表示

### Client Components に寄せる画面要素

- タブ切り替え
- フィルタ、ソート、検索
- iBOM iframe の状態制御
- 画像比較や成果物切り替え
- 軽いインタラクションを伴う preview UI

推奨ディレクトリの叩き台:

```text
app/
  repositories/[repositoryId]/page.tsx
  repositories/[repositoryId]/boards/[boardProjectId]/page.tsx
  repositories/[repositoryId]/boards/[boardProjectId]/runs/page.tsx
  repositories/[repositoryId]/boards/[boardProjectId]/runs/[boardRunId]/page.tsx
components/
lib/api/
```

## 4. UI の重点

BoardFlow は制作物そのものより、制作物の状態変化を追う画面が重要になる。そのため UI では以下を重視する。

- 今どの BoardProject が正常か失敗中かをすぐ把握できること
- 最新 Run と過去 Run の差が追いやすいこと
- PDF / SVG / iBOM / ZIP へ迷わず到達できること
- 期待 artifact が生成済みか、欠損・失敗・対象外かを自然に判断できること
- GitHub Issue と BoardProject の関係が自然に理解できること

一覧画面では、カードよりも表形式やセクション分割の方が相性がよい可能性が高い。特に MVP では、視覚的な派手さよりも運用時の読みやすさを優先する。
Repository / BoardProject の通常一覧には、初回 completed run 前の BoardProject も状態付きで表示する。
Plan APIで検出済み、処理中、失敗、timeout、completed を区別し、初回 import 失敗や未完了のBoardProjectもWeb UIから原因を追えるようにする。

## 5. 認証と権限

ログインは GitHub OAuth を前提にする。frontend で扱うべき主な要件は以下。

- GitHub ログイン済みユーザーだけが画面に入れる
- GitHub App installation と repository 権限に基づいて閲覧可否を切り替える
- 閲覧権限のない repository や artifact には到達できないようにする

認可判定の最終責務は backend に置き、frontend はその結果を自然に表示する。

## 6. Artifact 表示方針

Artifact は private 前提で、S3 互換ストレージから直接 public 配信しない。

frontend で意識する点:

- ダウンロード URL は短命である前提で扱う
- preview / download 用URLは `viewer-sources` APIからviewer単位で取得する
- Schematic / PCB Preview では KiCanvas を第一候補にしつつ、PDF/SVG fallback を必ず残す
- KiCanvas は補助 preview として扱い、`kicanvas` viewer が `missing` / `failed` の場合も静的artifactが `available` なら閲覧導線を残す
- KiCanvas は Client Component に閉じ込め、bundle script は vendoring して外部 CDN から読み込まない
- iBOM HTML は通常の app domain とは分離された artifact domain 上で表示する
- iframe 利用時はレイアウト崩れやクロスドメイン制約を前提に設計する
- 画像や PDF の preview は「すぐ見られること」を優先し、重い比較 UI は後回しにしてよい
- viewer単位の `available` / `partial` / `missing` / `failed` / `skipped` に応じて、表示、限定表示、fallback、理由表示を切り替える
- artifact 一覧では `available` / `missing` / `failed` / `skipped` を表示し、`available` のものだけプレビューやダウンロード導線を出す
- 個別artifactの `missing` / `failed` / `skipped` は警告として表示し、BoardRunが `completed` であればRun詳細や利用可能なartifact閲覧は継続できるようにする

## 7. API 連携

frontend は backend の OpenAPI 契約に追従する。

最低限必要な read API の例:

- repository 一覧取得
- repository 詳細取得
- BoardProject 詳細取得
- BoardRun 一覧取得
- BoardRun 詳細取得
- Artifact 一覧取得
- `GET /api/v1/board-runs/{board_run_id}/viewer-sources`

OpenAPI から TypeScript 型を生成して、画面側の props と API response をなるべく一致させる。

## 8. テスト方針

MVP の frontend test は次を基準にする。

- 主要コンポーネントの component test
- Playwright による主要導線の smoke test
- 権限なし時のガード確認
- 初回 completed run がない BoardProject も通常一覧に状態付きで表示されること
- artifact 欠損状態が一覧と詳細で表示されること
- `viewer-sources` APIの `available` / `partial` / `missing` / `failed` / `skipped` に応じてpreview導線が切り替わること
- KiCanvas viewer コンテナが表示され、失敗時にPDF/SVG fallbackへ戻れること
- timed_out のBoardProjectに「中断または未完了の可能性」が分かる表示を出すこと
- Run 一覧から成果物表示までの代表導線確認

重いビジュアル回帰テストは後回しでよいが、artifact preview 周りは早めに一度自動化したい。

## 9. 今後の深掘り候補

- Repository / BoardProject / Run の URL 設計
- 一覧画面の情報密度とフィルタ UX
- 差分表示の MVP 範囲
- iBOM / PDF / SVG を同じ画面でどう並べるか
- サーバーサイドでの認証セッション管理方法
- OpenAPI generated types の導入手順
