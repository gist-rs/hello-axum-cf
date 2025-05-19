use crate::types::{
    AddObservationItem,
    AddObservationsPayload,
    CreateEntitiesPayload,
    CreateRelationsPayload,
    DeleteEntitiesPayload,
    DeleteObservationItem,
    DeleteObservationsPayload,
    DeleteRelationsPayload,
    Edge as DoEdge, // For deserializing DO responses if needed for create_*
    EntityToCreate,
    KnowledgeGraphDataResponse,
    Node as DoNode,
    OpenNodesQuery,
    RelationToCreate,
    RelationToDelete,
    SearchNodesQuery,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use worker::{Headers, Method, Request as WorkerRequest, RequestInit, Response, Result, Stub};

// --- MCP Request/Response Structures ---

#[derive(Serialize, Deserialize, Debug)]
pub struct ToolInputSchema {
    #[serde(rename = "type")]
    pub schema_type: String,
    pub properties: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<Vec<String>>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    #[serde(rename = "inputSchema")]
    pub input_schema: Value, // Using Value for flexibility with complex schemas
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ListToolsResponse {
    pub tools: Vec<ToolDefinition>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct CallToolRequestParams {
    pub name: String,
    #[serde(default)]
    pub arguments: Value,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ContentBlock {
    #[serde(rename = "type")]
    pub block_type: String, // "text"
    pub text: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct CallToolResponse {
    pub content: Vec<ContentBlock>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct McpError {
    // Using string for code to match some potential MCP patterns, can be int
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct McpErrorResponse {
    pub error: McpError,
}

fn mcp_error_response(code: &str, message: &str) -> Response {
    Response::from_json(&McpErrorResponse {
        error: McpError {
            code: code.to_string(),
            message: message.to_string(),
            data: None,
        },
    })
    .unwrap()
    .with_status(400) // Default to 400 for tool errors
}

// --- Argument Structs for MCP Tool Calls (matching TS version schemas) ---

#[derive(Deserialize, Debug)]
struct McpEntityToCreate {
    name: String,
    #[serde(rename = "entityType")]
    entity_type: String,
    #[serde(default)]
    observations: Vec<String>,
}

#[derive(Deserialize, Debug)]
struct McpCreateEntitiesArgs {
    entities: Vec<McpEntityToCreate>,
}

#[derive(Deserialize, Debug)]
struct McpRelationToCreate {
    from: String,
    to: String,
    #[serde(rename = "relationType")]
    relation_type: String,
}

#[derive(Deserialize, Debug)]
struct McpCreateRelationsArgs {
    relations: Vec<McpRelationToCreate>,
}

#[derive(Deserialize, Debug)]
struct McpAddObservationItemArgs {
    #[serde(rename = "entityName")]
    entity_name: String,
    contents: Vec<String>,
}
#[derive(Deserialize, Debug)]
struct McpAddObservationsArgs {
    observations: Vec<McpAddObservationItemArgs>,
}

#[derive(Deserialize, Debug)]
struct McpDeleteEntitiesArgs {
    #[serde(rename = "entityNames")]
    entity_names: Vec<String>,
}

#[derive(Deserialize, Debug)]
struct McpDeleteObservationItemArgs {
    #[serde(rename = "entityName")]
    entity_name: String,
    observations: Vec<String>,
}
#[derive(Deserialize, Debug)]
struct McpDeleteObservationsArgs {
    deletions: Vec<McpDeleteObservationItemArgs>,
}

#[derive(Deserialize, Debug)]
struct McpDeleteRelationItemArgs {
    from: String,
    to: String,
    #[serde(rename = "relationType")]
    relation_type: String,
}
#[derive(Deserialize, Debug)]
struct McpDeleteRelationsArgs {
    relations: Vec<McpDeleteRelationItemArgs>,
}

#[derive(Deserialize, Debug)]
struct McpSearchNodesArgs {
    query: String,
}

#[derive(Deserialize, Debug)]
struct McpOpenNodesArgs {
    names: Vec<String>,
}

// --- Tool Schemas (as string literals) ---
mod schemas {
    pub const CREATE_ENTITIES_SCHEMA: &str = r#"{
        "type": "object",
        "properties": {
            "entities": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "name": { "type": "string", "description": "The name of the entity" },
                        "entityType": { "type": "string", "description": "The type of the entity" },
                        "observations": { "type": "array", "items": { "type": "string" }, "description": "An array of observation contents associated with the entity" }
                    },
                    "required": ["name", "entityType", "observations"]
                }
            }
        },
        "required": ["entities"]
    }"#;

    pub const CREATE_RELATIONS_SCHEMA: &str = r#"{
        "type": "object",
        "properties": {
            "relations": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "from": { "type": "string", "description": "The name of the entity where the relation starts" },
                        "to": { "type": "string", "description": "The name of the entity where the relation ends" },
                        "relationType": { "type": "string", "description": "The type of the relation" }
                    },
                    "required": ["from", "to", "relationType"]
                }
            }
        },
        "required": ["relations"]
    }"#;

    pub const ADD_OBSERVATIONS_SCHEMA: &str = r#"{
        "type": "object",
        "properties": {
            "observations": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "entityName": { "type": "string", "description": "The name of the entity to add the observations to" },
                        "contents": { "type": "array", "items": { "type": "string" }, "description": "An array of observation contents to add" }
                    },
                    "required": ["entityName", "contents"]
                }
            }
        },
        "required": ["observations"]
    }"#;

    pub const DELETE_ENTITIES_SCHEMA: &str = r#"{
        "type": "object",
        "properties": {
            "entityNames": { "type": "array", "items": { "type": "string" }, "description": "An array of entity names to delete" }
        },
        "required": ["entityNames"]
    }"#;

    pub const DELETE_OBSERVATIONS_SCHEMA: &str = r#"{
        "type": "object",
        "properties": {
            "deletions": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "entityName": { "type": "string", "description": "The name of the entity containing the observations" },
                        "observations": { "type": "array", "items": { "type": "string" }, "description": "An array of observations to delete" }
                    },
                    "required": ["entityName", "observations"]
                }
            }
        },
        "required": ["deletions"]
    }"#;

    pub const DELETE_RELATIONS_SCHEMA: &str = r#"{
        "type": "object",
        "properties": {
            "relations": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "from": { "type": "string", "description": "The name of the entity where the relation starts" },
                        "to": { "type": "string", "description": "The name of the entity where the relation ends" },
                        "relationType": { "type": "string", "description": "The type of the relation" }
                    },
                    "required": ["from", "to", "relationType"]
                },
                "description": "An array of relations to delete"
            }
        },
        "required": ["relations"]
    }"#;

    pub const READ_GRAPH_SCHEMA: &str = r#"{"type": "object", "properties": {}}"#;

    pub const SEARCH_NODES_SCHEMA: &str = r#"{
        "type": "object",
        "properties": {
            "query": { "type": "string", "description": "The search query to match against entity names, types, and observation content" }
        },
        "required": ["query"]
    }"#;

    pub const OPEN_NODES_SCHEMA: &str = r#"{
        "type": "object",
        "properties": {
            "names": { "type": "array", "items": { "type": "string" }, "description": "An array of entity names to retrieve" }
        },
        "required": ["names"]
    }"#;
}

