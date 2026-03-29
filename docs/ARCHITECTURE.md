# ARCHITECTURE

## なぜ `egui + eframe` にしたか

- Rust だけで UI を構築でき、別のフロントエンド言語やランタイムを増やさずに済むため
- `egui` は即時モード UI なので、ツールバー、状態表示、キャンバス操作を段階的に育てやすいため
- `eframe` は `egui` の公式フレームワークで、native と wasm の両方を同じアプリ本体から起動しやすいため

## web/native 両対応方針

- アプリ本体は `src/app.rs`
- キャンバス描画、ビュー状態、選択操作は `src/canvas.rs`
- 作品モデルと編集履歴は `src/model.rs`
- バケツ塗りの領域抽出は `src/fill.rs`
- 保存 / 読込 / export は `src/storage.rs`
- PNG ラスタライズは `src/render.rs`
- native 起動は `src/native.rs`
- wasm 起動は `src/web.rs`

## 主要モジュール責務

- `src/app.rs`
  - パネル構成
  - ツール状態
  - 線色 / 塗り色 / 不透明度の保持
  - 単一選択図形に対する線 / 塗り / 太さの直接編集
  - 最近使った色と簡易パレット
  - 現在ツール / 現在レイヤーが分かる UI summary
  - ボタン操作
  - ショートカット処理
  - ステータスメッセージ管理
  - `canvas` と `history` の橋渡し
- `src/canvas.rs`
  - キャンバス表示
  - ポインタ入力
  - ズーム / パン
  - グリッド / ガイド描画
  - スマートガイド描画
  - ルーラー描画
  - スナップ計算
  - 画面座標と作品座標の相互変換
  - 単一 / 複数選択状態
  - 編集中セッション
  - ハンドル判定と preview 表示
- `src/model.rs`
  - `PaintDocument`
  - `PaintLayer`
  - `GridSettings`
  - `GuideSettings`
  - `PaintElement`
  - `Stroke`
  - `ShapeElement`
  - `FillElement`
  - `GroupElement`
  - 色、点列、キャンバスサイズ
  - ブラシ種別ごとの基本スタイル係数
  - バウンディング / ヒットテスト
  - 図形のリサイズ / 回転ロジック
  - group 化 / group 解除 / 等間隔配置
  - `DocumentHistory` による `Undo` / `Redo`
- `src/render.rs`
  - `PaintDocument` から PNG 用ピクセルデータを生成
  - 表示倍率に依存しない作品基準のラスタライズ
  - 通常 PNG と透過 PNG の背景モード切り替え
  - 図形中心の SVG バイト列生成
  - freehand を軽く間引きつつ、滑らかめの SVG path へ簡略化
  - スポイト用のキャンバス色サンプリング
  - バケツ塗り結果の raster 描画と SVG 簡略出力
  - freehand stroke の tool 種別ごとの見た目差の反映
  - shape の vector path / style 分離による SVG / 将来の fill 拡張の土台
- `src/fill.rs`
  - visible layer を合成した見た目を基準にした flood fill
  - `塗りのゆるさ` に応じた色比較
  - scanline span による領域抽出
  - 失敗時の短いヒント生成
  - `FillElement` 生成
- `src/storage.rs`
  - JSON encode / decode
  - 保存形式 version 管理
  - 旧 format v1 の読込互換
  - native / web の保存導線差分吸収
  - PNG / 透過PNG / SVG export のバイト列生成と保存

## 図形データモデルの拡張

- 作品の中身は `PaintDocument.layers: Vec<PaintLayer>` で保持する
- `PaintLayer` は次の情報を持つ
  - `id`
  - `name`
  - `visible`
  - `locked`
  - `elements: Vec<PaintElement>`
- `PaintElement` は次の enum
  - `Stroke`
  - `Shape`
  - `Fill`
  - `Group`
- `Stroke`
  - `tool` は `pen / pencil / marker / eraser` を表せる
  - tool ごとに最小限の幅係数 / alpha 係数を持ち、重いブラシエンジンなしで描き味の差を出す
  - pencil / marker は deterministic な multi-pass 描画で、少しラフさや重ね感を出す
  - SVG export では freehand path として安全に簡略化し、質感差は PNG より控えめに扱う
- `GroupElement`
  - `elements: Vec<PaintElement>` を持つ
  - 子要素を再帰的に保持し、内部順序もそのまま描画順として扱う
- `FillElement`
  - `color`
  - `origin`
  - `spans`
  - バケツ塗り結果を scanline span で持つ第一版のラスタ塗り要素
  - PNG / 透過PNG では見たまま描画し、SVG では 1px 高の矩形列へ簡略化して出力する
  - `塗りのゆるさ` 自体は作品要素ではなく、UI 側の塗り判定設定として持つ
