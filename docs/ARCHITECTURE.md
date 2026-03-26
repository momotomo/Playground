# ARCHITECTURE

## なぜ `egui + eframe` にしたか

- Rust だけで UI を構築でき、別のフロントエンド言語やランタイムを増やさずに済むため
- `egui` は即時モード UI なので、ツールバー、状態表示、キャンバス操作を段階的に育てやすいため
- `eframe` は `egui` の公式フレームワークで、native と wasm の両方を同じアプリ本体から起動しやすいため

## web/native 両対応方針

- アプリ本体は `src/app.rs`
- キャンバス描画、ビュー状態、選択操作は `src/canvas.rs`
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
  - `canvas` と `history` の橋渡し
- `src/canvas.rs`
  - キャンバス表示
  - ポインタ入力
  - ズーム / パン
  - 画面座標と作品座標の相互変換
  - 選択状態
  - 描画中プレビュー
  - 移動プレビュー
- `src/model.rs`
  - `PaintDocument`
  - `PaintElement`
  - `Stroke`
  - `ShapeElement`
  - 色、点列、キャンバスサイズ
  - バウンディング / ヒットテスト
  - `DocumentHistory` による `Undo` / `Redo`
- `src/render.rs`
  - `PaintDocument` から PNG 用ピクセルデータを生成
  - 表示倍率に依存しない作品基準のラスタライズ
- `src/storage.rs`
  - JSON encode / decode
  - 保存形式 version 管理
  - 旧 format v1 の読込互換
  - native / web の保存導線差分吸収
  - PNG export のバイト列生成と保存

## 描画要素モデルの構造

- 作品の中身は `PaintDocument.elements: Vec<PaintElement>` で保持する
- `PaintElement` は次の enum
  - `Stroke`
  - `Shape`
- `Shape` は次の `kind` を持つ
  - `rectangle`
  - `ellipse`
  - `line`
- これにより、freehand stroke と図形を同じ履歴、保存、レンダリング経路で扱える
- 将来レイヤーを入れる場合も、layer ごとに `PaintElement` を持つ形へ拡張しやすい

## 作品状態 / ビュー状態 / 選択状態の分離

- 作品状態は `PaintDocument`
  - `canvas_size`
  - `background`
  - `elements`
- ビュー状態は `CanvasController` 内の `CanvasViewState`
  - `zoom`
  - `pan`
  - `viewport`
  - `needs_reset`
- 選択状態も `CanvasController` 側で保持する
  - `selected_index`
  - ドラッグ開始点
  - 移動プレビュー差分
- `Undo` / `Redo` は作品状態だけに作用させる
- 選択状態とビュー状態は履歴に積まない

## 座標変換の考え方

- ストロークや図形は常に作品座標で保持する
- 画面表示時だけ `zoom` と `pan` を用いて作品座標から画面座標へ変換する
- 入力時は逆変換して画面座標を作品座標へ戻す
- 移動プレビューも作品座標差分で持つため、ズーム倍率に依存しない
- PNG 出力では画面表示の transform を使わず、作品座標を直接ラスタライズする

## ヒットテストとバウンディングの考え方

- 各 `PaintElement` が最低限の `bounds()` と `hit_test()` を持つ
- stroke
  - 線分列への距離判定
- rectangle
  - 外枠付近のみをヒット対象にする
- ellipse
  - 楕円の外周付近のみをヒット対象にする
- line
  - 線分距離で判定する
- 選択表示は `bounds()` を元にした UI 専用オーバーレイで、保存や PNG 出力には含めない

## 移動操作と履歴管理

- `Select` ツールで選択した要素をドラッグすると、まずはキャンバス上でプレビュー表示する
- ドロップ時にだけ `DocumentHistory::translate_element` を呼び、1 回の移動を 1 編集として履歴へ積む
- これにより、ドラッグ中の細かいポインタ更新で履歴が汚れない
- 選択状態自体は履歴に含めない

## Undo / Redo とビュー操作の関係

- `DocumentHistory` が `current`, `undo_stack`, `redo_stack` を保持する
- 新規 stroke、図形作成、移動、`Clear`、`Load` は編集履歴に入る
- `Undo` 後に新規作成、移動、`Clear`、`Load` を行った場合、`redo_stack` は破棄する
- ズーム / パン / Reset View は view state の変更として扱い、編集履歴には影響させない

## 保存形式の責務

- JSON 保存は「再編集用」
- `storage` が JSON envelope の version 管理と encode / decode を担当する
- envelope には `format.id` と `format.version` を持たせる
- 現在の保存は `version = 2`
- `version = 1` の stroke-only 形式は decode 側で `PaintElement::Stroke` へ変換して読む
- `metadata` は将来のタイトル、作成時刻、タグなどの追加先として残す

## PNG 出力の責務

- PNG は「共有 / 閲覧用」
- `render` が作品データからピクセルデータを生成する
- `storage` が PNG バイト列化と native / web 保存導線を担当する
- 選択枠やズーム倍率など UI 専用情報は出力に含めない
- 将来 JPEG やサムネイル出力を追加するときも `app` を肥大化させずに済む

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
  - `PaintDocument` に layer 配列を導入し、各 layer が `PaintElement` 配列を持つ形へ拡張
- 複数選択
  - `selected_index` を集合へ拡張し、移動や整列をまとめて扱う
- 図形編集
  - リサイズハンドル、塗り、角丸矩形、線端設定などを `ShapeElement` に追加
- 選択 / 変形
  - 回転、拡縮、複製などを transform レイヤとして導入
- PNG / SVG 出力拡張
  - `render` を拡張してサムネイルやベクター出力経路を増やす
- 保存形式 migration
  - `format.version` ごとに decode 側で migration を入れる
