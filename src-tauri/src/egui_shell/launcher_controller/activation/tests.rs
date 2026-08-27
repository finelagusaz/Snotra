//! `activation.rs` の呼び出し点をソーステキストで固定する検査（#1077 / #1106 / #1112）。
//!
//! **母集団は `activation.rs` 1 枚である**——`include_str!("../activation.rs")`。起動の入口が
//! 別の子モジュールへ移れば母集団は割れるが、各検査はアンカーが 4 スペース字下げのメソッド
//! ヘッダとして見つかることを先に assert するので**沈黙ではなく赤になる**。
//!
//! **切り出しの helper はこの 1 か所に閉じる**（`docs/adr/ADR-source-text-probe-helper-locality.md`）
//! ——存在形が使う [`method_body`] と否定形が使う [`owners_of`] を並べて持つのは、極性が違えば
//! 要る不変条件が違うからであって写しではない。

/// メソッドの本体を切り出す（終端は 4 スペース字下げの閉じ括弧・内側のブロックはより深い）。
///
/// **母集団は狭すぎても広すぎても壊れる。両方を assert する。**
///
/// - **狭すぎる（空）**: 目印（`canary`）が本体に在ることで確かめる。沈黙する検知器は検知器ではない
/// - **広すぎる（終端を取り逃す）**: 終端が実際に見つかったことで確かめる。**取り逃すと本体が
///   母集団の EOF まで伸び、後続のメソッドを飲み込む**——`contains` 系の assert は隣のメソッドが
///   持つ綴りで真になり、**検知器が空虚になったまま緑で通る**（現に `activate_or_execute` の
///   終端を取り逃すと、下にある `shift_activate` の同じゲートの綴りが 2 本とも肩代わりする）
///
/// **行の走査に `str::lines` を使うのは改行コード非依存にするためである**（CI 実測）。
/// `find("\n    }\n")` は CRLF で checkout された作業ツリーに一致せず、上の「広すぎる」を
/// 起こした。手元の `core.autocrlf=input` では再現せず、**CI（git-for-windows の system
/// 既定 `core.autocrlf=true`）でだけ落ちた**。同じ非対称は `.gitattributes` の冒頭コメントが
/// `.githooks/**` について記録している。`str::lines` は `\n` で分割し末尾の `\r` を落とす。
fn method_body(src: &str, anchor: &str, canary: &str) -> String {
    let (before, after) = src
        .split_once(anchor)
        .unwrap_or_else(|| panic!("{anchor} が見つからない（改名したらこの検査も直す）"));
    // **アンカーの字下げは終端の字下げと組である。** ずれると既存の 2 assert は
    // どちらも発火しないまま母集団が壊れる——#1108 で両方向を実測した（列 0 のアンカーは
    // 内側ブロックの `    }` で黙って狭まり、8 スペースのアンカーは自分の終端を通り越して
    // 隣のメソッドを黙って飲み込む）。**見るのは字下げ幅だけである**——アンカーと行頭の
    // あいだには可視性修飾が挟まりうる（現に `pub(super) ` が挟まる呼び出しが在る）。
    // ゆえに**同じ字下げの doc コメント行にアンカー文字列が先行出現した場合は通る**——
    // そこは下流の canary が捕まえる（`top_level_fn_body` 側はアンカーを行頭に密着させる
    // 形なので、あちらでは doc の先行出現もこの assert が落とす。非対称は意図である）。
    // **空白文字の種類まで見る**——`trim_start` はタブも落とすので、バイト差だけで数えると
    // `\t\t\t\t` 字下げのアンカーが字下げ 4 として通る（終端は `    }` なので母集団は壊れる）。
    // [`method_header`] と同じ形の欠陥であり、同じ形で塞ぐ（2026-08-17 の反証レビューが
    // `method_header` 側で実測し、同一パターンの走査でこちらを見つけた）。
    let head = before.rsplit('\n').next().unwrap_or("");
    assert!(
        head.len() - head.trim_start().len() == 4 && head.starts_with("    "),
        "{anchor} を含む行が 4 スペース字下げで始まっていない——終端の `    }}` が内側ブロックか\
         外側の閉じ括弧に一致し、母集団が黙って狭まる／広がる"
    );
    let mut body = String::new();
    let mut terminated = false;
    for line in after.lines() {
        if line == "    }" {
            terminated = true;
            break;
        }
        body.push_str(line);
        body.push('\n');
    }
    assert!(
        terminated,
        "{anchor} の終端（4 スペース字下げの `}}`）が見つからない——母集団が EOF まで\
         伸びており、この検査は空虚である"
    );
    assert!(
        body.contains(canary),
        "母集団が {anchor} の本体を含まない——終端の切り出しがずれた。\
         沈黙する検知器は検知器ではない"
    );
    body
}

