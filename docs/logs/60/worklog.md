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

## 調査結果

### 1. GitHub Actions Docker container action の action.yml 仕様

**出典**: https://docs.github.com/en/actions/reference/workflows-and-actions/metadata-syntax#runs-for-docker-container-actions

#### `runs` セクション（Docker container actions向け）の完全なschema:

```yaml
runs:
  using: 'docker'           # 必須。'docker'固定
  image: 'Dockerfile'       # 必須。'Dockerfile'(ローカルビルド) or 'docker://image:tag'(レジストリ)
  env:                      # オプション。コンテナ内の環境変数をkey/valueで設定
    KEY: value
  pre-entrypoint: 'setup.sh'   # オプション。entrypoint前に実行するスクリプト
  pre-if: ''                    # オプション。pre-entrypointの実行条件
  entrypoint: 'main.sh'        # オプション。DockerfileのENTRYPOINTを上書き
  args:                         # オプション。ENTRYPOINTに渡す引数(CMDの代替)
    - ${{ inputs.param1 }}
    - 'literal-value'
  post-entrypoint: 'cleanup.sh' # オプション。entrypoint後のクリーンアップスクリプト
  post-if: ''                    # オプション。post-entrypointの実行条件
```

#### 重要なポイント:
- **Linuxランナー限定**: Docker Actionsは `ubuntu-latest` 等のLinuxランナーでのみ動作
- **image指定方法**: `'Dockerfile'`（リポジトリ内Dockerfileからビルド）または `'docker://debian:stretch-slim'`（レジストリから取得）
- **inputs → 環境変数**: inputを定義すると `INPUT_<UPPERCASE_NAME>` 環境変数が自動設定される
- **args経由で入力渡し**: Docker container actionsでは `args` キーワードでinputsをentrypointに渡す
- **ファイル名**: `action.yml` または `action.yaml`（名前変更するとMarketplaceで旧バージョンが非表示に）

#### action.yml トップレベル構造:
```yaml
name: 'Action Name'          # 必須
author: 'Author'             # オプション
description: 'Description'   # 必須
branding:                    # オプション (Marketplace用)
  icon: 'package'
  color: 'blue'
inputs:
  input-id:
    description: '...'       # 必須
    required: true/false     # オプション
    default: 'value'         # オプション
    deprecationMessage: ''   # オプション
outputs:
  output-id:
    description: '...'       # 必須
runs:
  using: 'docker'
  image: 'Dockerfile'
  args:
    - ${{ inputs.input-id }}
```

#### ファイルシステムマッピング:
- ランナーの `GITHUB_WORKSPACE` → コンテナ内 `/github/workspace` に自動マウント
- コンテナが `/github/workspace` に出力したファイルは後続ステップからアクセス可能

---

### 2. GitHub Actions Job Summary ($GITHUB_STEP_SUMMARY)

**出典**: https://github.blog/news-insights/product-news/supercharging-github-actions-with-job-summaries/

#### 基本的な使い方:
```bash
# シンプルなMarkdown追記
echo '### Hello world! 🚀' >> $GITHUB_STEP_SUMMARY

# 複数行のMarkdown
echo '### Build Results' >> $GITHUB_STEP_SUMMARY
echo '| File | Status |' >> $GITHUB_STEP_SUMMARY
echo '|------|--------|' >> $GITHUB_STEP_SUMMARY
echo '| main.c | ✅ Pass |' >> $GITHUB_STEP_SUMMARY
```

#### 仕組み:
- `$GITHUB_STEP_SUMMARY` はランナー上のファイルパスを格納する環境変数
- 例: `/home/runner/_layout/_work/_temp/_runner_file_commands/step_summary_<uuid>`
- **ステップごとにパスが変わる**（各ステップ固有のファイル）
- GitHub Flavored Markdown（GFM）をフルサポート
- テーブル、コードブロック、リンク、Mermaid図、HTML、絵文字が使用可能
- 複数ステップから `>>` で追記可能（同一ステップ内でも複数回追記OK）
- **Docker container action内でも `$GITHUB_STEP_SUMMARY` は利用可能**（環境変数として自動注入される）

