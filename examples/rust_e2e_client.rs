// mcp-memory/examples/rust_e2e_client.rs
//
// This is a simple E2E test client for the generic KnowledgeGraphDO.
// To run this:
// 1. Ensure your Cloudflare Worker (`mcp-memory`) is running locally,
//    typically via `wrangler dev` (which defaults to http://localhost:8787).
// 2. You'll need to compile this Rust file. If it were part of a Cargo project,
//    your Cargo.toml would need:
//    [dependencies]
//    reqwest = { version = "0.11", features = ["json"] }
//    tokio = { version = "1", features = ["full"] }
//    serde = { version = "1.0", features = ["derive"] }
//    serde_json = "1.0"
//
//    To compile and run it as a standalone file (ensure rustc is installed and crates are accessible):
//    rustc rust_e2e_client.rs --edition=2021 -L /path/to/your/compiled/crates # Adjust path as needed
//    ./rust_e2e_client
//
//    Alternatively, create a temporary Cargo project:
//    cargo new --bin temp_e2e_client
//    cd temp_e2e_client
//    # Add dependencies to Cargo.toml as listed above
//    # Replace src/main.rs with this file's content
//    cargo run

use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::json; // For creating JSON bodies easily
use serde_json::Value as JsonValue; // For generic data fields

const BASE_URL: &str = "http://localhost:8787/do"; // Adjust if your worker runs elsewhere

// Simplified structs to deserialize responses from the DO
// We mainly care about the 'id' for subsequent requests.
#[derive(Debug, Serialize, Deserialize, Clone)] // Added Clone
struct NodeResponse {
    id: String,
    #[serde(rename = "type")]
    node_type: String,
    data: JsonValue,
    created_at_ms: u64,
    updated_at_ms: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone)] // Added Clone
struct EdgeResponse {
    id: String,
    #[serde(rename = "type")]
    edge_type: String,
    source_node_id: String,
    target_node_id: String,
    data: Option<JsonValue>,
    created_at_ms: u64,
}

// --- New Structs for Batch/Query API Responses ---

