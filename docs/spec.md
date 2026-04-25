# KiCad + GitHub Actions 連携型 Board CI/CD SaaS 仕様書案

## 1. 概要

本サービスは、GitHub上で管理されているKiCadプロジェクトに対して、GitHub Actions上で `kicad-cli` や iBOM などを実行し、生成された成果物をSaaSにアップロードしてWeb上で閲覧・管理できるようにする、KiCad向けのBoard CI/CDサービスである。

SaaS側ではKiCadを直接実行せず、GitHub Actions上のDocker Actionが成果物生成を担当する。
SaaSは、成果物の保存・Web表示・差分判定・GitHub Issue連携・BoardProject管理を担当する。

主な目的は以下である。

* KiCad基板のレンダリング済み画像、PDF、iBOM、Gerber、BOMなどをWebで見やすく共有する
* 1リポジトリ内の複数KiCadプロジェクトに対応する
* GitHub Issueを基板ごとの管理単位として利用する
* DRC/ERCなどのCI結果をGitHub Issueへ連携する
* GitHub Actions側の再実行コストを抑えるため、SaaS側にハッシュ問い合わせを行い、変更がある基板のみ処理する

---

## 2. 基本方針

### 2.1 SaaS側でKiCadは実行しない

KiCad、iBOM、Python依存関係、フォント、ライブラリなどの実行環境は、GitHub Actions上のDocker Actionに含める。

SaaS側は以下のみを担当する。

* 成果物の受信
* 成果物の保存
* Webでの表示
* BoardProject / BoardRun / Artifact の管理
* ハッシュによる差分判定
* GitHub AppによるIssue作成・コメント更新

### 2.2 GitHub Actions側はDocker Actionで提供する

Composite Actionではなく、KiCad 9.0系の実行環境を含むDocker Actionとして提供する。

これにより、ユーザー側のworkflowは最小限になる。

```yaml
name: BoardCI

on:
  push:
    branches:
      - main
      - "board/**"
  workflow_dispatch:
    inputs:
      mode:
        description: "auto or all"
        required: false
        default: "auto"

permissions:
  contents: read

jobs:
  boardci:
    runs-on: ubuntu-latest

    steps:
      - uses: actions/checkout@v4

      - uses: example/boardci-action@v1
        with:
          token: ${{ secrets.BOARDCI_TOKEN }}
          mode: ${{ github.event.inputs.mode || 'auto' }}
          exclude-paths: |
            **/*.lck
            **/output/**
            **/fabrication/**
```

Issue作成やコメント更新はSaaS側のGitHub Appが行うため、基本的にActions側へ `GITHUB_TOKEN` を渡さない。

---

## 3. 対象KiCadプロジェクトの検出

### 3.1 グローバル設定ファイルは持たない

リポジトリルートなどにグローバル設定ファイルは置かない。

各KiCadプロジェクトのディレクトリに、個別の設定ファイルを置く。

```text
hardware/
  motor_driver/
    motor_driver.kicad_pro
    motor_driver.kicad_sch
    motor_driver.kicad_pcb
    .boardci.yml

  power_board/
    power_board.kicad_pro
    power_board.kicad_sch
    power_board.kicad_pcb
    .boardci.yml
```

### 3.2 検出ルール

Docker Actionは、リポジトリ内の `.boardci.yml` を探索する。

検出ルールは以下とする。

```text
1. リポジトリ内の .boardci.yml を探索する
2. .boardci.yml の同じ階層に .kicad_pro があるか確認する
3. 同階層に .kicad_pro が1つだけ存在する場合、そのディレクトリをBoardProject候補とする
4. .kicad_pro が0個の場合はエラーまたはスキップする
5. .kicad_pro が複数ある場合はエラーとする
```

MVPでは、`.boardci.yml` が存在するKiCadプロジェクトのみを対象とする。

### 3.3 BoardProjectの識別

ユーザーに `board key` や `issue id` は設定させない。

MVPでは、BoardProjectの同一性は以下で決定する。

```text
github_repository_id + project_path
```

ここで `project_path` は、リポジトリルートから見た `.kicad_pro` の相対パスである。

例:

```text
hardware/motor_driver/motor_driver.kicad_pro
```

### 3.4 project_path変更時の扱い

MVPでは、KiCadプロジェクトのファイルパス変更には追従しない。

