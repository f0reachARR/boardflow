# リファクタリングIssue調査 作業ログ

## 経緯

ユーザーが「既存のIssueに従ってリファクタリングを進めてほしい」と依頼。
まずGitHub上のopen状態のIssueを全件取得し、リファクタリングに関連するものを特定する調査を実施。

## ユーザー要望

既存のGitHub Issueに基づいてリファクタリングを進めたい。

## 調査結果

### 調査日時: 2026-05-14

open状態のIssue: 17件 (#96, #97, #98, #99, #100, #101, #102, #104, #105, #107, #108, #109, #110, #111, #112, #113, #114)

#### リファクタリング関連Issue（15件）

**バックエンド（Rust）: 7件**
- #96: plan/board_run系API handlerからユースケース処理をservice層へ切り出す
- #97: APIのrepository access認可処理を共通サービス化する
- #98: pagination cursor処理を共通化する
- #99: read APIの巨大moduleを機能単位に分割する
- #100: workerのartifact bundle import処理を段階ごとに分割する
- #101: GitHub access checkerを責務別moduleに分割する
- #102: action-runnerのrunner orchestrationを責務別に分割する

**フロントエンド（Next.js/React）: 8件**
- #107: ステータス色・日付・サイズなどの共通フォーマット処理を集約する
- #108: ルート生成を routes.ts に集約してリンク文字列の重複を減らす
- #109: DiffSummary の型ガードと解析処理を共通化する
- #110: RunDetailContent をセクション単位に分割する
- #111: DiffContent をセクション単位に分割する
- #112: ArtifactViewer のタブ選択と KiCanvas fallback 判定を分離する
- #113: Server Component の React Query prefetch パターンを共通化する
- #114: unknown JSON への型キャストを zod parse に置き換える

#### 非リファクタリングIssue（2件）
- #104: GitHub OAuth token 失効時の401ハンドリングを改善する（バグ修正）
- #105: レポジトリ一覧画面の軽量化（パフォーマンス/アーキテクチャ改善）

## 残リスク

- 一部Issue間に依存関係がある可能性（例: #109 → #110, #111; #98 → #99）
- Issue本文は十分に具体的で、そのまま実装可能な状態
- 「refactor」ラベルは存在しないが、「enhancement」ラベルがリファクタリング系Issueに使われている
