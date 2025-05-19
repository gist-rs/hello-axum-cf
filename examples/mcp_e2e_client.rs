use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};

const MCP_BASE_URL: &str = "http://localhost:8787/mcp"; // Adjust if your worker runs elsewhere

// --- MCP Generic Request/Response Structs (Client-Side) ---
#[derive(Serialize)]
struct CallToolRequestParams<T: Serialize> {
    name: String,
    arguments: T,
}

#[derive(Deserialize, Debug)]
struct ContentBlock {
    #[serde(rename = "type")]
    block_type: String, // Expected "text"
    text: String,
}

#[derive(Deserialize, Debug)]
struct CallToolResponse {
    content: Vec<ContentBlock>,
}

// --- Structs for `create_entities` Tool (Client-Side) ---
#[derive(Serialize, Debug)]
struct McpEntityToCreate {
    name: String,
    #[serde(rename = "entityType")]
    entity_type: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    observations: Vec<String>,
    // MCP `create_entities` in mcp.rs currently doesn't take a generic `data` field
    // for McpEntityToCreate, matching the TypeScript server's Entity interface.
    // If your DO's EntityToCreate requires `data`, the mcp.rs handler for create_entities
    // and this client struct would need to be updated.
    // data: Option<JsonValue>,
}

#[derive(Serialize, Debug)]
struct McpCreateEntitiesArgs {
    entities: Vec<McpEntityToCreate>,
}

// --- Structs for `delete_entities` Tool (Client-Side) ---\n
#[derive(Serialize, Debug)]
struct McpDeleteEntitiesArgs {
    #[serde(rename = "entityNames")]
    entity_names: Vec<String>,
}

// Struct to parse the JSON string within `ContentBlock.text` for `create_entities` response
// This should match the structure of `Node` from your `types.rs` or a client-specific version.
#[derive(Debug, Serialize, Deserialize, Clone)]
struct ClientNodeResponse {
    id: String,
    #[serde(rename = "type")]
    node_type: String,
    data: JsonValue,
    created_at_ms: u64,
    updated_at_ms: u64,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new();
    let mcp_tool_call_url = format!("{}/tool/call", MCP_BASE_URL);

    println!(
        "Starting E2E test against MCP KnowledgeGraph at {}",
        MCP_BASE_URL
    );

    // --- Pre-Step: Delete entities if they exist to ensure clean state for create ---\n
    println!("\n--- MCP: Pre-Step - Call `delete_entities` Tool ---");
    let entities_to_delete_names =
        vec!["mcp_blogpost_789".to_string(), "mcp_tag_ai".to_string()];
    let delete_entities_payload = McpDeleteEntitiesArgs {
        entity_names: entities_to_delete_names.clone(),
    };
    let mcp_delete_request_body = CallToolRequestParams {
        name: "delete_entities".to_string(),
        arguments: delete_entities_payload,
    };

    println!(
        "Sending to {}: {}",
        mcp_tool_call_url,
        json!(mcp_delete_request_body)
    );

    let delete_resp = client
        .post(&mcp_tool_call_url)
        .json(&mcp_delete_request_body)
        .send()
        .await?;

    if !delete_resp.status().is_success() {
        eprintln!(
            "MCP Pre-Step `delete_entities` failed. Status: {}. Response: {}",
            delete_resp.status(),
            delete_resp.text().await?
        );
        // Depending on strictness, you might want to return an error here
    } else {
        let delete_response_text = delete_resp.text().await?;
        println!(
            "MCP Pre-Step `delete_entities` raw response: {}",
            delete_response_text
        );
        // The `delete_entities` tool in mcp.rs currently returns a simple success message,
        // not a list of deleted IDs in the same way the DO does.
        // We can parse the CallToolResponse to check the text.
        match serde_json::from_str::<CallToolResponse>(&delete_response_text) {
            Ok(parsed_delete_resp) => {
                if let Some(content) = parsed_delete_resp.content.first() {
                    println!(
                        "MCP Pre-Step `delete_entities` success message: {}",
                        content.text
                    );
                }
            }
            Err(e) => {
                eprintln!(
                    "MCP Pre-Step `delete_entities`: Could not parse response: {}",
                    e
                );
            }
        }
    }

    // --- Step 1: Call `create_entities` Tool ---
    println!("\n--- MCP: Call `create_entities` Tool ---");
    let entities_to_create_payload = McpCreateEntitiesArgs {
        entities: vec![
            McpEntityToCreate {
                name: "mcp_blogpost_789".to_string(),
                entity_type: "BlogPost".to_string(),
                observations: vec!["MCP Test Post".to_string(), "First draft".to_string()],
                // data: Some(json!({ "title": "MCP Blog Post", "status": "new" })),
            },
            McpEntityToCreate {
                name: "mcp_tag_ai".to_string(),
                entity_type: "Tag".to_string(),
                observations: vec!["Related to Artificial Intelligence".to_string()],
                // data: Some(json!({ "slug": "ai-mcp" })),
            },
        ],
    };

    let mcp_request_body = CallToolRequestParams {
        name: "create_entities".to_string(),
        arguments: entities_to_create_payload,
    };

    println!(
        "Sending to {}: {}",
        mcp_tool_call_url,
        json!(mcp_request_body)
    );

    let resp = client
        .post(&mcp_tool_call_url)
        .json(&mcp_request_body)
        .send()
        .await?;

    if !resp.status().is_success() {
        eprintln!(
            "MCP `create_entities` failed. Status: {}. Response: {}",
            resp.status(),
            resp.text().await?
        );
        return Ok(());
    }

    let mcp_response_text = resp.text().await?;
    println!("MCP `create_entities` raw response: {}", mcp_response_text);

    // Parse the MCP response structure
    let parsed_mcp_response: CallToolResponse = serde_json::from_str(&mcp_response_text)?;

    if let Some(content_block) = parsed_mcp_response.content.first() {
        if content_block.block_type == "text" {
            println!(
                "MCP `create_entities` inner JSON string: {}",
                content_block.text
            );
            // Parse the inner JSON string (which is the actual result from the DO)
            let created_entities_result: Result<Vec<ClientNodeResponse>, _> =
                serde_json::from_str(&content_block.text);

            match created_entities_result {
                Ok(created_entities) => {
                    println!(
                        "Successfully parsed created entities: {:?}",
                        created_entities
                    );
                    if created_entities.len() == 2 {
                        println!("SUCCESS: MCP `create_entities` call seems successful and returned 2 entities.");
                    } else {
                        eprintln!(
                            "FAILURE: MCP `create_entities` did not return the expected number of entities. Got {}",
                            created_entities.len()
                        );
                    }
                }
                Err(e) => {
                    eprintln!(
                        "FAILURE: Could not parse inner JSON from `create_entities` response: {}",
                        e
                    );
                    eprintln!("Inner JSON was: {}", content_block.text);
                }
            }
        } else {
            eprintln!(
                "FAILURE: Unexpected content block type: {}",
                content_block.block_type
            );
        }
    } else {
        eprintln!("FAILURE: MCP response content was empty.");
    }

    println!("\n--- MCP E2E Test (create_entities) Completed ---");

    Ok(())
}