```text
旧:
hardware/motor_driver/motor_driver.kicad_pro

新:
boards/motor_driver/motor_driver.kicad_pro
```

この場合、SaaS上では別のBoardProjectとして扱う。

将来的には、SaaS UI上でBoardProjectの統合や移行を行えるようにする可能性がある。

---

## 4. `.boardci.yml` 仕様

`.boardci.yml` は、KiCadプロジェクトをBoardCI対象として認識させるためのマーカーであり、同時にプロジェクト固有の最小設定を持つ。

### 4.1 MVP設定例

```yaml
version: 1

outputs:
  preset: default

exclude_paths:
  - "output/**"
  - "fabrication/**"
```

### 4.2 将来的な拡張例

```yaml
version: 1

outputs:
  schematic_pdf: true
  pcb_pdf: true
  pcb_svg: true
  ibom: true
  fabrication: true
  bom: true
  position: true

checks:
  erc: true
  drc: true

exclude_paths:
  - "output/**"
  - "fabrication/**"
  - "tmp/**"

comments:
  run_results: on_change
```

### 4.3 設定ファイルに含めないもの

MVPでは、以下は設定ファイルに含めない。

```text
- board key
- issue number
- issue id
- GitHub repository id
- グローバルなプロジェクト一覧
```

Issue連携はSaaS側のGitHub Appが自動で管理する。

---

## 5. Docker Action仕様

### 5.1 Actionの役割

Docker Actionは以下を行う。

```text
1. .boardci.yml の自動検出
2. 同階層の .kicad_pro の特定
3. exclude-paths の適用
4. 各BoardProject候補のファイルハッシュ計算
5. SaaSのplan APIへの問い合わせ
6. build対象のKiCadプロジェクトのみ処理
7. kicad-cli によるERC/DRC/成果物生成
8. iBOM生成
9. 成果物manifest作成
10. SaaSへの成果物アップロード
11. GitHub Actions Job Summary出力
```

### 5.2 Action inputs

```yaml
name: BoardCI
description: Build KiCad projects and upload artifacts to BoardCI

inputs:
  token:
    description: BoardCI API token
    required: true

  mode:
    description: "auto or all"
    required: false
    default: "auto"

  exclude-paths:
    description: "Newline separated glob patterns to exclude from project hashing"
    required: false
    default: ""

  api-url:
    description: BoardCI API URL
    required: false
    default: "https://api.boardci.example.com"

  fail-on-drc:
    description: Fail action if DRC has errors
    required: false
    default: "false"

  fail-on-erc:
    description: Fail action if ERC has errors
    required: false
    default: "false"

runs:
  using: docker
  image: Dockerfile
  args:
    - run
```

### 5.3 mode仕様

MVPでは以下の2種類とする。

| mode   | 説明                                             |
| ------ | ---------------------------------------------- |
| `auto` | SaaS APIにハッシュを問い合わせ、変更のあるBoardProjectのみbuildする |
| `all`  | 検出した全BoardProjectを強制buildする                    |

将来的には、特定project_path指定などを追加できる。

```text
project:hardware/motor_driver/motor_driver.kicad_pro
```

---

## 6. 差分検出仕様

### 6.1 差分判定の考え方

差分判定はSaaS側に寄せる。

Docker Actionは、各BoardProject候補について対象ファイルのハッシュを計算し、SaaSへ送信する。
SaaSは過去に成功したBoardRunのハッシュと比較し、buildすべきかskipすべきかを返す。

### 6.2 ハッシュ対象

MVPでは、対象は `project_dir` 配下のファイルとする。

```text
対象:
  project_dir/**

除外:
  built-in excludes
  Action input exclude-paths
  .boardci.yml exclude_paths
```

共通ライブラリ、外部フットプリント、外部3Dモデルなどの依存関係は、MVPでは厳密に追跡しない。

### 6.3 built-in excludes案

```text
**/*.lck
**/*.bak
**/*-backups/**
**/fp-info-cache
**/.DS_Store
**/output/**
**/outputs/**
**/fabrication/**
**/gerber/**
**/gerbers/**
```

### 6.4 tree_hash

ファイル単位のハッシュに加え、プロジェクト全体の `tree_hash` を計算する。

計算方法は以下のような形式とする。

