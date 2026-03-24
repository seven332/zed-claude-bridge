use std::path::Path;
use tokio::sync::{broadcast, RwLock};

use crate::types::{Position, SelectionInput, SelectionRange, SelectionState};

pub struct EditorState {
    pub current_selection: RwLock<Option<SelectionState>>,
    pub latest_selection: RwLock<Option<SelectionState>>,
    pub workspace_folders: Vec<String>,
    pub selection_tx: broadcast::Sender<SelectionState>,
}

impl EditorState {
    pub fn new(workspace_folders: Vec<String>) -> Self {
        let (tx, _) = broadcast::channel(16);
        Self {
            current_selection: RwLock::new(None),
            latest_selection: RwLock::new(None),
            workspace_folders,
            selection_tx: tx,
        }
    }

    pub async fn update_selection(&self, input: SelectionInput) {
        let start_line = input.row.saturating_sub(1);
        let start_char = input.column.saturating_sub(1);
        let lines: Vec<&str> = input.text.split('\n').collect();
        let end_line = start_line + (lines.len() as u32).saturating_sub(1);
        let end_char = if lines.len() == 1 {
            start_char + input.text.chars().count() as u32
        } else {
            lines.last().map_or(0, |l| l.chars().count() as u32)
        };

        let state = SelectionState {
            text: input.text.clone(),
            file_path: input.file_path.clone(),
            file_url: format!("file://{}", input.file_path),
            language: input.language,
            selection: SelectionRange {
                start: Position {
                    line: start_line,
                    character: start_char,
                },
                end: Position {
                    line: end_line,
                    character: end_char,
                },
                is_empty: input.text.is_empty(),
            },
        };

        // Hold both write locks simultaneously to prevent concurrent updates
        // from interleaving and causing current/latest to diverge.
        // Lock order: current then latest (consistent everywhere, no deadlock).
        let mut current = self.current_selection.write().await;
        let mut latest = self.latest_selection.write().await;
        *current = Some(state.clone());
        *latest = Some(state.clone());
        let _ = self.selection_tx.send(state);
    }

    pub fn get_workspace_folders_response(&self) -> serde_json::Value {
        let folders: Vec<serde_json::Value> = self
            .workspace_folders
            .iter()
            .enumerate()
            .map(|(i, folder)| {
                let name = Path::new(folder)
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                serde_json::json!({
                    "name": name,
                    "uri": format!("file://{}", folder),
                    "path": folder,
                    "index": i,
                })
            })
            .collect();

        serde_json::json!({
            "success": true,
            "folders": folders,
            "rootPath": self.workspace_folders.first(),
            "workspaceFile": null,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn starts_with_null_selections() {
        let state = EditorState::new(vec!["/tmp".into()]);
        assert!(state.current_selection.read().await.is_none());
        assert!(state.latest_selection.read().await.is_none());
    }

    #[tokio::test]
    async fn updates_selection_from_input() {
        let state = EditorState::new(vec!["/tmp".into()]);
        state
            .update_selection(SelectionInput {
                text: "const x = 1;".into(),
                file_path: "/tmp/test.ts".into(),
                row: 5,
                column: 3,
                language: "typescript".into(),
            })
            .await;

        let sel = state.current_selection.read().await;
        let sel = sel.as_ref().unwrap();
        assert_eq!(sel.text, "const x = 1;");
        assert_eq!(sel.file_path, "/tmp/test.ts");
        assert_eq!(sel.file_url, "file:///tmp/test.ts");
        assert_eq!(sel.language, "typescript");
        assert_eq!(sel.selection.start.line, 4);
        assert_eq!(sel.selection.start.character, 2);
        assert_eq!(sel.selection.end.line, 4);
        assert_eq!(sel.selection.end.character, 14);
        assert!(!sel.selection.is_empty);
    }

    #[tokio::test]
    async fn converts_multi_line_selection() {
        let state = EditorState::new(vec!["/tmp".into()]);
        state
            .update_selection(SelectionInput {
                text: "line1\nline2\nline3".into(),
                file_path: "/tmp/test.ts".into(),
                row: 10,
                column: 1,
                language: "typescript".into(),
            })
            .await;

        let sel = state.current_selection.read().await;
        let sel = sel.as_ref().unwrap();
        assert_eq!(sel.selection.start.line, 9);
        assert_eq!(sel.selection.start.character, 0);
        assert_eq!(sel.selection.end.line, 11);
        assert_eq!(sel.selection.end.character, 5);
    }

    #[tokio::test]
    async fn handles_empty_selection() {
        let state = EditorState::new(vec!["/tmp".into()]);
        state
            .update_selection(SelectionInput {
                text: "".into(),
                file_path: "/tmp/test.ts".into(),
                row: 1,
                column: 1,
                language: "typescript".into(),
            })
            .await;

        let sel = state.current_selection.read().await;
        assert!(sel.as_ref().unwrap().selection.is_empty);
    }

    #[tokio::test]
    async fn emits_selection_event() {
        let state = EditorState::new(vec!["/tmp".into()]);
        let mut rx = state.selection_tx.subscribe();

        state
            .update_selection(SelectionInput {
                text: "hello".into(),
                file_path: "/tmp/test.ts".into(),
                row: 1,
                column: 1,
                language: "typescript".into(),
            })
            .await;

        let received = rx.recv().await.unwrap();
        assert_eq!(received.text, "hello");
    }

    #[test]
    fn returns_workspace_folders_response() {
        let state = EditorState::new(vec![
            "/home/user/project".into(),
            "/home/user/lib".into(),
        ]);
        let res = state.get_workspace_folders_response();

        assert_eq!(res["success"], true);
        assert_eq!(res["folders"][0]["name"], "project");
        assert_eq!(res["folders"][0]["uri"], "file:///home/user/project");
        assert_eq!(res["folders"][0]["path"], "/home/user/project");
        assert_eq!(res["folders"][0]["index"], 0);
        assert_eq!(res["folders"].as_array().unwrap().len(), 2);
        assert_eq!(res["rootPath"], "/home/user/project");
        assert!(res["workspaceFile"].is_null());
    }
}