/// [`method_body`] が改行コードに依存しないことを、**この作業ツリーの改行コードによらず**
/// 固定する（#1077）。`include_str!` は checkout された実ファイルを読むため、
/// LF の環境で `cargo test` が緑でも CRLF の環境で母集団が壊れうる——実際に CI でそうなった。
#[test]
fn method_body_is_line_ending_agnostic() {
    let lf = "    fn target(&self) {\n        marker();\n    }\n    fn next(&self) {\n";
    let crlf = lf.replace('\n', "\r\n");
    for (label, src) in [("LF", lf), ("CRLF", crlf.as_str())] {
        let body = method_body(src, "fn target(", "marker(");
        assert!(
            !body.contains("fn next("),
            "{label}: 終端を取り逃して次の関数まで飲み込んでいる"
        );
    }
}

/// [`method_body`] が**アンカーの字下げ違反を拒む**ことを固定する（#1108）。
///
/// 終端が「4 スペース字下げの `}`」なので、アンカーの字下げがずれると母集団は黙って狭まる
/// （列 0 のアンカーは内側ブロックの `    }` で切れる）か、黙って広がる（8 スペースの
/// アンカーは自分の終端を通り越して隣のメソッドを飲み込む）。**両方向とも既存の 2 assert
/// （終端・canary）は 1 つも発火しない**——#1108 で実測した。
#[test]
#[should_panic(expected = "4 スペース字下げで始まっていない")]
fn method_body_rejects_an_anchor_at_the_wrong_indent() {
    method_body(
        "pub fn target() {\n    marker();\n    if c {\n    }\n}\n",
        "pub fn target(",
        "marker(",
    );
}

/// [`method_body`] が**深すぎる字下げのアンカーも拒む**ことを固定する（#1108）。
///
/// 上のテストと**別に置く**——浅い側（列 0）の fixture だけでは、述語を `== 4` から `>= 4` へ
/// 弱める変異が捕まらない（どちらの述語でも赤になるため）。**広がる方向こそ #1077 / #1108 の
/// 沈黙そのものである**——自分の終端を通り越して隣のメソッドを飲み込む。
#[test]
#[should_panic(expected = "4 スペース字下げで始まっていない")]
fn method_body_rejects_an_anchor_indented_too_deeply() {
    method_body(
        "mod outer {\n    impl C {\n        fn target(&self) {\n            marker();\n        }\n        fn other(&self) {\n            secret();\n        }\n    }\n}\n",
        "fn target(",
        "marker(",
    );
}

/// [`method_body`] が**タブ字下げのアンカーを拒む**ことを固定する（#1112）。
///
/// 上の 2 本と**別に置く**——どちらも字下げの「幅」がずれる fixture なので、`trim_start` の
/// バイト差だけで数える形をどちらも落とさない。終端は `    }`（スペース 4）に固定なので、
/// タブ 4 個のアンカーを受理すると母集団は隣のメソッドまで伸びる。
#[test]
#[should_panic(expected = "4 スペース字下げで始まっていない")]
fn method_body_rejects_a_tab_indented_anchor() {
    method_body(
        "impl C {\n\t\t\t\tfn target(&self) {\n\t\t\t\t\tmarker();\n\t\t\t\t}\n    fn other(&self) {\n    }\n}\n",
        "fn target(",
        "marker(",
    );
}

/// [`method_body`] が**終端の無い母集団を拒む**ことを固定する（#1112）。
///
/// `top_level_fn_body`（`indexing.rs`）は同じ内容の回帰を 3 本持つのに、こちらは字下げの
/// 2 本しか持っていなかった——**assert は在るが、それを消しても全部緑のままだった**
/// （#1108 の PR 本文が形 B 側について記した状態が、形 A 側に残っていた）。
#[test]
#[should_panic(expected = "母集団が EOF まで伸びており")]
fn method_body_rejects_a_population_without_a_terminator() {
    method_body(
        "    fn target(&self) {\n        marker();\n",
        "fn target(",
        "marker(",
    );
}

/// [`method_body`] が**canary を含まない母集団を拒む**ことを固定する（#1112）。
///
/// 上のテストと**別に置く**——終端の fixture は canary を含むので、canary の assert を
/// 消す変異はあちらでは捕まらない。
#[test]
#[should_panic(expected = "の本体を含まない")]
fn method_body_rejects_a_population_without_the_canary() {
    method_body("    fn target(&self) {\n    }\n", "fn target(", "marker(");
}