```text
tree_hash = sha256(
  sorted(path + "\0" + file_sha256 + "\n")
)
```

これにより、ファイル順序に依存しないプロジェクト全体ハッシュを得る。

### 6.5 ActionがSaaSへ送るデータ例

```json
{
  "repository": {
    "github_repository_id": "123456789",
    "owner": "ForteFibre",
    "name": "hardware"
  },
  "git": {
    "ref": "refs/heads/board/motor-driver-v2",
    "branch": "board/motor-driver-v2",
    "commit_sha": "abc123",
    "event_name": "push"
  },
  "action": {
    "workflow": "BoardCI",
    "run_id": "987654321",
    "run_attempt": "1"
  },
  "projects": [
    {
      "project_path": "hardware/motor_driver/motor_driver.kicad_pro",
      "config_path": "hardware/motor_driver/.boardci.yml",
      "project_dir": "hardware/motor_driver",
      "tree_hash": "sha256:...",
      "files": [
        {
          "path": "hardware/motor_driver/motor_driver.kicad_pcb",
          "sha256": "sha256:..."
        }
      ]
    }
  ]
}
```

### 6.6 SaaSのplan APIレスポンス例

```json
{
  "projects": [
    {
      "project_path": "hardware/motor_driver/motor_driver.kicad_pro",
      "board_project_id": "bp_abc123",
      "decision": "build",
      "reason": "hash_changed"
    },
    {
      "project_path": "hardware/power_board/power_board.kicad_pro",
      "board_project_id": "bp_def456",
      "decision": "skip",
      "reason": "unchanged"
    }
  ]
}
```

### 6.7 decision

| decision | 説明            |
| -------- | ------------- |
| `build`  | 成果物生成を行う      |
| `skip`   | 成果物生成を行わない    |
| `error`  | 設定不備などで処理できない |

### 6.8 reason

| reason                 | 説明                        |
| ---------------------- | ------------------------- |
| `new_project`          | SaaS側に存在しない新規BoardProject |
| `hash_changed`         | 前回のtree_hashと異なる          |
| `config_changed`       | 設定ファイルの変更を検出した            |
| `manual_dispatch`      | 手動実行または強制実行               |
| `unchanged`            | 前回から変更なし                  |
| `previous_failed`      | 前回の成果物生成が失敗している           |
| `no_previous_snapshot` | 比較対象のsnapshotがない          |

---

## 7. 成果物生成仕様

### 7.1 使用ツール

MVPでは以下を使用する。

```text
- KiCad 9.0系
- kicad-cli
- InteractiveHtmlBom
- Python製の補助CLI
```

### 7.2 生成する成果物

MVPで生成する成果物候補は以下。

```text
review/
  schematic.pdf
  pcb_top.svg
  pcb_bottom.svg
  pcb.pdf

assembly/
  ibom.html
  bom.csv
  position.csv

fabrication/
  gerbers.zip
  drill.zip
  fabrication.zip

checks/
  erc.json または erc.rpt
  drc.json または drc.rpt

metadata/
  manifest.json
```

### 7.3 成果物種別

SaaSでは、Artifactに以下のようなtypeを持たせる。

```text
schematic_pdf
pcb_pdf
pcb_top_svg
pcb_bottom_svg
ibom
bom_csv
position_csv
gerber_zip
drill_zip
fabrication_zip
erc_report
drc_report
manifest
```

### 7.4 manifest例

```json
{
  "schema_version": 1,
  "board_project_id": "bp_abc123",
  "project": {
    "project_path": "hardware/motor_driver/motor_driver.kicad_pro",
    "project_dir": "hardware/motor_driver",
    "config_path": "hardware/motor_driver/.boardci.yml"
  },
  "git": {
    "ref": "refs/heads/board/motor-driver-v2",
    "branch": "board/motor-driver-v2",
    "commit_sha": "abc123"
  },
  "github_actions": {
    "run_id": "987654321",
    "run_attempt": "1",
    "workflow": "BoardCI"
  },
  "kicad": {
    "version": "9.0.x"
  },
  "hash": {
    "tree_hash": "sha256:..."
  },
  "checks": {
    "erc": {
      "enabled": true,
      "status": "passed",
      "errors": 0,
      "warnings": 2,
      "report": "checks/erc.rpt"
    },
    "drc": {
      "enabled": true,
      "status": "failed",
      "errors": 1,
      "warnings": 4,
      "report": "checks/drc.rpt"
    }
  },
  "artifacts": [
    {
      "type": "schematic_pdf",
      "path": "review/schematic.pdf",
      "content_type": "application/pdf",
      "sha256": "sha256:..."
    },
    {
      "type": "pcb_top_svg",
      "path": "review/pcb_top.svg",
      "content_type": "image/svg+xml",
      "sha256": "sha256:..."
    },
    {
      "type": "ibom",
      "path": "assembly/ibom.html",
      "content_type": "text/html",
      "sha256": "sha256:..."
    },
    {
      "type": "fabrication_zip",
      "path": "fabrication/fabrication.zip",
      "content_type": "application/zip",
      "sha256": "sha256:..."
    }
  ]
}
```

