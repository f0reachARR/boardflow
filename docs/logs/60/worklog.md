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

---

## レビュー結果

**pr_ready: false**

### Critical（修正必須）

#### C-1: 環境変数のパース不正（entrypoint.sh L14-17）

```bash
EXCLUDE_PATHS="${INPUT_EXCLUDE-PATHS:-}"
API_URL="${INPUT_API-URL:-https://api.boardflow.example.com}"
FAIL_ON_DRC="${INPUT_FAIL-ON-DRC:-false}"
FAIL_ON_ERC="${INPUT_FAIL-ON-ERC:-false}"
```

**問題**: bashの変数名にハイフンは使用不可。`${INPUT_EXCLUDE-PATHS:-}` はbashにより `INPUT_EXCLUDE` を変数名、`-` を「未定義時のデフォルト値」演算子、`PATHS:-` をデフォルト値として解釈される。結果として、全入力値が常にデフォルト値（または空文字）になる。

GitHub Actions Docker actionsでは、input名のハイフンはアンダースコアに変換されて環境変数化される。

**修正**: 以下に変更する:
```bash
EXCLUDE_PATHS="${INPUT_EXCLUDE_PATHS:-}"
API_URL="${INPUT_API_URL:-https://api.boardflow.example.com}"
FAIL_ON_DRC="${INPUT_FAIL_ON_DRC:-false}"
FAIL_ON_ERC="${INPUT_FAIL_ON_ERC:-false}"
```

#### C-2: Import API ペイロード不足（entrypoint.sh L381-383）

```bash
import_payload=$(jq -n \
  --arg sha256 "$bundle_sha256" \
  '{bundle_sha256: $sha256}')
```

**問題**: spec §9.3 では `staging_object_key` と `bundle_size_bytes` も必須フィールド。`staging_object_key` は BoardRun作成APIのレスポンスから取得し、`bundle_size_bytes` は bundle.zip のサイズを計算して含める必要がある。

**修正**: BoardRun作成レスポンスから `object_key` を取得し、bundle のファイルサイズを計算してペイロードに含める:
```bash
staging_object_key=$(echo "$create_response" | jq -r '.artifact_bundle.object_key')
bundle_size=$(stat -c%s "$bundle_path")
import_payload=$(jq -n \
  --arg key "$staging_object_key" \
  --arg sha256 "$bundle_sha256" \
  --argjson size "$bundle_size" \
  '{staging_object_key: $key, bundle_sha256: $sha256, bundle_size_bytes: $size}')
```

#### C-3: コマンドインジェクション脆弱性（config.sh, hash.sh, bundle.sh）

**問題**: Python呼び出しで変数をシングルクォート内にbash変数展開で埋め込んでいる:

```bash
# config.sh L7
python3 -c "... with open('$path', 'r') as f: ..."
# hash.sh L23
python3 -c "... path = '$path' ..."
# bundle.sh L56
python3 -c "... with open('$bom_csv_path', 'r') as f: ..."
```

ファイルパスにシングルクォートが含まれる場合、Pythonコード文字列を脱出し任意コード実行が可能。

**修正**: 環境変数経由でPythonに値を渡す:
```bash
parse_boardflow_yml() {
  local path="$1"
  BOARDFLOW_YML_PATH="$path" python3 -c "
import yaml, json, sys, os
try:
    with open(os.environ['BOARDFLOW_YML_PATH'], 'r') as f:
        data = yaml.safe_load(f)
    ...
"
}
```

### High（重要な乖離）

#### H-1: Plan APIレスポンスのフィールド名不一致（entrypoint.sh, api.sh）

**問題**: `call_plan_api` が `.decisions` を抽出しているが、spec §6.6 のレスポンス例では配列はトップレベルの `.projects` フィールドに格納される。

**修正**: `api.sh` L80を修正:
```bash
echo "$response" | jq '.projects'
```

#### H-2: BoardRun作成APIペイロードのフィールド名不一致（entrypoint.sh L143-152）

**問題**: 実装では `{repository, commit_sha, ref, project_path, tree_hash, run_id, run_attempt}` を送信しているが、spec §9.2 では:
- `github_run_id`（not `run_id`）
- `github_run_attempt`（not `run_attempt`）  
- `board_project_id`（Plan APIレスポンスから取得すべき）
- `branch` フィールドも必要

