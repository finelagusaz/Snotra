//! パスキーの正規化（小文字化 + `/` → `\`）と、その末尾セグメント版。
//!
//! **規則の定義はこのモジュールに閉じる。** 記録側（`super::scan` の重複排除・`crate::history`）と
//! 照合側（`crate::search`）が同じ関数を通ることがバイト一致の根拠であり、畳み込み比較を別実装で
//! 書き起こしてはならない。冪等性の契約と依存する 3 モジュールの一覧は
//! `snotra-core/CLAUDE.md`「`normalize_entry_key` の冪等性契約」。

pub fn normalize_entry_key(path: &str) -> String {
    let mut normalized = String::with_capacity(path.len());
    normalize_entry_key_into(&mut normalized, path);
    normalized
}

/// [`normalize_entry_key`] を**確保済みバッファへ**書き出す。`buf` の中身は捨てられる。
///
/// 検索ホットパスは `normalized_key` を索引に持たず、ここで毎回導出する。呼び出し側は
/// スレッドローカルのバッファを使い回すので、暖まったあとの確保は起きない。
///
/// **規則の定義はこの関数 1 つである。** `normalize_entry_key` はこれの薄い包みであり、
/// 記録時（`indexer` の重複排除キー・`history` の全キー）と照合時（`search`）が同じ規則を
/// 通ることがバイト一致の根拠になる。**畳み込み比較を別実装で書き起こしてはならない**
/// ——1 バイトずれると履歴照合が沈黙で外れる（クラッシュせず検索結果も返り、
/// ブーストだけが効かなくなるので気づく手段が無い）。
///
/// ASCII 高速路を持つ。**ASCII 範囲では Unicode 小文字化と ASCII 小文字化の結果が一致する**
/// ため分岐しても結果は変わらず、実運用点（312,377 パス・非 ASCII 混じりは 1.7%）で
/// 全件一致を実測してある。支配項は `char::to_lowercase()` のテーブル参照で、
/// 高速路の有無でパスクエリ全走査が 9.2-11.7 倍から 1.7-2.3 倍へ変わる
/// （`PERFORMANCE.md`「パスクエリ全走査のコスト — `normalized_keys` を保持するか導出するか」）。
pub fn normalize_entry_key_into(buf: &mut String, path: &str) {
    buf.clear();
    let trimmed = path.trim();
    buf.reserve(trimmed.len());
    if trimmed.is_ascii() {
        // 一括で動かす。1 文字ずつ `push` すると `String: Extend<char>` が毎回 UTF-8 符号化の
        // 分岐を通り、実測で 2.5-3 倍遅くなる（`unsafe` なしでこの速度を出すのが要点）。
        // `/` は Windows パスにまず現れず（スキャナが `\` で組む）、`find` は即 `None` を返して
        // `push_str` 1 回の memcpy に落ちる。
        let mut rest = trimmed;
        while let Some(pos) = rest.find('/') {
            buf.push_str(&rest[..pos]);
            buf.push('\\');
            rest = &rest[pos + 1..];
        }
        buf.push_str(rest);
        // `buf` は先頭で空にしてあるので全体が今回の書き込みぶんである。
        buf.make_ascii_lowercase();
        return;
    }
    for ch in trimmed.chars() {
        if ch == '/' {
            buf.push('\\');
        } else {
            buf.extend(ch.to_lowercase());
        }
    }
}

/// `target_path` の**末尾セグメントだけ**を [`normalize_entry_key_into`] で正規化して
/// `buf` へ書く。`buf` の中身は捨てられる。
///
/// [`super::path_env::scan_path_dirs`] の事前フィルタ専用。**照合する両辺は必ずこの 1 つを通すこと**——
/// `normalize_entry_key_into` と同じ理屈で、同じ手順を通ることだけが一致の根拠になる。
///
/// **これは篩であって判定ではない。** 正規化は「全体 `trim` → 文字単位の写像（小文字化と
/// `/` → `\`）」であり、写像は新たな `\` を生まないのでセグメント境界を保つ。ゆえに
/// `normalize_entry_key(a) == normalize_entry_key(b)` ならこのキーも必ず一致する
/// （＝**偽陰性を出さない**）。逆は成り立たない——別ディレクトリの同名ファイルが
/// 通り抜けるので、通した候補はフルパスの正規化キーで確かめること。
///
/// **論証が乗っている前提を 2 つ名指しする。どちらを触っても、この篩だけが静かに偽陰性を
/// 出す。**
///
/// 1. **ASCII 高速路の両分岐が ASCII 入力で一致すること。** [`normalize_entry_key_into`] は
///    高速路を持ち、フルパスとその末尾セグメントは**別の分岐を通りうる**（フルパスに非 ASCII
///    が混じり、ファイル名だけ ASCII の場合）。一致の根拠は同関数の doc が持ち、実インデックス
///    の全パスでの一致を `tests/path_query_cost.rs` の
///    `derives_same_bytes_as_normalize_entry_key` が固定する。
/// 2. **小文字化が空白を作らず消さないこと。** この関数はパス全体を `trim` してから
///    セグメントを切り出し、[`normalize_entry_key_into`] が**そのセグメントをもう一度
///    `trim` する**。区切りの直後に空白があるパス（`C:\dir\ tool.exe`）では、
///    「正規化してから切り出す」と「切り出してから正規化する」が一致するために、
///    写像と `trim` が可換であることが要る。
pub(crate) fn normalize_file_name_key_into(buf: &mut String, target_path: &str) {
    let trimmed = target_path.trim();
    let segment = match trimmed.rfind(['\\', '/']) {
        // 区切りはどちらも 1 バイトゆえ `i + 1` は char 境界。
        Some(i) => &trimmed[i + 1..],
        None => trimmed,
    };
    normalize_entry_key_into(buf, segment);
}