- `ShapeElement` は次の情報を保持する
  - `kind`
  - `color` (`線色`)
  - `fill_color`
  - `width`
  - `start`
  - `end`
  - `rotation_radians`
- 矩形 / 楕円
  - `start` と `end` は未回転 bbox の対角点
  - `rotation_radians` を中心回りに適用する
  - `fill_color` があれば線とは別に塗りを持てる
  - `paint_mode_label()` で `線だけ / 線と塗り` を UI に短く渡せる
  - SVG export では `effective_fill_color()` をそのまま `fill` として再利用できる
- 直線
  - `start` と `end` を endpoint として扱う
  - 回転は endpoint を中心回りに回した結果で表現する
  - `fill_color` は使わず、常に `None`
- group
  - レイヤー内の top-level 要素として `layer.elements[]` に入る
  - 移動 / スケール / 回転は子要素へ再帰的に適用する
  - PNG render や Save / Load でも同じ構造をそのまま使う

## 作品状態 / ビュー状態 / 選択状態 / 編集中一時状態の分離

- 作品状態は `PaintDocument`
  - `canvas_size`
  - `background`
  - `grid`
  - `guides`
  - `smart_guides`
  - `rulers`
  - `layers`
  - `active_layer_id`
- ビュー状態は `CanvasController` 内の `CanvasViewState`
  - `zoom`
  - `pan`
  - `viewport`
  - `needs_reset`
- 選択状態は `SelectionState`
  - `selected indices`
  - `selected layer id`
- 編集中一時状態は `SelectionSession`
  - `Move`
  - `SingleResize`
  - `SingleRotate`
  - `MultiResize`
  - `MultiRotate`
  - `GuideMove`
  - `Marquee`
- `SelectionSession` は preview 用だけに使い、確定時にだけ履歴へ流す
- 単一選択時は既存のハンドル編集を使い、複数選択時は group bbox ハンドルへ切り替える
- `Group / Ungroup / Distribute` は preview を持たず、完成した `document` を 1 回だけ履歴へ流す

## 選択モデルの拡張内容

- 選択は `active layer + Vec<usize>` ベースで保持し、作品データには保存しない
- 最小レイヤー実装では、選択と描画は active layer のみを対象にする
- active layer が hidden または locked の場合、選択と描画は無効化する
- レイヤー間移動 / 複製も active layer 上の選択を起点にし、完了後は destination layer を active にして選択を付け替える
- 選択の追加 / 解除は `Shift + Click`
- 空き領域ドラッグで矩形選択を行う
- `Shift` 付き矩形選択は既存選択へ加算する
- 単一選択
  - 要素固有の再編集に使う
  - shape なら move / resize / rotate
  - stroke なら group bbox ベースの move / scale / rotate
  - group なら group bbox ベースの move / resize / rotate
- 複数選択
  - 一括移動
  - 一括リサイズ
  - 一括回転
  - group 化
  - 整列
  - 等間隔配置
  - 重なり順変更
- 単一選択では shape は専用ハンドル、stroke / group は group bbox ハンドルを表示する
- 複数選択では group bbox ハンドルを表示する
- group 化した後は top-level では 1 要素扱いに戻るため、既存の単一選択フローを再利用できる

## レイヤーの責務

- レイヤー順は背景側から前景側へ `layers[0] -> layers[last]` で保持する
- 同一レイヤー内の重なり順は既存どおり `elements[]` の配列順で表現する
- layer `visible = false`
  - canvas 描画に含めない
  - hit test / marquee 選択に含めない
  - PNG 出力に含めない
- layer `locked = true`
  - canvas 描画には含める
  - 選択 / 描画 / 編集対象にはしない
- active layer の切替は UI 状態として扱い、Undo/Redo には積まない
- layer add/delete/rename/visible/locked/order は document 変更として Undo/Redo に積む
- active layer の複製は document 変更として扱い、複製後はコピー側を active にする
- 要素のレイヤー間移動 / 複製は visible かつ unlocked な destination layer にだけ許可する
- レイヤー間移動 / 複製は destination layer の末尾へ要素を追加し、同一レイヤー内の既存重なり順ルールを維持する

## グリッド / ガイド / スナップ

- `PaintDocument.grid`
  - `visible`
  - `snap_enabled`
  - `spacing`
- `PaintDocument.guides`
  - `visible`
  - `snap_enabled`
  - `lines`
- `PaintDocument.smart_guides`
  - `visible`
