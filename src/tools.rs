use std::sync::{Arc, LazyLock};

use rmcp::model::{
    CallToolResult, Content, Implementation, ListToolsResult, ServerInfo, Tool,
};
use rmcp::service::RequestContext;
use rmcp::{ErrorData, RoleServer, ServerHandler};

use crate::state::EditorState;
use crate::types::SelectionState;

pub struct BridgeHandler {
    pub state: Arc<EditorState>,
}

const GET_CURRENT_SELECTION: &str = "getCurrentSelection";
const GET_LATEST_SELECTION: &str = "getLatestSelection";
const GET_WORKSPACE_FOLDERS: &str = "getWorkspaceFolders";

const TOOL_DEFS: &[(&str, &str)] = &[
    (GET_CURRENT_SELECTION, "Get the current text selection in the active editor"),
    (GET_LATEST_SELECTION, "Get the most recent text selection"),
    (GET_WORKSPACE_FOLDERS, "Get all workspace folders open in the IDE"),
];

static EMPTY_SCHEMA: LazyLock<Arc<serde_json::Map<String, serde_json::Value>>> =
    LazyLock::new(|| {
        let obj: serde_json::Map<String, serde_json::Value> = serde_json::from_value(
            serde_json::json!({"type": "object", "properties": {}}),
        )
        .unwrap();
        Arc::new(obj)
    });

fn make_tool(name: &'static str, description: &'static str) -> Tool {
    Tool::new(name, description, EMPTY_SCHEMA.clone())
}

fn selection_to_json(sel: Option<&SelectionState>, no_data_message: &str) -> serde_json::Value {
    match sel {
        Some(s) => serde_json::json!({
            "success": true,
            "text": s.text,
            "filePath": s.file_path,
            "fileUrl": s.file_url,
            "selection": s.selection,
        }),
        None => serde_json::json!({
            "success": false,
            "message": no_data_message,
        }),
    }
}

impl ServerHandler for BridgeHandler {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::default()
            .with_server_info(Implementation::new("zed-claude-bridge", "0.1.0"))
    }

    fn list_tools(
        &self,
        _request: Option<rmcp::model::PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<ListToolsResult, ErrorData>> + Send + '_ {
        async {
            Ok(ListToolsResult {
                tools: TOOL_DEFS.iter().map(|(n, d)| make_tool(n, d)).collect(),
                next_cursor: None,
                meta: None,
            })
        }
    }

    fn call_tool(
        &self,
        request: rmcp::model::CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<CallToolResult, ErrorData>> + Send + '_ {
        async move {
            let name: &str = &request.name;
            match name {
                GET_CURRENT_SELECTION => {
                    let sel = self.state.current_selection.read().await;
                    let result = selection_to_json(sel.as_ref(), "No active editor found");
                    Ok(CallToolResult::success(vec![Content::text(result.to_string())]))
                }
                GET_LATEST_SELECTION => {
                    let sel = self.state.latest_selection.read().await;
                    let result = selection_to_json(sel.as_ref(), "No selection available");
                    Ok(CallToolResult::success(vec![Content::text(result.to_string())]))
                }
                GET_WORKSPACE_FOLDERS => {
                    let result = self.state.get_workspace_folders_response();
                    Ok(CallToolResult::success(vec![Content::text(result.to_string())]))
                }
                _ => Err(ErrorData::method_not_found::<rmcp::model::CallToolRequestMethod>()),
            }
        }
    }

    fn get_tool(&self, name: &str) -> Option<Tool> {
        TOOL_DEFS.iter().find(|(n, _)| *n == name).map(|(n, d)| make_tool(n, d))
    }
}
