// `private_intra_doc_links` は「フラグ無しで建てた doc では private 項目が出ないので link が
// 死ぬ」という警告だが、このリポジトリの doc 建ては `ci.yml` の
// `cargo doc --workspace --no-deps --document-private-items` 固定であり、private への link は
// 実際に解決する（crate は publish せず docs.rs 建ても無いので、フラグ無しで建てる消費者は居ない）。
// **リンク切れの検査は弱まらない**——存在しないシンボルへの link は `broken_intra_doc_links = "deny"`
// が引き続き捕まえる（変異注入で exit 101 を実測）。
//
// **ルート `[workspace.lints.rustdoc]` へ書いてはならない。** あの節は G-workspace-lints が
// 「全エントリが deny/forbid」の ∀ で見張っており、allow を 1 行足すと deny の行を残したまま
// 検査が赤くなる（#713 の設計。実測: governance:check / node-check / rust-check の 3 job が落ちた）。
// 抑止の射程を「この crate のこの lint だけ」に閉じるのが、あの ∀ と両立する唯一の置き場である。
#![allow(rustdoc::private_intra_doc_links)]

pub mod binfmt;
pub mod config;
pub mod engine;
pub mod error;
pub mod folder;
pub mod history;
pub mod hotkey;
pub mod index_tree;
pub mod indexer;
pub mod instant;
pub mod opener;
pub mod query;
pub mod search;
pub mod ui_types;
pub mod window_data;
