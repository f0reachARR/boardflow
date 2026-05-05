# GitHub Actions Docker Action with Rust Binary Entrypoint

## 要約

GitHub Actions の Docker container action で、entrypoint を bash スクリプトから Rust バイナリに差し替える際の仕組みと注意点。入力は `INPUT_*` 環境変数で渡され、出力は `GITHUB_OUTPUT` / `GITHUB_STEP_SUMMARY` ファイルへの書き込みで実現する。

## 確認した情報

### Docker Action の動作メカニズム

1. `action.yml` で `runs.using: 'docker'` を指定
2. GitHub Actions ランナーがコンテナを起動し、以下を設定:
   - `inputs` は `INPUT_<NAME>` 環境変数として注入 (ハイフンはアンダースコアに変換、大文字化)
   - GitHub コンテキスト変数 (`GITHUB_SHA`, `GITHUB_REF`, etc.) が環境変数として渡される
   - ワークスペースは `/github/workspace` にマウント
   - 各種ファイルパス環境変数 (`GITHUB_OUTPUT`, `GITHUB_STEP_SUMMARY`) が設定される
3. `ENTRYPOINT` で指定されたバイナリ/スクリプトが実行される

### 入力の受け取り方 (Rust 側)

```rust
use std::env;

struct ActionInputs {
    token: String,
    mode: String,
    exclude_paths: String,
    api_url: String,
    fail_on_drc: bool,
    fail_on_erc: bool,
}

impl ActionInputs {
    fn from_env() -> Result<Self, String> {
        Ok(Self {
            token: env::var("INPUT_TOKEN")
                .map_err(|_| "Input 'token' is required".to_string())?,
            mode: env::var("INPUT_MODE").unwrap_or_else(|_| "auto".to_string()),
            exclude_paths: env::var("INPUT_EXCLUDE-PATHS")  // 注: ハイフンのまま
                .or_else(|_| env::var("INPUT_EXCLUDE_PATHS"))  // アンダースコア版もチェック
                .unwrap_or_default(),
            api_url: env::var("INPUT_API-URL")
                .or_else(|_| env::var("INPUT_API_URL"))
                .unwrap_or_else(|_| "https://api.boardflow.example.com".to_string()),
            fail_on_drc: env::var("INPUT_FAIL-ON-DRC")
                .or_else(|_| env::var("INPUT_FAIL_ON_DRC"))
                .unwrap_or_else(|_| "false".to_string()) == "true",
            fail_on_erc: env::var("INPUT_FAIL-ON-ERC")
                .or_else(|_| env::var("INPUT_FAIL_ON_ERC"))
                .unwrap_or_else(|_| "false".to_string()) == "true",
        })
    }
}
```

**重要**: GitHub Actions はハイフン付き input 名を環境変数にする際、**ハイフンをアンダースコアに変換する**。`exclude-paths` → `INPUT_EXCLUDE_PATHS` (大文字化)。

### 出力の書き込み方 (Rust 側)

```rust
use std::fs::OpenOptions;
use std::io::Write;

/// GITHUB_OUTPUT にキー=値を追記
fn set_output(key: &str, value: &str) -> std::io::Result<()> {
    if let Ok(path) = std::env::var("GITHUB_OUTPUT") {
        let mut file = OpenOptions::new().append(true).open(&path)?;
        writeln!(file, "{}={}", key, value)?;
    }
    Ok(())
}

/// GITHUB_STEP_SUMMARY に Markdown を追記
fn write_summary(markdown: &str) -> std::io::Result<()> {
    if let Ok(path) = std::env::var("GITHUB_STEP_SUMMARY") {
        let mut file = OpenOptions::new().append(true).open(&path)?;
        write!(file, "{}", markdown)?;
    }
    Ok(())
}

/// 複数行の出力値 (delimiter 方式)
fn set_multiline_output(key: &str, value: &str) -> std::io::Result<()> {
    if let Ok(path) = std::env::var("GITHUB_OUTPUT") {
        let mut file = OpenOptions::new().append(true).open(&path)?;
        let delimiter = format!("EOF_{}", uuid::Uuid::new_v4().simple());
        writeln!(file, "{}<<{}", key, delimiter)?;
        writeln!(file, "{}", value)?;
        writeln!(file, "{}", delimiter)?;
    }
    Ok(())
}
```

