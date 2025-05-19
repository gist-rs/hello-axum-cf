use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use worker::{
    console_error, console_log, durable_object, Env, Headers, Method, Request, Response, Result,
    State,
};

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
// --- Batch Operations and Query Payloads/Responses ---

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EntityToCreate {
    pub name: String, // Will be used as Node ID
    #[serde(rename = "entityType")]
    pub entity_type: String,
    // Optional: if observations are always present or empty, Vec<String> might be better.
    // For flexibility with TS, Option<Vec<String>> is fine.
    pub observations: Option<Vec<String>>,
    pub data: Option<JsonValue>, // For any other arbitrary data not part of observations
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateEntitiesPayload {
    pub entities: Vec<EntityToCreate>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RelationToCreate {
    // pub id: Option<String>, // If you want to allow client-specified IDs for edges
    pub from: String, // Source Node ID
    pub to: String,   // Target Node ID
    #[serde(rename = "relationType")]
    pub relation_type: String,
    pub data: Option<JsonValue>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateRelationsPayload {
    pub relations: Vec<RelationToCreate>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AddObservationItem {
    #[serde(rename = "entityName")]
    pub entity_name: String, // Node ID
    pub contents: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AddObservationsPayload {
    pub observations: Vec<AddObservationItem>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DeleteEntitiesPayload {
    #[serde(rename = "entityNames")]
    pub entity_names: Vec<String>, // Node IDs to delete
}

#[derive(Debug, Clone, Deserialize)]
pub struct DeleteObservationItem {
    #[serde(rename = "entityName")]
    pub entity_name: String, // Node ID
    pub observations: Vec<String>, // Observations to remove
}

#[derive(Debug, Clone, Deserialize)]
pub struct DeleteObservationsPayload {
    pub deletions: Vec<DeleteObservationItem>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RelationToDelete {
    pub from: String,
    pub to: String,
    #[serde(rename = "relationType")]
    pub relation_type: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DeleteRelationsPayload {
    pub relations: Vec<RelationToDelete>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SearchNodesQuery {
    pub query: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OpenNodesQuery {
    pub names: Vec<String>, // Node IDs to fetch
}

// --- API Response Structs (mirroring TS KnowledgeGraph structure) ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiEntity {
    pub name: String,
    #[serde(rename = "entityType")]
    pub entity_type: String,
    pub observations: Vec<String>,
    // Potentially include other relevant fields from Node.data if necessary,
    // or keep it lean to match the TS example.
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiRelation {
    pub from: String,
    pub to: String,
    #[serde(rename = "relationType")]
    pub relation_type: String,
    // Potentially include Edge.data if necessary.
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct KnowledgeGraphDataResponse {
    pub entities: Vec<ApiEntity>,
    pub relations: Vec<ApiRelation>,
}

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

    // This is unsafe, use delete_node_and_connected_edges
    // fn remove_node(&mut self, node_id: &str) -> Option<Node> {
    //     self.nodes.remove(node_id)
    // }

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

    // --- Batch and Advanced Operations ---

    pub fn create_entities_batch(
        &mut self,
        entities_to_create: Vec<EntityToCreate>,
        now_ms: u64,
    ) -> Vec<Node> {
        let mut created_nodes = Vec::new();
        for entity_payload in entities_to_create {
            if self.nodes.contains_key(&entity_payload.name) {
                // Skip if a node with this ID (name) already exists
                // Alternatively, could return an error or update. For now, skipping.
                // Consider logging this event if running in a context with logging.
                continue;
            }

            let mut node_data = entity_payload.data.unwrap_or_else(|| serde_json::json!({}));

            if let Some(observations) = entity_payload.observations {
                if let Some(obj) = node_data.as_object_mut() {
                    obj.insert("observations".to_string(), serde_json::json!(observations));
                } else {
                    // This case should ideally not happen if node_data is initialized to an object
                    // or if entity_payload.data itself is an object.
                    // Fallback: create a new object if node_data was something else (e.g. null)
                    let mut new_obj = serde_json::Map::new();
                    new_obj.insert("observations".to_string(), serde_json::json!(observations));
                    node_data = serde_json::Value::Object(new_obj);
                }
            } else {
                // Ensure observations field exists as empty array if not provided
                if let Some(obj) = node_data.as_object_mut() {
                    obj.entry("observations".to_string())
                        .or_insert_with(|| serde_json::json!([]));
                }
            }

            let node = Node {
                id: entity_payload.name.clone(), // Use 'name' as ID
                node_type: entity_payload.entity_type,
                data: node_data,
                created_at_ms: now_ms,
                updated_at_ms: now_ms,
            };
            self.nodes.insert(node.id.clone(), node.clone());
            created_nodes.push(node);
        }
        created_nodes
    }

    pub fn create_relations_batch(
        &mut self,
        relations_to_create: Vec<RelationToCreate>,
        now_ms: u64,
        new_id_fn: &dyn Fn() -> String,
    ) -> Vec<Edge> {
        let mut created_edges = Vec::new();
        for relation_payload in relations_to_create {
            // Ensure source and target nodes exist
            if !self.nodes.contains_key(&relation_payload.from)
                || !self.nodes.contains_key(&relation_payload.to)
            {
                // Skip if source or target node doesn't exist
                // Consider logging or returning an error.
                continue;
            }
            let edge_id = new_id_fn();
            let edge = Edge {
                id: edge_id,
                edge_type: relation_payload.relation_type,
                source_node_id: relation_payload.from,
                target_node_id: relation_payload.to,
                data: Some(relation_payload.data.unwrap_or(serde_json::Value::Null)),
                created_at_ms: now_ms,
                // updated_at_ms is not typically on edges, but can be added if needed
            };
            self.edges.insert(edge.id.clone(), edge.clone());
            created_edges.push(edge);
        }
        created_edges
    }

    pub fn add_observations_batch(
        &mut self,
        items: Vec<AddObservationItem>,
        now_ms: u64,
    ) -> Vec<anyhow::Result<String, String>> {
        let mut results = Vec::new();
        for item in items {
            match self.nodes.get_mut(&item.entity_name) {
                Some(node) => {
                    let data_map = match node.data.as_object_mut() {
                        Some(map) => map,
                        None => {
                            // This implies node.data was not an object, which is unexpected.
                            // Initialize it as an object.
                            node.data = serde_json::json!({});
                            node.data.as_object_mut().unwrap() // Should now be an object
                        }
                    };

                    let obs_array = data_map
                        .entry("observations".to_string())
                        .or_insert_with(|| serde_json::json!([]))
                        .as_array_mut()
                        .ok_or_else(|| "Failed to ensure 'observations' is an array".to_string());

                    match obs_array {
                        Ok(current_obs) => {
                            let mut new_obs_added = false;
                            for content in item.contents {
                                let new_val = serde_json::json!(content);
                                if !current_obs.contains(&new_val) {
                                    current_obs.push(new_val);
                                    new_obs_added = true;
                                }
                            }
                            if new_obs_added {
                                node.updated_at_ms = now_ms;
                            }
                            results.push(Ok(item.entity_name.clone()));
                        }
                        Err(e_str) => {
                            results.push(Err(format!("Node {}: {}", item.entity_name, e_str)))
                        }
                    }
                }
                None => results.push(Err(format!("Node {} not found", item.entity_name))),
            }
        }
        results
    }

    pub fn delete_entities_batch(&mut self, entity_names: Vec<String>) -> Vec<String> {
        let mut deleted_ids = Vec::new();
        for name in entity_names {
            if self.delete_node_and_connected_edges(&name) {
                deleted_ids.push(name);
            }
        }
        deleted_ids
    }

    pub fn delete_observations_batch(
        &mut self,
        deletions: Vec<DeleteObservationItem>,
        now_ms: u64,
    ) -> Vec<anyhow::Result<String, String>> {
        let mut results = Vec::new();
        for item in deletions {
            match self.nodes.get_mut(&item.entity_name) {
                Some(node) => {
                    if let Some(data_map) = node.data.as_object_mut() {
                        if let Some(obs_val) = data_map.get_mut("observations") {
                            if let Some(obs_array) = obs_val.as_array_mut() {
                                let initial_len = obs_array.len();
                                obs_array.retain(|obs| {
                                    if let Some(s_obs) = obs.as_str() {
                                        !item.observations.contains(&s_obs.to_string())
                                    } else {
                                        true // Don't remove if not a string (should not happen with current logic)
                                    }
                                });
                                if obs_array.len() != initial_len {
                                    node.updated_at_ms = now_ms;
                                }
                                results.push(Ok(item.entity_name.clone()));
                            } else {
                                results.push(Err(format!(
                                    "Node {}: 'observations' field is not an array.",
                                    item.entity_name
                                )));
                            }
                        } else {
                            // No observations field to delete from, consider it a success or specific info.
                            results.push(Ok(item.entity_name.clone()));
                        }
                    } else {
                        results.push(Err(format!(
                            "Node {}: data field is not an object.",
                            item.entity_name
                        )));
                    }
                }
                None => results.push(Err(format!("Node {} not found", item.entity_name))),
            }
        }
        results
    }

    pub fn delete_relations_batch(
        &mut self,
        relations_to_delete: Vec<RelationToDelete>,
    ) -> Vec<String> {
        let mut deleted_edge_ids = Vec::new();
        let mut edge_ids_to_remove = Vec::new();

        for criteria in relations_to_delete {
            for (edge_id, edge) in self.edges.iter() {
                if edge.source_node_id == criteria.from
                    && edge.target_node_id == criteria.to
                    && edge.edge_type == criteria.relation_type
                {
                    edge_ids_to_remove.push(edge_id.clone());
                }
            }
        }

        for edge_id in edge_ids_to_remove
            .iter()
            .collect::<std::collections::HashSet<_>>()
        {
            // Avoid double remove if criteria overlap
            if self.edges.remove(edge_id).is_some() {
                deleted_edge_ids.push(edge_id.clone());
            }
        }
        deleted_edge_ids
    }

    fn node_to_api_entity(node: &Node) -> ApiEntity {
        let observations = node
            .data
            .get("observations")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|jv| jv.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        ApiEntity {
            name: node.id.clone(),
            entity_type: node.node_type.clone(),
            observations,
        }
    }

    fn edge_to_api_relation(edge: &Edge) -> ApiRelation {
        ApiRelation {
            from: edge.source_node_id.clone(),
            to: edge.target_node_id.clone(),
            relation_type: edge.edge_type.clone(),
        }
    }

    pub fn get_full_graph_data(&self) -> KnowledgeGraphDataResponse {
        let entities: Vec<ApiEntity> = self.nodes.values().map(Self::node_to_api_entity).collect();
        let relations: Vec<ApiRelation> = self
            .edges
            .values()
            .map(Self::edge_to_api_relation)
            .collect();
        KnowledgeGraphDataResponse {
            entities,
            relations,
        }
    }

    pub fn search_nodes(&self, query: &str) -> KnowledgeGraphDataResponse {
        let query_lower = query.to_lowercase();
        let mut matched_entities = Vec::new();
        let mut matched_entity_ids = std::collections::HashSet::new();

        for node in self.nodes.values() {
            let api_entity = Self::node_to_api_entity(node);
            let mut match_found = false;

            if api_entity.name.to_lowercase().contains(&query_lower) {
                match_found = true;
            }
            if !match_found && api_entity.entity_type.to_lowercase().contains(&query_lower) {
                match_found = true;
            }
            if !match_found {
                for obs in &api_entity.observations {
                    if obs.to_lowercase().contains(&query_lower) {
                        match_found = true;
                        break;
                    }
                }
            }

            if match_found {
                matched_entities.push(api_entity);
                matched_entity_ids.insert(node.id.clone());
            }
        }

        let filtered_relations: Vec<ApiRelation> = self
            .edges
            .values()
            .filter(|edge| {
                matched_entity_ids.contains(&edge.source_node_id)
                    && matched_entity_ids.contains(&edge.target_node_id)
            })
            .map(Self::edge_to_api_relation)
            .collect();

        KnowledgeGraphDataResponse {
            entities: matched_entities,
            relations: filtered_relations,
        }
    }

    pub fn open_nodes(&self, names: &[String]) -> KnowledgeGraphDataResponse {
        let mut found_entities = Vec::new();
        let name_set: std::collections::HashSet<&String> = names.iter().collect();
        let mut found_entity_ids = std::collections::HashSet::new();

        for node_id in names {
            if let Some(node) = self.nodes.get(node_id) {
                if name_set.contains(&node.id) {
                    // Ensure we only add if it was requested by name
                    found_entities.push(Self::node_to_api_entity(node));
                    found_entity_ids.insert(node.id.clone());
                }
            }
        }

        // Sort entities by the original order of names if desired, or by ID. Here, just by iteration order.
        // found_entities.sort_by_key(|e| e.name.clone()); // Optional: for consistent ordering

        let filtered_relations: Vec<ApiRelation> = self
            .edges
            .values()
            .filter(|edge| {
                found_entity_ids.contains(&edge.source_node_id)
                    && found_entity_ids.contains(&edge.target_node_id)
            })
            .map(Self::edge_to_api_relation)
            .collect();

        KnowledgeGraphDataResponse {
            entities: found_entities,
            relations: filtered_relations,
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
        console_log!("[DO FETCH START] Received request");
        let url = match req.url() {
            Ok(u) => u,
            Err(e) => {
                console_error!("[DO FETCH ERROR] Failed to get request URL: {}", e);
                return Response::error(format!("Failed to get request URL: {}", e), 500);
            }
        };
        console_log!("[DO FETCH] Request URL: {}", url.to_string());

        // Store the String from url.path() so that path_segments can borrow from it.
        let url_path_string = url.path().to_string(); // Ensure it's a String for lifetime
        console_log!("[DO FETCH] URL path string: {}", url_path_string);

        let path_segments: Vec<&str> = url_path_string
            .trim_start_matches('/')
            .split('/')
            .filter(|s| !s.is_empty())
            .collect();
        console_log!("[DO FETCH] Parsed path segments: {:?}", path_segments);

        let method = req.method().clone();
        console_log!("[DO FETCH] Request method: {:?}", method);

        console_log!(
            "DO [{}] Request: {} {:?}",
            self.state.id().to_string(),
            method,
            path_segments
        );

        // Ensure Date is imported, e.g.: use worker::Date;
        let now_ms = worker::Date::now().as_millis(); // Changed Date to worker::Date
        console_log!("[DO FETCH] Current timestamp (ms): {}", now_ms);

        console_log!("[DO FETCH] Attempting to load or initialize graph state...");
        let mut graph_state = match self.load_or_initialize_graph_state().await {
            Ok(state) => {
                console_log!("[DO FETCH] Graph state loaded/initialized successfully.");
                state
            }
            Err(e) => {
                console_error!(
                    "[DO FETCH ERROR] Failed to load/initialize graph state: {}",
                    e
                );
                return Response::error(
                    format!("Failed to load/initialize graph state: {}", e),
                    500,
                );
            }
        };

        console_log!("[DO FETCH] Routing request...");
        match (method.clone(), path_segments.as_slice()) {
            // --- Debug Route ---
            (Method::Get, ["hello"]) => {
                console_log!("[DO FETCH ROUTE] Matched GET /hello");
                Response::ok("Hello, world from KnowledgeGraphDO!")
            }
            // --- Node Operations ---
            (Method::Post, ["nodes"]) => {
                console_log!("[DO FETCH ROUTE] Matched POST /nodes");
                let payload: CreateNodePayload = match req.json().await {
                    Ok(p) => p,
                    Err(e) => {
                        return Response::error(
                            format!("Bad Request: Invalid JSON for CreateNodePayload: {}", e),
                            400,
                        )
                    }
                };
                let node_id = self.new_id();
                let node = Node {
                    id: node_id.clone(),
                    node_type: payload.node_type,
                    data: payload.data,
                    created_at_ms: now_ms,
                    updated_at_ms: now_ms,
                };
                graph_state.add_node(node.clone());
                self.save_graph_state(&graph_state).await?;
                console_log!("[DO FETCH SUCCESS] POST /nodes - Node created: {}", node_id);
                Response::from_json(&node)
            }
            (Method::Get, ["nodes", node_id]) => match graph_state.get_node(node_id) {
                // Dereferenced node_id
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
                    // Dereferenced node_id
                    // Operate directly on graph_state.nodes
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
                            node.updated_at_ms = now_ms; // Use now_ms from top of fetch
                        }
                        self.save_graph_state(&graph_state).await?;
                        Response::from_json(node)
                    }
                    _ => Response::error("Node not found", 404),
                }
            }
            (Method::Delete, ["nodes", node_id]) => {
                if graph_state.delete_node_and_connected_edges(node_id) {
                    // Dereferenced node_id
                    self.save_graph_state(&graph_state).await?;
                    Response::ok(format!("Node {} and connected edges deleted", *node_id))
                // Dereferenced node_id
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

                let edge_now_ms = worker::Date::now().as_millis(); // Using worker::Date consistently
                let edge_id = self.new_id(); // Changed Self::new_id() to self.new_id()
                let edge = Edge {
                    id: edge_id.clone(),
                    edge_type: payload.edge_type,
                    source_node_id: payload.source_node_id,
                    target_node_id: payload.target_node_id,
                    data: payload.data,
                    created_at_ms: edge_now_ms, // Use dedicated timestamp for edge creation
                };
                graph_state.add_edge(edge.clone());
                self.save_graph_state(&graph_state).await?;
                Response::from_json(&edge)
            }
            (Method::Get, ["edges", edge_id]) => match graph_state.get_edge(edge_id) {
                // Dereferenced edge_id
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
                    // Removed .clone() and dereferenced edge_id
                    if let Some(new_data) = payload.data {
                        edge.data = Some(new_data);
                    } else {
                        edge.data = None;
                    }
                    // Note: Edges typically don't have an `updated_at` field. If added, update here.
                    self.save_graph_state(&graph_state).await?;
                    Response::from_json(edge)
                } else {
                    Response::error("Edge not found", 404)
                }
            }
            (Method::Delete, ["edges", edge_id]) => {
                if graph_state.remove_edge(edge_id).is_some() {
                    // Dereferenced edge_id
                    self.save_graph_state(&graph_state).await?;
                    Response::ok(format!("Edge {} deleted", *edge_id)) // Dereferenced edge_id
                } else {
                    Response::error("Edge not found", 404)
                }
            }

            // --- Relationship Queries ---
            (Method::Get, ["nodes", node_id, "related"]) => {
                // GET /nodes/{id}/related?edge_type=YourEdgeType&direction={outgoing|incoming|both}
                if graph_state.get_node(node_id).is_none() {
                    // Dereferenced node_id
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
                let edges = graph_state.get_edges_for_node(node_id, direction_filter.as_deref()); // Dereferenced node_id

                for edge in edges {
                    if edge_type_filter.is_some()
                        && edge.edge_type != *edge_type_filter.as_ref().unwrap()
                    {
                        continue;
                    }

                    let target_node_id_str: &str = &edge.target_node_id;
                    let source_node_id_str: &str = &edge.source_node_id;

                    match direction_filter.as_deref() {
                        Some("outgoing") if edge.source_node_id.as_str() == *node_id => {
                            // *node_id is &str
                            if let Some(node_obj) = graph_state.get_node(target_node_id_str) {
                                related_nodes.push(node_obj);
                            }
                        }
                        Some("incoming") if edge.target_node_id.as_str() == *node_id => {
                            // *node_id is &str
                            if let Some(node_obj) = graph_state.get_node(source_node_id_str) {
                                related_nodes.push(node_obj);
                            }
                        }
                        Some("both") | None => {
                            if edge.source_node_id.as_str() == *node_id {
                                // *node_id is &str
                                if let Some(node_obj) = graph_state.get_node(target_node_id_str) {
                                    related_nodes.push(node_obj);
                                }
                            } else if edge.target_node_id.as_str() == *node_id {
                                // *node_id is &str
                                if let Some(node_obj) = graph_state.get_node(source_node_id_str) {
                                    related_nodes.push(node_obj);
                                }
                            }
                        }
                        _ => {}
                    }
                }
                related_nodes.sort_by_key(|n| &n.id); // Dereferenced n to access id
                related_nodes.dedup_by_key(|n| (n).id.clone()); // Dereferenced n to access id
                Response::from_json(&related_nodes)
            }

            // --- New Graph API Operations ---
            (Method::Get, ["graph", "state"]) => {
                let full_graph_data = graph_state.get_full_graph_data();
                Response::from_json(&full_graph_data)
            }

            (Method::Post, ["graph", "search"]) => {
                let payload: SearchNodesQuery = match req.json().await {
                    Ok(p) => p,
                    Err(e) => {
                        return Response::error(
                            format!("Bad Request: Invalid JSON for SearchNodesQuery: {}", e),
                            400,
                        );
                    }
                };
                let search_results = graph_state.search_nodes(&payload.query);
                Response::from_json(&search_results)
            }

            (Method::Post, ["graph", "open"]) => {
                let payload: OpenNodesQuery = match req.json().await {
                    Ok(p) => p,
                    Err(e) => {
                        return Response::error(
                            format!("Bad Request: Invalid JSON for OpenNodesQuery: {}", e),
                            400,
                        );
                    }
                };
                let open_results = graph_state.open_nodes(&payload.names);
                Response::from_json(&open_results)
            }

            (Method::Post, ["graph", "entities"]) => {
                let payload: CreateEntitiesPayload = match req.json().await {
                    Ok(p) => p,
                    Err(e) => {
                        return Response::error(
                            format!("Bad Request: Invalid JSON for CreateEntitiesPayload: {}", e),
                            400,
                        );
                    }
                };
                let created_nodes = graph_state.create_entities_batch(payload.entities, now_ms);
                self.save_graph_state(&graph_state).await?;
                Response::from_json(&created_nodes)
            }

            (Method::Post, ["graph", "relations"]) => {
                let payload: CreateRelationsPayload = match req.json().await {
                    Ok(p) => p,
                    Err(e) => {
                        return Response::error(
                            format!(
                                "Bad Request: Invalid JSON for CreateRelationsPayload: {}",
                                e
                            ),
                            400,
                        );
                    }
                };
                let created_edges =
                    graph_state
                        .create_relations_batch(payload.relations, now_ms, &|| self.new_id());
                self.save_graph_state(&graph_state).await?;
                Response::from_json(&created_edges)
            }

            (Method::Post, ["graph", "observations", "add"]) => {
                let payload: AddObservationsPayload = match req.json().await {
                    Ok(p) => p,
                    Err(e) => {
                        return Response::error(
                            format!(
                                "Bad Request: Invalid JSON for AddObservationsPayload: {}",
                                e
                            ),
                            400,
                        );
                    }
                };
                let results = graph_state.add_observations_batch(payload.observations, now_ms);
                self.save_graph_state(&graph_state).await?;
                Response::from_json(&results)
            }

            (Method::Post, ["graph", "entities", "delete"]) => {
                let payload: DeleteEntitiesPayload = match req.json().await {
                    Ok(p) => p,
                    Err(e) => {
                        return Response::error(
                            format!("Bad Request: Invalid JSON for DeleteEntitiesPayload: {}", e),
                            400,
                        );
                    }
                };
                let deleted_ids = graph_state.delete_entities_batch(payload.entity_names);
                self.save_graph_state(&graph_state).await?;
                Response::from_json(&deleted_ids)
            }

            (Method::Post, ["graph", "observations", "delete"]) => {
                let payload: DeleteObservationsPayload = match req.json().await {
                    Ok(p) => p,
                    Err(e) => {
                        return Response::error(
                            format!(
                                "Bad Request: Invalid JSON for DeleteObservationsPayload: {}",
                                e
                            ),
                            400,
                        );
                    }
                };
                let results = graph_state.delete_observations_batch(payload.deletions, now_ms);
                self.save_graph_state(&graph_state).await?;
                Response::from_json(&results)
            }

            (Method::Post, ["graph", "relations", "delete"]) => {
                let payload: DeleteRelationsPayload = match req.json().await {
                    Ok(p) => p,
                    Err(e) => {
                        return Response::error(
                            format!(
                                "Bad Request: Invalid JSON for DeleteRelationsPayload: {}",
                                e
                            ),
                            400,
                        );
                    }
                };
                let deleted_edge_ids = graph_state.delete_relations_batch(payload.relations);
                self.save_graph_state(&graph_state).await?;
                Response::from_json(&deleted_edge_ids)
            }

            // --- Utility/Debug ---
            (Method::Get, ["state"]) => {
                let mut headers = Headers::new();
                headers.set("content-type", "application/json")?;
                Ok(Response::from_json(&graph_state)?.with_headers(headers))
            }

            _ => {
                console_error!(
                    "[DO FETCH ROUTE MISMATCH] Path: {:?}, Method: {} did not match any configured routes.",
                    path_segments,
                    method.clone()
                );
                Response::error(
                    format!(
                        "Not Found or Method Not Allowed. Path: {:?}, Method: {}",
                        path_segments,
                        method // Use the 'method' variable captured at the start of fetch
                    ),
                    404, // Could also be 405 if you want to be more specific but 404 is common for "no match"
                )
            }
        }
    }
}

// --- Durable Object Helper Methods ---
impl KnowledgeGraphDO {
    fn new_id(&self) -> String {
        uuid::Uuid::new_v4().to_string()
    }

    // Wrapper for new_id to be passed as a closure if needed by KnowledgeGraphState methods
    // This is a bit of a workaround because `self.new_id` directly isn't `Fn() -> String`
    // due to `&self` capture. A static method or free function for ID generation might be cleaner
    // if used extensively within `KnowledgeGraphState` without access to `KnowledgeGraphDO` instance.
    // However, since `KnowledgeGraphState` methods are called from `KnowledgeGraphDO` where `self` is available,
    // we can call `self.new_id()` directly before passing data to `KnowledgeGraphState`.
    // The `new_id_fn: &dyn Fn() -> String` parameter in batch creation methods is one way,
    // or we generate IDs in the DO and pass them in.
    // For simplicity here, the DO will call its own new_id() and pass it to state methods when needed.

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
