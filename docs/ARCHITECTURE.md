# ARCHITECTURE

## なぜ `egui + eframe` にしたか

- Rust だけで UI を構築でき、別のフロントエンド言語やランタイムを増やさずに済むため
- `egui` は即時モード UI なので、ツールバー、状態表示、キャンバス操作を段階的に育てやすいため
- `eframe` は `egui` の公式フレームワークで、native と wasm の両方を同じアプリ本体から起動しやすいため

## web/native 両対応方針

- アプリ本体は `src/app.rs`
- キャンバス描画とビュー操作は `src/canvas.rs`
- 作品モデルと編集履歴は `src/model.rs`
- 保存 / 読込 / export は `src/storage.rs`
- PNG ラスタライズは `src/render.rs`
- native 起動は `src/native.rs`
- wasm 起動は `src/web.rs`
- `src/main.rs` はターゲットごとのエントリ呼び出しだけに保つ

## 主要モジュール責務

- `src/app.rs`
  - パネル構成
  - ツール状態
  - ボタン操作
  - ショートカット処理
  - ステータスメッセージ管理
- `src/canvas.rs`
  - キャンバス表示
  - ポインタ入力
  - ズーム / パン
  - 画面座標と作品座標の相互変換
  - 描画中ストロークの一時管理
- `src/model.rs`
  - `PaintDocument`
  - `Stroke`
  - 色、点列、キャンバスサイズ
  - `DocumentHistory` による `Undo` / `Redo`
- `src/render.rs`
  - `PaintDocument` から PNG 用ピクセルデータを生成
  - 表示倍率に依存しない作品基準のラスタライズ
- `src/storage.rs`
  - JSON encode / decode
  - native / web の保存導線差分吸収
  - PNG export のバイト列生成と保存

## 作品状態とビュー状態の分離

- 作品状態は `PaintDocument`
  - `canvas_size`
  - `background`
  - `strokes`
- ビュー状態は `CanvasController` 内の `CanvasViewState`
  - `zoom`
  - `pan`
  - `viewport`
  - `needs_reset`
- `Undo` / `Redo` は作品状態だけに作用させる
- ズーム / パンは履歴に積まない
- これにより保存形式、PNG 出力、将来のレイヤーや選択機能に対して、表示都合の情報を混ぜずに済む

## 座標変換の考え方

- ストロークは常に作品座標で保持する
- 画面表示時だけ `zoom` と `pan` を用いて作品座標から画面座標へ変換する
- 入力時は逆変換して画面座標を作品座標へ戻す
- PNG 出力では画面表示の transform を使わず、作品座標を直接ラスタライズする

## Undo / Redo とビュー操作の関係

- `DocumentHistory` が `current`, `undo_stack`, `redo_stack` を保持する
- 新規ストローク、`Clear`、`Load` は編集履歴に入る
- `Undo` 後に新規描画や `Clear` / `Load` を行った場合、`redo_stack` は破棄する
- ズーム / パン / Reset View は view state の変更として扱い、編集履歴には影響させない

## 保存形式の責務

- JSON 保存は「再編集用」
- `storage` が JSON envelope の version 管理と encode / decode を担当する
- envelope には `format.id` と `format.version` を持たせ、将来の migration を見据える
- `metadata` は将来のタイトル、作成時刻、タグなどの追加先として残す

## PNG 出力の責務

- PNG は「共有 / 閲覧用」
- `render` が作品データからピクセルデータを生成する
- `storage` が PNG バイト列化と native / web 保存導線を担当する
- これにより、将来 JPEG やサムネイル出力を追加するときも `app` を肥大化させずに済む

## native / web の保存方式の違い

- native
  - `rfd::FileDialog` による OS ダイアログ
  - JSON も PNG も実ファイルとして保存
- web
  - `rfd::AsyncFileDialog` によるブラウザダウンロード / ファイル選択
  - JSON と PNG はその都度ダウンロード
  - GitHub Pages 上でも同じブラウザ制約で動作

## 将来の拡張方針

- レイヤー
  - `PaintDocument` に layer 配列を導入し、stroke の所属を分離
- 図形ツール
  - freehand stroke と別に shape command を追加
- 選択 / 移動
  - 作品座標ベースの選択範囲と transform を導入
- PNG / SVG 出力拡張
  - `render` を拡張してサムネイルやベクター出力経路を増やす
- 保存形式 version migration
  - `format.version` ごとに decode 側で migration を入れる