---

## 8. SaaS API仕様

### 8.1 Plan API

Actionが、検出したBoardProject候補とハッシュ情報をSaaSへ送信し、build対象を問い合わせる。

```http
POST /api/v1/runs/plan
```

主な処理:

```text
- Repositoryの作成または取得
- BoardProjectの作成または取得
- latest_tree_hashとの比較
- build/skip decisionの返却
- Issue未作成の場合はIssue作成ジョブをenqueue
```

### 8.2 BoardRun作成API

成果物アップロード前に、BoardRunを作成する。

```http
POST /api/v1/board-runs
```

リクエスト例:

```json
{
  "board_project_id": "bp_abc123",
  "project_path": "hardware/motor_driver/motor_driver.kicad_pro",
  "tree_hash": "sha256:...",
  "commit_sha": "abc123",
  "branch": "board/motor-driver-v2",
  "ref": "refs/heads/board/motor-driver-v2",
  "github_run_id": "987654321",
  "github_run_attempt": "1",
  "checks": {
    "erc": {
      "status": "passed",
      "errors": 0,
      "warnings": 1
    },
    "drc": {
      "status": "failed",
      "errors": 2,
      "warnings": 0
    }
  },
  "artifacts": [
    {
      "type": "schematic_pdf",
      "filename": "schematic.pdf",
      "content_type": "application/pdf",
      "sha256": "sha256:..."
    },
    {
      "type": "ibom",
      "filename": "ibom.html",
      "content_type": "text/html",
      "sha256": "sha256:..."
    }
  ]
}
```

レスポンス例:

```json
{
  "board_run_id": "br_abc123",
  "upload_urls": [
    {
      "artifact_type": "schematic_pdf",
      "url": "https://storage.example.com/...",
      "method": "PUT"
    },
    {
      "artifact_type": "ibom",
      "url": "https://storage.example.com/...",
      "method": "PUT"
    }
  ]
}
```

### 8.3 BoardRun完了API

成果物アップロード完了後に呼び出す。

```http
POST /api/v1/board-runs/{board_run_id}/complete
```

リクエスト例:

```json
{
  "status": "completed",
  "tree_hash": "sha256:...",
  "summary": {
    "erc": "passed",
    "drc": "failed"
  }
}
```

完了時にSaaS側で以下を行う。

```text
- BoardRunをcompletedにする
- BoardProject.latest_tree_hashを更新する
- BoardProject.latest_run_idを更新する
- Dashboardコメント更新ジョブをenqueueする
- 必要ならRun Resultコメント作成ジョブをenqueueする
```

### 8.4 失敗API

ビルドやアップロードに失敗した場合に呼び出す。

```http
POST /api/v1/board-runs/{board_run_id}/fail
```

```json
{
  "status": "failed",
  "error": {
    "message": "kicad-cli export failed",
    "details": "..."
  }
}
```

---

## 9. SaaSデータモデル案

### 9.1 repositories

```text
repositories
- id
- github_repository_id
- owner
- name
- installation_id
- created_at
- updated_at
```

### 9.2 board_projects

```text
board_projects
- id
- repository_id
- project_path
- project_dir
- display_name
- issue_number
- issue_node_id
- issue_url
- issue_sync_status
- dashboard_comment_id
- latest_tree_hash
- latest_successful_run_id
- created_at
- updated_at
```

`repository_id + project_path` にunique制約を置く。

### 9.3 board_runs