- `PaintDocument.rulers`
  - `visible`
- guide は `GuideLine { axis, position }` で保持する
- グリッドとガイドは作品補助情報として document に保存し、view state とは分離する
- grid spacing は document 側の設定として保持し、UI ではプリセットと step ボタンから変更する
- visible な guide は canvas 上で直接ドラッグ移動できる
- smart guide は move preview 中だけ計算する一時表示で、document には表示設定だけを持つ
- ruler は viewport overlay として描き、zoom / pan に追従する
- スナップ対象は最低限 bbox の `min / center / max` とドラッグ中ハンドルに限定している
- move は選択全体の bounds を使って x/y を独立にスナップする
- smart guide は move 中の selection bounds と他の visible / unlocked 要素 bounds を比較し、edge / center の近い候補だけを拾う
- 図形作成と resize はドラッグ中 pointer をグリッド / ガイドへ寄せて preview を更新する
- guide drag は preview 中だけ `GuideMove` を持ち、リリース時にだけ document へ反映する
- grid / guides / smart guide / ruler の表示は canvas UI 専用で、PNG render には含めない

## PNG / SVG export の役割分担

- PNG / 透過PNG
  - 見たまま共有向け
  - multi-pass のブラシ質感や消しゴム結果も含めてラスタライズする
  - バケツ塗り結果も `FillElement` のまま見た目優先で反映する
  - UI 補助表示は含めない
- SVG
  - 図形 / 線の再利用や拡大向け
  - `直線` / `四角形` / `楕円` は shape geometry から安全に書き出す
  - freehand は点列を軽く整理して滑らかめの path として簡略化し、`ペン / えんぴつ / マーカー` の差は軽い width / alpha の違いとして反映する
  - バケツ塗り結果は 1px 高の矩形列へ簡略化し、見たまま完全一致は PNG 系へ寄せる
  - `消しゴム` は SVG では安全な再現が難しいため省略し、見たまま共有は PNG 系に寄せる
  - raster export と vector export を `src/render.rs` 内で分け、`src/storage.rs` は保存導線だけを担当する

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
  - 単一 / 複数選択の group bbox transform では簡易スケール / 回転対象に含める

## 複数選択変形の考え方

- group bbox は選択要素全体の union bounds から作る
- 一括リサイズ
  - group bbox の対角ハンドルを使う
  - 反対側の角を anchor にして scale 値を求める
  - 各要素は anchor 基準のワールド座標変換で preview する
- 一括回転
  - group center を pivot にする
  - pointer 角度差から回転量を求める
  - 各要素は pivot 回りに回転して preview する
- 単一選択の shape 編集は従来どおり shape 専用ロジックを使う

## バウンディングとヒットテストの考え方

- 各 `PaintElement` が最低限の `bounds()` と `hit_test()` を持つ
- stroke
  - 線分列への距離判定
- rectangle / ellipse
  - 点をローカル座標へ戻してから外周判定する
  - bounds は回転後の四隅から軸平行 bbox を計算する
- line
  - 線分距離で判定する
- 複数選択
  - 各要素の個別 bounds をハイライト表示する
  - 選択全体の union bounds をグループ bbox として表示する
- group
  - 子要素ごとの hit test を逆順でたどる
  - bounds は子要素 bounds の union
- 矩形選択
  - 要素 bounds と marquee rect の交差で選択する
- クリック優先順位は次の通り
  - 単一選択 shape のハンドル
  - 複数選択 group のハンドル
  - 選択済み要素本体
  - 未選択要素本体
  - 背景
- 選択枠、ハンドル、回転リンクは UI 表示専用で、保存や PNG 出力には含めない

## 整列アルゴリズムの考え方

- 整列は複数選択中のみ有効
- 基準は選択要素全体の union bounds
- 各要素は自分自身の bounds を使って位置差分だけ計算する
- 変えるのは位置だけで、rotation や shape の意味は維持する
- 実装上は `PaintDocument` から整列後の新しい `document` を生成し、履歴へ 1 回だけ流す

## 等間隔配置の考え方

- `Distribute Horizontally` / `Distribute Vertically` は 3 要素以上で有効
- 各要素の bounds を軸方向で並べ替え、先頭と末尾は固定する
- 中間要素だけを translation で動かし、gap が均等になるように配置する
- rotation や group 内部構造は壊さず、位置だけを調整する

## 重なり順変更のモデル

- 重なり順は「レイヤー順」と「各レイヤー内の要素順」の 2 段で表現する
- layer order
  - `layers[]` の配列順
- element order
  - `layer.elements[]` の配列順
