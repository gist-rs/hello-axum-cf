use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use uuid::Uuid;
use worker::{durable_object, Env, Headers, Method, Request, Response, Result, State};

// Key for storing the graph state in Durable Object storage
const KG_STATE_KEY: &str = "generic_kg_state_v1";

// --- Core Data Structures (Generic) ---

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Node {
    pub id: String,
    #[serde(rename = "type")]
    pub node_type: String, // User-defined type
    pub data: JsonValue, // User-defined data
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Edge {
    pub id: String,
    #[serde(rename = "type")]
    pub edge_type: String, // User-defined type
    pub source_node_id: String,
    pub target_node_id: String,
    pub data: Option<JsonValue>, // User-defined data
    pub created_at_ms: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct KnowledgeGraphState {
    pub nodes: HashMap<String, Node>,
    pub edges: HashMap<String, Edge>,
    pub metadata: HashMap<String, JsonValue>, // For any graph-level metadata
}

// --- API Payload Structs ---

#[derive(Deserialize)]
struct CreateNodePayload {
    #[serde(rename = "type")]
    node_type: String,
    data: JsonValue,
}

#[derive(Deserialize)]
struct UpdateNodePayload {
    #[serde(rename = "type")]
    node_type: Option<String>, // Optionally update type
    data: Option<JsonValue>, // Optionally update data
}

#[derive(Deserialize)]
struct CreateEdgePayload {
    #[serde(rename = "type")]
    edge_type: String,
    source_node_id: String,
    target_node_id: String,
    data: Option<JsonValue>,
}

#[derive(Deserialize)]
struct UpdateEdgePayload {
    data: Option<JsonValue>, // Only data is updatable for an edge post-creation
}

// --- KnowledgeGraphState Implementation ---
impl KnowledgeGraphState {
    fn new() -> Self {
        KnowledgeGraphState::default()
    }

    fn add_node(&mut self, node: Node) {
        self.nodes.insert(node.id.clone(), node);
    }

    fn get_node(&self, node_id: &str) -> Option<&Node> {
        self.nodes.get(node_id)
    }

    fn remove_node(&mut self, node_id: &str) -> Option<Node> {
        self.nodes.remove(node_id)
    }

    fn add_edge(&mut self, edge: Edge) {
        self.edges.insert(edge.id.clone(), edge);
    }

    fn get_edge(&self, edge_id: &str) -> Option<&Edge> {
        self.edges.get(edge_id)
    }

    fn remove_edge(&mut self, edge_id: &str) -> Option<Edge> {
        self.edges.remove(edge_id)
    }

    fn find_nodes_by_type(&self, node_type_filter: &str) -> Vec<&Node> {
        self.nodes
            .values()
            .filter(|n| n.node_type == node_type_filter)
            .collect()
    }

    fn get_edges_for_node(&self, node_id: &str, direction_filter: Option<&str>) -> Vec<&Edge> {
        self.edges
            .values()
            .filter(|edge| {
                match direction_filter {
                    Some("outgoing") => edge.source_node_id == node_id,
                    Some("incoming") => edge.target_node_id == node_id,
                    Some("both") | None => {
                        edge.source_node_id == node_id || edge.target_node_id == node_id
                    }
                    _ => false, // Invalid direction
                }
            })
            .collect()
    }

    fn delete_node_and_connected_edges(&mut self, node_id: &str) -> bool {
        if self.nodes.remove(node_id).is_some() {
            let mut edges_to_remove = Vec::new();
            for edge in self.edges.values() {
                if edge.source_node_id == node_id || edge.target_node_id == node_id {
                    edges_to_remove.push(edge.id.clone());
                }
            }
            for edge_id in edges_to_remove {
                self.edges.remove(&edge_id);
            }
            true
        } else {
            false
        }
    }
}

// --- Durable Object Definition ---
#[durable_object]
pub struct KnowledgeGraphDO {
    state: State,
    // initialized: bool, // Can be used for one-time setup on first activation
}

#[durable_object]
impl DurableObject for KnowledgeGraphDO {
    fn new(state: State, _env: Env) -> Self {
        Self {
            state,
            // initialized: false,
        }
    }

    async fn fetch(&mut self, mut req: Request) -> Result<Response> {
        let url = req.url()?;
        // Store the String from url.path() so that path_segments can borrow from it.
        let url_path_string = url.path();
        let path_segments: Vec<&str> = url_path_string
            .trim_start_matches('/')
            .split('/')
            .filter(|s| !s.is_empty())
            .collect();

        let method = req.method();

        // console_log!(
        //     "DO [{}] Request: {} {:?}",
        //     self.state.id().to_string(),
        //     method,
        //     path_segments
        // );

        let mut graph_state = self.load_or_initialize_graph_state().await?;

        match (method, path_segments.as_slice()) {
            // --- Node Operations ---
            (Method::Post, ["nodes"]) => {
                let payload: CreateNodePayload = match req.json().await {
                    Ok(p) => p,
                    Err(e) => {
                        return Response::error(
                            format!("Bad Request: Invalid JSON for CreateNodePayload: {}", e),
                            400,
                        )
                    }
                };
                let now_ms = worker::Date::now().as_millis();
                let node_id = Self::new_id();
                let node = Node {
                    id: node_id.clone(),
                    node_type: payload.node_type,
                    data: payload.data,
                    created_at_ms: now_ms,
                    updated_at_ms: now_ms,
                };
                graph_state.add_node(node.clone());
                self.save_graph_state(&graph_state).await?;
                Response::from_json(&node)
            }
            (Method::Get, ["nodes", node_id]) => match graph_state.get_node(node_id) {
                Some(node) => Response::from_json(node),
                None => Response::error("Node not found", 404),
            },
            (Method::Put, ["nodes", node_id]) => {
                let payload: UpdateNodePayload = match req.json().await {
                    Ok(p) => p,
                    Err(e) => {
                        return Response::error(
                            format!("Bad Request: Invalid JSON for UpdateNodePayload: {}", e),
                            400,
                        )
                    }
                };
                match graph_state.nodes.clone().get_mut(*node_id) {
                    // Corrected: removed .clone() on nodes and node_id, dereferenced node_id
                    Some(node) => {
                        let mut updated = false;
                        if let Some(new_type) = payload.node_type {
                            node.node_type = new_type;
                            updated = true;
                        }
                        if let Some(new_data) = payload.data {
                            node.data = new_data; // This replaces the whole data object
                            updated = true;
                        }
                        if updated {
                            node.updated_at_ms = worker::Date::now().as_millis();
                        }
                        self.save_graph_state(&graph_state).await?;
                        Response::from_json(node)
                    }
                    _ => Response::error("Node not found", 404),
                }
            }
            (Method::Delete, ["nodes", node_id]) => {
                if graph_state.delete_node_and_connected_edges(node_id) {
                    self.save_graph_state(&graph_state).await?;
                    Response::ok(format!("Node {} and connected edges deleted", node_id))
                } else {
                    Response::error("Node not found", 404)
                }
            }
            (Method::Get, ["nodes"]) => {
                // GET /nodes?type=YourType
                if let Some(type_filter) = url
                    .query_pairs()
                    .find(|(k, _)| k == "type")
                    .map(|(_, v)| v.into_owned())
                {
                    let nodes = graph_state.find_nodes_by_type(&type_filter);
                    Response::from_json(&nodes)
                } else {
                    // Return all nodes if no type filter (can be large!)
                    let all_nodes: Vec<&Node> = graph_state.nodes.values().collect();
                    Response::from_json(&all_nodes)
                }
            }

            // --- Edge Operations ---
            (Method::Post, ["edges"]) => {
                let payload: CreateEdgePayload = match req.json().await {
                    Ok(p) => p,
                    Err(e) => {
                        return Response::error(
                            format!("Bad Request: Invalid JSON for CreateEdgePayload: {}", e),
                            400,
                        )
                    }
                };
                // Validate source and target nodes exist
                if graph_state.get_node(&payload.source_node_id).is_none() {
                    return Response::error(
                        format!("Source node {} not found", payload.source_node_id),
                        400,
                    );
                }
                if graph_state.get_node(&payload.target_node_id).is_none() {
                    return Response::error(
                        format!("Target node {} not found", payload.target_node_id),
                        400,
                    );
                }

                let now_ms = worker::Date::now().as_millis();
                let edge_id = Self::new_id();
                let edge = Edge {
                    id: edge_id.clone(),
                    edge_type: payload.edge_type,
                    source_node_id: payload.source_node_id,
                    target_node_id: payload.target_node_id,
                    data: payload.data,
                    created_at_ms: now_ms,
                };
                graph_state.add_edge(edge.clone());
                self.save_graph_state(&graph_state).await?;
                Response::from_json(&edge)
            }
            (Method::Get, ["edges", edge_id]) => match graph_state.get_edge(edge_id) {
                Some(edge) => Response::from_json(edge),
                None => Response::error("Edge not found", 404),
            },
            (Method::Put, ["edges", edge_id]) => {
                let payload: UpdateEdgePayload = match req.json().await {
                    Ok(p) => p,
                    Err(e) => {
                        return Response::error(
                            format!("Bad Request: Invalid JSON for UpdateEdgePayload: {}", e),
                            400,
                        )
                    }
                };
                if let Some(edge) = graph_state.edges.clone().get_mut(*edge_id) {
                    // Corrected: removed .clone() on edges and edge_id, dereferenced edge_id
                    if let Some(new_data) = payload.data {
                        // Allow clearing data with null
                        edge.data = Some(new_data);
                    } else {
                        edge.data = None; // if payload.data is not present or explicitly null
                    }
                    // Note: Edges typically don't have an `updated_at` field, but could be added.
                    self.save_graph_state(&graph_state).await?;
                    Response::from_json(edge)
                } else {
                    Response::error("Edge not found", 404)
                }
            }
            (Method::Delete, ["edges", edge_id]) => {
                if graph_state.remove_edge(edge_id).is_some() {
                    self.save_graph_state(&graph_state).await?;
                    Response::ok(format!("Edge {} deleted", edge_id))
                } else {
                    Response::error("Edge not found", 404)
                }
            }

            // --- Relationship Queries ---
            (Method::Get, ["nodes", node_id, "related"]) => {
                // GET /nodes/{id}/related?edge_type=YourEdgeType&direction={outgoing|incoming|both}
                if graph_state.get_node(node_id).is_none() {
                    return Response::error("Start node not found", 404);
                }
                let query = url.query_pairs();
                let edge_type_filter = query
                    .clone()
                    .find(|(k, _)| k == "edge_type")
                    .map(|(_, v)| v.into_owned());
                let direction_filter = query
                    .clone()
                    .find(|(k, _)| k == "direction")
                    .map(|(_, v)| v.into_owned());

                let mut related_nodes = Vec::new();
                let edges = graph_state.get_edges_for_node(node_id, direction_filter.as_deref());

                for edge in edges {
                    if edge_type_filter.is_some()
                        && edge.edge_type != *edge_type_filter.as_ref().unwrap()
                    // Dereferenced here: *edge_type_filter...
                    {
                        continue;
                    }

                    let target_node_id_str: &str = &edge.target_node_id;
                    let source_node_id_str: &str = &edge.source_node_id;

                    match direction_filter.as_deref() {
                        Some("outgoing") if edge.source_node_id.as_str() == *node_id => {
                            if let Some(node_obj) = graph_state.get_node(target_node_id_str) {
                                related_nodes.push(node_obj);
                            }
                        }
                        Some("incoming") if edge.target_node_id.as_str() == *node_id => {
                            if let Some(node_obj) = graph_state.get_node(source_node_id_str) {
                                related_nodes.push(node_obj);
                            }
                        }
                        Some("both") | None => {
                            // Both directions or direction not specified
                            if edge.source_node_id.as_str() == *node_id {
                                if let Some(node_obj) = graph_state.get_node(target_node_id_str) {
                                    related_nodes.push(node_obj);
                                }
                            } else if edge.target_node_id.as_str() == *node_id {
                                // Check `else if` to avoid double-adding for self-loops when "both"
                                if let Some(node_obj) = graph_state.get_node(source_node_id_str) {
                                    related_nodes.push(node_obj);
                                }
                            }
                        }
                        _ => {} // Invalid direction or doesn't match
                    }
                }
                related_nodes.sort_by_key(|n| &n.id); // Consistent ordering
                related_nodes.dedup_by_key(|n| n.id.clone()); // Remove duplicates if "both" and self-loops
                Response::from_json(&related_nodes)
            }

            // --- Utility/Debug ---
            (Method::Get, ["state"]) => {
                let mut headers = Headers::new();
                headers.set("content-type", "application/json")?;
                // Wrapped in Ok() to match function signature Result<Response>
                Ok(Response::from_json(&graph_state)?.with_headers(headers))
            }

            _ => Response::error(
                format!(
                    "Not Found or Method Not Allowed. Path: {:?}, Method: {}",
                    path_segments, // path_segments is now Vec<&str>
                    req.method()
                ),
                404,
            ),
        }
    }
}

// --- Durable Object Helper Methods ---
impl KnowledgeGraphDO {
    fn new_id() -> String {
        Uuid::new_v4().to_string()
    }

    async fn load_or_initialize_graph_state(&mut self) -> Result<KnowledgeGraphState> {
        match self.state.storage().get(KG_STATE_KEY).await {
            Ok(state) => Ok(state),
            Err(_) => Ok(KnowledgeGraphState::new()), // Initialize if not found or error
        }
    }

    async fn save_graph_state(&mut self, graph_state: &KnowledgeGraphState) -> Result<()> {
        self.state.storage().put(KG_STATE_KEY, graph_state).await
    }
}
