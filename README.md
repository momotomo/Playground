# Rust Paint Foundation

`egui + eframe` だけで構成した、Rust オンリーのお絵かきツール基盤です。  
同じコードベースから native 実行と WebAssembly 実行を扱い、GitHub Pages へ静的配信できます。  
このフェーズでは、再編集用 JSON 保存、`PNG 出力`、`ズーム / パン` に加えて、`選択 / 移動` と基本 `図形ツール` を追加しています。

## プロジェクト概要

- Rust だけで UI とアプリケーション本体を構築
- `egui` による即時モード UI
- `eframe` による native / web 共通アプリ基盤
- 線描画、色変更、線幅変更、消しゴム、`Undo`、`Redo`、`Clear`
- 編集用 JSON 形式での `Save` / `Load`
- 共有用 `Export PNG`
- キャンバスのズーム、パン、表示リセット
- 単一選択、ドラッグ移動
- 矩形、楕円、直線ツール
- native / web の保存導線差分吸収

## 技術選定理由

- `egui`
  - Rust だけで UI を完結でき、ツールパネルや状態表示を素早く試作しやすい
- `eframe`
  - `egui` 公式フレームワークで、native と wasm を同じアプリ本体で扱いやすい
- `serde`
  - 作品データを堅実にシリアライズするため
- `serde_json`
  - 再編集用の独自保存形式を読みやすい JSON envelope で持たせるため
- `rfd`
  - native のファイルダイアログと web のダウンロード / ファイル選択を Rust だけで扱うため
- `tiny-skia`
  - 作品データから表示倍率に依存しない PNG をラスタライズするため
- `Trunk`
  - wasm ビルドとローカル確認を簡潔に行うため

## Rust バージョン要件

- `rust-toolchain.toml` で `1.94.0` に固定しています
- `Cargo.toml` 上の最小要件は `rust-version = 1.88`
- `egui` / `eframe` は breaking changes が入りやすいため、現時点では `0.33.3` に固定しています

## セットアップ手順

### 1. Rust を入れる

```bash
rustup toolchain install 1.94.0
rustup default 1.94.0
rustup component add rustfmt clippy
rustup target add wasm32-unknown-unknown
```

### 2. Trunk を入れる

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

## 操作方法

### ツール

- `Select`
  - 要素をクリックして選択し、そのままドラッグで移動します
- `Brush`
  - フリーハンドの線を描きます
- `Eraser`
  - キャンバス背景色で消すフリーハンドツールです
- `Rectangle`
  - ドラッグした範囲の外枠を描きます
- `Ellipse`
  - ドラッグした範囲の楕円外枠を描きます
- `Line`
  - ドラッグ開始点から終了点まで直線を描きます

### 編集

- `Undo`: 直前の編集を戻します
- `Redo`: `Undo` した編集を戻します
- `Clear`: 作品全体を消去します
- `Save`: 再編集用 JSON を保存します
- `Load`: JSON から再編集状態を復元します
- `Export PNG`: 背景と全要素を含む共有用 PNG を書き出します

### 選択 / 移動

- `Select` ツールでストロークと図形のどちらも選択できます
- 選択中の要素にはハイライト付きバウンディングボックスを表示します
- ドラッグ中はプレビュー表示し、ドロップ時に履歴へコミットします
- 選択状態そのものは `Undo` / `Redo` に含めません

### ズーム / パン

- `+` / `-` ボタンでズーム
- `Reset View` でキャンバス全体が入る表示に戻す
- `Ctrl/Cmd + マウスホイール` でズーム
- `Space + Drag` または `中ボタンドラッグ` でパン
- 現在のズーム倍率は上部ツールバーと左パネルに表示

### ショートカット一覧

- `Ctrl/Cmd + Z`: Undo
- `Ctrl/Cmd + Shift + Z`: Redo
- `Ctrl/Cmd + Y`: Redo の代替
- `Ctrl/Cmd + S`: Save
- `Ctrl/Cmd + O`: Load
- `Ctrl/Cmd + Shift + E`: Export PNG
- `Ctrl/Cmd + +` または `Ctrl/Cmd + =`: Zoom in
- `Ctrl/Cmd + -`: Zoom out
- `Ctrl/Cmd + 0`: Reset View
- `V`: Select
- `B`: Brush
- `R`: Rectangle
- `O`: Ellipse
- `L`: Line
- `E`: Eraser

## 保存形式

### JSON 保存

- 用途は「再編集用」
- 既定ファイル名は `untitled.paint.json`
- JSON envelope に次の情報を保持します
  - `format.id`
  - `format.version`
  - `metadata`
  - `document.canvas_size`
  - `document.background`
  - `document.elements[]`
  - 各 element の `element_type`
  - stroke の `tool` / `color` / `width` / `points`
  - shape の `kind` / `color` / `width` / `start` / `end`
- 現在の format version は `2`
- 旧 `version = 1` の stroke-only JSON も読込互換を残しています

### PNG 出力

- 用途は「共有 / 閲覧用」
- 既定ファイル名は `untitled.png`
- 表示中のズーム倍率や選択枠は含めません
- 作品のキャンバスサイズを基準に、背景色と全要素をラスタライズします

## native / web の違い

- native
  - `Save` / `Load` は OS のファイルダイアログを使います
  - `Export PNG` は OS ダイアログから `.png` を保存します
- web
  - `Save` はブラウザダウンロードとして JSON を保存します
  - `Load` はブラウザのファイル選択を使います
  - `Export PNG` はブラウザダウンロードとして保存します
  - GitHub Pages 上ではブラウザ制約のため、native のような継続的ファイルハンドル保持はしません
- 選択 / 移動 / 図形ツール / ズーム / パンの基本操作は native / web で同じです

## GitHub Pages デプロイ手順

### 前提

- GitHub Free で使う場合は、リポジトリを `public` にするのが基本です
- private repo では、プラン次第で GitHub Pages が使えないことがあります
- このリポジトリには `.github/workflows/pages.yml` を追加済みです

### GitHub 側設定

1. GitHub に push する
2. `Settings` -> `Pages` を開く
3. `Build and deployment` の `Source` を `GitHub Actions` にする
4. `master` へ push すると Pages workflow が走る

### workflow の挙動

- public repo では web build と Pages deploy を実行します
- private repo では Pages 非対応プランでも CI が分かりやすく終わるよう、deploy job を skip して理由を表示します

## ドキュメント

- `README.md`
- `docs/ARCHITECTURE.md`
- `docs/MVP_SCOPE.md`

## 今後の拡張候補

- レイヤー
- 複数選択
- リサイズハンドル
- 塗り、角丸矩形、図形編集
- キャンバス回転
- PNG 出力オプション
- 保存形式 migration
- ペン / タッチ入力最適化

## 開発時の確認コマンド

```bash
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo build
cargo build --target wasm32-unknown-unknown
trunk build --release
```

## GitHub Pages 上の制約

- Pages は静的配信なので、保存はブラウザダウンロードになります
- web 版の `Load` はユーザーが毎回ファイルを選択する形です
- サーバー保存、認証、DB は使っていません

## Trunk の色設定エラーを避けるメモ

- 環境変数 `NO_COLOR=1` が入っているシェルでは、`trunk 0.21.14` が `invalid value '1' for '--no-color'` で失敗することがあります
- その場合は次のように上書きしてください

```bash
NO_COLOR=false trunk serve
NO_COLOR=false trunk build --release
```
