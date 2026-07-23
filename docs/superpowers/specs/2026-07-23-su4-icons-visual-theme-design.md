# SU4 設計 — アイコン + 視覚 pass + §11 テーマ消費（#532 Phase 2）

- 種別: サブユニット設計（spec → plan → 実装サイクル）
- 日付: 2026-07-23
- 親: #532（メインウィンドウの egui/softbuffer 移行）・ロードマップ `docs/superpowers/specs/2026-07-21-phase2-softbuffer-migration-roadmap.md` の **SU4**
- 統合する follow-up: **#632**（結果行 legibility・scroll 追従）
- 前提となる計測: 本設計は着手前に 2 本の計測プローブで土俵を確定した（→「計測で確定した前提」）。#634 が SU3.5 の前に G-SYNC を実測したのと同じ刻み

## 位置づけ

SU3（M1–M3・PR #630/#636/#637）で検索体験の機能中核が egui 経路へ載り、SU3.5（PR #641）で tool-selection が parity に達した。SU4 は**視覚の pass** を一括で当てる。ロードマップが SU4 に束ねた 3 つを、同じ `draw_result_row` / `view.rs` を二度作り直さないよう**単一 spec/PR** で扱う:

1. **アイコン** — WebView2 経路の IPC `get_icons_batch` + フロント `lruIconCache`/`iconBatch`（Blob URL）を、egui テクスチャ層 + 既存 `IconCache`/`icons.bin` の直接消費へ置換
2. **#632** — 結果行の legibility（name/path 重なり）と scroll 追従の是正
3. **§11 テーマ消費** — ハードコード色/フォントサイズを config テーマ値（5 色 + font_size + font_family）から描く

これが無いと SU6 の「テーマ反映」に書き込む先が無い（ロードマップ SU4 の位置づけ）。また flip 基準 2（外観維持）に直結する。

## 計測で確定した前提

着手前に 2 つの設計フォークを実測で閉じた。数字は再現可能なプローブに由来する。

### Probe 1 — アイコン抽出コスト（`icon.rs` の `icon_extract_cost_probe`・`#[ignore]`）

`SHGetFileInfoW` → BGRA → PNG 全区間を代表パスで release 実測（`cargo test -p snotra --release icon_extract_cost_probe -- --ignored --nocapture`）:

| 対象 | warm per-call p50 | p95 | max |
|---|---|---|---|
| exe | 904µs | 1.41ms | 4.91ms |
| folder | 814µs | 1.60ms | 2.42ms |
| doc | 898µs | 1.45ms | 1.64ms |
| lnk（対象解決あり） | 1.92ms | 2.45ms | 4.37ms |
| **8 件バッチ warm 合計** | **8.07ms** | **11.2ms** | **11.66ms** |

**判定**: フレーム予算 16.7ms に対し 8 件バッチ warm 合計が p50=8ms・max=11.7ms。1 結果集合ぶんを `update()` 内で同期抽出すると、検索＋描画で既に消費している同フレームに 48〜70% を上乗せし、ほぼ確実に 1 フレーム脱落（目に見えるヒッチ）。しかもこれは warm で、cold first-touch はさらに重い。加えて `lnk` の対象解決 2ms が示す通り抽出はシェル/FS に触れるため、**dead UNC / 遅いプロバイダを含むパスでは同期抽出がイベントループを無期限に塞ぐ**（folder が per-nav thread を選んだのと同じ理由）。

→ **update() 内同期は不成立。worker が正当化される**（独立した二理由: 共通ケースが 1 フレームに重すぎる + dead-path がイベントループを塞ぐ）。

### Probe 2 — font_family honor の実挙動（throwaway spike・撤去済み）

`SNOTRA_FONT_SPIKE=1` で config `font_family`（既定 Segoe UI）を先頭・jp_font を fallback に積んだ版を egui 経路（`SNOTRA_EGUI_MAIN=1`）で起動し、混在行（Latin+CJK+長パス）を実機目視。

**判定**: **ベースラインずれ無し。honor 可**。当初仮説（honor は #579/#399 のベースラインずれを再発させる）を実測が覆した。和解: **ずれはフォントの組で決まる**。#579/#399 は egui 既定バンドルフォント vs Yu Gothic の組で生じた。Probe 2 は Segoe UI vs Yu Gothic の組で、両方 MS システムフォント（OS が Latin/JP に共用する設計）ゆえメトリクスが揃う。WebView2 の CSS スタック `font-family: var(--font-family, "Segoe UI"), "Yu Gothic UI", "Meiryo", sans-serif`（`ui/src/styles/global.css:20`）と同型で、honor は WebView2 parity に**近づく**。