- `Bring to Front` / `Send to Back`
  - active layer 内で選択要素を配列の末尾 / 先頭へまとまって移動する
- `Bring Forward` / `Send Backward`
  - active layer 内で 1 ステップだけ前後へ移動する
- 複数選択時は、選択要素どうしの相対順を保ったまま移動する
- group の内部順序は `GroupElement.elements[]` の順序で保持し、Ungroup 時もその順を維持して top-level へ戻す

## 履歴コミットの考え方

- 新規 stroke / 図形作成は `commit_element`
- 単一要素の Move / Resize / Rotate は `replace_document` ベースで 1 回だけ確定する
- 複数要素の Move / Resize / Rotate / Group / Ungroup / Align / Distribute / Reorder も `replace_document` を 1 回だけ積む
- layer Add / Delete / Rename / Visibility / Lock / Move、selection の layer transfer、grid / guides / smart guide / ruler 設定変更、guide drag も `replace_document` を 1 回だけ積む
- preview 中は `SelectionSession` の中だけで状態を持つ
- リリース時にだけ 1 回の編集として履歴へ積む
- 選択状態やビュー状態は履歴に積まない

## Undo / Redo とビュー操作の関係

- `DocumentHistory` が `current`, `undo_stack`, `redo_stack` を保持する
- 新規作成、移動、単一 / 複数リサイズ、単一 / 複数回転、Group / Ungroup、整列、等間隔配置、重なり順変更、layer add/delete/rename/visible/locked/order、selection の layer move / duplicate、grid / guides / smart guide / ruler toggle、spacing change、guide add / remove / move、`Clear`、`Load` は編集履歴に入る
- `Undo` 後に新規編集を行った場合、`redo_stack` は破棄する
- ズーム / パン / Reset View は view state の変更として扱い、編集履歴には影響させない

## 保存形式の責務

- JSON 保存は「再編集用」
- `storage` が JSON envelope の version 管理と encode / decode を担当する
- 現在の保存は `format.version = 4`
- 旧 `version = 1` の stroke-only 形式は decode 側で `PaintElement::Stroke` へ変換して読む
- 旧 `version = 2` と `version = 3` の flat な stroke / shape / group 形式は decode 側で単一 layer document へ migration する
- 旧 shape JSON に `rotation_radians` が無い場合は `0` 扱いで読める
- `fill_color` は serde default 付きで追加しているため、format version を上げずに後方互換を維持している
- `FillElement.spans` も同じ `format.version = 4` のまま保存できるため、バケツ塗り追加でも version は上げていない
- group は `PaintElement::Group` として再帰的に保存する
- レイヤー順は `document.layers[]` の配列順として保存する
- レイヤー内重なり順は `layer.elements[]` の配列順として保存する
- レイヤー間移動 / 複製の結果も追加メタデータなしでそのまま保存できるため、format version は `4` のまま維持している
- grid / guides も `document` に直接保存するため、同じ `format.version = 4` のまま後方互換を保てる
- spacing や guide position の変更も追加 migration なしで保存できる
- `塗りのゆるさ` は作品データではなく UI 状態として持つため、JSON format version は上げていない

## PNG 出力の責務

- PNG は「共有 / 閲覧用」
- `render` が作品データからピクセルデータを生成する
- `storage` が PNG バイト列化と native / web 保存導線を担当する
- `スポイト` は UI 補助表示ではなく、作品ラスタライズ結果から色を拾う
- バケツ塗りは visible layer を合成した見た目を境界判定に使い、`塗りのゆるさ` に応じた色比較を行い、結果は active layer へ `FillElement` として積む
- visible な layer だけを順番に描画する
- locked layer も visible なら描画する
- 回転やリサイズ後の図形も作品データからそのまま描画する
- group、整列、等間隔配置、group transform、重なり順変更、layer order も作品データどおりに反映する
- 選択枠、ハンドル、grid、guide は出力に含めない
- 透過PNG では背景 alpha 0 を維持しつつ、線 / 塗りの alpha もそのまま保持する

## 将来の拡張方針

- レイヤー
  - opacity
  - blend mode
  - layer 間ドラッグ移動
- ガイド
  - ドラッグでの位置編集
  - ruler 由来の追加
- 複数選択強化
  - group 内だけを直接編集する isolate モード
  - group のネスト可視化
- ストローク変形
  - 単一選択でも扱える専用ハンドルや変形 UI を追加
- 図形編集強化
  - 塗り、角丸矩形、矢印、スナップなどを追加
- 保存形式 migration
  - 将来の大きな形状拡張時に `format.version` を上げて decode 側で migration を入れる