**修正**: Plan APIのレスポンスから `board_project_id` を取得し、フィールド名をspec準拠に修正。

#### H-3: Fail APIペイロード形式不一致（api.sh L97-99）

**問題**: 実装は `{message, details}` を送信しているが、spec §9.6 では `{status: "failed", error: {message, details}}` 形式。

**修正**:
```bash
payload=$(jq -n --arg msg "$message" --arg det "$details" \
  '{status: "failed", error: {message: $msg, details: $det}}')
```

#### H-4: 検出エラー時にjob失敗にしていない（entrypoint.sh L49-75）

**問題**: spec §3.2「検出エラーが1件でもある場合、最終的なGitHub Actions jobは失敗とする」。現在の実装では、一部プロジェクトがバリデーション失敗（schema不正、.kicad_pro不在等）しても `::warning` を出すだけで、有効なプロジェクトが1つでもあれば `EXIT_CODE=0` で終了する。

**修正**: 検出エラーのカウンターを追加し、1件でもあれば最終的に `EXIT_CODE=1` にする。

#### H-5: KiCadソースファイルのbundle収集が未実装（entrypoint.sh）

**問題**: spec §8.2 で `kicad/` ディレクトリ配下に `.kicad_pro`, `.kicad_sch`, `.kicad_pcb`, `.kicad_wks` をコピーしてbundleに含める要件がある。実装には該当処理が存在しない。

**修正**: staging ディレクトリに `kicad/` サブディレクトリを作成し、project_dir配下の対象拡張子ファイルをexclude適用後にコピーする処理を追加。

#### H-6: bundleのディレクトリ構造がspec非準拠（entrypoint.sh L325-340）

**問題**: spec §8.6 では `review/`, `assembly/`, `fabrication/`, `checks/`, `diff/`, `kicad/` のサブディレクトリ構造を要求。実装ではPDF/SVG/gerber等を各出力ディレクトリ名のままフラットにコピーしている（`pdf/`, `svg/`, `gerber/` 等）。

**修正**: staging構築時にspec準拠のディレクトリ構造にマッピングする:
- `pdf/schematic.pdf` → `review/schematic.pdf`
- `pdf/pcb.pdf` → `review/pcb.pdf`
- `svg/pcb_top.svg` → `review/pcb_top.svg`
- `ibom/` → `assembly/ibom.html`
- `bom/bom.csv` → `assembly/bom.csv`
- `erc.json`, `drc.json` → `checks/`
- メタデータ → `diff/`

#### H-7: gerbers.zip / drill.zip の個別生成が未実装

**問題**: spec §8.2 では `fabrication/gerbers.zip`, `fabrication/drill.zip`, `fabrication/fabrication.zip` の3つを要求。実装は `fabrication.zip`（gerber+drill結合）のみ生成。

**修正**: gerber_dirをzip化した `gerbers.zip`、drill_dirをzip化した `drill.zip` をそれぞれ個別に生成し、fabrication/ に配置。

### Medium（改善推奨）

#### M-1: is_excluded の性能問題（hash.sh）

**問題**: `list_project_files` で各ファイルごとにPythonプロセスを起動してfnmatch判定している。100ファイルのプロジェクトなら100回のPython起動。大規模プロジェクトでは深刻な遅延になる。

**改善案**: ファイルリスト全体を一度のPython呼び出しでフィルタリングする。

#### M-2: パイプ区切りのVALID_PROJECTS配列（entrypoint.sh L78）

**問題**: `|` を区切り文字として使用しているが、`config_json` にJSON文字列が含まれる場合 `|` が出現する可能性がある（例: jqフィルタ文字列やURL）。

**改善案**: 一時ファイルにJSON形式で書き出すか、配列インデックスで管理する。

#### M-3: メタデータのディレクトリ名（entrypoint.sh）

**問題**: 実装では `meta/` ディレクトリに差分メタデータを生成しているが、spec §7.5 では `diff/` ディレクトリを要求。

**修正**: `meta_dir` → `diff_dir` にリネームし、出力パスを `diff/` 配下に変更。

#### M-4: manifest.json の構造がspec非準拠（bundle.sh L131-145）

**問題**: 実装の manifest は `{version, project, checks, artifacts, diff}` だが、spec §8.5 では `schema_version`, `board_project_id`, `project`, `git`, `github_actions`, `kicad`, `hash`, `diff_metadata`, `checks`, `artifacts` を含む詳細な構造。