/// 字下げ 4 のメソッドヘッダ行なら、その行を trim したものを返す。
///
/// 受理する形は `fn` の前に可視性修飾（`pub` / `pub(super)` 等）と `async` が挟まるもの
/// までである——[`method_body`] が字下げ幅だけを見ているのと同じ理由で、現に
/// `pub(super) ` が挟まる定義が在る。`///` の doc 行は `fn` へ辿り着かないのでヘッダに
/// ならない（doc はヘッダの**上**に在るため、[`owners_of`] では 1 つ前のメソッドへ
/// 帰属する——コードではないので取りこぼしてよい方向である）。
///
/// **字下げは幅だけでなく空白文字の種類まで見る。** `trim_start` はタブも落とすので
/// バイト差だけで数えると `\t\t\t\tfn …` が字下げ 4 として通り、[`owners_of`] では偽の
/// ヘッダとして帰属を横取りしうる（2026-08-17 の反証レビューが実測。当時の母集団——分割前の
/// `launcher_controller.rs`——にタブ字下げの行は 1 件も無く、`rustfmt.toml` が無いので
/// `hard_tabs=false` が効いていた。**露出は無かったが、露出が無いことは述語が正しいことを
/// 意味しない**）。
fn method_header(line: &str) -> Option<&str> {
    let trimmed = line.trim_start();
    if line.len() - trimmed.len() != 4 || !line.starts_with("    ") {
        return None;
    }
    let mut rest = trimmed;
    if let Some(after) = rest.strip_prefix("pub") {
        // `pub` / `pub(crate)` / `pub(super)` …——`(` から `)` までを読み飛ばす
        rest = match after.strip_prefix('(') {
            Some(scoped) => scoped.split_once(')').map_or(after, |(_, r)| r),
            None => after,
        };
    }
    rest = rest.trim_start();
    rest = rest.strip_prefix("async ").unwrap_or(rest).trim_start();
    rest.starts_with("fn ").then_some(trimmed)
}

/// `needle` の出現を**ファイル全体から**列挙し、各出現を直前の [`method_header`] へ
/// 帰属させる（出現行ごとに 1 要素・要素は帰属先のヘッダ行）。
///
/// **否定形の検査のための道具である。** 切り出した母集団 `P` に対する
/// `assert!(!P.contains(x))` は `P ⊇ B`（本体を取りこぼさないこと）を要求するのに、
/// [`method_body`] の 2 assert（終端・canary）が縛るのは `P ⊆ B` の側だけである——
/// **canary より後・禁止語より前で母集団が切れると、canary は通り禁止語は切り捨てられ、
/// 検査は緑のまま沈黙する**（#1112 で rustc 実測。存在形ならこの切れ方で赤になるので、
/// 沈黙は否定形に固有である）。ここでは**終端を求めない**ので切り詰めが起こりえず、
/// 下界は「ファイル全体は常に本体の上位集合」から構成で満たされる。
///
/// **境界の倒れ方**（網羅の主張はしない——下の残余がその実例である）:
/// - 本体内の入れ子 `fn`（字下げ 8）はヘッダに一致しないので、その中の出現は**外側の
///   メソッドへ帰属する**＝過剰発火＝赤
/// - 複数行文字列・コメントの中の**出現**も帰属して過剰発火する＝赤
/// - 最初のヘッダより前の出現（`use` 節・モジュール doc）は誰にも帰属せず無視される。
///   起動の入口の外なので赤にする理由が無い
///
/// # 受容する残余（緑へ倒れる形）
///
/// **少なくとも次の 2 つが在る**（尽くしてはいない）。どちらも**文字列の状態追跡や複数行の
/// 正規化では直さない**——検査の道具立てが検査対象より複雑になる。
///
/// **(1) 偽ヘッダによる帰属の横取り。** 複数行文字列やブロックコメントの中に**字下げ 4 で
/// `fn ` から始まる行**が在ると、それが偽のヘッダとして帰属先を横取りし、同じメソッドの
/// 後続の禁止語はその偽ヘッダへ帰属して**緑になる**（2026-08-17 に対照つきで実測——偽ヘッダ
/// 有りで緑・無しで赤）。**#1112 の穴より狭い**: 旧設計は文字列中の `    }` 1 行で切れたのに
/// 対し、こちらは起動の入口のいずれかの中で、禁止語より前に、ヘッダの形をした行が文字列／
/// コメントへ入ることを要する（実測時点の母集団に偽ヘッダは 1 件も無く、認識された
/// ヘッダはすべて実定義だった）。**タブ字下げも偽ヘッダの形だった**——`\t\t\t\tfn …` が
/// バイト差 4 で通っていた分は [`method_header`] の空白文字判定で塞いだが、**スペース
/// 字下げ 4 の偽ヘッダは残る**。
///
/// **(2) 整形器が出現そのものを消す形。** 照合は `line.contains(needle)` で**行内に閉じて
/// いる**ため、綴りが行をまたぐと出現が消える。`self.indexing()` を根とするメソッドチェーンが
/// 十分長いと rustfmt は `self` を単独の行へ落とし、次の行を `.indexing()` から始める——
/// 2026-08-17 に rustfmt へ直接与えて実測した（`&&` での折り返しや 2 連鎖では `self.indexing()`
/// は 1 行に保たれる。パス呼び出しの `read_config(` はチェーンではないのでこの形を取らない）。
/// **これは #1112 が入れた回帰ではない**——旧実装の `body.contains("self.indexing()")` も、
/// `body` が改行を含む文字列である以上まったく同じ折り返しで偽になる。
///
/// **列挙した 2 つのうち、人間の意図を要さないのは (2) である**——(1) はヘッダの形をした行を
/// 文字列やコメントへ書く人間を要するのに対し、(2) は整形器が勝手に作る（尽くしていない残余の
/// 側にも同じ性格のものが在りうる）。**その意味でこの検査は `cargo fmt` が実際に走ることに
/// 暗黙に依存している**——PostToolUse hook と PR CI の両方が現行の設定（`rustfmt.toml` を
/// 置いていないので `hard_tabs=false` を含む既定値）で走ることが、ここで扱う行の形を
/// 保っている。整形の設定を変えるときはこの検査の当たり方も一緒に見ること。
fn owners_of(src: &str, needle: &str) -> Vec<String> {
    let mut owners = Vec::new();
    let mut current: Option<&str> = None;
    for line in src.lines() {
        // 帰属先の更新を needle の照合より**先**に行う——ヘッダ行自身に出現がある場合は
        // そのメソッドへ帰属する（過剰発火＝赤側）。
        if let Some(header) = method_header(line) {
            current = Some(header);
        }
        if line.contains(needle) {
            owners.extend(current.map(str::to_string));
        }
    }
    owners
}