```text
board_runs
- id
- board_project_id
- commit_sha
- branch
- ref
- github_run_id
- github_run_attempt
- tree_hash
- status
- erc_status
- erc_errors
- erc_warnings
- drc_status
- drc_errors
- drc_warnings
- created_at
- completed_at
```

### 9.4 artifacts

```text
artifacts
- id
- board_run_id
- type
- filename
- content_type
- storage_key
- sha256
- size_bytes
- created_at
```

### 9.5 board_project_snapshots

将来的な差分表示用に、ファイルハッシュ一覧を保存する。

```text
board_project_snapshots
- id
- board_project_id
- board_run_id
- tree_hash
- commit_sha
- file_hashes_json
- created_at
```

MVPでは `latest_tree_hash` のみでもよいが、拡張性を考えると保存しておく価値がある。

### 9.6 github_jobs

GitHub API操作を非同期化するためのキュー。

```text
github_jobs
- id
- installation_id
- repository_id
- board_project_id
- board_run_id
- type
- payload_json
- status
- attempts
- run_after
- last_error
- created_at
- updated_at
```

type例:

```text
create_issue
create_label
update_issue_body
create_dashboard_comment
update_dashboard_comment
create_run_result_comment
```

---

## 10. GitHub App連携仕様

### 10.1 GitHub Appの役割

GitHub Issueの作成、コメント作成、コメント編集はSaaS側のGitHub Appが行う。

Actions側にはGitHub Issue書き込み権限を持たせない。

### 10.2 必要権限

MVPで必要なRepository permissionsは以下。

```text
Metadata: Read
Contents: Read
Issues: Read & Write
```

`Contents: Read` は必須ではない可能性もあるが、リポジトリ情報の確認や将来拡張を考慮してMVPでは付与してよい。

### 10.3 Issue自動作成

BoardProjectに対応するIssueが存在しない場合、SaaSはIssue作成ジョブをenqueueする。

Issue作成はキューで処理し、レートリミットや大量基板登録に備える。

### 10.4 Issueタイトル

基本形式:

```text
[Board] motor_driver
```

`display_name` は `project_path` の親ディレクトリ名などから自動生成する。

例:

```text
hardware/motor_driver/motor_driver.kicad_pro
```

であれば、

```text
[Board] motor_driver
```

とする。

同名の衝突が懸念される場合、本文に正規の `project_path` を記録するため、MVPではタイトル重複は許容する。

### 10.5 Issue本文

```markdown
<!-- boardci:repository_id=123456789 -->
<!-- boardci:project_path=hardware/motor_driver/motor_driver.kicad_pro -->

# Board Project

KiCad project:

`hardware/motor_driver/motor_driver.kicad_pro`

This issue tracks design, fabrication, assembly, and verification for this board.

## BoardCI

Latest board page:

https://boardci.example.com/repositories/123456789/boards/bp_abc123
```

### 10.6 Issue作成の冪等性

`board_projects` に以下のunique制約を置く。

```text
unique(repository_id, project_path)
```

Issue作成状態は以下で管理する。

```text
none
queued
creating
ready
failed
```

Issue作成ジョブの重複を避けるため、同一BoardProjectに対する `create_issue` ジョブは同時に複数作らない。

---

## 11. Issueコメント仕様

Issueコメントは2種類に分ける。

```text
A. Dashboardコメント
  - 最新成果物へのリンク
  - レンダリング済み画像
  - iBOM
  - Fabrication ZIP
  - 最新ステータス
  - 既存コメントを編集する

B. Run Resultコメント
  - DRC/ERCなどCI要素の強い情報
  - 重要なrunごとに追記する
```

### 11.1 Dashboardコメント

DashboardコメントはBoardProjectごとに1つだけ作成し、以後は編集更新する。

例:

```markdown
<!-- boardci:comment_type=dashboard -->
<!-- boardci:project_path=hardware/motor_driver/motor_driver.kicad_pro -->

## BoardCI Dashboard

Latest run: `abc1234` on `board/motor-driver-v2`

| Item | Link |
|---|---|
| Board page | https://boardci.example.com/... |
| Schematic PDF | https://boardci.example.com/... |
| PCB Preview | https://boardci.example.com/... |
| Interactive BOM | https://boardci.example.com/... |
| Fabrication ZIP | https://boardci.example.com/... |
| BOM CSV | https://boardci.example.com/... |

### Latest status

| Check | Result |
|---|---|
| ERC | ✅ 0 errors, 2 warnings |
| DRC | ❌ 1 error, 4 warnings |

Last updated by BoardCI.
```