**修正**: specに準拠したフィールド構造でmanifest.jsonを生成する。

#### M-5: upload用curlにタイムアウト未設定（entrypoint.sh L361-368）

**問題**: bundle uploadの `curl` に `--connect-timeout` / `--max-time` が設定されていない。大きなbundleの場合、ハングする可能性がある。

**改善案**: `--connect-timeout 30 --max-time 600` を追加。

#### M-6: Plan APIリクエストの `projects[].path` フィールド名

**問題**: 実装では `path` を使用しているが、spec §6.5 の例では `project_path`, `config_path`, `project_dir`, `tree_hash` 等より詳細な構造。

**修正**: Plan APIの正式なリクエストスキーマに合わせてフィールドを追加。

### Low（軽微）

#### L-1: Dockerコンテナがroot実行

**問題**: Dockerfileで `USER root` 設定後、非rootユーザーに戻していない。セキュリティベストプラクティスとして最小権限で実行すべき。ただしGitHub Actions Docker actionsでは `/github/workspace` へのアクセスにroot権限が必要な場合があり、実害は限定的。

#### L-2: 一時ディレクトリの明示的クリーンアップなし

**問題**: `mktemp -d` で作成した `output_dir` のクリーンアップが明示的に行われていない。コンテナ終了で自動削除されるため実害はないが、ディスク容量制約のあるランナーでは問題になる可能性。

#### L-3: jq による配列構築の O(n²) 性能

**問題**: `artifacts_status` や `RESULTS` の配列構築で毎回 `echo "$arr" | jq '. + [...]'` を使用。要素数に対して O(n²) のパフォーマンス特性。プロジェクト数が少ないMVPでは問題にならないが、将来的にはボトルネックになりうる。

#### L-4: python3-yaml パッケージ不足の可能性

**問題**: Dockerfile で `python3-yaml` をインストールしているが、パッケージ名は `python3-yaml` ではなく Debian では `python3-yaml` で正しい（PyYAMLのDebianパッケージ）。動作に問題はないが、`import yaml` の動作確認が必要。

#### L-5: tree_hash のファイル名にバックスラッシュを含む場合

**問題**: `printf "%b"` がファイル名中の `\n`, `\t`, `\x00` 等のエスケープシーケンスを解釈してしまう。KiCadファイル名にこれらが含まれることは極めて稀だが、理論上は不正なハッシュになる。

### 総合評価

| 観点 | 評価 |
|------|------|
| 仕様準拠性 | △ APIペイロード形式・bundle構造・ディレクトリレイアウトに複数の乖離 |
| 正確性 | △ 環境変数パースのCriticalバグ、KiCadコマンドオプションは正確 |
| エラーハンドリング | ○ 部分失敗継続は概ね仕様通り。検出エラー時のjob失敗が不足 |
| セキュリティ | △ コマンドインジェクション脆弱性あり |
| ロバスト性 | △ パイプ区切り・性能問題あり |
| 保守性 | ○ 関数分割・命名は適切 |

### 必要なアクション

1. Critical 3件を全て修正する（特にC-1は全入力が無視される致命的バグ）
2. High 7件のうち、少なくともH-1〜H-6を修正する（API通信とbundle構造の仕様準拠）
3. Medium のうち M-3, M-4 は仕様乖離のため修正推奨
4. 修正後に再レビューを実施する

---

## 再レビュー結果

**レビュー日**: 2026-05-04
**対象ブランチ**: feat/60-boardflow-action
**判定**: `pr_ready: false`

### 前回指摘事項の修正確認

