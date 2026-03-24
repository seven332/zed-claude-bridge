use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectionInput {
    pub text: String,
    pub file_path: String,
    pub row: u32,
    pub column: u32,
    pub language: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectionState {
    pub text: String,
    pub file_path: String,
    pub file_url: String,
    pub language: String,
    pub selection: SelectionRange,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectionRange {
    pub start: Position,
    pub end: Position,
    pub is_empty: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    pub line: u32,
    pub character: u32,
}
