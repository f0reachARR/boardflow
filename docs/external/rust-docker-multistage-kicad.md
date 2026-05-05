# Rust Docker Multi-Stage Build for KiCad Action

## 要約

`kicad/kicad:9.0` (Debian 12 Bookworm, glibc ベース) で動作する Rust バイナリを、マルチステージ Docker ビルドで構築するための戦略。ビルドステージで `rust:1.87-bookworm` を使い、ランタイムステージで `kicad/kicad:9.0` にバイナリをコピーする。

## 確認した情報

### kicad/kicad:9.0 のベース

- **OS**: Debian 12 Bookworm (glibc 2.36)
- **アーキテクチャ**: linux/amd64, linux/arm64
- **イメージサイズ**: ~1.23 GB (9.0-full)
- **用途**: kicad-cli の CI 利用を主目的としたイメージ
- **ソース**: https://gitlab.com/kicad/packaging/kicad-cli-docker

### musl vs glibc の選択

- **結論: glibc ビルドを採用**
- kicad/kicad:9.0 は Debian 12 (glibc) ベースのため、同じ glibc リンクが最も互換性が高い
- musl の問題点:
  - マルチスレッド性能が glibc の10倍遅い場合がある (標準アロケータが弱い)
  - DNS解決など一部機能で互換性問題がある
  - ビルド時間も glibc の方が速い
- glibc 動的リンクでも、ランタイムイメージが同じ Debian 12 なので問題なし

### 推奨 Dockerfile パターン

```dockerfile
# ============ Build Stage ============
FROM rust:1.87-bookworm AS builder

WORKDIR /app

# cargo-chef で依存キャッシュ (オプション、ビルド高速化)
RUN cargo install cargo-chef --locked
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM rust:1.87-bookworm AS cacher
RUN cargo install cargo-chef --locked
WORKDIR /app
COPY --from=builder /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json -p boardflow-action-runner

FROM rust:1.87-bookworm AS final-builder
WORKDIR /app
COPY --from=cacher /usr/local/cargo /usr/local/cargo
COPY --from=cacher /app/target target
COPY . .
RUN cargo build --release -p boardflow-action-runner

# ============ Runtime Stage ============
FROM kicad/kicad:9.0

USER root

RUN apt-get update && apt-get install -y --no-install-recommends \
    python3-pip \
    xvfb \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

RUN pip3 install --break-system-packages interactivehtmlbom

COPY --from=final-builder /app/target/release/boardflow-action-runner /usr/local/bin/boardflow-action-runner

ENTRYPOINT ["/usr/local/bin/boardflow-action-runner"]
```

### 簡易パターン (cargo-chef なし)

```dockerfile
# Build Stage
FROM rust:1.87-bookworm AS builder
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/
RUN cargo build --release -p boardflow-action-runner
RUN cp target/release/boardflow-action-runner /boardflow-action-runner

# Runtime Stage
FROM kicad/kicad:9.0
USER root
RUN apt-get update && apt-get install -y --no-install-recommends \
    python3-pip xvfb ca-certificates \
    && rm -rf /var/lib/apt/lists/*
RUN pip3 install --break-system-packages interactivehtmlbom
COPY --from=builder /boardflow-action-runner /usr/local/bin/boardflow-action-runner
ENTRYPOINT ["/usr/local/bin/boardflow-action-runner"]
```

### BuildKit キャッシュマウント

```dockerfile
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/target \
    cargo build --release -p boardflow-action-runner && \
    cp target/release/boardflow-action-runner /boardflow-action-runner
```

## BoardFlow への示唆

- **ビルドターゲット**: `x86_64-unknown-linux-gnu` (Debian 12 Bookworm 同等)
- **rust ベースイメージ**: `rust:1.87-bookworm` を使用し、glibc バージョンを一致させる
- **依存キャッシュ**: 初回は cargo-chef なしの簡易パターンで十分。CI でのビルド頻度が高くなった場合に cargo-chef を追加
- **不要ツール削除**: 旧スクリプト用の `jq`, `zip` CLI, `curl` は Rust バイナリに内包されるため不要に
- **Python依存**: `interactivehtmlbom` は Python パッケージのため引き続きランタイムに必要
- **xvfb**: iBOM 生成に必要なため引き続きランタイムに必要

## 採用/不採用判断

**採用**: glibc 動的リンク + `rust:1.87-bookworm` ビルド → `kicad/kicad:9.0` ランタイム

## 制約とpitfall

1. **glibc バージョン不一致**: ビルドステージの Debian バージョンがランタイムより新しいと、`GLIBC_2.xx not found` エラーになる。必ず同じ Debian バージョン (Bookworm) を使う
2. **イメージサイズ**: KiCad イメージは ~1.2GB と大きく、Rust バイナリ追加は +10-30MB 程度で相対的影響小
3. **GitHub Actions でのビルド時間**: Rust の初回ビルドは5-10分かかる。`docker/build-push-action` の GHA キャッシュまたは pre-built image で軽減可能
4. **ARM64 対応**: kicad/kicad:9.0 は arm64 もサポート。将来的に cross-compile が必要になる可能性あり
5. **OpenSSL vs rustls**: reqwest の TLS backend に注意。rustls を使えば OpenSSL 開発ヘッダが不要

## 未解決の疑問

- GitHub Actions での Docker ビルドキャッシュの最適な設定 (GHA cache vs Registry cache)
- KiCad 10.0 への将来的な移行時の影響

## 参照URL

- https://hub.docker.com/r/kicad/kicad
- https://gitlab.com/kicad/packaging/kicad-cli-docker
- https://docs.docker.com/dhi/core-concepts/glibc-musl/
- https://depot.dev/blog/rust-dockerfile-best-practices
- https://earthly.dev/blog/cargo-chef/
