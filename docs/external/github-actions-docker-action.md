# GitHub Actions Docker Container Action 仕様

## 概要

GitHub Actions Docker container actionは、Dockerコンテナ内でアクションロジックを実行する仕組み。
Linuxランナー専用。

**出典**:
- https://docs.github.com/en/actions/creating-actions/metadata-syntax-for-github-actions
- https://docs.github.com/en/actions/creating-actions/creating-a-docker-container-action
- https://docs.github.com/en/actions/reference/workflows-and-actions/variables

## ファイル構成

```
my-action/
  action.yml        # アクションメタデータ (必須)
  Dockerfile        # コンテナ定義
  entrypoint.sh     # 実行スクリプト
```

## action.yml 完全スキーマ (Docker container action)

```yaml
name: 'Action Name'          # 必須
author: 'Author Name'        # オプション
description: 'Description'   # 必須

branding:                    # オプション (GitHub Marketplace用)
  icon: 'package'            # Feather icon名
  color: 'blue'              # white|black|yellow|blue|green|orange|red|purple|gray-dark

inputs:
  input-id:                  # 入力パラメータID (小文字推奨)
    description: '...'       # 必須
    required: true           # オプション (デフォルト: false)
    default: 'value'         # オプション
    deprecationMessage: ''   # オプション (非推奨警告)

outputs:
  output-id:                 # 出力パラメータID
    description: '...'       # 必須

runs:
  using: 'docker'                      # 必須。'docker'固定
  image: 'Dockerfile'                  # 必須。ローカルDockerfile or 'docker://image:tag'
  env:                                 # オプション。コンテナ内環境変数
    KEY: value
  pre-entrypoint: 'setup.sh'           # オプション。entrypoint前スクリプト
  pre-if: 'always()'                   # オプション。pre-entrypoint実行条件
  entrypoint: 'main.sh'               # オプション。ENTRYPOINT上書き
  args:                                # オプション。entrypointへの引数
    - ${{ inputs.input-id }}
    - 'literal'
  post-entrypoint: 'cleanup.sh'        # オプション。entrypoint後スクリプト
  post-if: 'always()'                  # オプション。post-entrypoint実行条件
```

## image 指定方法

```yaml
# 方法1: リポジトリ内Dockerfileからビルド (初回実行時にビルド)
image: 'Dockerfile'

# 方法2: Docker Hubから取得
image: 'docker://debian:stretch-slim'

# 方法3: GitHub Container Registry等から取得 (推奨: 事前ビルド済みで高速)
image: 'docker://ghcr.io/owner/action-image:v1'
```

## inputs の受け渡し

inputsを定義すると、Docker container action内で自動的に環境変数として設定される:
- `input-id: api-url` → 環境変数 `INPUT_API-URL`
- 変換規則: `INPUT_` + 大文字化 (ハイフンはそのまま)

```yaml
# action.yml
inputs:
  api-url:
    description: 'API endpoint URL'
    required: true
runs:
  using: 'docker'
  image: 'Dockerfile'
  args:
    - ${{ inputs.api-url }}  # entrypointの引数として渡す方法
```

```bash
# entrypoint.sh 内でのアクセス方法
# 方法1: 環境変数から (自動注入)
echo "$INPUT_API-URL"

# 方法2: args経由 (位置引数)
API_URL="$1"
```

## outputs の設定

```bash
# entrypoint.sh 内
echo "artifact-url=https://example.com/artifact/123" >> "$GITHUB_OUTPUT"
echo "status=success" >> "$GITHUB_OUTPUT"
```

## Job Summary ($GITHUB_STEP_SUMMARY)

```bash
# entrypoint.sh 内
echo '## Build Results' >> "$GITHUB_STEP_SUMMARY"
echo '' >> "$GITHUB_STEP_SUMMARY"
echo '| Check | Result |' >> "$GITHUB_STEP_SUMMARY"
echo '|-------|--------|' >> "$GITHUB_STEP_SUMMARY"
echo "| ERC | ✅ Pass |" >> "$GITHUB_STEP_SUMMARY"
echo "| DRC | ❌ 3 errors |" >> "$GITHUB_STEP_SUMMARY"
echo '' >> "$GITHUB_STEP_SUMMARY"
echo '[View full report](https://example.com/report)' >> "$GITHUB_STEP_SUMMARY"
```

- GitHub Flavored Markdown完全サポート (テーブル、コードブロック、Mermaid等)
- 複数回の `>>` 追記可能 (同一ステップ内)
- ステップごとにファイルパスが変わる

## Docker container action 内で利用可能な環境変数

### GitHub提供のデフォルト環境変数 (entrypoint内で利用可能)

| 変数 | 説明 | 例 |
|------|------|-----|
| `GITHUB_REPOSITORY` | owner/repo | `octocat/Hello-World` |
| `GITHUB_SHA` | コミットSHA | `ffac537e6cbb...` |
| `GITHUB_REF` | ref (フル形式) | `refs/heads/main` |
| `GITHUB_REF_NAME` | ブランチ/タグ名 | `main` |
| `GITHUB_RUN_ID` | 実行ID | `1658821493` |
| `GITHUB_RUN_ATTEMPT` | 試行回数 | `1` |
| `GITHUB_RUN_NUMBER` | 実行番号 | `3` |
| `GITHUB_WORKSPACE` | ワークスペースパス | `/github/workspace` |
| `GITHUB_OUTPUT` | 出力ファイルパス | (ステップ毎に異なる) |
| `GITHUB_STEP_SUMMARY` | サマリファイルパス | (ステップ毎に異なる) |
| `GITHUB_SERVER_URL` | GitHubサーバURL | `https://github.com` |
| `GITHUB_API_URL` | API URL | `https://api.github.com` |
| `GITHUB_EVENT_NAME` | イベント名 | `push` |
| `GITHUB_EVENT_PATH` | イベントペイロードJSON | `/github/workflow/event.json` |
| `GITHUB_ACTOR` | 実行者名 | `octocat` |
| `GITHUB_TOKEN` | 自動生成トークン | (secrets経由で渡す必要あり) |
| `RUNNER_TEMP` | 一時ディレクトリ | `/home/runner/work/_temp` |

### 注意事項

- Dockerfile内の `RUN` 命令ではGITHUB_*変数は利用不可 (ビルド時にはまだ設定されない)
- entrypoint.sh の実行時にのみ利用可能
- `/github/workspace` にリポジトリのチェックアウト内容がマウントされる
- `GITHUB_TOKEN` はsecretsから明示的にinput経由で渡す必要がある:

```yaml
# ワークフロー側
- uses: owner/action@v1
  with:
    token: ${{ secrets.GITHUB_TOKEN }}
```

## Docker container actionの制約

- **Linuxランナー専用** (ubuntu-latest等)
- image指定が `'Dockerfile'` の場合、毎回ビルドが走る (キャッシュなし)
- 事前ビルド済みイメージ (`docker://ghcr.io/...`) を使えば起動が高速
- outputs上限: 1 MB/job, 全outputs合計 50 MB/workflow run
