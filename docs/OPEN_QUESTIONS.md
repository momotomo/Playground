# OPEN_QUESTIONS

## バケツ塗りの許容差

- 現状
  - `塗りのゆるさ` は 5 段階
  - visible な見た目を境界判定に使い、結果は作業レイヤーへ `FillElement` で保存
- 候補
  - 数値ベースの内部設定
  - anti-alias への追加対応
- 未決理由
  - UI を複雑にしすぎず、塗りすぎも防ぎたい

## freehand SVG の品質

- 現状
  - 軽い簡略化と smoothing で cubic path 化している
  - ブラシ質感は SVG で簡略化している
- 候補
  - 点列の追加整理
  - freehand path の品質改善
  - brush kind 差の少し強い反映
- 未決理由
  - wasm で重くしすぎず、PNG と役割を分けたい

## ブラシ質感をどこまで増やすか

- 現状
  - deterministic な multi-pass で軽い差があり、`ペン / えんぴつ / マーカー` の width / alpha / pass 差を強めている
- 候補
  - 追加ブラシ種別
  - さらに質感差を増やす
- 未決理由
  - 重いブラシエンジン化はまだ避けたい

## 複数要素の一括編集拡張

- 現状
  - 図形の一括 style 編集はある
  - freehand や fill は無理に巻き込んでいない
- 候補
  - stroke の一括色変更
  - fill 要素の扱い整理
  - mixed selection 向けの最小 UI
- 未決理由
  - 対象が増えると UI と undo の複雑さが上がる

## export の役割分担

- 現状
  - PNG 系は見たまま共有向け
  - SVG は図形や線の再利用向け
- 候補
  - SVG 側の freehand / fill 改善
  - export ごとの短い補助文の見直し
- 未決理由
  - 役割を混線させず、説明も増やしすぎたくない
