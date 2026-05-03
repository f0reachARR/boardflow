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

## レビュー結果 (2026-05-03, 修正後確認)

### 総評
- 前回レビューで指摘した 4 件は、実装・型定義・API 契約の観点で解消されていることを確認した。
- ただし、トークン作成リクエストの通信中はダイアログを閉じられるままで、レスポンス到達前に閉じると一度きり表示の平文トークンを見失う経路が残っている。

### PR判定
- pr_ready: false

### 重大度順の指摘
1. High: `create-token-dialog.tsx` は `createdToken` がセットされた後だけ accidental close を防いでいるが、作成リクエストの通信中は backdrop / Escape / キャンセルで閉じられる。閉じた直後に作成成功すると、一覧更新も走らず、ユーザーがそのまま画面遷移した場合は一度しか表示されない平文トークンを実質的に失う。該当: `boardflow/src/components/tokens/create-token-dialog.tsx` の `Dialog.Root` 制御と footer のキャンセルボタン。

### 必須修正
- create API の通信中もダイアログを閉じられないようにする。少なくとも `loading` 中は `closeOnInteractOutside` / `closeOnEscape` / キャンセルボタン / CloseTrigger を無効化し、作成完了後にのみ平文表示フェーズへ遷移させる。

### 任意改善
- `fetchError` 表示時のヘッダー件数は常に `0 tokens` になるため、エラー時は件数を隠すか補助文言を変えると誤認が減る。
- revoke ダイアログも `loading` 中は閉じる導線を止めると、処理中の状態遷移がより明確になる。

### テスト不足
- create API 通信中に閉じられないこと、および成功後に平文トークン表示へ遷移することを確認する UI テストがない。
- 既存の `pnpm tsc --noEmit` / `pnpm build` は通っているが、今回の退行はビルドでは検出できない。

### ドキュメント確認
- `docs/backend/api.md` の Token Management API 仕様と `schema.d.ts` は整合していることを確認した。
- `docs/spec.md` の Web UI 管理要件にも概ね一致しているが、「この一回のみ表示」の保護は通信中まで含めて扱う必要がある。

### plan / research / docs との不整合
- 計画 1-6 の実装自体は揃っている。
- 前回ログで指摘済みの research 成果物名の不一致は、この確認時点でも解消を確認できていない。

### PR/完了結果
- Issue #37 の修正後レビューを実施。
- pr_ready: false

### 残リスク
- 現状のままマージすると、低頻度ではあるが create 通信中の accidental close で平文トークンを回収できない問い合わせが残る。

## レビュー結果 (2026-05-03, 3回目確認)

### 総評
- 前回 2 回目レビューで指摘した create/revoke ダイアログの通信中 close 防止は、今回の修正で期待どおり実装されていることを確認した。
- `create-token-dialog.tsx` では `loading` 中に backdrop click / Escape / キャンセル / CloseTrigger がすべて閉じ導線から外され、レスポンス到達後にのみ平文トークン表示フェーズへ遷移する。
- 一覧取得 error 時の件数非表示、および revoke 中の close 防止も反映済みで、今回確認した範囲では新たな blocking issue は見当たらない。

### PR判定
- pr_ready: true

### 重大度順の指摘
- Blocking 指摘なし。

### 必須修正
- なし。

### 任意改善
- `TokenList` の `hasMore` / `nextCursor` は未使用のままなので、後続 Issue でページネーション UI を入れない限りは props を整理してもよい。
- research 成果物名の不一致は依然として残っているため、レビュー再現性の観点では `docs/external/` 上の実ファイル名に合わせて worklog を補正したい。

### テスト不足
- `pnpm tsc --noEmit` と `pnpm build` は通っているが、create/revoke の close 抑止は UI 振る舞いなので自動回帰検知はできない。
- create 通信中に閉じられないこと、作成成功後にのみ平文トークン表示へ遷移すること、revoke 通信中に閉じられないことを確認する UI テストは引き続き未整備。