// --- MCP Handlers ---

pub async fn list_tools_handler() -> Result<Response> {
    let tools = vec![
        ToolDefinition {
            name: "create_entities".to_string(),
            description: "Create multiple new entities in the knowledge graph".to_string(),
            input_schema: serde_json::from_str(schemas::CREATE_ENTITIES_SCHEMA).unwrap(),
        },
        ToolDefinition {
            name: "create_relations".to_string(),
            description: "Create multiple new relations between entities in the knowledge graph. Relations should be in active voice".to_string(),
            input_schema: serde_json::from_str(schemas::CREATE_RELATIONS_SCHEMA).unwrap(),
        },
        ToolDefinition {
            name: "add_observations".to_string(),
            description: "Add new observations to existing entities in the knowledge graph".to_string(),
            input_schema: serde_json::from_str(schemas::ADD_OBSERVATIONS_SCHEMA).unwrap(),
        },
        ToolDefinition {
            name: "delete_entities".to_string(),
            description: "Delete multiple entities and their associated relations from the knowledge graph".to_string(),
            input_schema: serde_json::from_str(schemas::DELETE_ENTITIES_SCHEMA).unwrap(),
        },
        ToolDefinition {
            name: "delete_observations".to_string(),
            description: "Delete specific observations from entities in the knowledge graph".to_string(),
            input_schema: serde_json::from_str(schemas::DELETE_OBSERVATIONS_SCHEMA).unwrap(),
        },
        ToolDefinition {
            name: "delete_relations".to_string(),
            description: "Delete multiple relations from the knowledge graph".to_string(),
            input_schema: serde_json::from_str(schemas::DELETE_RELATIONS_SCHEMA).unwrap(),
        },
        ToolDefinition {
            name: "read_graph".to_string(),
            description: "Read the entire knowledge graph".to_string(),
            input_schema: serde_json::from_str(schemas::READ_GRAPH_SCHEMA).unwrap(),
        },
        ToolDefinition {
            name: "search_nodes".to_string(),
            description: "Search for nodes in the knowledge graph based on a query".to_string(),
            input_schema: serde_json::from_str(schemas::SEARCH_NODES_SCHEMA).unwrap(),
        },
        ToolDefinition {
            name: "open_nodes".to_string(),
            description: "Open specific nodes in the knowledge graph by their names".to_string(),
            input_schema: serde_json::from_str(schemas::OPEN_NODES_SCHEMA).unwrap(),
        },
    ];
    Response::from_json(&ListToolsResponse { tools })
}

