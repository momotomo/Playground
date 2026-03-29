# WORKING_RULES

## 実装方針

- 最小変更を優先する
- 既存の save/load/export/render/layer/selection/fill/touch を壊さない
- 保存形式 version は可能なら `4` を維持する
- native / web / wasm / GitHub Pages を壊さない
- 日本語 UI は短く自然な表現を使う
- 初心者向け UX を後退させない
- 常設説明を増やしすぎない
- UI 補助表示は PNG / 透過PNG / SVG に含めない

## 事実ベース

- コードを読んで確認できる内容を書く
- 推測は断定しない
- 実装されていないことを docs に書かない

## 変更時の優先順位

1. 既存整合を守る
2. 小さく安全に直す
3. 小画面 / タブレットでも扱いやすいかを見る
4. 必要ならテストを足す

## Git ルール

- `master` 最新から作業ブランチを切る
- 最後は `master` へ `--no-ff` でマージする
- push まで行う
- fast-forward merge は使わない

## 最終報告

- 日本語で簡潔にまとめる
- 基本項目
  - 実施概要
  - 変更した主要ファイル
  - 実装内容 / 仕様
  - 確認結果
  - `--no-ff` マージ・push 結果
  - 注意点
