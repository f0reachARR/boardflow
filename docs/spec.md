# BoardFlow: KiCad + GitHub Actions 連携型 Board CI/CD SaaS 仕様書案

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

* staging zip の受け付けと import
* 成果物の保存
* Webでの表示
* BoardProject / BoardRun / Artifact の管理
* ハッシュによる差分判定
* GitHub AppによるIssue作成・コメント更新

### 2.2 GitHub Actions側はDocker Actionで提供する

Composite Actionではなく、KiCad 9.0系の実行環境を含むDocker Actionとして提供する。

これにより、ユーザー側のworkflowは最小限になる。

```yaml
name: BoardFlow

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
  boardflow:
    runs-on: ubuntu-latest

    steps:
      - uses: actions/checkout@v4

      - uses: example/boardflow-action@v1
        with:
          token: ${{ secrets.BOARDFLOW_TOKEN }}
          mode: ${{ github.event.inputs.mode || 'auto' }}
          exclude-paths: |
            **/*.lck
            **/output/**
            **/fabrication/**
```

Issue作成やコメント更新はSaaS側のGitHub Appが行うため、基本的にActions側へ `GITHUB_TOKEN` を渡さない。

MVPでは `push` と `workflow_dispatch` のみを正式対象とする。
`pull_request` や fork からの pull request は、secret や権限境界が複雑になるため対象外とする。
Action が `pull_request` event で実行された場合は、KiCad実行やSaaS API呼び出しを行わず、unsupported event として成功扱いで早期終了する。
この場合、GitHub Actions Job Summary に `unsupported event: pull_request` を表示し、PR が常に失敗状態になることを避ける。
fork PR、PR用artifact review、PRコメント連携はMVPでは扱わない。

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
    .boardflow.yml

  power_board/
    power_board.kicad_pro
    power_board.kicad_sch
    power_board.kicad_pcb
    .boardflow.yml
```

### 3.2 検出ルール

Docker Actionは、リポジトリ内の `.boardflow.yml` を探索する。

検出ルールは以下とする。

```text
1. リポジトリ内の .boardflow.yml を探索する
2. .boardflow.yml の同じ階層に .kicad_pro があるか確認する
3. 同階層に .kicad_pro が1つだけ存在する場合、そのディレクトリをBoardProject候補とする
4. .kicad_pro が0個の場合は検出エラーとしてJob Summaryに出す
5. .kicad_pro が複数ある場合はエラーとする
```

MVPでは、`.boardflow.yml` が存在するKiCadプロジェクトのみを対象とする。
`.boardflow.yml` が存在してもBoardProject候補にできないものはSaaSへ送らず、BoardProjectも作成しない。
複数BoardProjectのうち一部に検出エラーがあっても、処理可能なBoardProjectは継続する。
ただし、検出エラーが1件でもある場合、最終的なGitHub Actions jobは失敗とする。

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

## 4. `.boardflow.yml` 仕様

`.boardflow.yml` は、KiCadプロジェクトをBoardFlow対象として認識させるためのマーカーであり、同時にプロジェクト固有の最小設定を持つ。

MVPでは厳格にschema検証する。
`version` は必須で、対応する値は `1` のみとする。
未知フィールド、型不一致、非対応versionは検出エラーとして扱う。
検出エラーになったBoardProject候補はSaaSへ送らず、他の処理可能なBoardProjectは継続する。

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

上記の `checks` や `comments` は将来的な拡張例であり、MVPの厳格schemaでは未対応フィールドとして検出エラーになる。

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
1. .boardflow.yml の自動検出
2. 同階層の .kicad_pro の特定
3. exclude-paths の適用
4. 各BoardProject候補のファイルハッシュ計算
5. SaaSのplan APIへの問い合わせ
6. build対象ごとのBoardRun作成
7. build対象のKiCadプロジェクトのみ処理
8. kicad-cli によるERC/DRC/成果物生成
9. iBOM生成
10. 成果物manifest作成
11. 成果物 zip bundle 作成
12. staging bucket へのzip upload
13. import API 呼び出し
14. 生成失敗、upload失敗、import要求前失敗のfail API通知
15. GitHub Actions Job Summary出力
```