**残余（前提条件付き）**: config `font_family` は任意のシステムフォント名を取りうる（settings は全システムフォントを列挙・`snotra-settings/src/tabs/visual.rs:125-131`）。WebView2 は CSS の per-glyph ベースライン整列で任意フォントを綺麗に honor できるが、egui は**フォント単位の粗い `FontTweak`** しか持たないため任意の組は保証されない。既定（Segoe UI）は実測 clean だが、**非 MS フォントを選ぶと `snotra-settings/CLAUDE.md`「フォント登録の注意点」が記す 2 フォント分離のベースラインずれが起きうる（視覚スモークでのみ顕在化）**。これは egui 側の構造的残余であり、SPEC の parity-gap として明記する（実装より強い「honor は常に綺麗」という全称主張は**書かない**）。

## 決定事項

1. **worker + settled/trailing 抽出**（Probe 1）。毎打鍵 spawn しない。debounce が落ちついた（trailing 確定）結果集合の未キャッシュ path だけを 1 worker でバッチ抽出する。folder 展開の `spawn_folder_load`（per-nav `std::thread::spawn` + channel + `ctx.request_repaint()`）の構造を踏襲する。
2. **アイコンの staleness は path キー付けで構造的に無害化する。folder 式 token は載せない**（Probe 1 + advisor）。テクスチャは `path → TextureHandle` map に保持し、`draw_result_row` は現行行の path で引く。遅延到着した古い結果集合ぶんの texture は「引かれず map に座るだけ」で描画に混入しない。ロードマップ SU4 受け入れの「token drain」文言は**計測前の記述**であり、計測後の正は「path キー付けで stale 描画は構造的に起こらない + settled/trailing で spawn を抑制」。folder（stale リストが誤起動を招く）との非対称ゆえ、supersede/single-flight の全面復活はさせない（ロードマップ リスク欄「並行性の再導入」の規律）。
3. **font_family を honor する**（Probe 2）。fontdb でファミリ名→バイト列を解決し、user primary + jp_font fallback で登録する。既定フォントで parity・任意フォントは egui tweak 限界による残余（決定 8）。
4. **フォント解決は `fontdb` crate**（採用判断は別記・→「fontdb 採用の根拠」）。GDI `GetFontData` の手巻き（暗黙フォント置換の footgun・TTC face・bold 合成の手当て）を避ける。
5. **§11 テーマは runtime を触らず view 側で honor する**（SU1 境界不変）。config 背景は (a) 窓生成の `.background_color`（`egui_shell/mod.rs:55` の現ハードコード `0x282828`）を config 値へ、(b) view の egui visuals（panel/window fill）を config 値へ、で描く。`snotra-egui-runtime/src/renderer.rs:10` の `CLEAR_COLOR` は初回描画前の過渡だけに残るため変更しない。
6. **`show_icons=false` は 28px スロットを畳んでテキストを左端寄せする**（skip でなくレイアウト変更）。
7. **スコープは icon + #632 + §11 を単一 PR**。`draw_result_row` を二度作らない。

## 設計

### Part A — アイコン（Rust テクスチャ層）

**共有永続層は不変**: `src-tauri/src/icon.rs` の `IconCache`（`get`/`insert`・FIFO `enforce_cap`）・`icons.bin`・`IconCacheState = Mutex<Option<IconCache>>`・`icon_cache_cap`（`Config` が表示ワーキングセット ×5 で導出）・`invalidate_icon_cache`・`retain_paths` はそのまま。PNG バイト列の抽出（`extract_png`）と永続はここに閉じる。WebView2 経路の `encode_batch_binary` / IPC `get_icons_batch` は WebView2 が使い続けるため残す（撤去は SU7 の経路撤去で）。

**新規: egui テクスチャ層**（`SearchWindowView` 内）:

- フィールド: `icon_textures: HashMap<String, egui::TextureHandle>`（path → 生成済みテクスチャ）、`icon_tx/icon_rx`（`IconMsg` channel）、`icon_known_missing`（抽出したが None だった path の記録＝再 spawn 抑制）。
- **worker（settled/trailing で spawn）**: `run_search` が結果を確定した後（trailing poll / folder drain / settled Plain 検索）に、現結果集合の path のうち `icon_textures` にも `icon_known_missing` にも無いものを集め、**空でなければ 1 スレッドを spawn** する。worker は各 path につき `extract_png`（内部で `IconCache` を lock → get / なければ抽出 → insert・エンジンロックとは別 lock）で PNG を得て、`png` バイト列を `egui::ColorImage` へ decode し、`IconMsg::Loaded(path, ColorImage)`（None は `IconMsg::Missing(path)`）を channel 送信する。送信後 `egui_ctx.request_repaint()`（folder と同じくイベント駆動 runtime を起こす）。
  - **`load_texture` は worker で呼ばない**: `TextureHandle` 生成は egui context（メインスレッド）でのみ安全。ColorImage を channel で渡し `update()` で load する（folder が FolderMsg を drain して適用するのと同型）。
  - `show_icons=false` のときは worker を spawn しない（抽出コストも払わない）。
- **`update()` での適用**: 冒頭の folder drain と同じ場所で `icon_rx` を drain し、`IconMsg::Loaded` は `ctx.load_texture(path, color_image, TextureOptions::default())` の handle を `icon_textures` に挿入、`IconMsg::Missing` は `icon_known_missing` に記録する。
- **描画**: `draw_result_row` は現行行の path で `icon_textures` を引き、Some なら 28px スロットにテクスチャを paint、None ならスロット空（Part A fallback 節）。
- **throttle**: spawn は settled/trailing のみ。連打中（debounce armed）は spawn せず、既存テクスチャだけを描く。

**メモリ境界**（`revokeAll` analog・SU6.5 ゲート直結）:

- **hide / reset で全 drop**: `update()` の `reset_pending` 消費時（show 直後のリセット）に `icon_textures.clear()` + `icon_known_missing.clear()`。hide 中の常駐テクスチャメモリを残さない。
- **結果変化で retain**: 結果集合が変わったら現結果 path 集合で `icon_textures` を retain（`retain_paths` と同じ発想・現結果に無い path の handle を drop）。これでテクスチャメモリを実質「可視集合」に頭打ちする。retain は毎打鍵でも ~数件のイテレーションで安価。

**`show_icons=false`（レイアウト変更）**: `draw_result_row` はアイコンスロット幅を 0 にしてテキストを左端（`rect.left() + padding`）から描く。worker も spawn しない。`show_icons` は実行中 config から都度読む（`auto_hide_enabled` 等と同設計・`config().appearance` 系の該当キー）。

**fallback**: `SHGetFileInfoW` は未知拡張子でも generic shell icon を返すため「抽出成功だが真にアイコン無し（None）」は稀（`extract_icon` が `has_data` で全 0 ビットマップを None にするケース等）。真の欠落時、§3.4 は 📁📄 を規定するが、softbuffer + 単一 TTF は**色 emoji を描けない可能性が高い**。実装時に jp_font（Yu Gothic）の 📁📄 グリフ被覆を確認し、無ければ drawn placeholder（フォルダ/ファイルを示す単色の簡易図形、または generic shell icon への委譲）に倒す。この 1 点は**実装時の視覚確認を要する残余**として plan に明示する。

### Part B — #632（legibility + scroll）

現 `draw_result_row`（`view.rs:601`）は `painter.text` で name（左寄せ 14pt）と path（右寄せ 11pt 淡色）を**幅管理なし**で置くため、長い name/path が中央で重なる。scroll は選択行の `scroll_to_me(Center)` を**毎フレーム**呼び、(a) ホイールスクロールを上書き、(b) index 0 選択で上部に空白を作る。

- **legibility**: name を利用可能幅（アイコンスロット後〜行右端の一定割合）に egui galley で truncate、path を残り幅に中間省略（`ui/src/lib/truncatePath.ts` の中間 `...` 省略に相当。egui の galley 幅計測 or `Galley` の省略で実装）。name/path が同一 rect にクリップされ重ならない。
- **scroll 追従**: `scroll_to_me` を**選択 index が前フレームから変化したときのみ**発火。`SearchWindowView` に `last_scrolled_selected: Option<usize>` を持ち、`selected() != last_scrolled_selected` のときだけ `scroll_to_me` + 更新。結果リセット時は `None` に戻す。

### Part C — §11 テーマ消費

**config 値の出所**: `snotra_core::config::VisualConfig`（`config.rs`）— `background_color` / `input_background_color` / `text_color` / `selected_row_color` / `hint_text_color`（5 色・すべて `#RRGGBB` 文字列）+ `font_family` + `font_size`（u32）。§11 の 5 色はすべて実在（`selected_row_color` を含む）。

