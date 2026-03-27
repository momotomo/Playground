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
  - 編集中セッション
  - ハンドル判定と preview 表示
- `src/model.rs`
  - `PaintDocument`
  - `PaintElement`
  - `Stroke`
  - `ShapeElement`
  - 色、点列、キャンバスサイズ
  - バウンディング / ヒットテスト
  - 図形のリサイズ / 回転ロジック
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

## 図形データモデルの拡張

- 作品の中身は `PaintDocument.elements: Vec<PaintElement>` で保持する
- `PaintElement` は次の enum
  - `Stroke`
  - `Shape`
- `ShapeElement` は次の情報を保持する
  - `kind`
  - `color`
  - `width`
  - `start`
  - `end`
  - `rotation_radians`
- 矩形 / 楕円
  - `start` と `end` は未回転 bbox の対角点
  - `rotation_radians` を中心回りに適用する
- 直線
  - `start` と `end` を endpoint として扱う
  - 回転は endpoint を中心回りに回した結果で表現する

## 作品状態 / ビュー状態 / 選択状態 / 編集中一時状態の分離

- 作品状態は `PaintDocument`
  - `canvas_size`
  - `background`
  - `elements`
- ビュー状態は `CanvasController` 内の `CanvasViewState`
  - `zoom`
  - `pan`
  - `viewport`
  - `needs_reset`
- 選択状態は `SelectionState`
  - `selected_index`
- 編集中一時状態は `SelectionSession`
  - `Move`
  - `Resize`
  - `Rotate`
- `SelectionSession` は preview 用だけに使い、確定時にだけ履歴へ流す

## リサイズ / 回転操作の設計

- 角ハンドルと回転ハンドルは UI 専用オーバーレイとして `canvas` で描く
- 判定優先順位は次の通り
  - ハンドル
  - 選択済み / ヒットした要素本体
  - 背景
- 矩形 / 楕円
  - 角ハンドルは回転を維持したままサイズ変更する
  - 回転ハンドルは図形中心を回転中心にする
- 直線
  - endpoint ハンドルで長さと向きを編集する
  - 回転ハンドルは線分中心を回転中心にする
- ストローク
  - このフェーズでは移動のみ対応し、リサイズ / 回転は未対応

## バウンディングとヒットテストの考え方

- 各 `PaintElement` が最低限の `bounds()` と `hit_test()` を持つ
- stroke
  - 線分列への距離判定
- rectangle / ellipse
  - 点をローカル座標へ戻してから外周判定する
  - bounds は回転後の四隅から軸平行 bbox を計算する
- line
  - 線分距離で判定する
- 選択枠、ハンドル、回転リンクは UI 表示専用で、保存や PNG 出力には含めない

## 履歴コミットの考え方

- 新規 stroke / 図形作成は `commit_element`
- Move / Resize / Rotate は `replace_element`
- preview 中は `SelectionSession` の中だけで状態を持つ
- リリース時にだけ 1 回の編集として履歴へ積む
- 選択状態やビュー状態は履歴に積まない

## Undo / Redo とビュー操作の関係

- `DocumentHistory` が `current`, `undo_stack`, `redo_stack` を保持する
- 新規作成、移動、リサイズ、回転、`Clear`、`Load` は編集履歴に入る
- `Undo` 後に新規編集を行った場合、`redo_stack` は破棄する
- ズーム / パン / Reset View は view state の変更として扱い、編集履歴には影響させない

## 保存形式の責務

- JSON 保存は「再編集用」
- `storage` が JSON envelope の version 管理と encode / decode を担当する
- 現在の保存は `format.version = 2` を維持する
- 旧 `version = 1` の stroke-only 形式は decode 側で `PaintElement::Stroke` へ変換して読む
- 旧 `version = 2` の shape JSON に `rotation_radians` が無い場合は `0` 扱いで読める

## PNG 出力の責務

- PNG は「共有 / 閲覧用」
- `render` が作品データからピクセルデータを生成する
- `storage` が PNG バイト列化と native / web 保存導線を担当する
- 回転やリサイズ後の図形も作品データからそのまま描画する
- 選択枠やハンドルは出力に含めない

## 将来の拡張方針

- 複数選択
  - `selected_index` を集合へ拡張し、一括移動や整列を扱う
- レイヤー
  - `PaintDocument` に layer 配列を導入し、各 layer が `PaintElement` 配列を持つ形へ拡張
- ストローク変形
  - bbox ベースの簡易スケールや回転を追加
- 図形編集強化
  - 塗り、角丸矩形、矢印、スナップ、整列などを追加
- 保存形式 migration
  - 将来の大きな形状拡張時に `format.version` を上げて decode 側で migration を入れる