#### boardflow-actionでの活用想定:
- KiCad ERC/DRCの結果サマリ表示
- 生成アーティファクト一覧テーブル
- アップロード結果のリンク表示

---

### 3. Docker container action 内でアクセス可能な環境変数

**出典**: https://docs.github.com/en/actions/reference/workflows-and-actions/variables#default-environment-variables

Docker container actionのentrypoint内では、以下のGitHubデフォルト環境変数にすべてアクセス可能:

| 変数名 | 内容 | 用途(boardflow-action) |
|--------|------|----------------------|
| `GITHUB_REPOSITORY` | `owner/repo` 形式 | API呼び出し時のリポジトリ識別 |
| `GITHUB_SHA` | トリガーしたコミットSHA | アーティファクトとコミットの紐付け |
| `GITHUB_REF` | `refs/heads/branch` 形式 | ブランチ/タグ情報取得 |
| `GITHUB_REF_NAME` | ブランチ/タグの短縮名 | 表示用 |
| `GITHUB_RUN_ID` | ワークフロー実行ID (数値) | 一意な実行識別 |
| `GITHUB_RUN_ATTEMPT` | リラン回数 (1始まり) | リトライ判定 |
| `GITHUB_WORKSPACE` | ランナーのワークスペースパス | ソースコード読み取り先 |
| `GITHUB_OUTPUT` | 出力設定ファイルのパス | `echo "key=value" >> $GITHUB_OUTPUT` |
| `GITHUB_STEP_SUMMARY` | Job Summaryファイルのパス | Markdownサマリ出力 |
| `GITHUB_SERVER_URL` | `https://github.com` | URL組み立て |
| `GITHUB_API_URL` | `https://api.github.com` | API呼び出し |
| `GITHUB_ACTION_PATH` | アクション自体のパス (composite用) | - |
| `GITHUB_EVENT_NAME` | トリガーイベント名 | push/pull_request判定 |
| `GITHUB_EVENT_PATH` | イベントペイロードJSONのパス | 詳細なイベント情報取得 |
| `GITHUB_ACTOR` | ワークフロー起動者 | 情報表示 |
| `RUNNER_TEMP` | 一時ディレクトリ | 中間ファイル出力先 |

#### Docker container action固有の注意点:
- **すべてのデフォルト環境変数はentrypoint内で利用可能**（Dockerfile内のRUNでは不可、entrypoint.shの実行時のみ）
- inputs は `INPUT_<UPPERCASE_NAME>` として自動注入される（例: `input: api-url` → `$INPUT_API-URL`）
- `/github/workspace` にワークスペースがマウントされる
- `$GITHUB_OUTPUT` / `$GITHUB_STEP_SUMMARY` は実ファイルパス。Docker内でも書き込み可能

---

### 調査結論

| 項目 | ステータス |
|------|-----------|
| 結論ステータス | `implementation_required` |
| 追加ドキュメント | `docs/external/github-actions-docker-action.md` を新規作成推奨 |
| ブロッカー | なし |

#### 追加ドキュメント推奨理由:
既存の `docs/external/` には GitHub Actions Docker container action の action.yml 仕様・Job Summary・環境変数に関するドキュメントがない。実装時に参照できるよう、上記の調査結果を `docs/external/github-actions-docker-action.md` として整理することを推奨する。

#### 実装への示唆:
1. `action.yml` では `using: 'docker'` + `image: 'docker://ghcr.io/...'`（事前ビルド済みイメージ）方式が高速
2. entrypoint.sh 内で `$INPUT_*` で入力を受け取り、`$GITHUB_OUTPUT` / `$GITHUB_STEP_SUMMARY` で結果を返す
3. ワークスペース（KiCadプロジェクト）は `/github/workspace` でアクセス可能
4. `$GITHUB_REPOSITORY` / `$GITHUB_SHA` でAPI呼び出し時のコンテキスト情報を取得可能

---

## 計画

### 1. ファイル一覧と役割