/// [`method_header`] が**字下げ 4 ちょうど**を要求することを固定する（#1112）。
///
/// **コーパスからは測れない。** `include_str!` が読む `activation.rs` には字下げ 0 / 8 の
/// `fn ` 行が 1 本も無く（分割の前後で不変・2026-08-27 に再測）、述語を `>= 4` へ緩めても
/// 字下げを見なくしても、認識される
/// ヘッダの集合が変わらない——2026-08-17 に鏡の実装で 3 通りを実測し、いずれも
/// ヘッダ数が同数で [`activation_uses_frame_values_not_live_reads`] は緑のままだった。
/// ゆえに合成 fixture でしか固定できない。
///
/// **`>= 4` への緩みを落とすのは字下げ 8 の行が `None` である assert だけではない**——
/// 帰属の側から [`owners_of_attributes_a_nested_fn_to_the_outer_method`] が独立に
/// もう 1 本持つ。字下げ 0 の行はその変異では落ちないが、字下げを見なくする変異を落とす。
///
/// **fixture の形は「幅」だけでは足りない。** 2026-08-17 の反証レビューが、字下げ幅の
/// 変異（`>= 4` へ緩める・見なくする）は落ちるのに、**受理する幅の集合を `{4, 12}` へ
/// 広げる**・**スペースの個数を数える形へ替える**・**先頭 4 スペースの prefix 判定へ
/// 替える**の 3 つが素通りすることを実測した。ゆえに下の 3 fixture を足してある——
/// 字下げ 12・タブ 4 個・スペース 4 + タブ 1 個。**どれが何を落とすかは各行のコメント**。
#[test]
fn method_header_requires_exactly_four_spaces_of_indent() {
    assert_eq!(
        method_header("    fn target(&self) {"),
        Some("fn target(&self) {")
    );
    // 入れ子の `fn`——`>= 4` へ緩めるとここが偽のヘッダになり、その中の禁止語は
    // 外側のメソッドではなく偽ヘッダへ帰属して緑へ落ちる。prefix 判定への変異も落とす。
    assert_eq!(method_header("        fn nested() {"), None);
    // トップレベルの `fn`——字下げを見なくする変異をここが落とす。
    assert_eq!(method_header("fn top_level() {"), None);
    assert_eq!(method_header("  fn odd() {"), None);
    // 字下げ 12——受理する幅の集合を `{4, 12}` へ広げる変異をここが落とす。字下げ 8 の
    // 行だけでは、8 を除いたまま 12 を足す形が通ってしまう。
    assert_eq!(method_header("            fn deep() {"), None);
    // タブ 4 個——`trim_start` のバイト差だけで数える形をここが落とす（タブもバイト 1 個
    // ずつ落ちるので差は 4 になる）。**スペースを数える変異はここでは落ちない**（数えると
    // 0 個なので、その変異も `None` を返す）。
    assert_eq!(method_header("\t\t\t\tfn tabbed() {"), None);
    // スペース 4 + タブ 1 個——スペースを数える変異をここが落とす（数えると 4 個なので
    // 受理へ倒れる）。現行の実装はバイト差が 5 になるので拒む。
    assert_eq!(method_header("    \tfn mixed() {"), None);
    assert_eq!(method_header("    let counted = fn_like();"), None);
}

