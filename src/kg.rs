use crate::types::{
    AddObservationItem, ApiEntity, ApiRelation, DeleteObservationItem, Edge, EntityToCreate, Node,
    RelationToCreate, RelationToDelete,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;
use worker::Date;

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct KnowledgeGraphState {
    pub nodes: HashMap<String, Node>, // Node ID (which is entity name) -> Node
    pub edges: HashMap<String, Edge>, // Edge ID (UUID) -> Edge
    pub metadata: HashMap<String, JsonValue>, // Arbitrary metadata
}

impl KnowledgeGraphState {
    pub fn new() -> Self {
        KnowledgeGraphState::default()
    }

    pub fn add_node(&mut self, node: Node) -> String {
        let node_id = node.id.clone();
        self.nodes.insert(node_id.clone(), node);
        node_id
    }

    pub fn get_node(&self, node_id: &str) -> Option<&Node> {
        self.nodes.get(node_id)
    }

    pub fn add_edge(&mut self, edge: Edge) -> String {
        let edge_id = edge.id.clone();
        self.edges.insert(edge_id.clone(), edge);
        edge_id
    }

    pub fn get_edge(&self, edge_id: &str) -> Option<&Edge> {
        self.edges.get(edge_id)
    }

    pub fn remove_edge(&mut self, edge_id: &str) -> Option<Edge> {
        self.edges.remove(edge_id)
    }

    pub fn find_nodes_by_type(&self, node_type: &str) -> Vec<&Node> {
        self.nodes
            .values()
            .filter(|n| n.node_type == node_type)
            .collect()
    }

    pub fn get_edges_for_node(&self, node_id: &str, direction: Option<&str>) -> Vec<&Edge> {
        self.edges
            .values()
            .filter(|edge| match direction {
                Some("incoming") => edge.target_node_id == node_id,
                Some("outgoing") => edge.source_node_id == node_id,
                _ => edge.source_node_id == node_id || edge.target_node_id == node_id,
            })
            .collect()
    }

    pub fn delete_node_and_connected_edges(&mut self, node_id: &str) -> Option<Node> {
        let node_to_delete = self.nodes.remove(node_id);
        if node_to_delete.is_some() {
            let mut edge_ids_to_remove = Vec::new();
            for (edge_id, edge) in &self.edges {
                if edge.source_node_id == node_id || edge.target_node_id == node_id {
                    edge_ids_to_remove.push(edge_id.clone());
                }
            }
            for edge_id in edge_ids_to_remove {
                self.edges.remove(&edge_id);
            }
        }
        node_to_delete
    }

    pub fn update_node(
        &mut self,
        id_str: &str,
        node_type_opt: Option<String>,
        data_opt: Option<JsonValue>,
    ) -> Option<Node> {
        let current_time_ms = Date::now().as_millis();
        if let Some(node) = self.nodes.get_mut(id_str) {
            if let Some(new_type) = node_type_opt {
                node.node_type = new_type;
            }
            if let Some(new_data) = data_opt {
                node.data = new_data;
            }
            node.updated_at_ms = current_time_ms;
            Some(node.clone())
        } else {
            None
        }
    }

    // --- Batch/Query API Methods ---

    pub fn create_entities_batch(
        &mut self,
        entities_to_create: Vec<EntityToCreate>,
    ) -> Result<Vec<Node>, String> {
        worker::console_log!(
            "create_entities_batch called with {} entities to create.",
            entities_to_create.len()
        );
        let mut created_nodes = Vec::new();
        let current_time_ms = Date::now().as_millis();

        for entity_spec in entities_to_create {
            let node_id = entity_spec.name.clone();
            worker::console_log!("Processing entity_spec for ID: {}", node_id);

            if self.nodes.contains_key(&node_id) {
                worker::console_log!("Entity with ID: {} already exists. Skipping.", node_id);
                // Skip if entity with this name (ID) already exists
                continue;
            }

            let mut node_data = entity_spec.data.unwrap_or_else(|| json!({}));

            // Ensure node_data is an object to store observations
            if !node_data.is_object() {
                // If entity_spec.data was provided but not an object, this is a problem.
                // We'll overwrite it to store observations, or you could error out.
                // For simplicity, we create a new object, potentially losing original non-object data.
                worker::console_warn!(
                    "Data for entity '{}' was not an object and will be overwritten to store observations.",
                    node_id
                );
                node_data = json!({});
            }

            // Insert observations into the node_data
            if let Some(map) = node_data.as_object_mut() {
                map.insert("observations".to_string(), json!(entity_spec.observations));
            } else {
                // This case should ideally not be reached if the above `if !node_data.is_object()` handles it.
                // But as a fallback, create a new JSON object just for observations.
                node_data = json!({ "observations": entity_spec.observations });
            }

            let new_node = Node {
                id: node_id.clone(),
                node_type: entity_spec.entity_type,
                data: node_data,
                created_at_ms: current_time_ms,
                updated_at_ms: current_time_ms,
            };
            self.nodes.insert(node_id.clone(), new_node.clone());
            created_nodes.push(new_node);
            worker::console_log!("Successfully created and added node with ID: {}", node_id);
        }
        worker::console_log!(
            "create_entities_batch finished. {} nodes created.",
            created_nodes.len()
        );
        Ok(created_nodes)
    }

    pub fn create_relations_batch(
        &mut self,
        relations_to_create: Vec<RelationToCreate>,
    ) -> Result<Vec<Edge>, String> {
        let mut created_edges = Vec::new();
        let current_time_ms = Date::now().as_millis();

        for rel_data in relations_to_create {
            // Check if source and target nodes exist
            if !self.nodes.contains_key(&rel_data.from) {
                return Err(format!(
                    "Source node with name {} not found for relation",
                    rel_data.from
                ));
            }
            if !self.nodes.contains_key(&rel_data.to) {
                return Err(format!(
                    "Target node with name {} not found for relation",
                    rel_data.to
                ));
            }

            // Check if this exact relation already exists (by from, to, and type)
            // This is O(N) for N edges. If performance is critical for many edges, consider indexing.
            let exists = self.edges.values().any(|edge| {
                edge.source_node_id == rel_data.from
                    && edge.target_node_id == rel_data.to
                    && edge.edge_type == rel_data.relation_type
            });

            if exists {
                // Skip creating if it already exists, mirroring TS behavior.
                continue;
            }

            let edge_id = Uuid::new_v4().to_string();
            let new_edge = Edge {
                id: edge_id.clone(),
                edge_type: rel_data.relation_type,
                source_node_id: rel_data.from,
                target_node_id: rel_data.to,
                data: rel_data.data, // Assumes RelationToCreate::data is Option<JsonValue>
                created_at_ms: current_time_ms,
                // updated_at_ms for edges is not in the original Edge struct, add if needed.
                // For now, keeping Edge struct as is.
            };
            self.edges.insert(edge_id, new_edge.clone());
            created_edges.push(new_edge);
        }
        Ok(created_edges)
    }

    // Returns a Vec of Results, each indicating success (with entity name) or failure (with error message)
    pub fn add_observations_batch(
        &mut self,
        observations_to_add: Vec<AddObservationItem>,
    ) -> Vec<Result<String, String>> {
        let mut results = Vec::new();
        let current_time_ms = Date::now().as_millis();

        for item in observations_to_add {
            match self.nodes.get_mut(&item.entity_name) {
                Some(node) => {
                    // The problematic block that caused diagnostic errors has been removed.
                    // The logic below correctly handles adding observations.

                    if !node.data.is_object() {
                        node.data = serde_json::json!({}); // Ensure it's an object
                    }
                    let node_data_map = node.data.as_object_mut().unwrap(); // Safe

                    let obs_vec: &mut Vec<serde_json::Value> =
                        if let Some(serde_json::Value::Array(arr)) =
                            node_data_map.get_mut("observations")
                        {
                            arr
                        } else {
                            node_data_map.insert("observations".to_string(), serde_json::json!([]));
                            node_data_map
                                .get_mut("observations")
                                .unwrap()
                                .as_array_mut()
                                .unwrap()
                        };

                    let mut actually_added_count = 0;
                    for content_str in item.contents {
                        let content_val = serde_json::json!(content_str);
                        if !obs_vec.iter().any(|v| v == &content_val) {
                            obs_vec.push(content_val);
                            actually_added_count += 1;
                        }
                    }

                    if actually_added_count > 0 {
                        node.updated_at_ms = current_time_ms;
                        results.push(Ok(format!(
                            "Added {} new observation(s) to entity {}",
                            actually_added_count, item.entity_name
                        )));
                    } else {
                        results.push(Ok(format!(
                            "No new observations added to entity {} (all existed or empty input)",
                            item.entity_name
                        )));
                    }
                }
                None => {
                    results.push(Err(format!(
                        "Entity with name {} not found",
                        item.entity_name
                    )));
                }
            }
        }
        results
    }

    // Returns list of IDs of entities that were successfully deleted.
    pub fn delete_entities_batch(
        &mut self,
        entity_names: Vec<String>,
    ) -> Result<Vec<String>, String> {
        let mut deleted_ids = Vec::new();
        for name in entity_names {
            if self.nodes.contains_key(&name) {
                self.delete_node_and_connected_edges(&name);
                deleted_ids.push(name);
            }
            // If not found, we silently ignore, similar to TS version.
        }
        Ok(deleted_ids)
    }

    // Returns Vec of Results for each deletion attempt.
    pub fn delete_observations_batch(
        &mut self,
        deletions: Vec<DeleteObservationItem>,
    ) -> Vec<Result<String, String>> {
        let mut results = Vec::new();
        let current_time_ms = Date::now().as_millis();

        for item in deletions {
            match self.nodes.get_mut(&item.entity_name) {
                Some(node) => {
                    if !node.data.is_object() {
                        results.push(Err(format!(
                            "Entity {} data is not an object, cannot delete observations.",
                            item.entity_name
                        )));
                        continue;
                    }
                    let node_data_map = node.data.as_object_mut().unwrap();

                    let mut obs_modified = false;
                    if let Some(serde_json::Value::Array(obs_array)) =
                        node_data_map.get_mut("observations")
                    {
                        let original_len = obs_array.len();
                        obs_array.retain(|obs_val| {
                            !item.observations.iter().any(|obs_to_delete_str| {
                                obs_val.as_str().map_or(false, |s| s == obs_to_delete_str)
                            })
                        });
                        if obs_array.len() < original_len {
                            obs_modified = true;
                        }
                    } else {
                        // No "observations" field or not an array, so nothing to delete.
                        results.push(Ok(format!("No observations found or field is not an array for entity {}, nothing deleted.", item.entity_name)));
                        continue;
                    }

                    if obs_modified {
                        node.updated_at_ms = current_time_ms;
                        results.push(Ok(format!(
                            "Observations processed for entity {}",
                            item.entity_name
                        )));
                    } else {
                        results.push(Ok(format!(
                            "No matching observations deleted for entity {}",
                            item.entity_name
                        )));
                    }
                }
                None => {
                    results.push(Err(format!(
                        "Entity with name {} not found",
                        item.entity_name
                    )));
                }
            }
        }
        results
    }

    // Returns list of IDs of relations that were successfully deleted.
    pub fn delete_relations_batch(
        &mut self,
        relations_to_delete: Vec<RelationToDelete>,
    ) -> Result<Vec<String>, String> {
        let mut deleted_edge_ids = Vec::new();
        let mut edge_ids_to_actually_remove: HashSet<String> = HashSet::new();

        for rel_spec in relations_to_delete {
            // Find edge IDs matching the spec. There might be multiple if data differs but we don't check data for deletion.
            for (edge_id, edge) in &self.edges {
                if edge.source_node_id == rel_spec.from
                    && edge.target_node_id == rel_spec.to
                    && edge.edge_type == rel_spec.relation_type
                {
                    edge_ids_to_actually_remove.insert(edge_id.clone());
                }
            }
        }

        for edge_id in edge_ids_to_actually_remove {
            if self.edges.remove(&edge_id).is_some() {
                deleted_edge_ids.push(edge_id);
            }
        }
        Ok(deleted_edge_ids)
    }

    // Helper to convert Node to ApiEntity (matching types.rs ApiEntity)
    fn node_to_api_entity(&self, node: &Node) -> ApiEntity {
        let observations = node
            .data
            .get("observations")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|val| val.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_else(Vec::new);

        let mut other_data = node.data.clone();
        if let Some(obj) = other_data.as_object_mut() {
            obj.remove("observations");
        }

        let final_other_data = if other_data.is_null()
            || (other_data.is_object() && other_data.as_object().unwrap().is_empty())
        {
            None
        } else {
            Some(other_data)
        };

        ApiEntity {
            name: node.id.clone(), // node.id is the entity name
            entity_type: node.node_type.clone(),
            observations,
            data: final_other_data,
        }
    }

    // Helper to convert Edge to ApiRelation (matching types.rs ApiRelation)
    fn edge_to_api_relation(&self, edge: &Edge) -> ApiRelation {
        ApiRelation {
            from: edge.source_node_id.clone(),
            to: edge.target_node_id.clone(),
            relation_type: edge.edge_type.clone(),
            data: edge.data.clone(),
        }
    }

    pub fn get_full_graph_data(&self) -> (Vec<ApiEntity>, Vec<ApiRelation>) {
        let entities = self
            .nodes
            .values()
            .map(|n| self.node_to_api_entity(n))
            .collect();
        let relations = self
            .edges
            .values()
            .map(|e| self.edge_to_api_relation(e))
            .collect();
        (entities, relations)
    }

    // Basic search: matches query against node ID (name), type, and observations.
    // Returns graph data (entities and their interconnecting relations).
    pub fn search_nodes(&self, query: &str) -> (Vec<ApiEntity>, Vec<ApiRelation>) {
        let query_lower = query.to_lowercase();
        let mut matching_nodes_set = HashSet::new();

        for node in self.nodes.values() {
            if node.id.to_lowercase().contains(&query_lower)
                || node.node_type.to_lowercase().contains(&query_lower)
            {
                matching_nodes_set.insert(node.id.clone());
                continue;
            }

            if let Some(observations_val) = node.data.get("observations") {
                if let Some(observations_arr) = observations_val.as_array() {
                    for obs_val in observations_arr {
                        if let Some(obs_str) = obs_val.as_str() {
                            if obs_str.to_lowercase().contains(&query_lower) {
                                matching_nodes_set.insert(node.id.clone());
                                break; // Found a match in observations for this node
                            }
                        }
                    }
                }
            }
            // Optionally, search in other parts of node.data if it's structured and known.
        }

        let filtered_entities: Vec<ApiEntity> = matching_nodes_set
            .iter()
            .filter_map(|id| self.nodes.get(id))
            .map(|n| self.node_to_api_entity(n))
            .collect();

        let filtered_relations: Vec<ApiRelation> = self
            .edges
            .values()
            .filter(|edge| {
                matching_nodes_set.contains(&edge.source_node_id)
                    && matching_nodes_set.contains(&edge.target_node_id)
            })
            .map(|e| self.edge_to_api_relation(e))
            .collect();

        (filtered_entities, filtered_relations)
    }

    // Get specific nodes by name (ID) and their interconnecting relations.
    pub fn open_nodes(&self, names: &[String]) -> (Vec<ApiEntity>, Vec<ApiRelation>) {
        let names_set: HashSet<&String> = names.iter().collect();

        let filtered_entities: Vec<ApiEntity> = self
            .nodes
            .values()
            .filter(|n| names_set.contains(&n.id))
            .map(|n| self.node_to_api_entity(n))
            .collect();

        let node_ids_found: HashSet<String> =
            filtered_entities.iter().map(|e| e.name.clone()).collect();

        let filtered_relations: Vec<ApiRelation> = self
            .edges
            .values()
            .filter(|edge| {
                node_ids_found.contains(&edge.source_node_id)
                    && node_ids_found.contains(&edge.target_node_id)
            })
            .map(|e| self.edge_to_api_relation(e))
            .collect();

        (filtered_entities, filtered_relations)
    }
}
