# MVP_SCOPE

## 今回追加した範囲

- 最小レイヤー実装
- レイヤー追加 / 削除 / 名前変更 / 表示切替 / ロック / 並び替え
- active layer の切り替え
- active layer への新規要素追加
- visible / locked 状態を反映した選択 / 描画 / PNG 出力
- 選択要素のレイヤー間移動
- 選択要素のレイヤー間複製
- active layer 強調、状態表示、要素数表示を含むレイヤー UI 改善
- レイヤー順と同一レイヤー内の重なり順の分離
- `format.version = 4` の JSON 保存
- `v3 / v2 / v1` の読込互換を維持した migration
- レイヤー変更を含む Undo / Redo
- layer round-trip、hidden / locked、layer add/delete、layer transfer のテスト

## 今回あえて入れなかった範囲

- layer opacity / blend mode
- レイヤー間ドラッグ移動
- 複数レイヤー横断の同時選択 / 一括編集
- 単一選択ストロークの専用リサイズ / 回転ハンドル
- 塗り
- 角丸矩形
- スナップ、グリッド
- group 内だけを直接選ぶ isolate 編集
- 出力解像度指定
- JPEG / SVG 出力
- サーバー連携、DB、認証

## 次フェーズ候補

- layer opacity / blend mode
- レイヤー間ドラッグ移動
- 複数レイヤー横断の選択 / 整列
- group 内部編集とネスト可視化
- 単一選択ストロークの専用変形 UI
- 塗り、角丸矩形、矢印付き線
- スナップ、ガイド
- タッチ / ペン入力調整
- 保存形式 migration の強化