### GitHub コンテキスト環境変数

```rust
struct GitHubContext {
    workspace: String,      // GITHUB_WORKSPACE (/github/workspace)
    event_name: String,     // GITHUB_EVENT_NAME
    repository: String,     // GITHUB_REPOSITORY (owner/repo)
    sha: String,            // GITHUB_SHA
    ref_name: String,       // GITHUB_REF
    branch: String,         // GITHUB_REF_NAME
    run_id: String,         // GITHUB_RUN_ID
    run_attempt: String,    // GITHUB_RUN_ATTEMPT
}
```

### action.yml の変更

```yaml
runs:
  using: 'docker'
  image: 'Dockerfile'
  # args は不要 — すべて INPUT_* 環境変数経由で取得
```

### エラー報告

```rust
/// GitHub Actions の ::error:: アノテーション
fn error(message: &str) {
    eprintln!("::error::{}", message);
}

/// GitHub Actions の ::warning:: アノテーション
fn warning(message: &str) {
    eprintln!("::warning::{}", message);
}

/// GitHub Actions の ::notice:: アノテーション
fn notice(message: &str) {
    eprintln!("::notice::{}", message);
}
```

### ローカルテスト

```bash
docker build -t boardflow-action:test .
docker run --rm \
  -e INPUT_TOKEN="test-token" \
  -e INPUT_MODE="auto" \
  -e INPUT_API_URL="http://localhost:3000" \
  -e INPUT_FAIL_ON_DRC="false" \
  -e INPUT_FAIL_ON_ERC="false" \
  -e GITHUB_OUTPUT="/dev/stdout" \
  -e GITHUB_STEP_SUMMARY="/dev/null" \
  -e GITHUB_WORKSPACE="/workspace" \
  -e GITHUB_EVENT_NAME="push" \
  -e GITHUB_REPOSITORY="owner/repo" \
  -e GITHUB_SHA="abc123" \
  -e GITHUB_REF="refs/heads/main" \
  -e GITHUB_REF_NAME="main" \
  -e GITHUB_RUN_ID="12345" \
  -e GITHUB_RUN_ATTEMPT="1" \
  -v "$(pwd):/workspace" \
  boardflow-action:test
```

## BoardFlow への示唆

- `action.yml` は変更不要 (入力/出力定義はそのまま)
- entrypoint を `/usr/local/bin/boardflow-action-runner` に変更するだけ
- `args` セクションは削除可能 (すべて環境変数で取得)
- pre-built image (`docker://ghcr.io/...`) を使えばビルド時間を排除可能 (将来)

## 採用/不採用判断

**採用**: `INPUT_*` 環境変数読み取り + `GITHUB_OUTPUT`/`GITHUB_STEP_SUMMARY` ファイル書き込み

## 制約とpitfall

1. **INPUT_ 変数名の変換ルール**: ハイフンはアンダースコアに変換され、全て大文字化される。`exclude-paths` → `INPUT_EXCLUDE_PATHS`
2. **GITHUB_OUTPUT はファイルパス**: 環境変数の値はファイルパスであり、そのファイルに追記する必要がある
3. **マルチライン出力**: `<<DELIMITER` ... `DELIMITER` 形式が必要
4. **ワークスペースパス**: GitHub Actions では `/github/workspace`、ローカルテストでは任意パス
5. **終了コード**: 非ゼロで Action 失敗。Rust 側では `std::process::exit(1)` を使う
6. **ファイル権限**: コンテナ内で root 実行が一般的だが、ワークスペースファイルの owner に注意

## 未解決の疑問

- なし (十分に文書化された仕様)

## 参照URL

- https://docs.github.com/en/actions/sharing-automations/creating-actions/creating-a-docker-container-action
- https://docs.github.com/en/actions/writing-workflows/choosing-what-your-workflow-does/workflow-commands-for-github-actions
- https://dev.to/cicirello/how-to-write-to-workflow-job-summary-from-a-github-action-23ah
