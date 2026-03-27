# Rust Paint Foundation

`egui + eframe` だけで構成した、Rust オンリーのお絵かきツール基盤です。  
同じコードベースから native 実行と WebAssembly 実行を扱い、GitHub Pages へ静的配信できます。  
このフェーズでは、再編集用 JSON 保存、`PNG 出力`、`ズーム / パン`、`図形再編集`、`ドラッグ矩形選択`、`複数選択の一括リサイズ / 回転`、`Group / Ungroup`、`整列 / 等間隔配置`、`重なり順操作`、`最小レイヤー機能`、`グリッド / ガイド / スナップ` に加えて、`スマートガイド` と `ルーラー UI` を追加しています。

## プロジェクト概要

- Rust だけで UI とアプリケーション本体を構築
- `egui` による即時モード UI
- `eframe` による native / web 共通アプリ基盤
- フリーハンド描画、消しゴム、`Undo`、`Redo`、`Clear`
- 編集用 JSON 形式での `Save` / `Load`
- 共有用 `Export PNG`
- キャンバスのズーム、パン、表示リセット
- 単一選択と複数選択
- 単一選択での移動、リサイズ、回転
- 複数選択での一括移動、グループリサイズ、グループ回転、Group / Ungroup、整列、等間隔配置、重なり順変更
- 矩形、楕円、直線ツール
- レイヤーの追加 / 削除 / 名前変更 / 表示切替 / ロック / 並び替え
- グリッド表示、ガイド表示、グリッド間隔調整、グリッド / ガイドスナップ
- スマートガイド表示とルーラー UI
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

## 最初の使い方

1. `Brush`、`Rectangle`、`Ellipse`、`Line` のどれかを選び、canvas をドラッグして 1 つ描きます。
2. `Select` に切り替えてクリックすると、移動や再編集ができます。
3. `Shift + Click` または空き領域ドラッグで複数選択できます。
4. `Space + Drag` または中ボタンドラッグでパン、`Ctrl/Cmd + Wheel` か `+ / -` でズームします。
5. 編集を続けるなら `Save JSON`、共有するなら `Export PNG` を使います。
6. 操作に迷ったら上部の `Help` を開くと、最小ヘルプとショートカットを確認できます。

## 操作方法

### ツール

- `Select`
  - 要素をクリックして選択します
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

### 選択 / 再編集

- `Select` ツールでストロークと図形のどちらも選択できます
- 最小レイヤー実装では、選択と描画の対象は `active layer` のみです
- `visible = false` のレイヤーは描画、選択、PNG 出力に含まれません
- `locked = true` のレイヤーは表示されますが、選択や編集の対象になりません
- 選択中要素は `Move Here` / `Duplicate Here` で別レイヤーへ送れます
- 移動先 / 複製先には `visible = true` かつ `locked = false` のレイヤーだけを使えます
- レイヤー間移動 / 複製の完了後は、移動先レイヤーが active になり、新しい要素群の選択を維持します
- `Shift + Click` で選択に追加 / 解除できます
- 空き領域をドラッグすると矩形選択できます
- `Shift` を押したまま矩形選択すると、既存選択へ追加できます
- 単一選択中の図形にはハイライト付きアウトライン、角ハンドル、回転ハンドルを表示します
- 複数選択中は各要素のハイライト、グループバウンディングボックス、グループ用リサイズ / 回転ハンドルを表示します
- 単一選択の図形は次の再編集に対応します
  - 移動
  - リサイズ
  - 回転
- 複数選択は次の編集に対応します
  - 一括移動
  - 一括リサイズ
  - 一括回転
  - Group
  - Horizontal / Vertical Distribute
  - 左 / 横中央 / 右 / 上 / 縦中央 / 下揃え
  - 前面 / 背面 / 前へ / 後ろへ の重なり順変更
