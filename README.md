# Rust Paint Foundation

`egui + eframe` だけで構成した、Rust オンリーのお絵かきツール基盤です。  
同じコードベースから native 実行と WebAssembly 実行を行い、GitHub Pages へ静的配信できる最小 MVP を整えています。

## プロジェクト概要

- Rust だけで UI とアプリケーション本体を構築
- `egui` で分かりやすい即時モード UI を実装
- `eframe` で native / web の両ターゲットを共通化
- MVP として「描ける・色を変えられる・太さを変えられる・消せる・Undo できる・Clear できる」まで実装
- 保存はまだ未実装だが、将来の編集用独自形式と PNG 出力に進みやすいデータ構造を用意

## 技術選定理由

- `egui`
  - Rust だけで UI を完結でき、即時モードなのでツール類の試作と拡張が速い
- `eframe`
  - `egui` 公式フレームワークであり、native と wasm の両対応を同一コードベースで進めやすい
- `serde`
  - 作品データを将来ローカルファイル保存するためのシリアライズ基盤として導入
- `Trunk`
  - `eframe_template` 系でも使われる定番構成で、wasm ビルドとローカル確認を簡潔にできる
  - このリポジトリでは `0.21.14` で検証

## Rust バージョン要件

- このリポジトリは `rust-toolchain.toml` で `1.94.0` に固定しています
- `Cargo.toml` 上の最小要件は `rust-version = 1.88`
- `eframe` / `egui` は breaking changes が比較的入りやすいため、現時点では `0.33.3` に固定しています
- 将来更新する場合は 1 バージョンずつ上げて、`egui` / `eframe` の changelog を確認してください

## セットアップ手順

### 1. Rust を入れる

```bash
rustup toolchain install 1.94.0
rustup default 1.94.0
rustup component add rustfmt clippy
rustup target add wasm32-unknown-unknown
```

### 2. Web ビルド用の Trunk を入れる

```bash
cargo install --locked trunk --version 0.21.14
```

### 3. 依存解決

```bash
cargo fetch
```

## native 起動手順

```bash
cargo run
```

## web 起動手順

### ローカル開発サーバー

```bash
trunk serve
```

- 既定の URL は `http://127.0.0.1:8080`
- ブラウザで上記 URL を開くと WebAssembly 版が動作します

### 配布用ビルド

```bash
trunk build --release
```

- 出力は `dist/`
- GitHub Pages へ載せる静的成果物になります

## GitHub Pages デプロイ手順

### 前提

- GitHub Free で公開する場合は、リポジトリを `public` にしておくのが安全です
- このリポジトリには [`.github/workflows/pages.yml`](.github/workflows/pages.yml) を追加済みです

### GitHub 側設定

1. GitHub に push する
2. リポジトリの `Settings` -> `Pages` を開く
3. `Build and deployment` の `Source` を `GitHub Actions` にする
4. `main` または `master` へ push すると workflow が走る

### 公開 URL の扱い

- プロジェクト Pages (`https://<user>.github.io/<repo>/`) は workflow が自動で `public-url` を `/<repo>/` に設定します
- ユーザー/組織 Pages (`<user>.github.io`) 形式のリポジトリでは `public-url` を `/` に切り替えるよう workflow 内で分岐しています

## ドキュメント

- [README.md](README.md)
- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)
- [docs/MVP_SCOPE.md](docs/MVP_SCOPE.md)

## 今後の拡張候補

- 編集用保存形式の実装
- PNG 出力
- レイヤー
- 図形ツール
- 選択ツール
- パレット管理
- ショートカットキー
- タッチ / ペン入力の最適化

## 開発時の確認コマンド

```bash
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo build
cargo build --target wasm32-unknown-unknown
trunk build --release
```

### Trunk の色設定エラーを避けるメモ

- 環境変数 `NO_COLOR=1` が入っているシェルでは、`trunk 0.21.14` が `invalid value '1' for '--no-color'` で失敗することがあります
- その場合は次のように上書きしてください

```bash
NO_COLOR=false trunk serve
NO_COLOR=false trunk build --release
```