| ID | 指摘内容 | 修正状況 |
|----|----------|----------|
| C-1 | INPUT環境変数パース | ✅ `INPUT_EXCLUDE_PATHS`等正しく参照 |
| C-2 | Import APIペイロード | ✅ `staging_object_key`, `bundle_sha256`, `bundle_size_bytes` 追加 |
| C-3 | コマンドインジェクション | ✅ `sys.argv`/`stdin`経由に変更 |
| H-1 | Plan APIレスポンス `.projects` | ✅ `jq '.projects'` で正しく抽出 |
| H-2 | BoardRun作成ペイロード | ✅ `board_project_id`, `github_run_id`等追加 |
| H-3 | Fail APIペイロード | ✅ `{status, error: {message, details}}` 形式 |
| H-4 | 検出エラー時job失敗 | ✅ `DETECTION_ERRORS`カウンター実装 |
| H-5 | KiCadソースファイル収集 | ✅ `kicad/`ディレクトリへコピー |
| H-6 | bundleディレクトリ構造 | ✅ `review/assembly/fabrication/checks/diff/kicad/` |
| H-7 | gerbers.zip/drill.zip個別生成 | ✅ 各ディレクトリから個別zip生成 |
| M-1 | is_excluded性能 | ✅ `list_project_files`でバッチ処理 |
| M-3 | diffディレクトリ名 | ✅ `diff_dir` 変数使用 |
| M-4 | manifest構造 | ⚠️ 部分的修正。構造は改善されたが `artifacts` フィールドが不完全 |
| M-5 | upload curlタイムアウト | ✅ `--connect-timeout 30 --max-time 600` |
| M-6 | Plan APIリクエスト | ⚠️ 部分的修正。構造は改善されたが `project_path` と `files` 形式に乖離あり |

### 新規発見事項

#### R-1: manifest `artifacts` 配列が仕様不適合 【Critical】

**箇所**: `bundle.sh` → `create_manifest()` / `entrypoint.sh` の `artifacts_status` 構築

**問題**: 現在のartifacts配列は `[{"name":"erc","status":"success"}]` 形式だが、spec §8.5 / §8.6 は以下を要求:

```json
{
  "type": "schematic_pdf",
  "status": "available",
  "path": "review/schematic.pdf",
  "content_type": "application/pdf",
  "sha256": "sha256:...",
  "size_bytes": 123456
}
```

spec §8.6 のbundle検証ルール:
- "manifest内の各artifactに type / status がある"
- "`available` なartifactには path / content_type / sha256 / size_bytes がある"

**影響**: backendのimport workerがbundleを拒否し、全BoardRunが `failed` になる。

**修正方針**: `artifacts_status` の各エントリに `type`（spec §8.3準拠）、`status`（`available`/`failed`）、`path`（staging内相対パス）、`content_type`、`sha256`、`size_bytes` を含める。

#### R-2: `project_path` がディレクトリパスを使用 【High】

**箇所**: `entrypoint.sh` L107-115（plan_projects構築）、L172（decision matching）、L189（BoardRun作成）

**問題**: `project_path` に `$rel_dir`（ディレクトリ）を設定しているが、spec §6.5, §6.6, §8.5, §9.2 は全て `.kicad_pro` ファイルパスを `project_path` として使用。

```
spec例: "project_path": "hardware/motor_driver/motor_driver.kicad_pro"
実装:   "project_path": "hardware/motor_driver"  (ディレクトリ)
```

**影響**: Plan APIの応答とのマッチング自体は（SaaS側がミラーする前提で）動作するが、manifestの `project.project_path` が仕様不適合となり、backend検証で問題になる可能性。

**修正方針**: `pro_file` の相対パスを `project_path` として使用する。

#### R-3: Plan API `files` 配列がフラット文字列配列 【High】

**箇所**: `entrypoint.sh` L108

**問題**: 現在:
```bash
files=$(list_project_files ... | jq -R -s 'split("\n") | map(select(. != ""))')
# → ["motor_driver.kicad_pcb", "motor_driver.kicad_sch", ...]
```

spec §6.5 が期待する形式:
```json
"files": [{"path": "hardware/motor_driver/motor_driver.kicad_pcb", "sha256": "sha256:..."}]
```

**影響**: SaaSのplan APIが `files` 配列の `sha256` フィールドを使って将来的なファイル単位diff判定に活用できない。ただしMVPでは `tree_hash` のみで判定しているため、即時の機能不全にはならない。

**修正方針**: `list_project_files` の出力に対してファイル単位sha256を計算し、objects配列として構築する。パスはリポジトリルート相対にする。

#### R-4: manifest `checks` 構造が不完全 【Medium】

**箇所**: `bundle.sh` → `generate_checks_summary_json()` / `create_manifest()`

**問題**: 現在の `checks` は `{erc: {errors: N, warnings: N}, drc: {errors: N, warnings: N}}` だが、spec §8.5 は `enabled`, `status`, `report` フィールドも含む:

```json
"erc": {"enabled": true, "status": "passed", "errors": 0, "warnings": 2, "report": "checks/erc.json"}
```

**修正方針**: `enabled: true`, `status` (`passed`/`failed` をerrors数で判定), `report` パスを追加。

#### R-5: Plan API `repository` に `github_repository_id` 不足 【Medium】

**箇所**: `entrypoint.sh` L133

**問題**: spec §6.5 の `repository` オブジェクトには `github_repository_id` フィールドがあるが、実装では `owner` と `name` のみ。spec §9.1 には "SaaSは、Actionから送られた `github_repository_id` をそのまま信用せず、tokenに紐づく…と一致するか必ず検証する" とある。

**影響**: SaaS側がこのフィールドを必須バリデーションしている場合、Plan APIが400で拒否される。

**修正方針**: `GITHUB_REPOSITORY_ID` 環境変数（GitHub Actionsが提供）を取得して送信する。

#### R-6: manifest `github_actions.workflow` フィールド不足 【Low】

**箇所**: `bundle.sh` → `create_manifest()`

**問題**: spec §8.5 の manifest例では `github_actions` に `workflow` フィールドがあるが、実装では `run_id` と `run_attempt` のみ。

### 総合評価

| 観点 | 前回 | 今回 | 備考 |
|------|------|------|------|
| 仕様準拠性 | △ | ○△ | API構造改善。manifest artifacts が未対応 |
| 正確性 | △ | ○ | Critical バグ修正完了。新規致命バグなし |
| エラーハンドリング | ○ | ○ | DETECTION_ERRORS追加で改善 |
| セキュリティ | △ | ○ | コマンドインジェクション修正済み |
| ロバスト性 | △ | ○ | タイムアウト追加、バッチ処理改善 |
| 保守性 | ○ | ○ | 変更なし |

### 判定

**`pr_ready: false`**

#### ブロッカー (修正必須)

- **R-1** (Critical): manifest `artifacts` 配列が仕様不適合。backendがbundleを拒否するため、E2E動作しない。

#### 強く推奨 (PR前に修正すべき)

- **R-2** (High): `project_path` のディレクトリ/ファイル不一致。backend実装が spec に従えば不整合が発生する。
- **R-3** (High): `files` 配列形式。MVPではtree_hash判定のみのため即時影響は限定的だが、仕様乖離を残すとbackend側の対応コストが増大する。

#### 許容可 (PR後でも対応可)

- **R-4** (Medium): checks構造の追加フィールド
- **R-5** (Medium): `github_repository_id` の送信
- **R-6** (Low): `workflow` フィールド追加

### 推奨対応順序

1. R-1 修正（artifacts配列をspec準拠に構築）
2. R-2 修正（project_pathを.kicad_proファイルパスに変更）
3. R-3 修正（files配列をobjects形式に変更）
4. 再レビュー実施

---

## 最終レビュー結果

**実施日時**: 2026-05-04
**対象コミット**: `bcbc4ff` (fix: spec-compliant artifacts array, project_path, and files format (R-1,R-2,R-3))

### 前回指摘の修正確認

| ID | 内容 | 状態 |
|----|------|------|
| R-1 | artifacts配列: type/status/path/content_type/sha256/size_bytes (§8.5準拠) | ✅ 修正済み |
| R-2 | project_pathが.kicad_proの相対パス | ✅ 修正済み |
| R-3 | Plan API files配列が[{path, sha256}]形式 | ✅ 修正済み |
| C-1 | INPUT_* ハイフン→アンダースコア変換 | ✅ 維持 |
| C-2 | create_board_run応答からboard_run_id/upload_url/staging_object_key抽出 | ✅ 維持 |
| C-3 | Pythonスクリプトでsys.argv使用 | ✅ 維持 |
| H-1 | Plan API応答 `.projects` 参照 | ✅ 維持 |
| H-2 | board_project_idをplan decisionsから取得 | ✅ 維持 |
| H-3 | Fail API spec準拠ペイロード | ✅ 維持 |
| H-4 | DETECTION_ERRORSによるjob失敗 | ✅ 維持 |
| H-5 | KiCad sourceファイルをstaging収集 | ✅ 維持 |
| H-6 | staging構造がspec §8.6準拠 | ✅ 維持 |
| H-7 | gerbers.zip/drill.zip個別作成 | ✅ 維持 |
| M-1 | Python一括フィルタリング | ✅ 維持 |
| M-3 | diff metadata生成 | ✅ 維持 |
| M-4 | manifest spec準拠 | ✅ 維持 |
| M-5 | アップロードtimeout設定 | ✅ 維持 |
| M-6 | Plan APIペイロード spec準拠 | ✅ 維持 |