複数BoardProjectのうち一部が失敗しても、処理可能なBoardProjectは継続する。
ただし、検出不備、成果物生成失敗、upload失敗、import要求失敗が1件でもある場合、GitHub Actions job全体は失敗とする。
差分判定でskipされたBoardProjectは失敗扱いにしない。
生成できなかった個別artifactは欠損として記録し、manifestとDRC/ERC結果を保存できる限りBoardRun自体は失敗扱いにしない。

### 5.2 Action inputs

```yaml
name: BoardFlow
description: Build KiCad projects and upload artifacts to BoardFlow

inputs:
  token:
    description: BoardFlow API token
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
    description: BoardFlow API URL
    required: false
    default: "https://api.boardflow.example.com"

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
SaaSは `latest_completed_run_id` に紐づく `latest_tree_hash` と比較し、buildすべきかskipすべきかを返す。
ここで `completed` は artifact import が成立したことを表し、DRC/ERCの成功を意味しない。

### 6.2 ハッシュ対象

MVPでは、対象は `project_dir` 配下のファイルとする。

```text
対象:
  project_dir/**

除外:
  built-in excludes
  Action input exclude-paths
  .boardflow.yml exclude_paths
```

共通ライブラリ、外部フットプリント、外部3Dモデルなどの依存関係は、MVPでは厳密に追跡しない。
これらの共通依存を変更した場合、MVPでは自動で差分検出されない可能性がある。
共通依存の変更を反映したい場合は、`workflow_dispatch` で `mode: all` を指定して全BoardProjectをbuildする運用を推奨する。
`.boardflow.yml` の `include_paths` やKiCad依存関係の自動解析は将来拡張とする。

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
    "workflow": "BoardFlow",
    "run_id": "987654321",
    "run_attempt": "1"
  },
  "projects": [
    {
      "project_path": "hardware/motor_driver/motor_driver.kicad_pro",
      "config_path": "hardware/motor_driver/.boardflow.yml",
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

`previous_failed` は、前回のBoardRunが `failed` または `timed_out` で、比較に使える completed snapshot がない場合などに使う。
DRC/ERC failed でもBoardRunが `completed` になっている場合は、差分判定上は `latest_tree_hash` の比較対象になり得る。

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

MVPの `outputs.preset: default` で期待する成果物は以下。
Fabrication ZIPまでMVP対象に含めるが、個別artifactを生成できない場合は欠損として記録し、最低条件を満たす限りBoardRun自体は失敗扱いにしない。

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

`manifest` はDB上のArtifact typeとして扱うが、zip bundle内ではrootの `manifest.json` を正本とする。
`metadata/manifest.json` はMVPでは使わない。

### 7.4 Artifact状態

SaaSは、実体があるartifactだけでなく、default outputsで期待されるartifactごとの状態も保存する。

```text
available  # 生成され、final bucket に保存済み
missing    # 期待されていたが成果物として存在しない
failed     # 生成コマンドが失敗した
skipped    # プロジェクト構成や設定により生成対象外
```

`available` のartifactのみ、`storage_key`、`sha256`、`size_bytes` を必須とする。
`missing`、`failed`、`skipped` のartifactは、Web UIやJob Summaryで理由を表示できるように `error_message` や `status_reason` 相当の説明を持つ。

BoardRunを `completed` にできる最低条件は以下とする。

```text
- rootの manifest.json を検証し保存できる
- ERC / DRC のcheck結果、または skipped 状態を保存できる
```

BoardRunの `completed` は artifact import が成立したことを表す。
DRC/ERCの結果がfailedでも、結果を保存できる場合はBoardRun自体は `completed` とする。
ERC/DRCがプロジェクト構成上実行不能、対象外、または将来の設定で無効化されている場合は、`run_checks.status = skipped` として保存する。

### 7.5 manifest例

```json
{
  "schema_version": 1,
  "board_project_id": "bp_abc123",
  "project": {
    "project_path": "hardware/motor_driver/motor_driver.kicad_pro",
    "project_dir": "hardware/motor_driver",
    "config_path": "hardware/motor_driver/.boardflow.yml"
  },
  "git": {
    "ref": "refs/heads/board/motor-driver-v2",
    "branch": "board/motor-driver-v2",
    "commit_sha": "abc123"
  },
  "github_actions": {
    "run_id": "987654321",
    "run_attempt": "1",
    "workflow": "BoardFlow"
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
      "status": "available",
      "path": "review/schematic.pdf",
      "content_type": "application/pdf",
      "sha256": "sha256:...",
      "size_bytes": 123456
    },
    {
      "type": "pcb_top_svg",
      "status": "available",
      "path": "review/pcb_top.svg",
      "content_type": "image/svg+xml",
      "sha256": "sha256:...",
      "size_bytes": 234567
    },
    {
      "type": "ibom",
      "status": "available",
      "path": "assembly/ibom.html",
      "content_type": "text/html",
      "sha256": "sha256:...",
      "size_bytes": 345678
    },
    {
      "type": "fabrication_zip",
      "status": "failed",
      "path": "fabrication/fabrication.zip",
      "content_type": "application/zip",
      "error_message": "fabrication zip export failed"
    }
  ]
}
```

### 7.6 zip bundle仕様

Actionは、生成した成果物群を単一の zip bundle として送信する。

想定レイアウト:

```text
bundle.zip
  manifest.json
  review/
  assembly/
  fabrication/
  checks/
```

backend は zip bundle を展開前に検証し、少なくとも以下を満たすものだけを受理する。

```text
- rootのmanifest.json が存在する
- manifest未記載のzip entryは原則拒否する
- 例外として、rootのmanifest.jsonと、仕様で許可した補助ファイルのみmanifest未記載でも許可する
- entry path が相対パスであり、.. や絶対パスを含まない
- artifact type ごとの許可拡張子 / content type に一致する
- manifest内の各artifactに type / status がある
- `available` なartifactには path / content_type / sha256 / size_bytes がある
- `available` なartifactの sha256 と size_bytes が zip entry と一致する
- bundle / entry ごとの上限サイズを超えない
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
- BoardFlow API token の検証
- token に紐づく installation_id / github_repository_id と request repository の一致確認
- Repositoryの作成または取得
- BoardProjectの作成または取得
- latest_tree_hashとの比較
- build/skip decisionの返却
```

Plan APIではIssue作成ジョブをenqueueしない。
Issueは初回 `BoardRun.status = completed` 後に作成する。

BoardFlow API token は repository 単位で発行する。
SaaSは、Actionから送られた `github_repository_id` をそのまま信用せず、tokenに紐づく `installation_id + github_repository_id` と一致するか必ず検証する。
token はDBにhashのみ保存し、成功した認証時のみ `last_used_at` を更新する。
revoke済みtokenは Plan API、BoardRun作成API、Artifact Bundle Import API、失敗APIのすべてで認可エラーにする。
GitHub App installation が解除済み、権限不足、またはrepository不一致の場合、Plan APIはbuild/skip decisionではなく認可エラーを返す。

### 8.2 BoardRun作成API

build対象になったBoardProjectについて、KiCad実行前にBoardRunを作成する。
BoardRunは「成果物生成を試みた記録」であり、DRC/ERCの成功失敗とは別に管理する。

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
  "github_run_attempt": "1"
}
```

レスポンス例:

```json
{
  "board_run_id": "br_abc123",
  "status": "created",
  "artifact_bundle": {
    "upload_mode": "staging_s3",
    "object_key": "staging/runs/br_abc123/bundle.zip",
    "upload_url": "https://storage.example.com/...",
    "method": "PUT",
    "expires_at": "2026-04-28T12:00:00Z"
  }
}
```

作成直後の `board_runs.status` は `created` とする。
Actionは、presigned URLを受け取った後にbuild、zip作成、staging upload、import API呼び出しへ進む。
staging upload 用URLを発行済みで、import API呼び出し前の状態は `uploading` として扱ってよい。
KiCad実行失敗、zip作成失敗、upload失敗、import要求前の失敗は、作成済みの `board_run_id` に対して失敗APIで記録する。

同じ `board_project_id + github_run_id + github_run_attempt` のBoardRun作成リクエストが再送された場合、SaaSは新規作成せず既存のBoardRunを返す。
これにより、Action側のAPI再送や一時的なネットワーク失敗に対して冪等に扱える。
既存BoardRunが `completed`、`failed`、`timed_out` のいずれかのterminal状態の場合も、SaaSは新しいBoardRunや新しいupload URLを作らず、既存BoardRunの `status` を返す。
Actionはterminal状態の既存BoardRunを受け取った場合、追加のbuild、upload、import要求を行わない。
BoardRun作成から12時間以内に `completed` または `failed` へ到達しない場合、workerが `timed_out` に遷移させる。
GitHub Actionsのcancel、runner停止、Actionプロセス異常終了などでfail APIを呼べなかった場合も、MVPでは原因を細分化せず `timed_out` に集約する。

### 8.3 Artifact Bundle Import API

staging bucket に置いた zip を backend に読ませる経路。

```http
POST /api/v1/board-runs/{board_run_id}/artifact-bundles/import
```

リクエスト例:

```json
{
  "staging_object_key": "staging/runs/br_abc123/bundle.zip",
  "bundle_sha256": "sha256:...",
  "bundle_size_bytes": 12345678
}
```

主な処理:

```text
- staging object の存在確認
- artifact_bundles レコード作成
- BoardRunをimportingへ更新
- artifact import job を queue に積む
- 受理レスポンスを返す
- 実際の zip 読み出し、検証、解析、final bucket 反映、DB 保存は worker が行う
- staging object のTTL管理
```

レスポンス例:

```json
{
  "bundle_id": "ab_abc123",
  "status": "queued"
}
```

### 8.4 BoardRun完了処理

MVPでは `complete` API を提供しない。
zip の検証、artifact登録、checks保存、snapshot保存、BoardRun完了処理は artifact import worker が行う。

完了時にworkerは以下を行う。

```text
- rootのmanifest.jsonを検証して保存する
- ERC / DRC のcheck結果、または skipped 状態を保存する
- 期待artifactごとに available / missing / failed / skipped を保存する
- BoardRunをcompletedにする
- BoardProject.latest_tree_hashを更新する
- BoardProject.latest_completed_run_idを更新する
- 初回completed runでIssue未作成の場合はIssue作成ジョブをenqueueする
- Dashboardコメント更新または作成ジョブをenqueueする
- 必要ならRun Resultコメント作成ジョブをenqueueする
```

DRC/ERCがfailedでも、manifestとチェック結果または skipped 状態を保存できた場合は `BoardRun.status = completed` とする。
DRC/ERCの結果は `run_checks.status` または `board_runs.erc_status` / `board_runs.drc_status` で表す。
import成功済みのstaging bundleは24時間以内に削除対象とする。
failedまたはtimed_outになったrunのstaging bundleは7日後に削除対象とする。
final bucketに保存されたartifactはMVPでは無期限保存とする。

### 8.5 失敗API

ビルド、zip作成、アップロード、import要求前の失敗時に呼び出す。

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
- recreate_issue_on_update
- latest_tree_hash
- latest_completed_run_id
- created_at
- updated_at
```

`repository_id + project_path` にunique制約を置く。
`recreate_issue_on_update` はSaaS側のBoardProject設定として持ち、`.boardflow.yml` には含めない。
Web UIの通常一覧には、初回completed前のBoardProjectも状態付きで表示する。
Plan APIで作成されたが成果物未完成のBoardProjectも、ユーザーがWeb UIから原因を追えるようにする。

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
- review_status
- expires_at
- timed_out_at
- created_at
- completed_at
```

`board_runs` には一覧や絞り込みに必要な集計値を持たせ、UI で使う詳細な review data は別テーブルに正規化して保存する。

`board_runs.status` は成果物生成、upload、importの状態を表す。

```text
created
uploading
importing
completed
failed
timed_out
```

DRC/ERCの成功失敗はBoardRun自体の成功失敗とは分ける。
`completed` は artifact import が成立したことを表し、DRC/ERCの成功を意味しない。
DRC/ERCがfailedでも、manifestとチェック結果または skipped 状態のimportが成功した場合は `completed` として扱い、差分判定の基準になる `latest_tree_hash` を更新する。
`created`、`uploading`、`importing` のまま12時間を超えたBoardRunはworkerが `timed_out` に遷移させる。
GitHub Actionsのcancel、runner停止、fail API未送信の異常終了もMVPでは `timed_out` に集約する。
`board_project_id + github_run_id + github_run_attempt` にunique制約を置き、同一attemptの再送は既存BoardRunを返す。

### 9.4 artifacts

```text
artifacts
- id
- board_run_id
- type
- status
- filename
- content_type
- storage_key
- sha256
- size_bytes
- status_reason
- error_message
- source_bundle_id
- created_at
```

`status` は以下を取る。

```text
available
missing
failed
skipped
```

`available` のartifactのみ `storage_key`、`sha256`、`size_bytes` を必須とする。
`missing`、`failed`、`skipped` のartifactは実体ファイルを持たないが、期待artifactごとの欠損状態をWeb UIとJob Summaryに表示するためDBに行を作る。

### 9.5 artifact_bundles

zip intake 自体の追跡用テーブル。

```text
artifact_bundles
- id
- board_run_id
- intake_mode
- staging_object_key
- original_filename
- sha256
- size_bytes
- status
- error_message
- received_at
- validated_at
- delete_after
```

import成功済みのstaging bundleは24時間以内に削除対象とする。
failedまたはtimed_outになったrunのstaging bundleは7日後に削除対象とする。
final bucketのartifactはMVPでは無期限保存とする。

### 9.6 run_checks

DRC / ERC の集計情報。

```text
run_checks
- id
- board_run_id
- check_kind          # erc | drc
- tool_name
- tool_version
- status
- error_count
- warning_count
- notice_count
- report_artifact_id
- raw_summary_json
- created_at
```

`status` は少なくとも以下を取る。

```text
passed
failed
skipped
```

`skipped` は、ERC/DRCがプロジェクト構成上実行不能、対象外、または設定で無効化された場合に使う。
`skipped` の場合、`error_count`、`warning_count`、`notice_count` は0として扱い、`raw_summary_json` に理由を保存する。

### 9.7 run_check_findings

レビュー UI で直接使う明細。

```text
run_check_findings
- id
- run_check_id
- severity            # error | warning | notice
- rule_code
- title
- message
- subject_kind        # schematic | pcb | net | footprint | symbol
- subject_ref
- sheet_path
- pcb_layer
- x_um
- y_um
- bbox_json
- raw_payload_json
- sort_index
- created_at
```

方針:

```text
- board_runs には集計を保持する
- run_checks には ERC / DRC 単位の結果を保持する
- run_check_findings には UI で一覧・フィルタ・詳細表示する行データを保持する
- parser が取りこぼしたくない項目は raw JSON も併せて保持する
```

### 9.8 board_project_snapshots

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

### 9.9 boardflow_api_tokens

ActionからSaaS APIを呼び出すためのrepository単位token。
tokenの平文は作成時のみ表示し、DBにはhashのみ保存する。

```text
boardflow_api_tokens
- id
- installation_id
- repository_id
- name
- token_hash
- created_at
- last_used_at
- revoked_at
```

revoke済みtokenは認可に使えない。
`last_used_at` は認証に成功したAPI呼び出しでのみ更新する。
複数tokenの高度な管理UI、ローテーション専用UX、自動失効ポリシーはMVPでは扱わない。

### 9.10 github_jobs

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

### 9.11 board_project_issue_history

BoardProjectに紐づいていた過去Issueを履歴として保持する。
active Issueは `board_projects` の `issue_number` / `issue_node_id` / `issue_url` が指す。

```text
board_project_issue_history
- id
- board_project_id
- issue_number
- issue_node_id
- issue_url
- reason              # recreated | deleted | manual_archive
- replaced_by_issue_node_id
- created_at
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

BoardProjectに対応するIssueが存在しない場合でも、Plan API時点ではIssue作成ジョブをenqueueしない。
初回 `BoardRun.status = completed` になった後、SaaSはIssue作成ジョブをenqueueする。
初回completed run前のBoardProjectもWeb UIの通常一覧には表示するが、Issueは作成しない。

Issue作成はキューで処理し、レートリミットや大量基板登録に備える。

Issueは基板の設計、発注、実装、検査の管理単位として使う。
発注などで基板が固まったタイミングでIssueがcloseされる運用を想定するため、close済みIssueを自動でreopenしない。

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
<!-- boardflow:repository_id=123456789 -->
<!-- boardflow:project_path=hardware/motor_driver/motor_driver.kicad_pro -->

# Board Project

KiCad project:

`hardware/motor_driver/motor_driver.kicad_pro`

This issue tracks design, fabrication, assembly, and verification for this board.

## BoardFlow

Latest board page:

https://boardflow.example.com/repositories/123456789/boards/bp_abc123
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

### 10.7 Issueライフサイクル

Issueのタイトルや本文がユーザーにより編集された場合、BoardFlowは原則として上書きしない。
Issueは `issue_node_id` / `issue_number` で追跡する。

BoardProjectにはSaaS側設定として `recreate_issue_on_update` を持つ。
active Issueがclosedで、`recreate_issue_on_update = true` かつ前回completed runから `tree_hash` が変わった場合、BoardFlowは完全に新しいIssueを作成する。
このとき既存Issueはreopenせず、新Issue作成後に `board_projects.issue_number` / `issue_node_id` / `issue_url` / `dashboard_comment_id` を新Issue側へ更新する。
旧Issueは履歴として保持する。

active Issueがclosedで `recreate_issue_on_update = false` の場合、SaaS上のBoardProjectとRunは更新するが、Issueコメント更新は行わない。
Issueが削除済みまたは404相当の場合は、GitHub API更新ジョブ実行時に検出し、Issue未作成相当として扱う。

---

## 11. Issueコメント仕様

Issueコメントは2種類に分ける。

```text
A. Dashboardコメント
  - SaaSのBoardProjectページへのリンク
  - 最新Runページへのリンク
  - 最新ステータス
  - 既存コメントを編集する

B. Run Resultコメント
  - DRC/ERCなどCI要素の強い情報
  - 重要なrunごとに追記する
```

### 11.1 Dashboardコメント

DashboardコメントはBoardProjectごとに1つだけ作成し、以後は編集更新する。
private artifact はGitHub Issueへ直接表示しない。
Issueコメントにはartifactの直接リンク、署名付きURL、画像埋め込み、iBOMやFabrication ZIPへの直リンクを載せず、SaaSの認可付きページへのリンクのみを載せる。

例:

```markdown
<!-- boardflow:comment_type=dashboard -->
<!-- boardflow:project_path=hardware/motor_driver/motor_driver.kicad_pro -->

## BoardFlow Dashboard

Latest run: `abc1234` on `board/motor-driver-v2`

| Item | Link |
|---|---|
| Board page | https://boardflow.example.com/... |
| Schematic PDF | https://boardflow.example.com/... |
| PCB Preview | https://boardflow.example.com/... |
| Interactive BOM | https://boardflow.example.com/... |
| Fabrication ZIP | https://boardflow.example.com/... |
| BOM CSV | https://boardflow.example.com/... |

### Latest status

| Check | Result |
|---|---|
| ERC | ✅ 0 errors, 2 warnings |
| DRC | ❌ 1 error, 4 warnings |

Last updated by BoardFlow.
```

`dashboard_comment_id` は `board_projects` に保存する。
コメントが手動削除された場合は、GitHub API更新ジョブ実行時に検出する。
active IssueがopenでIssue連携が有効な場合はDashboardコメントを再作成し、`dashboard_comment_id` を更新する。

### 11.2 Run Resultコメント

DRC/ERCなど、CIとしての履歴を残したい情報はrunごとに追記する。

例:

```markdown
<!-- boardflow:comment_type=run_result -->
<!-- boardflow:board_run_id=br_abc123 -->

## BoardFlow Run Result

Commit: `abc1234`  
Branch: `board/motor-driver-v2`  
Run: https://boardflow.example.com/...

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
  -> Issue状態確認
  -> 初回completed runでIssue未作成ならcreate issue job
  -> updateまたはcreate dashboard comment job
  -> create run result comment job if needed
```

Issueがまだない場合:

```text
BoardRun completed
  -> create issue job
  -> create dashboard comment job
  -> create run result comment job if needed
```

GitHub APIジョブは、実行時にactive IssueとDashboardコメントの現在状態を確認する。
Issueがclosedの場合は `recreate_issue_on_update` と `tree_hash` 変更有無に基づき、新Issue作成またはIssue更新停止を選ぶ。
Issueやコメントが404相当の場合は、未作成または削除済みとして扱い、必要に応じて再作成する。

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
通常一覧には初回completed前のBoardProjectも表示する。
Plan APIで作成されたが成果物未完成のBoardProject、初回importに失敗したBoardProject、timeoutしたBoardProjectも、状態付きで表示して原因を追えるようにする。

一覧上の状態は、少なくとも以下を区別する。

```text
detected    # Plan APIで検出済みだが、まだBoardRunがない
building    # created / uploading / importing のBoardRunがある
failed      # 最新BoardRunが failed
timed_out   # 最新BoardRunが timed_out。中断または未完了の可能性あり
completed   # latest_completed_run_id がある
```

表示項目:

```text
- display_name
- project_path
- BoardProject状態
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
- Gerber ZIP のartifact状態とリンク
- Drill ZIP のartifact状態とリンク
- Fabrication ZIP のartifact状態とリンク
- 製造用チェックリスト
```

artifact状態は `available`、`missing`、`failed`、`skipped` を表示する。
`available` の場合のみダウンロードまたはプレビューへの導線を出す。
`missing`、`failed`、`skipped` の場合は理由を表示する。

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

Run詳細やartifact一覧では、期待artifactごとに `available`、`missing`、`failed`、`skipped` を表示する。

---

## 14. GitHub Actions Job Summary

Actionは、GitHub ActionsのJob Summaryに結果を出力する。

例:

```markdown
# BoardFlow Summary

## Unsupported

- `pull_request` event is not supported in MVP. Skipped without calling BoardFlow API.

## Built

- `hardware/motor_driver/motor_driver.kicad_pro` — hash changed
- `hardware/sensor_board/sensor_board.kicad_pro` — new project

## Skipped

- `hardware/power_board/power_board.kicad_pro` — unchanged

## Build/Upload/Import Failed

- `hardware/broken_export/broken_export.kicad_pro` — kicad-cli export failed

## Check Failed

- `hardware/test_board/test_board.kicad_pro` — DRC failed

## Missing Artifacts

- `hardware/motor_driver/motor_driver.kicad_pro` — fabrication_zip failed

## Detection errors

- `hardware/broken_board/.boardflow.yml` — no .kicad_pro in same directory
```

SaaSへのアップロードURLやBoardProjectページも表示する。
複数BoardProjectのうち一部のみ失敗した場合も、成功、skip、build/upload/import失敗、check失敗、artifact欠損、検出エラーを分けて表示する。
`pull_request` event の場合は、KiCad実行やSaaS API呼び出しを行わず、unsupported として表示したうえで job は成功扱いにする。
共通ライブラリ、外部フットプリント、外部3Dモデルなどを変更した場合は、MVPでは自動検知できない可能性があるため、`workflow_dispatch` で `mode: all` を指定する案内を表示してよい。
検出不備、成果物生成失敗、upload失敗、import要求失敗が1件でもある場合、最終的なGitHub Actions jobは失敗とする。
個別artifactの `missing`、`failed`、`skipped` はJob Summaryに警告として表示するが、manifestとDRC/ERC結果を保存できる限り、それだけではGitHub Actions jobを失敗にしない。
DRC/ERCのfailedをjob失敗にするかは `fail-on-drc` / `fail-on-erc` に従う。

---

## 15. MVPでやること・やらないこと

### 15.1 やること

```text
- Docker Action提供
- KiCad 9.0系環境の固定
- .boardflow.yml 自動検出
- .boardflow.yml の厳格schema検証
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
- 欠損artifactの状態管理
- ERC/DRC skipped状態の管理
- BoardRunの12時間timeout
- 初回completed前を含むBoardProjectのWeb表示
- 初回completed run後のGitHub AppによるIssue自動作成
- close済みIssueに対する設定ON時の新Issue作成
- Dashboardコメントの作成・編集
- DRC/ERC結果コメントの条件付き追記
- GitHub API操作のキュー処理
- BoardFlow API tokenの最小ライフサイクル管理
```

### 15.2 やらないこと

```text
- グローバル設定ファイル
- board keyのユーザー設定
- issue numberのユーザー設定
- project_path変更の追従
- 共通ライブラリ変更の厳密な影響解析
- include_paths / 共通依存の自動解析
- SaaS側でのKiCad実行
- 高度なKiCad差分ビューア
- 部品調達API連携
- BoardProjectの手動統合
- PR中心のレビュー機能
- pull_request / fork PR 対応
- PR artifact review / PRコメント連携
- GitHub Issue上でのprivate artifact直接表示
- final artifact保存期限のプラン別制御
- 複数tokenの高度な管理UIや自動ローテーション
```

### 15.3 MVP受け入れシナリオ

```text
- pull_request eventではActionがKiCad実行もAPI呼び出しもせず、job成功でunsupported summaryを出す
- Plan APIで新規BoardProjectが作られた時点でWeb UI通常一覧に detected として表示されるが、初回completed run前はIssueを作らない
- .boardflow.yml に未知フィールドがあるBoardProjectは検出エラーになり、他の正常BoardProjectは処理されるがjob全体は失敗する
- 初回artifact import completed後にIssue作成ジョブとDashboardコメントジョブがenqueueされる
- ERCが対象外のプロジェクトで run_checks.status=skipped が保存され、manifest import成功ならBoardRunはcompletedになる
- fabrication ZIP生成失敗時、BoardRunはcompleted、artifact行はfailed、Job Summaryは警告になる
- DRC failedかつ fail-on-drc=false の場合、BoardRunはcompleted、GitHub Actions jobは成功する
- DRC failedかつ fail-on-drc=true の場合、BoardRunはcompleted、GitHub Actions jobは失敗する
- BoardRun作成から12時間を超えた未完了runはtimed_outになる
- 同一 github_run_id + github_run_attempt のBoardRun作成再送は既存runを返し、terminal状態なら新upload URLを作らない
- manifest未記載のzip entryを含むbundleは、許可済み補助ファイルを除いてimport failedになる
- revoke済みtokenでPlan APIを呼ぶと認可エラーになり、last_used_atは成功認証時のみ更新される
- import成功済みstaging bundleは24時間以内に削除対象、timed_out/failed bundleは7日後に削除対象になる
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

将来的に `.boardflow.yml` に `include_paths` を追加する。

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
- .boardflow.yml をKiCadプロジェクト横に置くだけで対象化
- Docker ActionでKiCad 9.0系成果物を自動生成
- SaaS側のhash判定で必要な基板だけbuild
- 初回成果物ができた後、GitHub Appが基板ごとのIssueを自動作成
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