**色**（ハードコード撤廃）:

- view の update で egui visuals を config から設定する: `panel_fill` / `window_fill` = `background_color`、`override_text_color` 系は使わず `text_color` を name 描画色に、`hint_text_color` を path/hint 色に、`selection.bg_fill` = `selected_row_color`、TextEdit 背景 = `input_background_color`。hex 文字列 → `egui::Color32` は egui の hex パース（`Color32::from_hex`・0.35）で変換し、失敗時は現行既定色にフォールバック。
- `draw_result_row` の `ui.visuals().text_color()` / `weak_text_color()` / `selection.bg_fill` を config 由来の値へ差し替える。
- 窓生成 `egui_shell/mod.rs:55` の `.background_color(Color(0x28,0x28,0x28,0xff))` を config `background_color` から構築（過渡/リサイズ時の下地色）。SU2 が窓生成時に config を読める経路は既存（`main.rs:580` の `bg_color` が WebView2 経路で既に background_color を読む）。

**フォントサイズ**: name = config `font_size`、path = 従属的に小さいサイズ（WebView2 `ResultRow` の CSS 比を踏襲・例 `font_size` から一定比）。現行ハードコード 14/11 を置換。TextEdit も font_size に追従。

**font_family（fontdb・#579 不変条件の進化）**:

- **解決**: フォント設定時（view setup・SU6 で config 変更にも）に `fontdb::Database::load_system_fonts()` → `db.query(&Query { families: &[Family::Name(&font_family)], .. })` で ID を得て `db.with_face_data(id, |data, face_index| ...)` でバイト列 + **face index**（TTC 対応）を取り出す。`egui::FontData::from_owned(bytes)` に face index を設定（egui 0.35 の `FontData.index`）。Database は解決後に drop（常駐させない・列挙コストはフォント設定時の一度きり・settings が `list_system_fonts` を起動時キャッシュする前例と同型）。
- **登録**: 解決成功時は `configure_japanese_font` を **user primary + jp_font fallback** で構成する（両ファミリの index 0 を user_font、index 1 を jp_font）。解決失敗（フォント不在・パース不能）時は**現行の jp_font 単一スタック（`insert(0)`）にフォールバック**する。
- **#579 不変条件の進化（重要）**: 現行の不変条件「jp_font を両ファミリ index 0 に置き単一フォント化する」（`view.rs` の `jp_font_is_registered_at_index_zero_for_both_families` テスト・`snotra-egui-mvp/CLAUDE.md`）は、#579 が **egui 既定バンドルフォント vs Yu Gothic** の 2 フォント分離ドリフトを潰すために置いた。honor はこの単一スタックを **supersede** する: 進化後の不変条件は「**font_family 解決時は user primary + jp_font fallback（WebView2 CSS スタック parity）。解決失敗時のみ jp_font 単一スタック**」。#579 のドリフトは (a) WebView2 が実際に使う MS システムフォント（Segoe UI + Yu Gothic）を使い Probe 2 で clean を実測、(b) 任意フォントの残余は決定 8 で受容、で扱う。**テストは「どの命題を証明していたか」を追跡して書き換える**（AGENTS.md「既存テストの改名で不変条件を孤立させない」）: 単一スタックの命題は「解決失敗時」の枝で残し、解決成功時は「user 先頭・jp fallback」を新たに固定する。

8. **残余（parity-gap・SPEC 明記）**: egui の per-font `FontTweak` は任意 Latin フォント × Yu Gothic の組でベースライン整列を保証しない。既定（Segoe UI）は実測 clean だが、非 MS フォント選択時に混在行でドリフトしうる（視覚スモークでのみ顕在化）。WebView2 は CSS per-glyph 整列でこれを回避するため、これは egui 経路固有の受容残余。

## スコープ

**やる**: icon テクスチャ層 + worker + メモリ境界 + show_icons レイアウト + fallback / #632 legibility + scroll gate / §11 5 色 + font_size + font_family(fontdb) + 窓背景。単一 PR。

**やらない（他 SU）**: config 変更の live 反映（テーマ/font_family の `config_watcher` → egui 反映は **SU6**）。updater トースト（**SU5**）。#633 stale クリア（**SU6**）。§12 IME parity（**SU6**）。メモリゲート再測（**SU6.5**）。WebView2 経路の icon IPC 撤去（**SU7**）。

## 受け入れ条件

ロードマップ SU4 受け入れ条件を土台に、計測で更新した箇所を supersede と明記:

1. アイコン抽出/キャッシュ/非同期が現行と同等: 欠落時プレースホルダ（Part A fallback）・N 件上限の下流整合（`IconCache` の `icon_cache_cap` を共有ゆえ不変）。
2. 非同期アイコンが stale 描画を起こさない: **path キー付けで構造的に**（決定 2・「token drain」文言を supersede）。遅延到着 texture が現行行に混入しない。
3. worker がイベントループを止めない（dead UNC で `update()` を塞がない・folder parity）。settled/trailing で spawn が連打中に堆積しない。
4. hide / 結果変化でテクスチャメモリが頭打ち（clear-on-hide + retain-on-change）。
5. 行視覚が #632 の症状を解消: name/path が重ならない（truncate + 中間省略）・`scroll_to_me` が選択変化時のみ（ホイール上書きせず・index 0 で上部空白を作らない）。
6. **色・フォントサイズが config テーマ値から描かれ、ハードコード色/14/11 が残らない**（§11 parity）。窓背景も config `background_color`。
7. font_family が解決時 user primary + jp_font fallback で描かれ、解決失敗時 jp_font 単一へフォールバック。#579 テストが進化後の不変条件（2 枝）を固定する。
8. `show_icons=false` で 28px スロットが畳まれテキストが左端から描かれ、worker も spawn しない。

## リスク・残余

- **fallback emoji（📁📄）が softbuffer 単一 TTF で描けない可能性**（Part A）: 実装時に jp_font グリフ被覆を視覚確認し、無ければ drawn placeholder に倒す。plan の視覚確認項目。
- **font_family 任意フォントのベースライン残余**（決定 8）: 受容・SPEC parity-gap 注記。非 MS フォント選択時の混在行は視覚スモークでのみ顕在化。
- **egui hex パース / FontData.index / fontdb Query API のバージョン依存**: egui 0.35 / fontdb 0.23 の実 API を実装前に確認（`Color32::from_hex` の戻り型・`FontData.index` の有無・`with_face_data` のシグネチャ）。src-tauri ルール「windows/egui クレートの API を使用前に確認」と同根。
- **並行性の局所再導入**（ロードマップ リスク欄）: worker は folder の per-nav thread パターンに限定し、supersede/single-flight は復活させない。

## スキャフォールド始末

- **icon bench（`icon.rs` の `icon_extract_cost_probe`）→ 恒久 `#[ignore]` probe として残す**（#634 の `search_frame_cost.rs` と同型・将来の抽出コスト回帰の接地点）。本 PR に含める。
- **font spike（`view.rs` の throwaway・`SNOTRA_FONT_SPIKE`）→ 撤去済み**（Probe 2 の数字が出た時点で revert・2026-07-23）。

## 依存クレート追加

- `fontdb`（現行 0.23.0・MIT・RazrFalcon）を `src-tauri` に追加。採用根拠は「fontdb 採用の根拠」。`load_system_fonts` の列挙はフォント設定時の一度きり。Database は解決後 drop（非常駐）。

### fontdb 採用の根拠

- name→バイト列解決は「専門ツールに委ねるべき仕事」であり、GDI `GetFontData` の手巻き（フォント名不在時の**暗黙置換 footgun**・TTC face index・bold 合成の手当て）を避ける。fontdb は `query()` がマッチ有無を明示的に返し `with_face_data` が face index 込みで正しいバイト列を返す。
- 保守は健全: crates.io 0.23.0（2024-10）は約 1 年 9 か月更新なしだが、これは「機能完成・低 churn の安定運用」であり放置ではない。GitHub master は 2026-07-16〜17（本設計の数日前）に活発（iOS 対応・**ttf-parser 最小サブセットのインライン化＝依存フットプリント縮小**・MSRV 1.71）。総 DL ~2,600 万・cosmic-text/resvg が依存する load-bearing クレート。
- コスト: dep 1 個 + ttf-parser 推移。常駐ランチャーへの重みは、作者が今まさに縮小している方向で軽くなりつつある。

## 進め方

本設計は SU4 の spec。次は writing-plans で実装計画（TDD タスク分割）を建てる。実装前チェックのトリガー: 非同期 worker 追加（`/race-check`）・新規フィールド/関数（`/dry-check` + 呼び出し元 grep）・#579 不変条件を担うテストの改名（命題追跡）・`Cargo.toml` 変更（cargo check）・視覚レンダリング欠陥（PR 前の実機視覚スモーク・`ui.md` トリガー相当）。