- `Group` は複数選択を 1 つの top-level 要素にまとめます
- group 化した要素は 1 つのまとまりとして選択、移動、リサイズ、回転、整列、重なり順変更できます
- `Ungroup` は選択中の group を 1 階層だけ展開します
- group の内部要素順は保持され、描画順と保存順にもそのまま反映されます
- 複数選択のリサイズはグループ bbox を基準に、要素同士の相対位置を保ちながら行います
- 複数選択の回転はグループ中心回りで行います
- ストロークもこのフェーズでは簡易的な一括スケール / 回転対象に含めています
- 角ハンドルをドラッグするとリサイズします
- 回転ハンドルをドラッグすると中心回りに回転します
- ドラッグ中はプレビューし、リリース時に履歴へコミットします
- `Esc` で進行中の編集プレビューをキャンセルできます
- 選択状態そのものは `Undo / Redo` に含めません

### 整列 / 重なり順

- `Align` メニューは複数選択時のみ有効です
- 整列基準は選択要素全体の bounding box です
- 整列では rotation は保ち、位置だけを調整します
- `Distribute` メニューは 3 要素以上の複数選択時のみ有効です
- `Distribute Horizontally` / `Distribute Vertically` は、先頭と末尾の要素を基準に中間要素の間隔を均等化します
- `Order` メニューは単一選択でも複数選択でも使えます
- 複数選択の重なり順変更では、選択要素どうしの相対順を保ったまま前後へ移動します

### 編集

- `Undo`: 直前の編集を戻します
- `Redo`: `Undo` した編集を戻します
- `Clear`: 作品全体を消去します
- `Save JSON`: 再編集用 JSON を保存します
- `Open JSON`: JSON から再編集状態を復元します
- `Export PNG`: 背景と全要素を含む共有用 PNG を書き出します

### レイヤー

- 右側の `Layers` パネルでレイヤーを管理します
- `Add Layer` で新規レイヤーを追加し、そのレイヤーを active にします
- `Delete Layer` は active layer を削除します
- 最低 1 レイヤーは必ず残ります
- レイヤー名は `Rename Layer` で変更できます
- `Show` / `Hide` で表示切替、`Lock` / `Unlock` で編集可否を切り替えます
- `Up` / `Down` でレイヤー順を並び替えます
- active layer は強調表示され、`ACTIVE` / `HIDDEN` / `LOCKED` と要素数をレイヤーカード上に表示します
- 選択があると、他の編集可能レイヤーに `Move Here` / `Duplicate Here` を表示します
- レイヤー間移動と複製は、移動先レイヤーの末尾へ追加する単純なルールです
- 要素の重なり順は「同一レイヤー内」で維持され、レイヤー順はその外側の描画順として効きます

### グリッド / ガイド / スナップ / ルーラー

- 左パネルの `Layout Aids` から表示とスナップを切り替えます
- `Show Rulers` で canvas に追従する上端 / 左端ルーラーを表示します
- `Smart Guides` を有効にすると、要素移動中に他要素の左 / 中央 / 右、上 / 中央 / 下へ揃う候補線を表示し、近ければ吸着します
- `Show Grid` はキャンバスの補助グリッド表示です
- `Snap to Grid` は移動、図形作成、単一 / 複数リサイズ時にグリッドへ吸着します
- `-` / `+` と spacing presets でグリッド間隔を調整できます
- `Show Guides` は水平 / 垂直ガイドの表示切替です
- `Snap to Guides` は表示とは独立した吸着設定です
- `Add H Guide` / `Add V Guide` は、選択中なら選択 bbox の中心、未選択ならキャンバス中央にガイドを追加します
- visible なガイドは canvas 上でドラッグして位置を変更できます
- ガイド付近にホバーすると線が少し強調され、ドラッグ可能だと分かるようにしています
- ガイド一覧の `Remove` で削除できます
- スナップ対象は bbox の辺 / 中心とドラッグ中ハンドルを基本にしています
- ルーラー、グリッド、ガイド、スマートガイドは補助表示で、PNG には含めません

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
- `Ctrl/Cmd + G`: Group
- `Ctrl/Cmd + Shift + G`: Ungroup
- `Ctrl/Cmd + +` または `Ctrl/Cmd + =`: Zoom in
- `Ctrl/Cmd + -`: Zoom out
- `Ctrl/Cmd + 0`: Reset View
- `Esc`: 進行中の編集プレビューをキャンセル
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
- 現在の format version は `4` です
- 旧 `version = 1` の stroke-only JSON も読込互換を残しています
- 旧 `version = 2` と `version = 3` の flat な stroke / shape / group JSON も読込互換を残しています
- `document.layers[]` にレイヤーを保存します
- 各レイヤーは次の情報を持ちます
  - `id`
  - `name`
  - `visible`
  - `locked`
  - `elements[]`
