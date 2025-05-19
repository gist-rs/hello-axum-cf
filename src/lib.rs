use worker::*;

// Declare the new modules
mod kg;
mod mcp;
mod types;
mod worker_do;

// Re-export KnowledgeGraphDO from the `worker_do` module
// and can be recognized by wrangler for Durable Object bindings.
pub use worker_do::KnowledgeGraphDO;

#[event(start)]
pub fn start() {
    // Initialize the panic hook for better error messages.
    console_error_panic_hook::set_once();
}

#[event(fetch)]
pub async fn main(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    let mut router = Router::new();

    router = router
        .get_async("/", |_req, _ctx| async move {
            Response::ok(
                "mcp-memory worker is running. Use /do/... for direct DO interaction or /mcp/... for MCP.",
            )
        })
        .on_async("/do/*path", |worker_req, route_ctx| async move {
            // Existing logic for /do/*path to forward to Durable Object
            let env = route_ctx.env.clone();
            let durable_object_binding_name = "KNOWLEDGE_GRAPH_DO";

            let namespace = match env.durable_object(durable_object_binding_name) {
                Ok(ns) => ns,
                Err(e) => {
                    console_error!("Failed to get Durable Object namespace '{}': {}", durable_object_binding_name, e);
                    return Response::error(format!("Error getting DO namespace: {}", e), 500);
                }
            };

            let do_id_name = "default_knowledge_graph"; // Consider making this configurable or dynamic
            let id = match namespace.id_from_name(do_id_name) {
                Ok(i) => i,
                Err(e) => {
                    console_error!(
                        "Failed to get Durable Object ID from name '{}' for namespace '{}': {}",
                        do_id_name, durable_object_binding_name, e
                    );
                    return Response::error(format!("Error getting DO ID from name: {}", e), 500);
                }
            };

            let stub = match id.get_stub() {
                Ok(s) => s,
                Err(e) => {
                    console_error!("Failed to get Durable Object stub for ID '{}': {}", id, e);
                    return Response::error(format!("Error getting DO stub: {}", e), 500);
                }
            };

            let path_param = match route_ctx.param("path") {
                Some(p) => p.to_string(),
                None => String::new(), // Or handle as an error
            };

            let mut internal_path_for_do = format!("/{}", path_param);
            if let Ok(url_obj) = worker_req.url() {
                if let Some(query_str) = url_obj.query() {
                    if !query_str.is_empty() {
                        internal_path_for_do.push('?');
                        internal_path_for_do.push_str(query_str);
                    }
                }
            }

            let full_do_url = format!("https://durable-object.internal-url{}", internal_path_for_do);
            let mut do_req_init = RequestInit::new();
            do_req_init.with_method(worker_req.method());

            if let Some(content_type) = worker_req.headers().get("content-type")? {
                let mut do_headers = Headers::new();
                do_headers.set("content-type", &content_type)?;
                do_req_init.with_headers(do_headers);
            }

            let method = worker_req.method();
            if method == Method::Post || method == Method::Put || method == Method::Patch {
                if let Ok(mut cloned_req) = worker_req.clone()  { // Ensure cloning is successful and make the clone mutable
                    let body_bytes = cloned_req.bytes().await?;
                    do_req_init.with_body(Some(body_bytes.into()));
                } else {
                     return Response::error("Failed to clone request for body forwarding", 500);
                }
            }

            let do_req = Request::new_with_init(&full_do_url, &do_req_init)?;
            stub.fetch_with_request(do_req).await
        });

    // Conditionally add MCP routes if "mcp" feature is enabled

    {
        router = router
            .get_async("/mcp/tools", |_req, _ctx| async move {
                mcp::list_tools_handler().await
            })
            .post_async("/mcp/tool/call", |worker_req, route_ctx| async move {
                // Removed mut from worker_req
                // MCP tool calls need access to the DO stub
                let env = route_ctx.env.clone();
                let durable_object_binding_name = "KNOWLEDGE_GRAPH_DO";

                let namespace = match env.durable_object(durable_object_binding_name) {
                    Ok(ns) => ns,
                    Err(e) => {
                        console_error!(
                            "MCP: Failed to get DO namespace '{}': {}",
                            durable_object_binding_name,
                            e
                        );
                        // Return an MCP-formatted error
                        let err_resp = serde_json::json!({
                            "error": {
                                "code": "NamespaceError",
                                "message": format!("Error getting DO namespace: {}", e)
                            }
                        });
                        return Response::from_json(&err_resp).map(|r| r.with_status(500));
                    }
                };

                let do_id_name = "default_knowledge_graph";
                let id = match namespace.id_from_name(do_id_name) {
                    Ok(i) => i,
                    Err(e) => {
                        console_error!(
                            "MCP: Failed to get DO ID from name '{}' for namespace '{}': {}",
                            do_id_name,
                            durable_object_binding_name,
                            e
                        );
                        let err_resp = serde_json::json!({
                            "error": {
                                "code": "DurableObjectIdError",
                                "message": format!("Error getting DO ID from name: {}", e)
                            }
                        });
                        return Response::from_json(&err_resp).map(|r| r.with_status(500));
                    }
                };

                let stub = match id.get_stub() {
                    Ok(s) => s,
                    Err(e) => {
                        console_error!("MCP: Failed to get DO stub for ID '{}': {}", id, e);
                        let err_resp = serde_json::json!({
                            "error": {
                                "code": "StubError",
                                "message": format!("Error getting DO stub: {}", e)
                            }
                        });
                        return Response::from_json(&err_resp).map(|r| r.with_status(500));
                    }
                };
                mcp::call_tool_handler(worker_req, stub).await
            });
    }

    router.run(req, env).await
}