### 新規指摘

#### H-NEW-1 (High): KiCad source artifacts に `source_path` フィールドが欠落

**根拠**: spec §8.6 "source artifact は `kicad/` 以下に閉じ込め、元の repository 相対pathを `source_path` としてmanifestに保存する"
spec §8.5 example にも `"source_path": "hardware/motor_driver/motor_driver.kicad_pro"` が明示。

**該当箇所**: `action/entrypoint.sh` L393
```bash
artifacts_status=$(add_artifact_available "$artifacts_status" "$kicad_type" "$staging_path" "application/octet-stream" "$src_file")
```

**修正方針**: `add_artifact_available` にオプション引数として `source_path` を追加するか、KiCad source専用の `add_artifact_kicad_source()` を作成して `source_path` フィールドを含める。`source_path` の値は `$rel_dir/$src_rel`（rel_dir="." の場合は `$src_rel` のみ）。

#### M-NEW-1 (Medium, 非ブロッカー): KiCad source の content_type

spec §8.5 example: `"text/plain; charset=utf-8"`
実装: `"application/octet-stream"`

#### M-NEW-2 (Medium, 非ブロッカー): root-levelプロジェクトでの staging_path

`rel_dir` が "." の場合、`staging_path = "kicad/./filename.kicad_pro"` となる。
`"kicad/filename.kicad_pro"` が正しい。

### 判定

**`pr_ready: false`**

H-NEW-1 (source_path欠落) はbackend検証で拒否される可能性が高く、修正必須。

### 修正手順

1. `bundle.sh` に `add_artifact_kicad_source()` 関数追加（source_path, content_type="text/plain; charset=utf-8" を含む）
2. `entrypoint.sh` L381-395 で当該関数を使用、rel_dir="." 時のパス正規化を追加
3. 再レビュー実施

---

## ドキュメント確認

**実施日時**: 2026-05-04
**対象コミット**: `1e7fc9b` (fix: add source_path to KiCad source artifacts (H-NEW-1))

### 1. spec.md §2.2 workflow例 vs action.yml

| 項目 | spec.md §2.2 | action.yml | 整合性 |
|------|-------------|------------|--------|
| uses | `example/boardflow-action@v1` | - | ✅ 参考パス |
| token | `${{ secrets.BOARDFLOW_TOKEN }}` | input定義あり | ✅ |
| mode | `${{ github.event.inputs.mode \|\| 'auto' }}` | default: 'auto' | ✅ |
| exclude-paths | 複数行glob | input定義あり | ✅ |
| api-url | (未使用) | default設定あり | ✅ |
| fail-on-drc | (未使用) | default: 'false' | ✅ |
| fail-on-erc | (未使用) | default: 'false' | ✅ |

**結果**: ✅ 整合

### 2. spec.md §5.2 Action inputs vs action.yml

| 項目 | spec §5.2 | action.yml (実装) | 整合性 |
|------|-----------|-------------------|--------|
| name | `BoardFlow` | `BoardFlow Action` | ⚠️ 軽微差異（機能影響なし） |
| description | `Build KiCad projects and upload artifacts to BoardFlow` | `KiCad CI/CD - Generate artifacts and upload to BoardFlow` | ⚠️ 軽微差異 |
| inputs 全6項目 | 定義あり | 完全一致 | ✅ |
| outputs.result | なし | 定義あり（追加） | ✅ 追加は問題なし |
| runs.using | `docker` | `docker` | ✅ |
| runs.image | `Dockerfile` | `Dockerfile` | ✅ |
| runs.args | `- run` | **なし** | ⚠️ 乖離あり |

**乖離点**: spec §5.2 では `args: - run` を定義しているが、action.yml にはargsが含まれない。entrypoint.shはargs不要で動作するため機能影響はないが、将来サブコマンド追加時に不整合となる。

**推奨対応**: spec.md側の `args: - run` を削除するか、action.ymlに `args: - run` を追加してentrypoint.shで引数を受け取るようにする。現時点ではどちらも実害なし。