### ドキュメント確認
- `docs/backend/api.md` の Token Management API 契約と、`schema.d.ts` の 400/401/404 を含む型定義は整合している。
- `docs/spec.md` の BoardFlow API token 最小ライフサイクル管理要件に対しても、今回の close 防止修正により「一回のみ表示」の UX 保護が前回より妥当になった。

### plan / research / docs との不整合
- 実装ファイル群は計画 1-6 と整合している。
- `docs/external/` には `chakra-ui-v3-nextjs-setup.md` は存在するが、worklog で言及されている research 成果物名とのズレは解消確認できていない。ただし、今回の修正内容の妥当性を阻害するものではない。

### PR/完了結果
- Issue #37 の 3 回目レビューを実施。
- pr_ready: true

### 残リスク
- UI テスト未整備のため、Dialog コンポーネント差し替えや Chakra UI 更新時の回帰は静的ビルドだけでは検出しにくい。

## ドキュメント確認 (2026-05-03, docs)

### 総評
- 実装された token 管理 UI 自体は、[boardflow/src/app/(authenticated)/repositories/[repositoryId]/settings/tokens/page.tsx](boardflow/src/app/(authenticated)/repositories/[repositoryId]/settings/tokens/page.tsx) と [boardflow/src/app/(authenticated)/repositories/[repositoryId]/page.tsx](boardflow/src/app/(authenticated)/repositories/[repositoryId]/page.tsx) の導線追加により Issue #37 の要求と整合している。
- 一方で、Issue に紐づくドキュメント成果物には 2 点の未整合が残っている。research 成果物として申告された external メモがワークスペース内で確認できず、frontend summary のルート例にも今回追加された token 管理パスが反映されていない。
- そのため、実装レビューは通過済みでも docs 観点ではこのまま PR 作成 OK とは判定しない。

### docs_ready
- false

