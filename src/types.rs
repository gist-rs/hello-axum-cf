use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::HashMap;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Node {
    pub id: String,
    #[serde(rename = "type")]
    pub node_type: String,
    pub data: JsonValue,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Edge {
    pub id: String,
    #[serde(rename = "type")]
    pub edge_type: String,
    pub source_node_id: String,
    pub target_node_id: String,
    pub data: Option<JsonValue>,
    pub created_at_ms: u64,
    // As per context, Edge doesn't have updated_at_ms
}

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct KnowledgeGraphState {
    pub nodes: HashMap<String, Node>,
    pub edges: HashMap<String, Edge>,
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CreateNodePayload {
    #[serde(rename = "type")]
    pub node_type: String,
    pub data: JsonValue,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UpdateNodePayload {
    #[serde(rename = "type")]
    pub node_type: Option<String>,
    pub data: Option<JsonValue>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CreateEdgePayload {
    #[serde(rename = "type")]
    pub edge_type: String,
    pub source_node_id: String,
    pub target_node_id: String,
    pub data: Option<JsonValue>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UpdateEdgePayload {
    pub data: Option<JsonValue>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct EntityToCreate {
    pub name: String,
    #[serde(rename = "entityType")]
    pub entity_type: String,
    #[serde(default)] // If observations might be missing in payload
    pub observations: Vec<String>,
    pub data: Option<JsonValue>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CreateEntitiesPayload {
    pub entities: Vec<EntityToCreate>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RelationToCreate {
    pub from: String,
    pub to: String,
    #[serde(rename = "relationType")]
    pub relation_type: String,
    pub data: Option<JsonValue>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CreateRelationsPayload {
    pub relations: Vec<RelationToCreate>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AddObservationItem {
    #[serde(rename = "entityName")]
    pub entity_name: String,
    pub contents: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AddObservationsPayload {
    pub observations: Vec<AddObservationItem>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DeleteEntitiesPayload {
    #[serde(rename = "entityNames")]
    pub entity_names: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DeleteObservationItem {
    #[serde(rename = "entityName")]
    pub entity_name: String,
    pub observations: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DeleteObservationsPayload {
    pub deletions: Vec<DeleteObservationItem>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RelationToDelete {
    pub from: String,
    pub to: String,
    #[serde(rename = "relationType")]
    pub relation_type: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DeleteRelationsPayload {
    pub relations: Vec<RelationToDelete>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SearchNodesQuery {
    pub query: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OpenNodesQuery {
    pub names: Vec<String>,
}

// API Response Structures
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ApiEntity {
    pub name: String,
    #[serde(rename = "entityType")]
    pub entity_type: String,
    pub observations: Vec<String>,
    pub data: Option<JsonValue>, // To match node_to_api_entity logic
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ApiRelation {
    pub from: String,
    pub to: String,
    #[serde(rename = "relationType")]
    pub relation_type: String,
    pub data: Option<JsonValue>, // To match edge_to_api_relation logic
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct KnowledgeGraphDataResponse {
    pub entities: Vec<ApiEntity>,
    pub relations: Vec<ApiRelation>,
}