### 3. docs/external/kicad-docker-cli.md §4 vs action/Dockerfile

| 項目 | §4参考例 | 実装Dockerfile | 整合性 |
|------|---------|---------------|--------|
| ベースイメージ | `kicad/kicad:9.0` | `kicad/kicad:9.0` | ✅ |
| USER root | あり | あり | ✅ |
| python3-pip | あり | あり | ✅ |
| xvfb | あり | あり | ✅ |
| interactivehtmlbom | `pip install` | `pip3 install` | ✅ 同等 |
| --break-system-packages | あり | あり | ✅ |
| 追加パッケージ | なし | jq, curl, zip, ca-certificates, python3-yaml | ✅ 正当な追加 |
| ENTRYPOINT | コメントアウト | `/action/entrypoint.sh` | ✅ |

**結果**: ✅ 整合。実装は参考例を適切に拡張している。

### 4. docs/external/github-actions-docker-action.md の正確性

| セクション | 内容 | 正確性 |
|-----------|------|--------|
| ファイル構成 | action.yml / Dockerfile / entrypoint.sh | ✅ |
| action.yml スキーマ | runs.using, image, args等 | ✅ |
| image指定方法 | Dockerfile / docker:// | ✅ |
| **inputs受け渡し** | `INPUT_` + 大文字化 (ハイフンはそのまま) | ❌ **誤り** |
| outputs設定 | `$GITHUB_OUTPUT` | ✅ |
| Job Summary | `$GITHUB_STEP_SUMMARY` | ✅ |
| 環境変数一覧 | GITHUB_* 各種 | ✅ |
| Docker制約 | Linux限定, imageビルドキャッシュなし | ✅ |

**重大な誤り**: ドキュメント内の inputs 受け渡しセクションに以下の記述がある:

> `input-id: api-url` → 環境変数 `INPUT_API-URL`
> 変換規則: `INPUT_` + 大文字化 (ハイフンはそのまま)

**実際の動作**: GitHub Actionsはハイフンをアンダースコアに変換する。正しくは:
- `input-id: api-url` → 環境変数 `INPUT_API_URL`
- 変換規則: `INPUT_` + 大文字化 + ハイフン→アンダースコア変換

entrypoint.shの実装（C-1修正後）は `INPUT_API_URL` で正しく参照しており、ドキュメントの方が不正確。

### 5. docs/logs/60/worklog.md の正確性

| セクション | 内容 | 正確性 |
|-----------|------|--------|
| Issue経緯 | 既存issue/docの参照 | ✅ |
| 調査結果 | action.yml仕様, env変数, Job Summary | ✅（上記INPUT_*の誤り除く） |
| 計画 | ファイル構成, 関数設計, 実装順序 | ✅ |
| 実装内容 | ファイル一覧, 主要判断 | ✅ |
| レビュー結果 (初回) | C-1〜C-3, H-1〜H-7, M-1〜M-6 | ✅ 指摘は正確 |
| 再レビュー結果 | R-1〜R-6 | ✅ |
| 最終レビュー結果 | H-NEW-1, M-NEW-1, M-NEW-2 | ✅ |

### 6. 未修正の残存乖離（M-NEW-2）

`entrypoint.sh` L392-393 で `rel_dir="."` 時のパス正規化が未実装:

```bash
staging_path="kicad/$rel_dir/$src_rel"  # → "kicad/./file.kicad_pro"
source_path="$rel_dir/$src_rel"          # → "./file.kicad_pro"
```

spec §8.5 の例では `"kicad/hardware/motor_driver/motor_driver.kicad_pro"` のようにクリーンなパスを要求。root-levelプロジェクトでは `"kicad/file.kicad_pro"` が正しい。

### 判定

**`docs_ready: false`**

### 修正が必要な箇所

| # | ファイル | 内容 | 優先度 |
|---|---------|------|--------|
| 1 | `docs/external/github-actions-docker-action.md` | INPUT_* 環境変数のハイフン→アンダースコア変換ルールの記述を修正 | High |
| 2 | `docs/spec.md` §5.2 または `action/action.yml` | `args: - run` の有無を一致させる | Low |
| 3 | `action/entrypoint.sh` L392-393 | M-NEW-2: `rel_dir="."` 時のパス正規化（`./` prefix除去） | Medium |
