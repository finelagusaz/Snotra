//! `indexer` のテストが共有する治具——`index.bin` を書くテストの直列化ガードと、プロセスごとに
//! 一意な作業ディレクトリ。
//!
//! **どちらも定義はここ 1 か所である。** ガードを写して 2 つの `static` にすると相互排除が消え、
//! 「ロック空き」を期待するテストと「ロック保持中」を作るテストが食い合う。

use std::fs;
use std::sync::Mutex;

/// `INDEX_WRITE_LOCK` に触れるテストを直列化するガード。
/// `cargo test` は同一バイナリのテストを並列実行するため、「ロック空き」を
/// 期待するテストと「ロック保持中」を作るテストが食い合わないよう、
/// これらのテストは先頭でこのガードを取得する。
pub(super) static INDEX_LOCK_TEST_GUARD: Mutex<()> = Mutex::new(());

/// テスト用の作業ディレクトリを作り直して返す。
///
/// 名前には `tag`（プロセス内の一意性）に加えて **`std::process::id()`** を含める。
/// `INDEX_WRITE_LOCK` はプロセス内の `static Mutex` ゆえ、テストバイナリが複数
/// プロセスに分かれる状況（`cargo test` と `cargo test --release` の重なり・
/// temp root を共有する 2 ジョブ・別 worktree での並行実行）では効かない。
/// pid を落とすと、片方の `remove_dir_all` がもう片方の `create_dir_all` や
/// `index.bin.tmp` の書き込みに割り込み、コード変更と無関係な panic になる（#978）。
pub(super) fn temp_dir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("snotra_idx_test_{}-{}", tag, std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

#[test]
fn temp_dir_name_contains_process_id() {
    let dir = temp_dir("process_unique");
    let name = dir
        .file_name()
        .and_then(|n| n.to_str())
        .expect("temp dir name");
    assert_eq!(
        name,
        format!("snotra_idx_test_process_unique-{}", std::process::id()),
        "作業ディレクトリ名に自プロセスの pid が入っていない（#978）"
    );
    let _ = fs::remove_dir_all(&dir);
}