`dashboard_comment_id` は `board_projects` に保存する。
コメントが手動削除された場合は再作成する。

### 11.2 Run Resultコメント

DRC/ERCなど、CIとしての履歴を残したい情報はrunごとに追記する。

例:

```markdown
<!-- boardci:comment_type=run_result -->
<!-- boardci:board_run_id=br_abc123 -->

## BoardCI Run Result

Commit: `abc1234`  
Branch: `board/motor-driver-v2`  
Run: https://boardci.example.com/...

| Check | Result |
|---|---|
| ERC | ✅ 0 errors, 2 warnings |
| DRC | ❌ 1 error, 4 warnings |

### DRC summary

- Clearance violation near U3 pin 12
- Track too close to board edge near J1
```

### 11.3 Run Resultコメントの追記条件

MVPでは、以下の場合に追記する。

```text
- 新しいDRC/ERC errorが発生した
- 前回成功 → 今回失敗
- 前回失敗 → 今回成功
- Fab Ready相当のrun
- 手動実行で明示的に記録対象になったrun
```

毎回のbuildで必ずコメントするとIssueが汚れるため、デフォルトでは避ける。

将来的には設定で制御する。

```yaml
comments:
  run_results: on_change
```

設定値候補:

| 値            | 説明                        |
| ------------ | ------------------------- |
| `off`        | Run Resultコメントを作らない       |
| `on_failure` | 失敗時のみ追記                   |
| `on_change`  | 成功/失敗状態やエラー内容に変化があった時のみ追記 |
| `always`     | buildごとに追記                |

---

## 12. GitHub APIキューとレートリミット対応

### 12.1 GitHub API操作はすべてジョブ化する

SaaS側では、GitHub Issue作成・コメント作成・コメント更新を直接同期的に行わず、キューに積む。

```text
BoardRun completed
  -> update dashboard comment job
  -> create run result comment job if needed
```

Issueがまだない場合:

```text
BoardRun completed
  -> create issue job
  -> create dashboard comment job
  -> create run result comment job if needed
```

### 12.2 並列数制御

以下の単位で並列数を制限する。

```text
- installation_id単位
- repository_id単位
- job type単位
```

特に、Issue作成やコメント作成は通知が発生するため低速にする。

### 12.3 Dashboardコメント更新のdebounce

同じBoardProjectに対して短時間に複数runが完了した場合、Dashboardコメント更新ジョブは最新状態にまとめる。

```text
同一BoardProjectに未処理のupdate_dashboard_commentジョブがある場合:
  payloadを最新runに置き換える
```

### 12.4 レートリミット時の挙動

GitHub APIのレスポンスヘッダーを確認し、以下を行う。

```text
x-ratelimit-remaining が少ない:
  該当installationのジョブを遅延

x-ratelimit-reset がある:
  reset時刻まで待機

retry-after がある:
  指定時間待機

403 / 429:
  exponential backoff
```

---

## 13. Web UI仕様

### 13.1 Repositoryページ

リポジトリ内のBoardProject一覧を表示する。

表示項目:

```text
- display_name
- project_path
- 最新run
- 最新commit
- ERC状態
- DRC状態
- 最終更新日時
- Issueリンク
```

### 13.2 BoardProjectページ

1つの基板に対応するページ。

タブ構成例:

```text
Overview
Schematic
PCB Preview
iBOM
BOM
Fabrication
Checks
Runs
History
```

### 13.3 Overview

表示内容:

```text
- project_path
- 最新commit
- branch
- tree_hash
- ERC/DRC概要
- GitHub Issueリンク
- 最新成果物リンク
```

### 13.4 PCB Preview

表示内容:

```text
- 表面SVG
- 裏面SVG
- PDFリンク
```

### 13.5 iBOM

表示内容:

```text
- iBOM HTMLへのリンク
- iframe表示可否はセキュリティ設定次第
```

### 13.6 Fabrication

表示内容:

```text
- Gerber ZIP
- Drill ZIP
- Fabrication ZIP
- 製造用チェックリスト
```

### 13.7 Checks