async fn call_do_post(stub: &Stub, path: &str, body_value: Value) -> Result<Response> {
    let mut req_init = RequestInit::new();
    req_init.with_method(Method::Post);
    let mut headers = Headers::new();
    headers.set("Content-Type", "application/json")?;
    req_init.with_headers(headers);
    req_init.with_body(Some(serde_json::to_vec(&body_value)?.into()));

    let do_url = format!("https://durable-object.internal-url{}", path);
    let do_req = WorkerRequest::new_with_init(&do_url, &req_init)?;
    stub.fetch_with_request(do_req).await
}

async fn call_do_get(stub: &Stub, path: &str) -> Result<Response> {
    let mut req_init = RequestInit::new();
    req_init.with_method(Method::Get);
    let do_url = format!("https://durable-object.internal-url{}", path);
    let do_req = WorkerRequest::new_with_init(&do_url, &req_init)?;
    stub.fetch_with_request(do_req).await
}

fn format_do_response_as_mcp_content<T: Serialize>(
    do_response_data: &T,
) -> Result<CallToolResponse> {
    let text = serde_json::to_string_pretty(do_response_data)
        .map_err(|e| worker::Error::RustError(format!("Serialization error: {}", e)))?;
    Ok(CallToolResponse {
        content: vec![ContentBlock {
            block_type: "text".to_string(),
            text,
        }],
    })
}

fn format_simple_mcp_success_message(message: &str) -> Result<CallToolResponse> {
    Ok(CallToolResponse {
        content: vec![ContentBlock {
            block_type: "text".to_string(),
            text: message.to_string(),
        }],
    })
}