#[derive(Debug, Serialize, Deserialize, Clone)]
struct ClientApiEntity {
    name: String,
    #[serde(rename = "entityType")]
    entity_type: String,
    observations: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct ClientApiRelation {
    from: String,
    to: String,
    #[serde(rename = "relationType")]
    relation_type: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct ClientKnowledgeGraphDataResponse {
    entities: Vec<ClientApiEntity>,
    relations: Vec<ClientApiRelation>,
}

// Generic result type to deserialize {"Ok": T} or {"Err": E}
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
enum ClientResult<T, E> {
    Ok(T),
    Err(E),
}

// For batch delete entities/relations responses which return Vec<String>
// No new struct needed, can deserialize directly to Vec<String>

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new();

    println!("Starting E2E test against KnowledgeGraphDO at {}", BASE_URL);

    // --- Step 1: Create User Profile Node ---
    println!("\n--- Step 1: Create UserProfile Node ---");
    let user_payload = json!({
        "type": "UserProfile",
        "data": {
            "email": "e2e_user@example.com",
            "displayName": "E2E Test User",
            "registeredAt": "2024-01-01T12:00:00Z"
        }
    });

    let resp = client
        .post(format!("{}/nodes", BASE_URL))
        .json(&user_payload)
        .send()
        .await?;

    if !resp.status().is_success() {
        eprintln!(
            "Failed to create user node. Status: {}. Response: {}",
            resp.status(),
            resp.text().await?
        );
        return Ok(());
    }
    let user_node: NodeResponse = resp.json().await?;
    println!("Created UserProfile Node: {:?}", user_node);
    let user_node_id = user_node.id.clone();

    // --- Step 2: Create CryptoWallet Node ---
    println!("\n--- Step 2: Create CryptoWallet Node ---");
    let wallet_payload = json!({
        "type": "CryptoWallet",
        "data": {
            "address": "0xE2eTe5t123456789012345678901234567890E2E",
            "blockchain": "TestChain",
            "label": "E2E Test Wallet"
        }
    });

    let resp = client
        .post(format!("{}/nodes", BASE_URL))
        .json(&wallet_payload)
        .send()
        .await?;

    if !resp.status().is_success() {
        eprintln!(
            "Failed to create wallet node. Status: {}. Response: {}",
            resp.status(),
            resp.text().await?
        );
        return Ok(());
    }
    let wallet_node: NodeResponse = resp.json().await?;
    println!("Created CryptoWallet Node: {:?}", wallet_node);
    let wallet_node_id = wallet_node.id.clone();

    // --- Step 3: Link User to Wallet ---
    println!("\n--- Step 3: Link User to Wallet (OWNS_ASSET Edge) ---");
    let edge_payload = json!({
        "type": "OWNS_ASSET",
        "source_node_id": user_node_id,
        "target_node_id": wallet_node_id,
        "data": {
            "relationship_label": "Primary Test Wallet",
            "linkedAt": "2024-01-01T12:05:00Z"
        }
    });

    let resp = client
        .post(format!("{}/edges", BASE_URL))
        .json(&edge_payload)
        .send()
        .await?;

    if !resp.status().is_success() {
        eprintln!(
            "Failed to create edge. Status: {}. Response: {}",
            resp.status(),
            resp.text().await?
        );
        return Ok(());
    }
    let edge: EdgeResponse = resp.json().await?;
    println!("Created OWNS_ASSET Edge: {:?}", edge);

    // --- Step 4: Query User's Wallets ---
    println!("\n--- Step 4: Query User's Wallets ---");
    let query_url = format!(
        "{}/nodes/{}/related?edge_type=OWNS_ASSET&direction=outgoing",
        BASE_URL, user_node.id
    );
    println!("Querying: {}", query_url);

    let resp = client.get(&query_url).send().await?;

    if !resp.status().is_success() {
        eprintln!(
            "Failed to query related nodes. Status: {}. Response: {}",
            resp.status(),
            resp.text().await?
        );
        return Ok(());
    }
    let related_wallets: Vec<NodeResponse> = resp.json().await?;
    println!("Found Related Wallet Nodes: {:?}", related_wallets);

    if related_wallets.iter().any(|w| w.id == wallet_node.id) {
        println!("SUCCESS: E2E test successfully created user, wallet, linked them, and queried the relationship.");
    } else {
        eprintln!("FAILURE: E2E test did not find the linked wallet in the query results.");
    }

    // --- Optional: Get full state for debugging ---
    // println!("\n--- Optional: Get Full Graph State ---");
    // let resp = client.get(format!("{}/state", BASE_URL)).send().await?;
    // if resp.status().is_success() {
    //     let full_state: JsonValue = resp.json().await?;
    //     println!("Full Graph State:\n{}", serde_json::to_string_pretty(&full_state)?);
    // } else {
    //     eprintln!("Error getting full state: {} - {}", resp.status(), resp.text().await?);
    // }

    // --- Pre-Step 5: Ensure entities to be created in Step 5 do not exist ---
    println!("\n--- Pre-Step 5: Deleting entities if they exist (blogpost_123, tag_rust, tag_async) ---");
    let entities_to_delete_names = vec!["blogpost_123".to_string(), "tag_rust".to_string(), "tag_async".to_string()];
    let delete_payload_for_step_5 = json!({ "entityNames": entities_to_delete_names });

    let resp_delete_pre_step_5 = client
        .post(format!("{}/graph/entities/delete", BASE_URL))
        .json(&delete_payload_for_step_5)
        .send()
        .await?;

    if !resp_delete_pre_step_5.status().is_success() {
        eprintln!(
            "Pre-Step 5: Failed to delete entities. Status: {}. Response: {}",
            resp_delete_pre_step_5.status(),
            resp_delete_pre_step_5.text().await?
        );
        // Decide if this should be a fatal error for the test
    } else {
        let deleted_ids_pre_step_5: Vec<String> = resp_delete_pre_step_5.json().await?;
        println!("Pre-Step 5: Successfully called delete for entities. Deleted IDs reported: {:?}", deleted_ids_pre_step_5);
    }


    // --- Step 5: Batch Create New Entities ( BlogPost and Tag ) ---
    println!("\n--- Step 5: Batch Create BlogPost and Tag Nodes ---");
    let entities_payload = json!({
        "entities": [
            {
                "name": "blogpost_123", // Using name as ID
                "entityType": "BlogPost",
                "observations": ["Initial post draft", "Needs review"],
                "data": { "title": "My First Blog Post", "status": "draft" }
            },
            {
                "name": "tag_rust",
                "entityType": "Tag",
                "observations": ["Popular programming language"],
                "data": { "slug": "rust-lang" }
            },
            {
                "name": "tag_async",
                "entityType": "Tag",
                "data": { "slug": "async-programming" } // No initial observations
            }
        ]
    });

    let resp = client
        .post(format!("{}/graph/entities", BASE_URL))
        .json(&entities_payload)
        .send()
        .await?;

    if !resp.status().is_success() {
        eprintln!(
            "Failed to batch create entities. Status: {}. Response: {}",
            resp.status(),
            resp.text().await?
        );
    } else {
        let created_entities: Vec<NodeResponse> = resp.json().await?;
        println!("Batch Created Entities: {:?}", created_entities);
        assert_eq!(created_entities.len(), 3); // Assuming all are new and created
    }
    let blog_post_id = "blogpost_123".to_string();
    let tag_rust_id = "tag_rust".to_string();
    let tag_async_id = "tag_async".to_string();

    // --- Step 6: Add Observations ---
    println!("\n--- Step 6: Add Observations ---");
    let add_obs_payload = json!({
        "observations": [
            {
                "entityName": blog_post_id,
                "contents": ["Revised by editor", "Scheduled for publication"]
            },
            {
                "entityName": user_node_id, // Add to existing user node
                "contents": ["User participated in E2E test"]
            }
        ]
    });
    let resp = client
        .post(format!("{}/graph/observations/add", BASE_URL))
        .json(&add_obs_payload)
        .send()
        .await?;

    if !resp.status().is_success() {
        eprintln!(
            "Failed to add observations. Status: {}. Response: {}",
            resp.status(),
            resp.text().await?
        );
    } else {
        let obs_results: Vec<ClientResult<String, String>> = resp.json().await?;
        println!("Add Observations Results: {:?}", obs_results);
        // Add assertions based on expected success/failure if needed
    }

    // --- Step 7: Batch Create Relations ---
    println!("\n--- Step 7: Batch Create Relations ---");
    let relations_payload = json!({
        "relations": [
            { "from": blog_post_id, "to": tag_rust_id, "relationType": "HAS_TAG", "data": { "relevance": 0.9 } },
            { "from": blog_post_id, "to": tag_async_id, "relationType": "HAS_TAG" }
        ]
    });
    let resp = client
        .post(format!("{}/graph/relations", BASE_URL))
        .json(&relations_payload)
        .send()
        .await?;

    if !resp.status().is_success() {
        eprintln!(
            "Failed to batch create relations. Status: {}. Response: {}",
            resp.status(),
            resp.text().await?
        );
    } else {
        let created_relations: Vec<EdgeResponse> = resp.json().await?;
        println!("Batch Created Relations: {:?}", created_relations);
        assert_eq!(created_relations.len(), 2);
    }

    // --- Step 8: Search Nodes ---
    println!("\n--- Step 8: Search Nodes (query: 'rust') ---");
    let search_payload = json!({ "query": "rust" });
    let resp = client
        .post(format!("{}/graph/search", BASE_URL))
        .json(&search_payload)
        .send()
        .await?;

    if !resp.status().is_success() {
        eprintln!(
            "Failed to search nodes. Status: {}. Response: {}",
            resp.status(),
            resp.text().await?
        );
    } else {
        let search_results: ClientKnowledgeGraphDataResponse = resp.json().await?;
        println!("Search Results for 'rust': {:?}", search_results);
        // Add more specific assertions, e.g., check if tag_rust_id is in entities
        assert!(search_results
            .entities
            .iter()
            .any(|e| e.name == tag_rust_id));
    }

    // --- Step 9: Open Nodes ---
    println!("\n--- Step 9: Open Nodes (user_node_id, blog_post_id) ---");
    let open_payload = json!({ "names": [user_node_id.clone(), blog_post_id.clone()] });
    let resp = client
        .post(format!("{}/graph/open", BASE_URL))
        .json(&open_payload)
        .send()
        .await?;
    if !resp.status().is_success() {
        eprintln!(
            "Failed to open nodes. Status: {}. Response: {}",
            resp.status(),
            resp.text().await?
        );
    } else {
        let open_results: ClientKnowledgeGraphDataResponse = resp.json().await?;
        println!("Open Nodes Results: {:?}", open_results);
        assert_eq!(open_results.entities.len(), 2);
        assert!(open_results.entities.iter().any(|e| e.name == user_node_id));
        assert!(open_results.entities.iter().any(|e| e.name == blog_post_id));
    }

    // --- Step 10: Get Full Graph State ---
    println!("\n--- Step 10: Get Full Graph State ---");
    let resp = client
        .get(format!("{}/graph/state", BASE_URL))
        .send()
        .await?;
    if !resp.status().is_success() {
        eprintln!(
            "Failed to get full graph state. Status: {}. Response: {}",
            resp.status(),
            resp.text().await?
        );
    } else {
        let graph_state: ClientKnowledgeGraphDataResponse = resp.json().await?;
        println!("Full Graph State: {:?}", graph_state);
        // Add assertions on the number of entities/relations if stable
    }

    // --- Step 11: Delete Observations ---
    println!("\n--- Step 11: Delete Observations ---");
    let delete_obs_payload = json!({
        "deletions": [
            {
                "entityName": blog_post_id,
                "observations": ["Needs review", "NonExistentObservation"] // Test deleting one existing, one not
            }
        ]
    });
    let resp = client
        .post(format!("{}/graph/observations/delete", BASE_URL))
        .json(&delete_obs_payload)
        .send()
        .await?;
    if !resp.status().is_success() {
        eprintln!(
            "Failed to delete observations. Status: {}. Response: {}",
            resp.status(),
            resp.text().await?
        );
    } else {
        let delete_obs_results: Vec<ClientResult<String, String>> = resp.json().await?;
        println!("Delete Observations Results: {:?}", delete_obs_results);
    }

    // --- Step 12: Delete Relations ---
    println!("\n--- Step 12: Delete Relations ---");
    let delete_relations_payload = json!({
        "relations": [
            // Delete one of the relations created earlier
            { "from": blog_post_id, "to": tag_async_id, "relationType": "HAS_TAG" },
            // Attempt to delete a non-existent relation
            { "from": "non_existent_node1", "to": "non_existent_node2", "relationType": "DOES_NOT_EXIST" }
        ]
    });
    let resp = client
        .post(format!("{}/graph/relations/delete", BASE_URL))
        .json(&delete_relations_payload)
        .send()
        .await?;
    if !resp.status().is_success() {
        eprintln!(
            "Failed to delete relations. Status: {}. Response: {}",
            resp.status(),
            resp.text().await?
        );
    } else {
        let deleted_relation_ids: Vec<String> = resp.json().await?;
        println!("Deleted Relation IDs: {:?}", deleted_relation_ids);
        // Assert that only one relation was actually deleted, if possible to know its ID
    }

    // --- Step 13: Delete Entities ---
    println!("\n--- Step 13: Delete Entities ---");
    let delete_entities_payload = json!({
        // Delete some of the batch-created entities
        "entityNames": [tag_rust_id.clone(), tag_async_id.clone(), "non_existent_entity_to_delete".to_string()]
    });
    let resp = client
        .post(format!("{}/graph/entities/delete", BASE_URL))
        .json(&delete_entities_payload)
        .send()
        .await?;
    if !resp.status().is_success() {
        eprintln!(
            "Failed to delete entities. Status: {}. Response: {}",
            resp.status(),
            resp.text().await?
        );
    } else {
        let deleted_entity_ids: Vec<String> = resp.json().await?;
        println!("Deleted Entity IDs: {:?}", deleted_entity_ids);
        assert!(deleted_entity_ids.contains(&tag_rust_id));
        assert!(deleted_entity_ids.contains(&tag_async_id));
        assert_eq!(deleted_entity_ids.len(), 2); // Only existing ones should be reported as deleted
    }

    println!("\n--- E2E Test Suite for new APIs Completed ---");

    // --- Step 14: Test GET /nodes?type=... ---
    println!("\n--- Step 14: Test GET /nodes?type=... ---");

    let filter_test_type = "FilterTestType";
    let other_type_for_filter = "OtherTypeForFilter";

    // Create nodes for the filter test
    let node_filter_1_payload = json!({
        "type": filter_test_type,
        "data": { "name": "FilterNode1" }
    });
    let node_filter_2_payload = json!({
        "type": filter_test_type,
        "data": { "name": "FilterNode2" }
    });
    let node_other_payload = json!({
        "type": other_type_for_filter,
        "data": { "name": "OtherNodeForFilter" }
    });

    let mut created_node_ids_for_filter_test: Vec<String> = Vec::new();

    for (i, payload) in [node_filter_1_payload, node_filter_2_payload, node_other_payload].iter().enumerate() {
        let resp_create = client
            .post(format!("{}/nodes", BASE_URL))
            .json(payload)
            .send()
            .await?;
        if !resp_create.status().is_success() {
            eprintln!(
                "Step 14: Failed to create test node {}. Status: {}. Response: {}",
                i + 1,
                resp_create.status(),
                resp_create.text().await?
            );
            // Potentially return error or skip further assertions for this step
            continue;
        }
        let node_resp: NodeResponse = resp_create.json().await?;
        println!("Step 14: Created test node: {:?}", node_resp);
        created_node_ids_for_filter_test.push(node_resp.id.clone());
    }
    
    assert_eq!(created_node_ids_for_filter_test.len(), 3, "Step 14: Expected 3 nodes to be created for filter test");

    // Call GET /nodes?type=FilterTestType
    println!("\nStep 14: Querying GET /nodes?type={}", filter_test_type);
    let filter_query_url = format!("{}/nodes?type={}", BASE_URL, filter_test_type);
    let resp_filter = client.get(&filter_query_url).send().await?;

    if !resp_filter.status().is_success() {
        eprintln!(
            "Step 14: Failed to query nodes by type. Status: {}. Response: {}",
            resp_filter.status(),
            resp_filter.text().await?
        );
    } else {
        let filtered_nodes: Vec<NodeResponse> = resp_filter.json().await?;
        println!("Step 14: Filtered Nodes ({}) response: {:?}", filter_test_type, filtered_nodes);
        assert_eq!(filtered_nodes.len(), 2, "Step 14: Expected 2 nodes of type {}", filter_test_type);
        for node in filtered_nodes {
            assert_eq!(node.node_type, filter_test_type, "Step 14: Node type mismatch in filtered results");
            // Ensure the IDs match two of the created nodes (order might vary)
            assert!(created_node_ids_for_filter_test.contains(&node.id));
        }
        println!("Step 14: Successfully verified GET /nodes?type={} endpoint.", filter_test_type);
    }
    
    // Call GET /nodes (all nodes)
    println!("\nStep 14: Querying GET /nodes (all nodes)");
    let all_nodes_query_url = format!("{}/nodes", BASE_URL);
    let resp_all_nodes = client.get(&all_nodes_query_url).send().await?;

    if !resp_all_nodes.status().is_success() {
        eprintln!(
            "Step 14: Failed to query all nodes. Status: {}. Response: {}",
            resp_all_nodes.status(),
            resp_all_nodes.text().await?
        );
    } else {
        let all_nodes_result: Vec<NodeResponse> = resp_all_nodes.json().await?;
        println!("Step 14: All nodes response count: {}", all_nodes_result.len());
        // We created 3 nodes in this test. The total number of nodes should be at least 3
        // plus whatever existed from previous steps (user_node, wallet_node, blog_post_node)
        // So, we expect at least 3 + 3 = 6 if this is run after a full test.
        // For simplicity, we'll assert that it found our specific test nodes.
        let mut found_filter_node_1 = false;
        let mut found_filter_node_2 = false;
        let mut found_other_node = false;
        for node in &all_nodes_result {
            if node.id == created_node_ids_for_filter_test[0] { found_filter_node_1 = true; }
            if node.id == created_node_ids_for_filter_test[1] { found_filter_node_2 = true; }
            if node.id == created_node_ids_for_filter_test[2] { found_other_node = true; }
        }
        assert!(found_filter_node_1, "Step 14: Did not find FilterNode1 in all nodes list");
        assert!(found_filter_node_2, "Step 14: Did not find FilterNode2 in all nodes list");
        assert!(found_other_node, "Step 14: Did not find OtherNodeForFilter in all nodes list");
        println!("Step 14: Successfully verified GET /nodes endpoint returns all nodes (including test nodes).");
    }


    // Cleanup nodes created for this specific test step
    if !created_node_ids_for_filter_test.is_empty() {
        println!("\nStep 14: Cleaning up test nodes for GET /nodes?type=... test");
        // The /nodes endpoint doesn't take IDs. We need to use /graph/entities/delete with names.
        // For this test, we used generic data, not specific names as IDs.
        // To properly clean up, the nodes should have been created via /graph/entities
        // or the client needs to get their names from the NodeResponse.
        // Assuming the IDs are the names for now for cleanup simplicity, if they were created via /graph/entities
        // Since they were created via POST /nodes, their IDs are UUIDs.
        // A direct DELETE /nodes/{id} would be better here.
        // For now, as a quick fix and demonstration, we'll call DELETE /nodes/{id} for each.

        for node_id_to_delete in created_node_ids_for_filter_test {
             let resp_delete_cleanup = client
                .delete(format!("{}/nodes/{}", BASE_URL, node_id_to_delete))
                .send()
                .await?;
            if !resp_delete_cleanup.status().is_success() {
                eprintln!(
                    "Step 14 Cleanup: Failed to delete node {}. Status: {}. Response: {}",
                    node_id_to_delete,
                    resp_delete_cleanup.status(),
                    resp_delete_cleanup.text().await?
                );
            } else {
                 println!("Step 14 Cleanup: Successfully deleted node {}", node_id_to_delete);
            }
        }
    }
    
    println!("\n--- Full E2E Test Suite Completed ---");


    Ok(())
}