/// [`method_header`] が **`fn` の前の可視性修飾と `async` を読み飛ばす**ことを固定する
/// （#1112）。現に `pub(super) ` が挟まる定義が起動の入口に在り、読み飛ばしが壊れると
/// [`activation_uses_frame_values_not_live_reads`] の入口が 1 本認識されなくなる。
#[test]
fn method_header_accepts_visibility_and_async_before_fn() {
    for line in [
        "    pub fn a(&self) {",
        "    pub(crate) fn b(&self) {",
        "    pub(super) fn c(&self) {",
        "    async fn d(&self) {",
        "    pub(crate) async fn e(&self) {",
        "    pub(super) async fn f(&self) {",
    ] {
        assert_eq!(method_header(line), Some(line.trim_start()), "{line}");
    }
}

/// [`owners_of`] が**入れ子の `fn` の中の出現を外側のメソッドへ帰属させる**ことを固定する
/// （#1112）。[`owners_of`] の doc が「境界の倒れ方」の 1 つ目として主張している挙動で、
/// それを成立させている実装事実は [`method_header`] の字下げ 4 ちょうどである。
///
/// 帰属先を**完全一致で**測る——`contains` で測ると、偽ヘッダへ横取りされた帰属が
/// 綴りの部分一致で通ってしまう形を作りやすい。
#[test]
fn owners_of_attributes_a_nested_fn_to_the_outer_method() {
    let src = "impl C {\n    fn outer(&self) {\n        fn nested() {\n            forbidden();\n        }\n        forbidden();\n    }\n    fn other(&self) {\n    }\n}\n";
    assert_eq!(
        owners_of(src, "forbidden("),
        vec![
            "fn outer(&self) {".to_string(),
            "fn outer(&self) {".to_string(),
        ]
    );
}

/// [`owners_of`] が**字下げ 4 のヘッダを持たない出現を落とす**ことを固定する（#1112）。
///
/// [`owners_of`] の doc が「最初のヘッダより前の出現は誰にも帰属せず無視される」と書く
/// 側の挙動である。トップレベル（字下げ 0）の `fn ` がヘッダに数えられないことも同時に
/// 固定する——数えられると、この fixture の 1 件目が帰属先を持って列挙へ現れる。
#[test]
fn owners_of_drops_occurrences_without_an_indent_four_owner() {
    let src = "fn top_level() {\n    forbidden();\n}\nimpl C {\n    fn outer(&self) {\n        forbidden();\n    }\n}\n";
    assert_eq!(
        owners_of(src, "forbidden("),
        vec!["fn outer(&self) {".to_string()]
    );
}

/// [`owners_of`] が改行コードに依存しないことを固定する（#1112）。
///
/// [`method_body`] と同じ処方である（`docs/development-principles.md`「検証の層と、層と層の隙間」——切り出しの helper 自身を LF / CRLF 両方の fixture で測る。#1077 の CI 実害から
/// 生えた条項で、あちらは終端の探索が CRLF の作業ツリーに一致せず母集団が壊れた）。
///
/// **帰属先を完全一致で測るのが要点である**——`contains` で測ると、`src.lines()` を
/// `src.split('\n')` へ替える変異が捕まらない。`trim_start` は行頭しか見ないので末尾の
/// `\r` はヘッダ文字列の中に残り、部分一致はそれでも通る（2026-08-17 に対照つきで実測）。
#[test]
fn owners_of_is_line_ending_agnostic() {
    let lf = "impl C {\n    fn outer(&self) {\n        forbidden();\n    }\n}\n";
    let crlf = lf.replace('\n', "\r\n");
    let expected = vec!["fn outer(&self) {".to_string()];
    for (label, src) in [("LF", lf), ("CRLF", crlf.as_str())] {
        assert_eq!(owners_of(src, "forbidden("), expected, "{label}");
    }
}

