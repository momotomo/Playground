# ARCHITECTURE

## なぜ `egui + eframe` にしたか

- Rust だけで UI を構築でき、フロントエンド言語や別ランタイムを増やさずに済むため
- `egui` は即時モード UI なので、ツールパネルや状態表示を MVP で素早く組み替えやすいため
- `eframe` は `egui` の公式フレームワークで、native と wasm の両方を同じアプリ本体から起動しやすいため
- `eframe_template` 系が示す構成を参考にしつつ、今回の用途に必要な最小構成へ整理しやすいため

## web/native 両対応方針

- アプリ本体は [src/app.rs](../src/app.rs) に集約
- 描画面と入力処理は [src/canvas.rs](../src/canvas.rs) に分離
- native 起動は [src/native.rs](../src/native.rs)
- wasm 起動は [src/web.rs](../src/web.rs)
- [src/main.rs](../src/main.rs) はターゲットごとに適切なエントリを呼び出すだけにして薄く保つ

## 主要モジュール責務

- [src/app.rs](../src/app.rs)
  - 画面レイアウト
  - ツール選択
  - 基本操作ボタン
  - ステータスメッセージ管理
- [src/canvas.rs](../src/canvas.rs)
  - キャンバス描画
  - ポインタ入力の解釈
  - 一時的な描画中ストローク管理
  - Undo / Clear の補助
- [src/model.rs](../src/model.rs)
  - 作品データ
  - ストローク
  - 色
  - 点列
  - `DocumentHistory` による `Undo` / `Redo` 状態
  - 将来の保存に耐える `serde` 対応データ
- [src/storage.rs](../src/storage.rs)
  - 保存形式の encode / decode
  - native の path / dialog ベース保存
  - web の download / upload ベース保存
  - 将来の PNG 出力の着地点

## 保存形式の責務

- サーバー保存、DB、認証は入れない
- 編集用保存形式は JSON ベースの独自 envelope
- `storage` が `PaintDocument` と保存形式の相互変換を担当する
- envelope には `format.id` と `format.version` を持たせ、将来の migration を見据える
- `document` 直下にキャンバスサイズ、背景、ストローク列を保持する
- 将来のメタ情報追加は `metadata` 側に寄せる
- PNG は今回未実装だが、`storage` に export 系 API を追加する想定

## native / web の保存方式の違い

- native
  - `rfd::FileDialog` により OS ダイアログで保存先 / 読込元を選ぶ
  - 実ファイルは `storage` の path ベース関数で読み書きする
- web
  - `rfd::AsyncFileDialog` によりブラウザのダウンロード / ファイル選択を使う
  - ファイルハンドルの継続保持はせず、その場で read / write を完結させる
  - Pages 上でも同じ制約で動くため、native と完全同一の UX にはしない

## Undo / Redo の状態管理方針

- `DocumentHistory` が `current`, `undo_stack`, `redo_stack` を保持する
- 新しいストロークのコミット、`Clear`, `Load` はすべて history に対する変更として扱う
- `Undo` 時は `current` を `redo_stack` へ積み、`undo_stack` から復元する
- `Redo` 時はその逆を行う
- `Undo` 後に新規描画や `Clear` / `Load` を行った場合、`redo_stack` は破棄する
- 描画中ストロークは `canvas` が一時管理し、確定した瞬間だけ `history` に反映する

## 将来の拡張方針

- `core` 切り出し
  - 現在の `model` / `storage` を元に、描画データとコマンド管理を将来別 crate 化しやすいよう依存を薄くしている
- レイヤー
  - `PaintDocument` に layer 配列を導入し、ストローク所属を持たせる
- 図形
  - freehand stroke と別に `ShapeCommand` を追加
- 出力
  - PNG ラスタライズ
  - 将来的には SVG 出力も候補
- 保存形式
  - 編集用の独自保存形式の version 増分
  - バージョニング対応
  - 後方互換 migration
- PNG / SVG 出力
  - まず `storage` に raster export API を追加し、次に UI へ公開する
- 操作性
  - ショートカット
  - ズーム / パン
  - タッチ / スタイラス最適化