```
action/
├── action.yml          # GitHub Actions メタデータ (inputs/outputs/runs定義)
├── Dockerfile          # KiCad 9.0 + 必要ツールのコンテナ定義
├── entrypoint.sh       # メインスクリプト (全フローのオーケストレーション)
└── lib/
    ├── detect.sh       # .boardflow.yml検出・BoardProject候補決定
    ├── config.sh       # .boardflow.yml schema検証・exclude_paths解析
    ├── hash.sh         # file hash / tree_hash 計算
    ├── api.sh          # SaaS API呼び出し (plan/board-run/import/fail)
    ├── kicad.sh        # KiCad CLI実行 (ERC/DRC/PDF/SVG/Gerber/BOM/Position/3D)
    ├── ibom.sh         # InteractiveHtmlBom生成 (xvfb-run)
    ├── bundle.sh       # diff metadata/manifest/fabrication.zip/bundle.zip作成
    └── summary.sh      # GitHub Actions Job Summary出力
```

### 2. action.yml 設計

```yaml
name: 'BoardFlow Action'
description: 'KiCad CI/CD - Generate artifacts and upload to BoardFlow'
author: 'BoardFlow'

branding:
  icon: 'cpu'
  color: 'blue'

inputs:
  token:
    description: 'BoardFlow API token'
    required: true
  mode:
    description: '"auto" or "all"'
    required: false
    default: 'auto'
  exclude-paths:
    description: 'Newline-separated glob patterns to exclude'
    required: false
    default: ''
  api-url:
    description: 'BoardFlow API URL'
    required: false
    default: 'https://api.boardflow.example.com'
  fail-on-drc:
    description: 'Fail action on DRC errors'
    required: false
    default: 'false'
  fail-on-erc:
    description: 'Fail action on ERC errors'
    required: false
    default: 'false'

outputs:
  result:
    description: 'JSON summary of processed projects'

runs:
  using: 'docker'
  image: 'Dockerfile'
```

### 3. Dockerfile 設計

```dockerfile
FROM kicad/kicad:9.0

RUN apt-get update && apt-get install -y --no-install-recommends \
    python3-pip \
    xvfb \
    jq \
    curl \
    zip \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

RUN pip3 install --break-system-packages interactivehtmlbom

COPY entrypoint.sh /action/entrypoint.sh
COPY lib/ /action/lib/

RUN chmod +x /action/entrypoint.sh /action/lib/*.sh

ENTRYPOINT ["/action/entrypoint.sh"]
```

### 4. entrypoint.sh 関数分割設計

#### 4.1 メインフロー (entrypoint.sh)

```
main()
  ├── source lib/*.sh
  ├── parse_inputs()           # INPUT_* 環境変数を正規化
  ├── check_unsupported_event()  # pull_request → 早期成功終了
  ├── detect_projects()        # .boardflow.yml探索 → 候補リスト
  ├── validate_projects()      # schema検証 + 必須ファイル確認
  ├── compute_hashes()         # exclude適用 → file hash → tree_hash
  ├── call_plan_api()          # POST /api/v1/runs/plan
  ├── for each decision=build:
  │     ├── create_board_run()   # POST /api/v1/board-runs
  │     ├── run_kicad_checks()   # ERC + DRC
  │     ├── run_kicad_exports()  # PDF/SVG/Gerber/BOM/Position/3D
  │     ├── run_ibom()           # xvfb-run generate_interactive_bom
  │     ├── collect_kicad_sources()  # kicad/ ディレクトリ収集
  │     ├── generate_diff_metadata() # file_hashes/bom_summary/checks_summary/artifacts_summary/previews
  │     ├── create_fabrication_zip() # gerber + drill → fabrication.zip
  │     ├── create_manifest()    # manifest.json生成
  │     ├── create_bundle()      # bundle.zip作成
  │     ├── upload_bundle()      # presigned URL PUT
  │     └── call_import_api()    # POST /api/v1/board-runs/{id}/artifact-bundles/import
  ├── on failure per project:
  │     └── call_fail_api()      # POST /api/v1/board-runs/{id}/fail
  ├── write_job_summary()        # $GITHUB_STEP_SUMMARY
  └── determine_exit_code()      # fail-on-drc/erc + 検出エラー判定
```

#### 4.2 補助関数一覧