/// Enter の判定と表示ゲートが**同一フレームの同じ値**を見ることを固定する（#1077 / #1106）。
///
/// 対象は表示ゲートの入力 2 つである。`AppState.indexing` は `AtomicBool` の live-read で
/// **同一フレーム内でも変わりうる**。`visible_rows` は config の live-read で、
/// `config_watcher` の適用が同じフレームへ割り込みうる。**どちらも、起動の入口が自分で
/// 読み直すと `view.rs` が表示ゲートへ渡す値と食い違う**——「画面には出ていないが Enter は
/// 起動する」あるいはその逆が構築可能になる。値は `view.rs` が 1 回だけ読み、
/// [`FrameIndexing`] / [`FrameVisibleRows`] として配る。
///
/// **測れるのは構造だけである。** どちらの型にもテスト席が無く、食い違いの発生は
/// タイミング依存ゆえ決定的に再現できない。ゆえに「渡された値を使っていること」を
/// ソーステキストで固定する——読み直しの形が本体に無いことがその形である。
///
/// **この検査は母集団を切り出さない**（#1112）。禁止語の不在を測る否定形ゆえ、
/// [`method_body`] で切り出すと**canary より後・禁止語より前で切れたときに沈黙する**
/// （機序と境界規則は [`owners_of`] が正本）。代わりに `activation.rs` の全体から出現を
/// 列挙し、各出現を直前のメソッドヘッダへ**帰属**させて、起動の入口 3 本に帰属するものが
/// 1 つも無いことを測る。
///
/// **対象外へ落ちる経路は 2 通りあり、機序が違う。** 帰属で落ちる（母集団の中に在るが、
/// 帰属先が起動の入口ではない）ものと、**母集団の外に在って初めから見えない**ものである。
/// 後者は分割で生まれた——`run_search_with` の `indexing` の live-read（あちらは用途が違い、
/// 行をクリアするかを到達経路ごとに判断するのが正しい）と `lang()` の `read_config` は
/// `search_flow.rs` と `launcher_controller.rs` に在り、**この検査はもう見ていない**。
/// **受け入れ条件はどちらの規則も「起動の入口が自分の中で読み直さない」ことであって、
/// 対象外の件数ではない**——件数を書くと、正当な読みを 1 つ足すたびにこの散文だけが黙って
/// 腐る（#1076 で `read_config` を使うヘルパーが増えたときに実際に腐った）。
///
/// **帰属は間接で抜ける。** 起動の入口がヘルパーを呼び、そのヘルパーが `read_config` を
/// 呼ぶ形は、何段挟まっていても緑のまま通る（`instant_prefix` や `resolve_tools` を経る形が
/// 実在する）。**現時点で欠陥ではない**——どれも `visible_rows` を読み直しておらず、読み自体は
/// #1076 の移行より前から在ったものである。**この検査が塞ぐのは
/// 「起動の入口が自分の中で読み直す」形だけだと読むこと**、そして**ヘルパーの本体を入口へ
/// インライン展開しないこと**（展開した瞬間に帰属が入口へ移り、この検査は赤になる）。
///
/// **この検査は禁止語と needle を自分のソースへリテラルで綴るが、それは母集団の外に在る。**
/// 母集団は production 1 枚（`activation.rs`）で、この `mod tests` は別ファイルだからである
/// ——**それが構造で保証されていた時期と、帰属の副作用で緑だった時期がこの検査にはある**
/// （#1112 で母集団をファイル全体へ広げたとき、テスト側のリテラルは自分のテスト関数の
/// ヘッダへ帰属することで通っていた。分割で母集団が production だけになり構造へ戻った）。
/// **帰属の濾過が要らなくなったわけではない**——母集団には起動の入口の外で `read_config(` を
/// 正当に呼ぶ production の行が在り、それを通しているのは今も帰属である（どこが該当するかは
/// 数えない。母集団は `grep 'read_config(' activation.rs` が持つ）。
///
/// **禁止語の中には、母集団での出現が今 0 件のものもある。それでも検知器は空虚ではない**
/// ——塞いでいるのは「入口が読み直す形が**足された**とき」であって現存の出現ではない。
/// 発火することは変異注入で実測した（`on_enter` へ `self.indexing()` を 1 行挿すと赤になる）。
///
/// **母集団は `activation.rs` 1 枚である。** 起動の入口 3 本がそこに在ることは冒頭の
/// ヘッダ assert が測るので、入口が別の子モジュールへ移れば**沈黙ではなく赤になる**。
/// そのとき直すのは母集団であって assert ではない（`activation.rs` の `//!` が正本）。
#[test]
fn activation_uses_frame_values_not_live_reads() {
    let src = include_str!("../activation.rs");
    let entry_points = [
        "fn on_enter(",
        "fn activate_or_execute(",
        "fn shift_activate(",
    ];
    // **canary の代役はここである。** 切り出しを無くしたので「母集団が空」は起こりえないが、
    // 「対象が 1 つも認識されていない」なら検査は同じように沈黙する。3 本のアンカーは
    // 可視性修飾の有無で 2 形（`pub(super) fn` / 素の `fn`）に分かれるので、この assert は
    // 改名だけでなく [`method_header`] の修飾読み飛ばしが壊れた場合にも赤になる。
    // **消さないこと**——これが沈黙を塞いでいる唯一の assert である。
    let headers: Vec<&str> = src.lines().filter_map(method_header).collect();
    for anchor in entry_points {
        assert!(
            headers.iter().any(|header| header.contains(anchor)),
            "{anchor} が字下げ 4 のメソッドヘッダとして見つからない——改名したかヘッダの\
             認識が壊れており、以下の検査は 1 つも発火しない（沈黙する検知器は検知器ではない）"
        );
    }
    for owner in owners_of(src, "self.indexing()") {
        for anchor in entry_points {
            assert!(
                !owner.contains(anchor),
                "{anchor} が `indexing` を自分で読み直している——`view.rs` が表示ゲートへ渡す値と\
                 同一フレーム内で食い違いうる（#1077）。引数で受けた FrameIndexing を使うこと"
            );
        }
    }
    // 連言④も同じ形で守る（#1106）。**構築子が private なので偽の値は作れない**——
    // 残る一手が「本物をもう 1 回読む」ことであり、それをここで塞ぐ。読み直す形は
    // `read_visible_rows` の直呼びと、`read_config` から `effective_visible_rows` を
    // 引く形の 2 つである（後者は `lang()` が同じ関数を正当に使うので、起動の入口へ
    // 帰属する出現だけを見るこの検査でしか禁止にできない）。
    for forbidden in ["read_visible_rows(", "read_config("] {
        for owner in owners_of(src, forbidden) {
            for anchor in entry_points {
                assert!(
                    !owner.contains(anchor),
                    "{anchor} が `{forbidden}` を呼んでいる——**起動の入口での config 読みは\
                     `visible_rows` の読み直しと区別できない**（無関係な読みでもここは落ちる。\
                     それでよい: 読み直しなら `view.rs` が表示ゲートへ渡す値と同一フレーム内で\
                     食い違い、#1106 の症状が再発する）。連言④は引数で受けた FrameVisibleRows で\
                     判定し、他の config 値が要るならこの入口の外で読むこと"
                );
            }
        }
    }
}

