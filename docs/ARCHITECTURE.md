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
  - 将来の保存に耐える `serde` 対応データ
- [src/storage.rs](../src/storage.rs)
  - 保存と読み込みの入口
  - MVP では未実装 stub
  - 将来のローカルファイル保存や PNG 出力の着地点を示す

## 保存方針

- サーバー保存、DB、認証は入れない
- 当面の編集用保存形式はローカルファイル前提
- MVP の内部モデルは `serde` でシリアライズ可能
- 実保存実装時の候補:
  - 編集用: `.paint.json` または `.paint.ron`
  - 配布/共有用: PNG
- 現時点では UI 上の `Save` / `Load` はプレースホルダーに留め、設計だけ先に固定している

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
  - 編集用の独自保存形式
  - バージョニング対応
  - 後方互換 migration
- 操作性
  - ショートカット
  - ズーム / パン
  - タッチ / スタイラス最適化