- group は `elements[]` を再帰的に保持します
- shape では次の情報を保持します
  - `kind`
  - `color`
  - `width`
  - `start`
  - `end`
  - `rotation_radians`
- 要素の並び順は各レイヤー内の `elements[]` 順として保持され、重なり順としてそのまま復元されます
- レイヤーの並び順も保存され、描画順としてそのまま復元されます
- レイヤー間移動 / 複製の結果も、最終的な各レイヤーの `elements[]` 構成としてそのまま保存されます
- グリッド表示、ガイド表示、グリッド間隔、スマートガイド表示、ルーラー表示、スナップ設定、ガイド位置も同じ `document` 内に保存されます
- 旧 shape JSON に `rotation_radians` がない場合は `0` として読み込みます

### PNG 出力

- 用途は「共有 / 閲覧用」
- 既定ファイル名は `untitled.png`
- 表示中のズーム倍率や選択枠、ハンドルは含めません
- 作品のキャンバスサイズを基準に、背景色と全要素をラスタライズします
- `visible = true` のレイヤーだけを出力します
- `locked` は表示だけに影響せず、可視なら出力されます
- グリッド、ガイド、スナップ用の補助表示は出力に含めません
- 回転やリサイズ後の図形も、そのまま出力へ反映されます
- 複数選択による整列結果、等間隔配置、グループ変形、重なり順変更、group 化結果も、そのまま出力へ反映されます

## 対応している要素と未対応要素

対応済み:
- ストロークの移動
- ストロークの複数選択時の簡易スケール / 回転
- 矩形の移動 / リサイズ / 回転
- 楕円の移動 / リサイズ / 回転
- 直線の移動 / endpoint リサイズ / 回転
- 単一選択
- `Shift + Click` による複数選択
- ドラッグ矩形選択
- 複数要素の一括移動
- 複数要素の一括リサイズ / 回転
- Group / Ungroup
- 左 / 横中央 / 右 / 上 / 縦中央 / 下揃え
- 水平方向 / 垂直方向の等間隔配置
- Bring to Front / Send to Back / Bring Forward / Send Backward
- レイヤーの追加 / 削除 / 名前変更 / 表示切替 / ロック / 並び替え
- 選択要素のレイヤー間移動 / 複製
- グリッド / ガイド表示とスナップ

未対応:
- 塗り
- グループ内だけを直接選択する isolate モード
- レイヤー opacity / blend mode
- レイヤー間ドラッグ移動
- スマートガイドのより高度な候補表示

## native / web の違い

- native
  - `Save JSON` / `Open JSON` は OS のファイルダイアログを使います
  - `Export PNG` は OS ダイアログから `.png` を保存します
- web
  - `Save JSON` はブラウザダウンロードとして JSON を保存します
  - `Open JSON` はブラウザのファイル選択を使います
  - `Export PNG` はブラウザダウンロードとして保存します
  - GitHub Pages 上ではブラウザ制約のため、native のような継続的ファイルハンドル保持はしません
- 上部の `Help` と空状態の案内は native / web の両方で同じです
- 選択 / 矩形選択 / Group / Ungroup / 一括リサイズ / 一括回転 / 整列 / 等間隔配置 / 重なり順変更 / 図形再編集 / レイヤー操作 / グリッド / ガイド / ズーム / パンの基本操作は native / web で同じです

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

## ドキュメント

- `README.md`
- `docs/ARCHITECTURE.md`
- `docs/MVP_SCOPE.md`

## 今後の拡張候補

- group 内だけを直接編集する isolate モード
- 塗り、角丸矩形、矢印付き線
- レイヤー opacity / blend mode
- レイヤー間ドラッグ移動
- スマートガイドの高度化 / ruler UI の強化
- ストロークの専用変形ハンドル
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
