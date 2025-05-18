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
//    You can compile and run it as a standalone file (if you have rustc and the crates downloaded):
//    rustc rust_e2e_client.rs -L /path/to/your/compiled/crates
//    ./rust_e2e_client
//    A more common way is to create a temporary Cargo project:
//    cargo new --bin temp_e2e_client
//    cd temp_e2e_client
//    # Add dependencies to Cargo.toml
//    # Replace src/main.rs with this file's content
//    cargo run

use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::json; // For creating JSON bodies easily
use serde_json::Value as JsonValue; // For generic data fields

const BASE_URL: &str = "http://localhost:8787/do"; // Adjust if your worker runs elsewhere

// Simplified structs to deserialize responses from the DO
// We mainly care about the 'id' for subsequent requests.
#[derive(Debug, Deserialize)]
struct NodeResponse {
    id: String,
    #[serde(rename = "type")]
    node_type: String,
    data: JsonValue,
    created_at_ms: u64,
    updated_at_ms: u64,
}

#[derive(Debug, Deserialize)]
struct EdgeResponse {
    id: String,
    #[serde(rename = "type")]
    edge_type: String,
    source_node_id: String,
    target_node_id: String,
    data: Option<JsonValue>,
    created_at_ms: u64,
}

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
            "Error creating user node: {} - {}",
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
            "Error creating wallet node: {} - {}",
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
            "Error creating edge: {} - {}",
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
            "Error querying related nodes: {} - {}",
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

    Ok(())
}