表示内容:

```text
- ERC結果
- DRC結果
- エラー件数
- 警告件数
- レポートファイル
```

### 13.8 Runs

過去のBoardRun一覧を表示する。

表示項目:

```text
- 実行日時
- commit
- branch
- tree_hash
- ERC状態
- DRC状態
- 成果物リンク
```

---

## 14. GitHub Actions Job Summary

Actionは、GitHub ActionsのJob Summaryに結果を出力する。

例:

```markdown
# BoardCI Summary

## Built

- `hardware/motor_driver/motor_driver.kicad_pro` — hash changed
- `hardware/sensor_board/sensor_board.kicad_pro` — new project

## Skipped

- `hardware/power_board/power_board.kicad_pro` — unchanged

## Failed

- `hardware/test_board/test_board.kicad_pro` — DRC failed
```

SaaSへのアップロードURLやBoardProjectページも表示する。

---

## 15. MVPでやること・やらないこと

### 15.1 やること

```text
- Docker Action提供
- KiCad 9.0系環境の固定
- .boardci.yml 自動検出
- 同階層 .kicad_pro の検出
- 1リポジトリ複数KiCadプロジェクト対応
- project_pathベースのBoardProject識別
- exclude-paths対応
- ファイルハッシュ / tree_hash計算
- SaaS plan APIによる差分判定
- 変更がある基板のみbuild
- kicad-cliによる成果物生成
- iBOM生成
- 成果物アップロード
- Webでの成果物表示
- GitHub AppによるIssue自動作成
- Dashboardコメントの作成・編集
- DRC/ERC結果コメントの条件付き追記
- GitHub API操作のキュー処理
```

### 15.2 やらないこと

```text
- グローバル設定ファイル
- board keyのユーザー設定
- issue numberのユーザー設定
- project_path変更の追従
- 共通ライブラリ変更の厳密な影響解析
- SaaS側でのKiCad実行
- 高度なKiCad差分ビューア
- 部品調達API連携
- BoardProjectの手動統合
- PR中心のレビュー機能
```

---

## 16. 将来拡張

### 16.1 BoardProjectの移行・統合

MVPではproject_path変更を別BoardProjectとして扱うが、将来的にはSaaS UI上で以下を提供する。

```text
- BoardProjectの統合
- project_path変更の手動追従
- 旧Issueから新Issueへのリンク付与
```

### 16.2 共通ライブラリ変更への対応

将来的に `.boardci.yml` に `include_paths` を追加する。

```yaml
include_paths:
  - "../../symbols/**"
  - "../../footprints/**"
  - "../../3dmodels/**"
```

### 16.3 差分レビュー

BoardRun間で以下を比較する。

```text
- 回路図PDF差分
- PCBレンダリング差分
- BOM差分
- 部品定数変更
- フットプリント変更
- Board outline変更
```

### 16.4 Fab Ready管理

特定BoardRunを製造固定版として扱う。

```text
Fab Ready Revision
- board_run_id
- commit_sha
- artifact checksums
- fabrication_zip
- created_at
```

### 16.5 実装・検査管理

IssueやSaaS上で以下を管理する。

```text
- 発注日
- 製造業者
- 実装進捗
- 動作確認状況
- 不具合メモ
- 次版への改善点
```

---

## 17. コンセプトまとめ

このサービスは、単なるKiCad CIではなく、以下を実現する。

> GitHub Actionsで生成したKiCad成果物を、基板単位で蓄積・レビュー・製造管理できるWebサービス。

特にMVPでは、次の価値に集中する。

```text
- .boardci.yml をKiCadプロジェクト横に置くだけで対象化
- Docker ActionでKiCad 9.0系成果物を自動生成
- SaaS側のhash判定で必要な基板だけbuild
- GitHub Appが基板ごとのIssueを自動作成
- Issue上では最新成果物リンクを編集更新し、DRC/ERC履歴は必要に応じて追記
- SaaS上でPDF、SVG、iBOM、Gerber、BOMをWeb表示
```

設計上の中核は以下の4点である。

```text
1. BoardProject = repository_id + project_path
2. Action = KiCad成果物生成エージェント
3. SaaS = 状態管理・成果物表示・差分判定・Issue連携
4. GitHub Issue = 1基板ごとの開発・製造・実装管理単位
```