**lib/detect.sh**:
- `find_boardflow_ymls()` — リポジトリ内の全.boardflow.ymlパスをリスト
- `resolve_kicad_pro()` — 同階層の.kicad_proを特定（0個/複数はエラー）
- `resolve_pcb_file()` — 主要.kicad_pcb特定（stem優先 → 単一候補）
- `resolve_root_schematic()` — root .kicad_sch特定（stem優先 → 単一候補）
- `validate_required_files()` — 必須ファイルが除外されていないか確認

**lib/config.sh**:
- `parse_boardflow_yml()` — jqでyaml→JSON変換（yqまたはpython yaml使用）
- `validate_schema_v1()` — version:1のみ許可、未知フィールド拒否
- `get_exclude_paths()` — yml exclude_paths抽出
- `merge_excludes()` — built-in + input + yml の和集合

**lib/hash.sh**:
- `compute_file_sha256()` — 単一ファイルのsha256sum
- `list_project_files()` — project_dir配下のファイル列挙（exclude適用後）
- `compute_tree_hash()` — sorted(path\0sha256\n) の sha256

**lib/api.sh**:
- `api_request()` — curl共通ラッパー（token/content-type/error handling）
- `call_plan_api()` — Plan APIリクエスト組み立て・レスポンス解析
- `call_create_board_run()` — BoardRun作成・presigned URL取得
- `call_import_api()` — Import API呼び出し
- `call_fail_api()` — 失敗API呼び出し

**lib/kicad.sh**:
- `run_erc()` — `kicad-cli sch erc --format json`
- `run_drc()` — `kicad-cli pcb drc --format json`
- `export_schematic_pdf()` — `kicad-cli sch export pdf`
- `export_pcb_pdf()` — `kicad-cli pcb export pdf`
- `export_pcb_svg()` — `kicad-cli pcb export svg` (top/bottom)
- `export_gerbers()` — `kicad-cli pcb export gerbers`
- `export_drill()` — `kicad-cli pcb export drill`
- `export_bom()` — `kicad-cli sch export bom` (CSV)
- `export_position()` — `kicad-cli pcb export pos`
- `export_3d_render()` — `kicad-cli pcb render` (top/bottom)

**lib/ibom.sh**:
- `run_ibom()` — `xvfb-run python3 -m InteractiveHtmlBom ...`

**lib/bundle.sh**:
- `create_fabrication_zip()` — gerber/ + drill/ → fabrication/fabrication.zip
- `generate_file_hashes_json()` — diff/file_hashes.json
- `generate_bom_summary_json()` — diff/bom_summary.json
- `generate_checks_summary_json()` — diff/checks_summary.json
- `generate_artifacts_summary_json()` — diff/artifacts_summary.json
- `generate_previews_json()` — diff/previews.json
- `create_manifest()` — manifest.json (artifacts + checks + diff_metadata)
- `create_bundle_zip()` — 全成果物を1つのzipにまとめる
- `compute_bundle_sha256()` — bundle.zipのsha256

**lib/summary.sh**:
- `write_job_summary()` — GFM形式でProject一覧/check結果/artifact状態をSummaryへ
- `write_unsupported_event_summary()` — unsupported eventの場合の簡易表示

### 5. エラーハンドリング方針

| レベル | 条件 | 対応 |
|--------|------|------|
| Fatal (即時終了) | Plan API認可エラー、.boardflow.yml 0件 | Job失敗、全プロジェクト中断 |
| Project Fatal | BoardRun作成失敗、bundle upload失敗、import要求失敗 | fail API呼び出し → 他プロジェクト継続 → 最終的にJob失敗 |
| Project Error | 検出エラー（schema不正、.kicad_pro不在、必須ファイル除外） | Plan APIに送らず → Job Summary表示 → Job失敗 |
| Artifact Error | 個別kicad-cli実行失敗 | artifact status=failed → manifest記録 → BoardRunは継続 |
| Soft Fail | DRC/ERCエラー | 成果物uploadは完了 → fail-on-drc/ercの場合のみ最終exit code=1 |
| Skip | decision=skip | Job Summary表示のみ → 正常扱い |