pub async fn call_tool_handler(mut req: WorkerRequest, stub: Stub) -> Result<Response> {
    let params: CallToolRequestParams = match req.json().await {
        Ok(p) => p,
        Err(e) => {
            return Ok(mcp_error_response(
                "ParseError",
                &format!("Failed to parse request: {}", e),
            ))
        }
    };

    let tool_name = params.name.as_str();
    let args = params.arguments;

    let mcp_response_result: Result<CallToolResponse> = match tool_name {
        "create_entities" => {
            let mcp_args: McpCreateEntitiesArgs = serde_json::from_value(args)?;
            let do_payload = CreateEntitiesPayload {
                entities: mcp_args
                    .entities
                    .into_iter()
                    .map(|e| EntityToCreate {
                        name: e.name,
                        entity_type: e.entity_type,
                        observations: e.observations,
                        data: None, // MCP TS version doesn't have data for entities
                    })
                    .collect(),
            };
            let mut do_resp =
                call_do_post(&stub, "/graph/entities", serde_json::to_value(do_payload)?).await?;
            if do_resp.status_code() != 200 {
                return Ok(mcp_error_response(
                    "DOError",
                    &format!(
                        "DO Error: {} - {}",
                        do_resp.status_code(),
                        do_resp.text().await?
                    ),
                ));
            }
            let created_nodes: Vec<DoNode> = do_resp.json().await?;
            format_do_response_as_mcp_content(&created_nodes)
        }
        "create_relations" => {
            let mcp_args: McpCreateRelationsArgs = serde_json::from_value(args)?;
            let do_payload = CreateRelationsPayload {
                relations: mcp_args
                    .relations
                    .into_iter()
                    .map(|r| RelationToCreate {
                        from: r.from,
                        to: r.to,
                        relation_type: r.relation_type,
                        data: None, // MCP TS version doesn't have data for relations
                    })
                    .collect(),
            };
            let mut do_resp =
                call_do_post(&stub, "/graph/relations", serde_json::to_value(do_payload)?).await?;
            if do_resp.status_code() != 200 {
                return Ok(mcp_error_response(
                    "DOError",
                    &format!(
                        "DO Error: {} - {}",
                        do_resp.status_code(),
                        do_resp.text().await?
                    ),
                ));
            }
            let created_edges: Vec<DoEdge> = do_resp.json().await?;
            format_do_response_as_mcp_content(&created_edges)
        }
        "add_observations" => {
            let mcp_args: McpAddObservationsArgs = serde_json::from_value(args)?;
            let do_payload = AddObservationsPayload {
                observations: mcp_args
                    .observations
                    .into_iter()
                    .map(|o| AddObservationItem {
                        entity_name: o.entity_name,
                        contents: o.contents,
                    })
                    .collect(),
            };
            let mut do_resp = call_do_post(
                &stub,
                "/graph/observations/add",
                serde_json::to_value(do_payload)?,
            )
            .await?;
            if do_resp.status_code() != 200 {
                return Ok(mcp_error_response(
                    "DOError",
                    &format!(
                        "DO Error: {} - {}",
                        do_resp.status_code(),
                        do_resp.text().await?
                    ),
                ));
            }
            // DO returns Vec<Result<String,String>>
            let results: Value = do_resp.json().await?; // Keep as Value for direct stringification
            format_do_response_as_mcp_content(&results)
        }
        "delete_entities" => {
            let mcp_args: McpDeleteEntitiesArgs = serde_json::from_value(args)?;
            let do_payload = DeleteEntitiesPayload {
                entity_names: mcp_args.entity_names,
            };
            let mut do_resp = call_do_post(
                &stub,
                "/graph/entities/delete",
                serde_json::to_value(do_payload)?,
            )
            .await?;
            if do_resp.status_code() != 200 {
                return Ok(mcp_error_response(
                    "DOError",
                    &format!(
                        "DO Error: {} - {}",
                        do_resp.status_code(),
                        do_resp.text().await?
                    ),
                ));
            }
            // TS version returns generic success. Do not parse do_resp.json().
            format_simple_mcp_success_message("Entities deleted successfully")
        }
        "delete_observations" => {
            let mcp_args: McpDeleteObservationsArgs = serde_json::from_value(args)?;
            let do_payload = DeleteObservationsPayload {
                deletions: mcp_args
                    .deletions
                    .into_iter()
                    .map(|d| DeleteObservationItem {
                        entity_name: d.entity_name,
                        observations: d.observations,
                    })
                    .collect(),
            };
            let mut do_resp = call_do_post(
                &stub,
                "/graph/observations/delete",
                serde_json::to_value(do_payload)?,
            )
            .await?;
            if do_resp.status_code() != 200 {
                return Ok(mcp_error_response(
                    "DOError",
                    &format!(
                        "DO Error: {} - {}",
                        do_resp.status_code(),
                        do_resp.text().await?
                    ),
                ));
            }
            format_simple_mcp_success_message("Observations deleted successfully")
        }
        "delete_relations" => {
            let mcp_args: McpDeleteRelationsArgs = serde_json::from_value(args)?;
            let do_payload = DeleteRelationsPayload {
                relations: mcp_args
                    .relations
                    .into_iter()
                    .map(|r| RelationToDelete {
                        from: r.from,
                        to: r.to,
                        relation_type: r.relation_type,
                    })
                    .collect(),
            };
            let mut do_resp = call_do_post(
                &stub,
                "/graph/relations/delete",
                serde_json::to_value(do_payload)?,
            )
            .await?;
            if do_resp.status_code() != 200 {
                return Ok(mcp_error_response(
                    "DOError",
                    &format!(
                        "DO Error: {} - {}",
                        do_resp.status_code(),
                        do_resp.text().await?
                    ),
                ));
            }
            format_simple_mcp_success_message("Relations deleted successfully")
        }
        "read_graph" => {
            let mut do_resp = call_do_get(&stub, "/graph/state").await?;
            if do_resp.status_code() != 200 {
                return Ok(mcp_error_response(
                    "DOError",
                    &format!(
                        "DO Error: {} - {}",
                        do_resp.status_code(),
                        do_resp.text().await?
                    ),
                ));
            }
            let graph_data: KnowledgeGraphDataResponse = do_resp.json().await?;
            format_do_response_as_mcp_content(&graph_data)
        }
        "search_nodes" => {
            let mcp_args: McpSearchNodesArgs = serde_json::from_value(args)?;
            let do_payload = SearchNodesQuery {
                query: mcp_args.query,
            };
            let mut do_resp =
                call_do_post(&stub, "/graph/search", serde_json::to_value(do_payload)?).await?;
            if do_resp.status_code() != 200 {
                return Ok(mcp_error_response(
                    "DOError",
                    &format!(
                        "DO Error: {} - {}",
                        do_resp.status_code(),
                        do_resp.text().await?
                    ),
                ));
            }
            let search_results: KnowledgeGraphDataResponse = do_resp.json().await?;
            format_do_response_as_mcp_content(&search_results)
        }
        "open_nodes" => {
            let mcp_args: McpOpenNodesArgs = serde_json::from_value(args)?;
            let do_payload = OpenNodesQuery {
                names: mcp_args.names,
            };
            let mut do_resp =
                call_do_post(&stub, "/graph/open", serde_json::to_value(do_payload)?).await?;
            if do_resp.status_code() != 200 {
                return Ok(mcp_error_response(
                    "DOError",
                    &format!(
                        "DO Error: {} - {}",
                        do_resp.status_code(),
                        do_resp.text().await?
                    ),
                ));
            }
            let open_results: KnowledgeGraphDataResponse = do_resp.json().await?;
            format_do_response_as_mcp_content(&open_results)
        }
        _ => Err(worker::Error::RustError(format!(
            "Unknown tool: {}",
            tool_name
        ))),
    };

    match mcp_response_result {
        Ok(call_response) => Response::from_json(&call_response),
        Err(e) => Ok(mcp_error_response(
            "ToolExecutionError",
            &format!("Error executing tool '{}': {}", tool_name, e),
        )),
    }
}
