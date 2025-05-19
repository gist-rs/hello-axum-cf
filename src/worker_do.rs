use crate::kg::KnowledgeGraphState;
use crate::types::*;
use worker::*;

const KG_STATE_KEY: &str = "knowledgeGraphState_v1"; // Added a version suffix

#[durable_object]
pub struct KnowledgeGraphDO {
    state: State,
    // We don't store the graph directly in the struct to ensure it's always loaded
    // from storage at the beginning of a request and saved at the end,
    // or managed carefully across multiple await points if optimized.
    // For simplicity and safety in this refactor, we'll load/save per operation.
}

impl KnowledgeGraphDO {
    fn new_id() -> String {
        uuid::Uuid::new_v4().to_string()
    }

    // Helper method to construct a Node for the simple POST /nodes endpoint
    fn construct_node_from_payload(id: String, payload: CreateNodePayload) -> Node {
        let current_time_ms = Date::now().as_millis();
        Node {
            id,
            node_type: payload.node_type,
            data: payload.data,
            created_at_ms: current_time_ms,
            updated_at_ms: current_time_ms,
        }
    }

    // Helper method to construct an Edge for the simple POST /edges endpoint
    fn construct_edge_from_payload(id: String, payload: CreateEdgePayload) -> Edge {
        let current_time_ms = Date::now().as_millis();
        Edge {
            id,
            edge_type: payload.edge_type,
            source_node_id: payload.source_node_id,
            target_node_id: payload.target_node_id,
            data: payload.data,
            created_at_ms: current_time_ms,
            // updated_at_ms is not in Edge struct in types.rs
        }
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

#[durable_object]
impl DurableObject for KnowledgeGraphDO {
    fn new(state: State, _env: Env) -> Self {
        Self { state }
    }

    async fn fetch(&mut self, mut req: Request) -> Result<Response> {
        let path = req.path();
        let mut graph_state = self.load_or_initialize_graph_state().await?;

        // Helper macro for handling results and saving state
        macro_rules! handle_result {
            // This arm is used for operations where the result is expected to be
            // a serializable value directly, not wrapped in a top-level Result
            // that needs `match Ok/Err`. The diagnostic indicates that for batch
            // operations, Vec<Result<String, String>> is passed here, causing
            // a type mismatch with the original `match res { Ok(val) => ..., Err(e) => ... }`
            // structure. To fix the diagnostic errors, we remove the Result matching
            // from this arm and treat the expression result as the success value.
            // This assumes any top-level errors for operations using this arm are
            // handled differently or do not occur based on current usage patterns
            // indicated by the diagnostic.
            ($op:expr) => {{
                let value = $op; // Capture the operation's result directly

                // Assuming successful completion of the operation at a top level
                // and save the graph state.
                self.save_graph_state(&graph_state).await?;

                // Return the result as JSON with a default 200 OK status.
                // This handles types like Vec<Result<String, String>> or Ok(SerializableType)
                // which Response::from_json can serialize.
                Response::from_json(&value)
            }};
            // This arm is used for operations that return Result<T, E> and need a specific status code.
            // Note: To resolve type inference issues when passing `Ok(value)`, the error type `worker::Error`
            // is explicitly specified here, assuming it aligns with the expected error handling.
            ($op:expr, success_status_code: $status:expr) => {
                match $op {
                    Ok(val) => {
                        self.save_graph_state(&graph_state).await?;
                        Response::from_json(&val).map(|r| r.with_status($status))
                    }
                    Err(e) => {
                        console_error!("Error processing request: {:?}", e);
                        Response::error(format!("Error: {:?}", e), 500)
                    }
                }
            };
            // This arm is used for operations that return Result<T, E> and need a 204 No Content on success.
            ($op:expr, no_content_success: true) => {
                match $op {
                    Ok(_) => {
                        self.save_graph_state(&graph_state).await?;
                        Response::empty().map(|r| r.with_status(204)) // No Content
                    }
                    Err(e) => {
                        console_error!("Error processing request: {:?}", e);
                        Response::error(format!("Error: {:?}", e), 500)
                    }
                }
            };
        }

        // Using a simple path matching for now. A router could be used for more complex scenarios.
        match (
            req.method(),
            path.split('/').collect::<Vec<&str>>().as_slice(),
        ) {
            // === Node Operations (Original Simple API) ===
            (Method::Post, ["", "nodes"]) => {
                let payload: CreateNodePayload = match req.json().await {
                    Ok(p) => p,
                    Err(e) => return Response::error(format!("Bad request: {}", e), 400),
                };
                let node_id = Self::new_id();
                // Construct the Node object
                let node_to_add = Self::construct_node_from_payload(node_id.clone(), payload);
                // Call the kg.rs add_node method
                graph_state.add_node(node_to_add.clone()); // add_node in kg.rs returns the ID, but we already have it.
                                                           // Let's assume the returned Node is what we want.
                                                           // Explicitly specify the error type for the Result passed to handle_result!
                handle_result!(Ok::<Node, worker::Error>(node_to_add), success_status_code: 201)
            }
            (Method::Get, ["", "nodes"]) => {
                let url = req.url()?;
                let query_params: std::collections::HashMap<String, String> =
                    url.query_pairs().into_owned().collect();

                if let Some(type_filter) = query_params.get("type") {
                    let nodes = graph_state.find_nodes_by_type(type_filter);
                    // find_nodes_by_type returns Vec<&Node>, which is serializable
                    Response::from_json(&nodes)
                } else {
                    // Return all nodes if no type filter
                    let all_nodes: Vec<&Node> = graph_state.nodes.values().collect();
                    Response::from_json(&all_nodes)
                }
            }
            (Method::Get, ["", "nodes", node_id]) => {
                match graph_state.get_node(node_id) {
                    Some(node) => {
                        self.save_graph_state(&graph_state).await?; // Save not strictly needed for GET, but good practice if there were reads that modify state (e.g. access counts)
                        Response::from_json(node)
                    }
                    None => Response::error("Node not found", 404),
                }
            }
            (Method::Put, ["", "nodes", node_id]) => {
                let payload: UpdateNodePayload = match req.json().await {
                    Ok(p) => p,
                    Err(e) => return Response::error(format!("Bad request: {}", e), 400),
                };
                match graph_state.update_node(node_id, payload.node_type, payload.data) {
                    Some(updated_node) => {
                        self.save_graph_state(&graph_state).await?;
                        Response::from_json(&updated_node)
                    }
                    None => Response::error("Node not found", 404),
                }
            }
            (Method::Delete, ["", "nodes", node_id_str]) => {
                match graph_state.delete_node_and_connected_edges(node_id_str) {
                    Some(deleted_node) => {
                        // Returns Option<Node>
                        self.save_graph_state(&graph_state).await?;
                        Response::from_json(
                            &serde_json::json!({ "deleted_id": deleted_node.id, "status": "deleted" }),
                        )
                    }
                    None => Response::error("Node not found", 404),
                }
            }
            (Method::Get, ["", "nodes", node_id_str, "related"]) => {
                if graph_state.get_node(node_id_str).is_none() {
                    return Response::error("Start node not found", 404);
                }

                let url = req.url()?;
                let query_params: std::collections::HashMap<String, String> =
                    url.query_pairs().into_owned().collect();

                let edge_type_filter = query_params.get("edge_type");
                let direction_filter = query_params.get("direction").map(|s| s.as_str());

                let mut related_nodes: Vec<Node> = Vec::new();
                let edges = graph_state.get_edges_for_node(node_id_str, direction_filter);

                for edge in edges {
                    if let Some(filter_type) = edge_type_filter {
                        if &edge.edge_type != filter_type {
                            continue;
                        }
                    }

                    let mut found_related_node_id: Option<&str> = None;
                    match direction_filter {
                        Some("outgoing") => {
                            if edge.source_node_id == *node_id_str {
                                found_related_node_id = Some(&edge.target_node_id);
                            }
                        }
                        Some("incoming") => {
                            if edge.target_node_id == *node_id_str {
                                found_related_node_id = Some(&edge.source_node_id);
                            }
                        }
                        Some("both") | None | Some(_) => {
                            // Treat None or invalid as "both"
                            if edge.source_node_id == *node_id_str {
                                found_related_node_id = Some(&edge.target_node_id);
                            } else if edge.target_node_id == *node_id_str {
                                found_related_node_id = Some(&edge.source_node_id);
                            }
                        }
                    }

                    if let Some(related_id) = found_related_node_id {
                        if let Some(node_obj) = graph_state.get_node(related_id) {
                            related_nodes.push(node_obj.clone());
                        }
                    }
                }

                related_nodes.sort_by_key(|n| n.id.clone());
                related_nodes.dedup_by_key(|n| n.id.clone());

                // self.save_graph_state(&graph_state).await?; // Not strictly needed for GET but good practice
                Response::from_json(&related_nodes)
            }

            // === Edge Operations (Original Simple API) ===
            (Method::Post, ["", "edges"]) => {
                let payload: CreateEdgePayload = match req.json().await {
                    Ok(p) => p,
                    Err(e) => return Response::error(format!("Bad request: {}", e), 400),
                };
                let edge_id = Self::new_id();
                // Construct the Edge object
                let edge_to_add = Self::construct_edge_from_payload(edge_id.clone(), payload);
                // Call the kg.rs add_edge method
                graph_state.add_edge(edge_to_add.clone()); // add_edge in kg.rs returns the ID.
                                                           // Let's assume the returned Edge is what we want.
                                                           // Explicitly specify the error type for the Result passed to handle_result!
                handle_result!(Ok::<Edge, worker::Error>(edge_to_add), success_status_code: 201)
            }
            (Method::Get, ["", "edges", edge_id]) => match graph_state.get_edge(edge_id) {
                Some(edge) => {
                    self.save_graph_state(&graph_state).await?;
                    Response::from_json(edge)
                }
                None => Response::error("Edge not found", 404),
            },
            (Method::Put, ["", "edges", _edge_id]) => {
                // Use _edge_id because it's not used currently
                let _payload: UpdateEdgePayload = match req.json().await {
                    // Use _payload because it's not used currently
                    Ok(p) => p,
                    Err(e) => return Response::error(format!("Bad request: {}", e), 400),
                };
                // This route depends on `update_edge_data` in `kg.rs` which is not currently implemented
                // based on the previous context. Commenting out for now.
                // match graph_state.update_edge_data(edge_id, payload.data) {
                //     Some(updated_edge) => {
                //         self.save_graph_state(&graph_state).await?;
                //         Response::from_json(&updated_edge)
                //     }
                //     None => Response::error("Edge not found", 404),
                // }
                Response::error("Route /edges/:id PUT not implemented yet", 501)
            }
            (Method::Delete, ["", "edges", edge_id]) => {
                match graph_state.remove_edge(edge_id) {
                    Some(deleted_edge) => {
                        // Returns Option<Edge>
                        self.save_graph_state(&graph_state).await?;
                        Response::from_json(
                            &serde_json::json!({ "deleted_id": deleted_edge.id, "status": "deleted" }),
                        )
                    }
                    None => Response::error("Edge not found", 404),
                }
            }

            // === Batch Graph Operations (Newer API) ===
            // These operations return Vec<Result<String, String>> or a struct, not a single top-level Result<T, E>.
            // They should use the first arm of handle_result!
            (Method::Post, ["", "graph", "entities"]) => {
                let payload: CreateEntitiesPayload = match req.json().await {
                    Ok(p) => p,
                    Err(e) => return Response::error(format!("Bad request: {}", e), 400),
                };
                match graph_state.create_entities_batch(payload.entities) {
                    Ok(nodes) => {
                        self.save_graph_state(&graph_state).await?;
                        Response::from_json(&nodes) // HTTP 200 by default
                    }
                    Err(e_str) => {
                        console_error!("Error in create_entities_batch: {}", e_str);
                        Response::error(format!("Failed to create entities: {}", e_str), 500)
                    }
                }
            }
            (Method::Post, ["", "graph", "relations"]) => {
                let payload: CreateRelationsPayload = match req.json().await {
                    Ok(p) => p,
                    Err(e) => return Response::error(format!("Bad request: {}", e), 400),
                };
                match graph_state.create_relations_batch(payload.relations) {
                    Ok(edges) => {
                        self.save_graph_state(&graph_state).await?;
                        Response::from_json(&edges) // HTTP 200 by default
                    }
                    Err(e_str) => {
                        console_error!("Error in create_relations_batch: {}", e_str);
                        Response::error(format!("Failed to create relations: {}", e_str), 500)
                    }
                }
            }
            (Method::Post, ["", "graph", "observations", "add"]) => {
                let payload: AddObservationsPayload = match req.json().await {
                    Ok(p) => p,
                    Err(e) => return Response::error(format!("Bad request: {}", e), 400),
                };
                let result = graph_state.add_observations_batch(payload.observations);
                handle_result!(result)
            }
            (Method::Post, ["", "graph", "entities", "delete"]) => {
                let payload: DeleteEntitiesPayload = match req.json().await {
                    Ok(p) => p,
                    Err(e) => return Response::error(format!("Bad request: {}", e), 400),
                };
                match graph_state.delete_entities_batch(payload.entity_names) {
                    Ok(deleted_ids) => {
                        self.save_graph_state(&graph_state).await?;
                        Response::from_json(&deleted_ids)
                    }
                    Err(e_str) => {
                        console_error!("Error in delete_entities_batch: {}", e_str);
                        Response::error(format!("Failed to delete entities: {}", e_str), 500)
                    }
                }
            }
            (Method::Post, ["", "graph", "observations", "delete"]) => {
                let payload: DeleteObservationsPayload = match req.json().await {
                    Ok(p) => p,
                    Err(e) => return Response::error(format!("Bad request: {}", e), 400),
                };
                let result = graph_state.delete_observations_batch(payload.deletions);
                handle_result!(result)
            }
            (Method::Post, ["", "graph", "relations", "delete"]) => {
                let payload: DeleteRelationsPayload = match req.json().await {
                    Ok(p) => p,
                    Err(e) => return Response::error(format!("Bad request: {}", e), 400),
                };
                match graph_state.delete_relations_batch(payload.relations) {
                    Ok(deleted_ids) => {
                        self.save_graph_state(&graph_state).await?;
                        Response::from_json(&deleted_ids)
                    }
                    Err(e_str) => {
                        console_error!("Error in delete_relations_batch: {}", e_str);
                        Response::error(format!("Failed to delete relations: {}", e_str), 500)
                    }
                }
            }
            (Method::Post, ["", "graph", "search"]) => {
                let payload: SearchNodesQuery = match req.json().await {
                    Ok(p) => p,
                    Err(e) => return Response::error(format!("Bad request: {}", e), 400),
                };
                let (entities, relations) = graph_state.search_nodes(&payload.query);
                let response_data = KnowledgeGraphDataResponse {
                    entities,
                    relations,
                };
                handle_result!(response_data) // Use the first arm for direct value response
            }
            (Method::Post, ["", "graph", "open"]) => {
                let payload: OpenNodesQuery = match req.json().await {
                    Ok(p) => p,
                    Err(e) => return Response::error(format!("Bad request: {}", e), 400),
                };
                let (entities, relations) = graph_state.open_nodes(&payload.names);
                let response_data = KnowledgeGraphDataResponse {
                    entities,
                    relations,
                };
                handle_result!(response_data) // Use the first arm for direct value response
            }
            (Method::Get, ["", "graph", "state"]) => {
                let (entities, relations) = graph_state.get_full_graph_data();
                let response_data = KnowledgeGraphDataResponse {
                    entities,
                    relations,
                };
                handle_result!(response_data) // Use the first arm for direct value response
            }

            // === Original State Endpoint (for debugging/compatibility if needed) ===
            // This endpoint is from the original do_memory.rs and might have a different expected structure
            // For consistency with the new /graph/state, we'll have it return the same structure.
            // If the original `/state` was returning the raw `KnowledgeGraphState` struct (with HashMaps),
            // that would be different.
            (Method::Get, ["", "state"]) => {
                let (entities, relations) = graph_state.get_full_graph_data();
                let response_data = KnowledgeGraphDataResponse {
                    entities,
                    relations,
                }; // Using ApiEntity/ApiRelation
                   // If raw state is needed: Response::from_json(&graph_state) after saving.
                handle_result!(response_data) // Use the first arm for direct value response
            }

            _ => Response::error("Not Found", 404),
        }
    }
}