**共通方針**:
- `set -euo pipefail` は使わない（部分失敗で継続するため）
- 各関数は戻り値で成否を返す（0=成功、非0=失敗）
- エラー詳細は変数/一時ファイルに蓄積し、最後にSummary出力
- API呼び出しは最大3回リトライ（exponential backoff、5xx/タイムアウトのみ）
- KiCad CLI実行はタイムアウト300秒（個別コマンドごと）

### 6. テスト方針

#### 6.1 ユニットテスト（shell関数単位）

テストフレームワーク: **bats-core** (Bash Automated Testing System)

```
action/tests/
├── test_detect.bats       # detect.sh の各関数テスト
├── test_config.bats       # config.sh schema検証テスト
├── test_hash.bats         # hash.sh tree_hash計算テスト
├── test_bundle.bats       # bundle.sh manifest/zip生成テスト
├── test_summary.bats      # summary.sh 出力フォーマットテスト
├── test_entrypoint.bats   # メインフロー分岐テスト
└── fixtures/
    ├── valid_project/     # 正常なKiCadプロジェクト構造
    ├── invalid_schema/    # schema不正な.boardflow.yml
    ├── no_pcb/            # .kicad_pcb欠損
    ├── multi_pcb/         # 複数.kicad_pcb
    └── mock_api_responses/ # API応答のモックJSON
```

**テスト対象（優先度高）**:
1. `compute_tree_hash()` — ファイル順序非依存、exclude適用の正確性
2. `validate_schema_v1()` — 有効/無効パターン網羅
3. `resolve_kicad_pro()` — 0/1/複数 .kicad_pro の各ケース
4. `merge_excludes()` — built-in/input/yml 和集合の正確性
5. `create_manifest()` — JSON構造の正確性

#### 6.2 統合テスト

- Docker buildが成功するか
- サンプルKiCadプロジェクト(samples/)を使った end-to-end テスト
- API呼び出しはモックサーバー（ncat等）で受ける
- unsupported event (pull_request) の早期終了テスト

#### 6.3 CIでの実行

- GitHub Actions上で `bats` テストを実行するワークフロー
- Docker build テスト（imageが正常にビルドできるか）
- KiCad CLI の基本動作確認（samples/ を使用）

### 7. 実装順序（TDD前提）

#### Phase 1: 基盤 (スケルトン + 検出)
1. `action/action.yml` — inputs/outputs定義
2. `action/Dockerfile` — ビルド確認のみ（ENTRYPOINTはecho）
3. `action/entrypoint.sh` — スケルトン（source + parse_inputs + event判定）
4. `action/lib/detect.sh` — `find_boardflow_ymls()` + テスト
5. `action/lib/detect.sh` — `resolve_kicad_pro()` + テスト
6. `action/lib/detect.sh` — `resolve_pcb_file()` / `resolve_root_schematic()` + テスト

#### Phase 2: 設定・ハッシュ
7. `action/lib/config.sh` — YAML解析（python3 -c 'import yaml; ...' でパース）
8. `action/lib/config.sh` — `validate_schema_v1()` + テスト
9. `action/lib/config.sh` — `merge_excludes()` + テスト
10. `action/lib/hash.sh` — `list_project_files()` (exclude適用) + テスト
11. `action/lib/hash.sh` — `compute_tree_hash()` + テスト

#### Phase 3: API連携
12. `action/lib/api.sh` — `api_request()` 共通ラッパー
13. `action/lib/api.sh` — `call_plan_api()` + モックテスト
14. `action/lib/api.sh` — `call_create_board_run()` + モックテスト
15. `action/lib/api.sh` — `call_import_api()` / `call_fail_api()` + モックテスト

#### Phase 4: KiCad実行
16. `action/lib/kicad.sh` — `run_erc()` / `run_drc()` + samples/テスト
17. `action/lib/kicad.sh` — PDF/SVG export関数群 + テスト
18. `action/lib/kicad.sh` — Gerber/Drill/BOM/Position/3D export + テスト
19. `action/lib/ibom.sh` — `run_ibom()` + テスト

#### Phase 5: Bundle作成
20. `action/lib/bundle.sh` — `create_fabrication_zip()` + テスト
21. `action/lib/bundle.sh` — diff metadata生成関数群 + テスト
22. `action/lib/bundle.sh` — `create_manifest()` + テスト
23. `action/lib/bundle.sh` — `create_bundle_zip()` + テスト

