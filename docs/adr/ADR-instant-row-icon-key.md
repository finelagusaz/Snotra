# ADR-instant-row-icon-key: instant 行のアイコンを「行の種別」で決め、SPEC の全スキップ宣言を採らない

## 文脈

`SPEC.md` §19.5 は「インスタントコマンドモード中はアイコン取得をスキップする（`path` がファイルパスではないため）」と宣言していたが、**egui 経路にこれを実装する機構は無かった**（#1133）。実装実体は WebView2 経路の `skipIcons` prop で、#532 SU7（`15933afa`）のフロント撤去で消え、SPEC の当該行だけが残っていた。

as-built を実機で測ったところ（`SNOTRA_CONFIG_DIR` で隔離したプロファイル・trace `icon:extract_failed`）:

- URL 種別の行に対して**抽出は実際に走り、上限（3 回）まで再試行していた**。`SHGetFileInfoW` の失敗は `ShellQueryFailed` で `is_transient() == true` ゆえ恒久失敗として latch されない
- **exec 種別・`args` 空・`description` 空の行には、本物の exe のアイコンが出ていた**——`display_text` が `exe` をそのまま `path` に入れるため、`path` は普通のファイルパスになる。**§19.5 の括弧書きの前提（「`path` がファイルパスではないため」）は as-built で崩れていた**

## 決定

**仕様を変える。** 結果行のアイコン抽出キーは**行ごとに定まる値**とし（規則の正本は `SPEC.md` §3.4）、instant 行では種別で分ける——exec 種別は `expand_env(exe)` をキーにし、URL / Legacy 種別は抽出そのものを行わない。

表現は `snotra_core::ui_types::IconSource`（`FromPath` / `Skip` / `Explicit(String)`）で、読みは `SearchResult::icon_key()` の 1 つに閉じる。

## 検討した代替案と却下理由

- **案 A: SPEC の字面に合わせる（instant 行を種別に依らず全スキップ）**: 却下。字面どおりに直すと、**exec 種別で今日出ている本物のアイコンが消える**。それは退行に見える挙動であり、しかも §19.5 が根拠にしていた前提（`path` はファイルパスではない）自体が偽である以上、「仕様に合わせる」は誤った前提へ合わせることになる。`RowsSnapshot` に `bool` を 1 つ足すだけで済む点は案 C より単純だが、単純さは誤った挙動を選ぶ理由にならない。
- **案 B: as-built を追認する（§19.5 の当該行を書き換えるだけ・コードは触らない）**: 却下。差分はゼロで済むが、**仕様が無駄を追認する形になる**——URL 行に対する毎打鍵の抽出要求と、`is_transient` ゆえ latch されない再試行がそのまま残る。しかも instant 枝は毎打鍵 `search_debounce.cancel()` を撃つので、plain 側には効いている perf ゲート（`input_idle`）が instant では構造的に常に開いている。「今そうなっている」ことは「そうあるべき」の理由にならない。
- **案 C の変種: exec 種別でも `description` が設定されていればスキップする**（副テキストとアイコンが同じものを指す形に揃える）: 却下（2026-08-18 のユーザー裁定）。`description` は**副テキストを何にするかを決める設定**であってアイコンの話ではない。二軸（種別 / description の有無）を交差させると、同じ exe を起動する 2 つのコマンドが description の有無だけでアイコンの有無を分ける。
- **キーを `SearchResult` に持たせず、行と並ぶ別ベクタ（`Vec<Option<String>>`）で運ぶ**: 却下。行は `SearchState.results` に一元化されており、`set_results` / `put_rows` / `enter_tool` / folder drain / search worker が総入れ替えする。並列ベクタは**それら全経路で長さと順序を手で同期させる規約**を生む。行自身に持たせれば、移行漏れは全構築点で compile-fail（E0063）になる。
- **スキップを `RowsSnapshot::input_idle` 側で表現する**: 却下。あれは「main の `search_debounce` が予約を持っていないか」を運ぶ perf ヒューリスティックであり、**「打鍵が止まった」より広い**（#1074）。ここへ instant の条件を足すと、**worker 走査中のアイコン取得まで一緒に遅れる**退行が入り、しかも絵は正しく見えるので挙動テストでは捕まらない。この禁止は `icon_textures.rs` の `icon_gate_keeps_input_idle_semantics` がソーステキストで固定する。

## 帰結

- URL / Legacy 種別の instant 行は、`SHGetFileInfoW` を 1 度も呼ばなくなる。**絵は変わらない**（どちらにせよ placeholder）が、毎打鍵の無駄仕事と再試行が消える
- exec 種別は `args` の有無・`description` の有無に依らず exe のアイコンが出る。**`args` 有りの行は今日失敗している経路であり、ここだけが絵の変化である**
- `icons.bin` は形式もバージョンも変えない。変わるのは**キーの中身**（instant exec 行が display 文字列 → 実 exe パス）だけで、旧キーのエントリは FIFO 上限で自然に押し出される
- **抽出に失敗した文字列はそもそもキャッシュへ入らない**（`load_icon_pngs` は `Ok` のときだけ挿入する）。#1133 の issue が ⚠未確認 3 として挙げた「表示文字列が `icons.bin` のキーとして永続化される」懸念は as-built で成立しておらず、この決定が解消したものでもない
- 実運用点への届き方は非対称である——実 config・既定 config のどちらにも exec 種別の instant コマンドは 0 件なので、**「exec は本物のアイコン」の側は現状の利用者には届かない**。仕様は 1 つの config ではなく製品の姿を定めるため設計判断は変えないが、検証労力の配分はこの非対称に従う