/// 起動の入口が §4.7 の表示ゲートを見ていることを、**ソーステキストで**固定する（#1077）。
///
/// **述語のテストでは呼び出し点の脱落を捕まえられない。** `plain_results_hidden` 自身は
/// [`crate::egui_shell::search_state`] の `mod tests` が固定しているが、それは
/// 「述語がどんな値を返すか」しか測らず、**入口がその述語を呼んでいるか**は測らない。
/// この型にはテスト席が無い——`launcher_controller` に `mod tests` が無かったのは
/// [`LauncherController`] の構築が `AppHandle` と engine lock を要求するためで、
/// **ソーステキスト検査はそのどちらも要らない**（`indexing.rs` の
/// `start_index_build_invalidates_the_icon_cache` と同じ形）。
///
/// **これが落ちたとき失うもの**: index 再構築中は §4.7 の表示ゲートが通常結果を隠すが、
/// 行データは保持される（「データと選択は保持——クリアしない」）。入口がゲートを見なくなると、
/// **画面に 1 行も出ていない状態の Enter / クリック / Shift+Enter が古い行を起動する**。
/// 2026-08-16 に実機で再現済みで、行は正しく出るため挙動テストでは捕まらない。
///
/// **見るべきゲートは 2 つあり、独立である**（#1106 で④を足した）。③（`plain_results_hidden`）は
/// index 再構築中の Results ビューの通常結果だけを隠すが、④（`results_area_collapsed`）は
/// 最大表示件数そのものが 0 なので tool 選択・instant 行・フォルダ展開を含む**すべてのビュー**が
/// 1 行も出ない。**片方を見ているだけでは足りない**——④の症状も 2026-08-16 に実機で再現した
/// （`visible_rows = 0` で `egui_results:show` が 0 件のまま `egui_launch` が出た）。
///
/// **残る死角**: 母集団は当該メソッドのソーステキストだけであり、呼び出しグラフは辿らない。
/// **ゲートをこのメソッドの外のヘルパーへ移すこと自体は、この検査が赤にする**——本体から
/// `plain_results_hidden(` / `results_area_collapsed(` の綴りが消えるためである（同じ機序を
/// #1108 で実測した）。**ただし測っているのは本体テキストへの部分文字列一致であって呼び出しでは
/// ない**——移設後も本体にその綴りが残れば緑のまま通る（移し先の名前がそれを含む場合も、
/// 説明コメントへ書き残した場合も同じ）。捕まらないのは、**移した先でゲートが落ちる**退行の
/// 方である。
#[test]
fn activation_entry_points_consult_the_display_gate() {
    let src = include_str!("../activation.rs");
    // (アンカー, 母集団が空でないことを示す目印)
    let targets = [
        ("fn activate_or_execute(", "execute_tool_selected("),
        ("fn shift_activate(", "folder_load_pending("),
    ];
    for (anchor, canary) in targets {
        let body = method_body(src, anchor, canary);
        assert!(
            body.contains("plain_results_hidden("),
            "{anchor} が §4.7 の表示ゲート（連言③）を見ていない——index 再構築中に\
             画面から消えた行を Enter / クリック / Shift+Enter が起動する（#1077 で実機再現済み）"
        );
        assert!(
            body.contains("results_area_collapsed("),
            "{anchor} が §4.5 の表示ゲート（連言④）を見ていない——最大表示件数が 0 で\
             1 行も描かれていない状態を Enter / クリック / Shift+Enter が起動する\
             （#1106 で実機再現済み）"
        );
    }
}

