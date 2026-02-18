use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult {
    pub name: String,
    pub path: String,
    pub is_folder: bool,
    pub is_error: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderExpansionState {
    pub current_dir: String,
    pub saved_results: Vec<SearchResult>,
    pub saved_selected: usize,
    pub saved_query: String,
}