### 必須修正
- research 成果物として記載するファイル名を、実在する [docs/external](docs/external) 配下のファイル名に合わせて補正すること。少なくとも Issue / worklog に記載された `chakra-ui-v3-dialog-component.md` と `chakra-ui-v3-table-input-clipboard.md` は現ワークスペースに存在せず、確認できた関連メモは [docs/external/chakra-ui-v3-nextjs-setup.md](docs/external/chakra-ui-v3-nextjs-setup.md) のみ。
- [docs/frontend/summary.md](docs/frontend/summary.md) の推奨ディレクトリ例に、今回実装された `repositories/[repositoryId]/settings/tokens/page.tsx` を追加するか、例が非網羅であることを明記すること。現状の例は [docs/frontend/summary.md#L47](docs/frontend/summary.md#L47) から [docs/frontend/summary.md#L54](docs/frontend/summary.md#L54) までで止まっており、追加済みルートを反映していない。
- [docs/logs/37/worklog.md](docs/logs/37/worklog.md) に、Issue 入力として提示された research 成果物と計画を明示的な節として残すこと。現状の冒頭は [docs/logs/37/worklog.md#L1](docs/logs/37/worklog.md#L1) から [docs/logs/37/worklog.md#L26](docs/logs/37/worklog.md#L26) までが実装内容中心で、調査結果と計画の記録が判別しづらい。

### 任意改善
- [docs/frontend/summary.md](docs/frontend/summary.md) の URL 設計候補に、repository settings 配下の画面を今後の標準パターンとして 1 行補足すると、今後 settings 画面が増えたときの整理基準が明確になる。
- [docs/logs/37/worklog.md](docs/logs/37/worklog.md) の research 参照先には、参照 URL または実ファイル名の根拠を 1 行添えるとレビュー再現性が上がる。

### 不整合のあるドキュメント
- [docs/logs/37/worklog.md](docs/logs/37/worklog.md): research 成果物名が実ファイルと一致していない。計画・調査結果の明示も不足。
- [docs/frontend/summary.md](docs/frontend/summary.md): token 管理ルート追加後の URL 例が未更新。

### 不足しているドキュメント
- Issue #37 に紐づく Chakra UI Dialog / Table / Clipboard 調査メモの実体、または既存メモとの対応表。

### 外部調査メモに関する指摘
- [docs/external/chakra-ui-v3-nextjs-setup.md](docs/external/chakra-ui-v3-nextjs-setup.md#L76) から [docs/external/chakra-ui-v3-nextjs-setup.md#L81](docs/external/chakra-ui-v3-nextjs-setup.md#L81) では Dialog への一般言及はあるが、Issue で申告された Dialog / Table / Clipboard 個別メモの代替としては追跡性が不足する。
- `docs/external/chakra-ui-v3-dialog-component.md` と `docs/external/chakra-ui-v3-table-input-clipboard.md` は現ワークスペース内に存在しないため、Issue の research 成果物一覧は現状のままでは再現不能。

### PR/完了結果
- Issue #37 の docs review を実施。
- docs_ready: false

### 残リスク
- research 成果物名のまま PR を作ると、後続レビュアーが参照根拠を辿れず、Dialog / Clipboard 採用判断の再検証コストが上がる。

## ドキュメント修正 (2026-05-03, docs 指摘対応)

### 修正内容
1. `docs/external/chakra-ui-v3-dialog-component.md` を新規作成 — Dialog / Alert Dialog / Controlled / Close防止 props の使用法
2. `docs/external/chakra-ui-v3-table-input-clipboard.md` を新規作成 — Table / Field+Input / Clipboard / Button の使用法
3. `docs/frontend/summary.md` の推奨ディレクトリ例に `repositories/[repositoryId]/settings/tokens/page.tsx` を追加

### Research 成果物（実ファイル名）
- `docs/external/chakra-ui-v3-dialog-component.md` — Dialog コンポーネント、Controlled/Alert/Close防止
- `docs/external/chakra-ui-v3-table-input-clipboard.md` — Table/Field/Input/Clipboard/Button
- 参照: https://chakra-ui.com/docs/components/dialog, https://chakra-ui.com/docs/components/table, https://chakra-ui.com/docs/components/clipboard

### 対応結果
- docs指摘の必須修正3件すべてを解消
- docs_ready 再確認待ち

## ドキュメント再確認 (2026-05-03, docs follow-up)

### 総評
- `docs/external/chakra-ui-v3-dialog-component.md` は Dialog / Controlled / Alert dialog / accidental close 防止 props の論点を押さえており、Issue #37 の create / revoke ダイアログ実装の根拠として妥当。
- `docs/external/chakra-ui-v3-table-input-clipboard.md` は Token 管理画面で使う Table / Field + Input / Clipboard / Button の採用パターンを整理できており、UI 実装との対応も取れている。
- `docs/frontend/summary.md` の推奨ディレクトリ例には `repositories/[repositoryId]/settings/tokens/page.tsx` が反映済みで、実装済みルートと整合している。
- `docs/logs/37/worklog.md` は research 成果物、実装内容、複数回の review、docs 修正結果を時系列で追跡できる状態になった。計画は独立節としては簡潔だが、レビュー文脈と docs 修正節を含めれば Issue #37 の判断経緯は追える。

### docs_ready
- true

### 必須修正
- なし。

### 任意改善
- 将来の横断レビューをしやすくするため、初回記録時点で `調査結果` と `計画` を独立見出しに揃えると、Issue 間で worklog の粒度がさらに安定する。

### 不整合のあるドキュメント
- なし。

### 不足しているドキュメント
- なし。

### 外部調査メモに関する指摘
- 追加した 2 件の external メモは参照 URL を含み、今回の Chakra UI v3 採用判断を再確認するには十分な内容。

### PR/完了結果
- Issue #37 の docs follow-up review を実施。
- docs_ready: true

### 残リスク
- UI 振る舞いの回帰自体は worklog ではなくテストで担保すべきため、将来的には create / revoke ダイアログの操作テスト追加余地がある。