#### Phase 6: Summary + 統合
24. `action/lib/summary.sh` — Job Summary出力 + テスト
25. `action/entrypoint.sh` — メインフロー統合
26. 統合テスト（Docker + samples/ + モックAPI）
27. fail-on-drc / fail-on-erc の終了コード制御

### 8. .boardflow.yml パース方法

Dockerイメージには python3 が含まれるため、YAMLパースは以下で行う:

```bash
parse_boardflow_yml() {
  local yml_path="$1"
  python3 -c "
import sys, json, yaml
with open(sys.argv[1]) as f:
    print(json.dumps(yaml.safe_load(f)))
" "$yml_path"
}
```

Dockerfile に `pyyaml` を追加インストールする（`pip3 install pyyaml`）。

### 9. glob マッチング方法

bash の `find` + カスタムマッチング関数、または python3 ワンライナーで対応:

```bash
matches_glob() {
  local path="$1" pattern="$2"
  python3 -c "
import sys, fnmatch
sys.exit(0 if fnmatch.fnmatch(sys.argv[1], sys.argv[2]) else 1)
" "$path" "$pattern"
}
```

### 10. 前提条件・依存関係

- samples/ ディレクトリのKiCadプロジェクトをテスト用fixtureとして活用
- SaaS API (Plan/BoardRun/Import/Fail) のエンドポイントが実装済みであること（Issue #55-#59相当）
- bats-core はDockerイメージ内ではなくCIランナー上で実行（テスト用）

### 11. 計画ステータス

**実装可能**: すべての技術要素が調査済みであり、spec.mdの仕様が十分に詳細。ブロッカーなし。

---

## 実装内容

### 作成したファイル一覧

| ファイル | 役割 |
|---------|------|
| `action/action.yml` | GitHub Actions メタデータ（inputs/outputs/runs定義） |
| `action/Dockerfile` | KiCad 9.0 + python3-pip, xvfb, jq, curl, zip, interactivehtmlbom |
| `action/entrypoint.sh` | メインオーケストレーション（全フロー制御） |
| `action/lib/detect.sh` | .boardflow.yml検出・.kicad_pro/.kicad_pcb/.kicad_sch解決 |
| `action/lib/config.sh` | YAML→JSONパース・schema v1バリデーション・exclude_pathsマージ |
| `action/lib/hash.sh` | SHA256計算・tree_hash計算・BUILTIN_EXCLUDES定数定義 |
| `action/lib/api.sh` | API呼び出しラッパー（plan/create/import/fail）＋3回リトライ |
| `action/lib/kicad.sh` | KiCad CLI全コマンドラッパー（ERC/DRC/PDF/SVG/Gerber/Drill/BOM/Pos/3D） |
| `action/lib/ibom.sh` | xvfb-run generate_interactive_bom（非致命的失敗） |
| `action/lib/bundle.sh` | fabrication.zip/manifest.json/bundle.zip/メタデータJSON生成 |
| `action/lib/summary.sh` | GITHUB_STEP_SUMMARYへのGFMテーブル出力 |

### 主要な実装判断

1. **`set -euo pipefail` を使用しない**: 部分失敗時にBoardRunを継続するため、各コマンドの戻り値を個別にチェック
2. **ERC/DRC exit code 5を成功扱い**: 違反検出だがJSONは正常に生成される仕様に準拠
3. **iBOM失敗は非致命的**: artifact status=failedとして記録し、BoardRun全体は継続
4. **APIリトライ**: exponential backoff (1s→2s→4s)、5xx/タイムアウト時のみ
5. **tree_hash**: `printf "%b"` でnull byte + hash文字列を連結し、sorted relative_pathsで計算
6. **INPUT_*環境変数**: ハイフン付き入力名はそのまま `INPUT_EXCLUDE-PATHS` 等で参照
7. **pull_requestイベント**: サマリ出力のみで即座にexit 0（未サポートイベント）
8. **タイムアウト**: 全KiCad CLIコマンドに300秒のtimeoutを設定