/// `on_enter` が flush 判定を**述語へ委ねている**ことを、ソーステキストで固定する（#1112）。
///
/// **述語のテストでは呼び出し点の脱落を捕まえられない**（この規範の正本は上の
/// [`activation_entry_points_consult_the_display_gate`] の doc）。`should_flush_on_enter`
/// を綴る production はこの呼び出しの 1 行だけで、述語自身のテストは
/// [`crate::egui_shell::search_state`] の `mod tests` に在る——呼び出しを外しても
/// あちらは緑のままである（2026-08-17 に対照つきで実測）。
///
/// **`on_enter` を上の `targets` へ足す形は採らない。** あちらが当てる 2 つのゲート
/// （`plain_results_hidden` / `results_area_collapsed`）を `on_enter` の本体は持たず、
/// 持つ場所でもない——ゲートを見るのは委譲先の `activate_or_execute` / `shift_activate`
/// で、両方とも既に `targets` に在る。固定したい不変条件が別物なのでテストを分ける。
///
/// **これは存在形の assert である**——母集団が途中で切れれば綴りごと消えて赤になるので、
/// [`owners_of`] が塞いだ否定形の沈黙はここには当たらない。**ただし赤になるのは、探す綴りが
/// canary（`self.activate_or_execute(`）より前に在る現在の並びにおいてである**——切り詰めが
/// 綴りより後・canary より前で起きれば canary が落ちるが、綴りより前で起きれば綴りが落ちる。
/// **綴りを canary より後ろへ動かすと、その間で切れた母集団は canary を通しつつ綴りを
/// 捨てる**（否定形の沈黙と同じ機序が、極性を変えて存在形に当たる形）。並びを変えるなら
/// canary も動かすこと。
///
/// # 何を保証し、何を保証しないか
///
/// **保証するのは 1 つだけである**——`if crate::egui_shell::should_flush_on_enter(` という
/// 綴りが `on_enter` の本体テキストに現れること。#631 の flush 判定が丸ごと落ちる形（当該の
/// 行が消える）はこれで赤になる。
///
/// **保証しないもの**（2026-08-17 の反証レビューが実測した経路。**少なくとも次を含む**
/// ——尽くしてはいない）:
/// - **テキストであって呼び出しではない。** 同じ綴りを説明コメントや文字列リテラルへ
///   書き残せば緑で通る。パスまで含む長い綴りなので偶然そう書く形ではないが、機構としては
///   何も止めていない
/// - **呼び出しが在ることは委譲が在ることを意味しない。** これは上の「部分文字列一致」の
///   特殊例ではなく**別種の欠落**である——`let _ = crate::egui_shell::should_flush_on_enter(…);`
///   と書けば**本物の述語への本物の呼び出しが残ったまま**判定は本体の書き下ろしへ移る。
///   `rustc -D warnings` も `clippy -D warnings -W pedantic` も exit 0 で通る（実測）
///
/// **綴りを長くして塞いだもの**: 同名のクロージャで影を作る形・別レシーバの同名メソッドへ
/// 差し替える形・上の `let _ =` の形は、`if ` と `crate::egui_shell::` を綴りへ含めたことで
/// 落ちるようになった（対照つきで実測——短い綴り `should_flush_on_enter(` では 3 つとも緑）。
/// **述語の道具立ては増やしていない**——増えたのは探す文字列の長さだけである。
///
/// **代償は整形と書き方への脆さで、向きは赤側である**: `let should = crate::egui_shell::…;` と
/// `if should {` へ割る分解や、rustfmt が `if` とパスのあいだで折り返す形はここを赤にする。
/// 偽陽性であって沈黙ではないので受容する（直すなら綴りを短くするのではなく、当該の並びへ
/// 合わせて綴りを更新すること）。
#[test]
fn on_enter_delegates_the_flush_decision_to_the_predicate() {
    let src = include_str!("../activation.rs");
    let body = method_body(src, "fn on_enter(", "self.activate_or_execute(");
    assert!(
        body.contains("if crate::egui_shell::should_flush_on_enter("),
        "on_enter が `should_flush_on_enter` を分岐の条件式として呼んでいない——#631 の\
         flush-on-Enter が判定ごと落ちたか、判定の写しが本体へ書き下ろされている（呼び出し\
         だけ残して判定から外す形も含む）。どちらも述語側のテストは緑のまま通る（最終クエリの\
         結果が行へ反映される前の Enter が、leading 時点の結果や連打前のクエリの結果で\
         起動しうる）。**整形や分解でこの綴りが崩れただけの偽陽性もありうる**——その場合は\
         綴りを短くせず、現在の並びへ合わせて更新すること"
    );
}
