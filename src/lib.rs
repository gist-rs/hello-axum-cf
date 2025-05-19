use worker::*;

// Declare the module where KnowledgeGraphDO is defined.
mod do_memory;

// Re-export KnowledgeGraphDO so it's part of the library's public API
// and can be recognized by wrangler for Durable Object bindings.
pub use do_memory::KnowledgeGraphDO;

#[event(start)]
pub fn start() {
    // Initialize the panic hook for better error messages.
    console_error_panic_hook::set_once();
}

#[event(fetch)]
pub async fn main(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    // Note: The "hello world" paths have been updated to reflect the generic DO API.
    // Specific test paths like /test-mock-data or /test-decision-context were removed
    // when do_memory.rs was made fully generic.
    // The client (e.g., rust_e2e_client.rs) now handles creating specific types of nodes/edges.
    let router = Router::new();

    router
        .get_async("/", |_req, _ctx| async move {
            Response::ok(
                "mcp-memory worker is running. Use /do/... to interact with the KnowledgeGraphDO.",
            )
        })
        .on_async("/do/:path", |worker_req, route_ctx| async move {
            console_log!(
                "[WORKER LIB /do/:path] Entered handler for worker_req path: {}",
                worker_req.path()
            );

            let env = route_ctx.env.clone();
            let durable_object_binding_name = "KNOWLEDGE_GRAPH_DO";

            let namespace = match env.durable_object(durable_object_binding_name) {
                Ok(ns) => ns,
                Err(e) => {
                    console_error!("[WORKER LIB /do/:path] Error getting DO namespace: {}", e);
                    return Response::error(format!("Error getting DO namespace: {}", e), 500);
                }
            };
            console_log!("[WORKER LIB /do/:path] Got DO namespace.");

            let do_id_name = "default_knowledge_graph";
            let id = match namespace.id_from_name(do_id_name) {
                Ok(i) => i,
                Err(e) => {
                    console_error!(
                        "[WORKER LIB /do/:path] Error getting DO ID from name: {}",
                        e
                    );
                    return Response::error(format!("Error getting DO ID from name: {}", e), 500);
                }
            };
            console_log!("[WORKER LIB /do/:path] Got DO ID from name.");

            let stub = match id.get_stub() {
                Ok(s) => s,
                Err(e) => {
                    console_error!("[WORKER LIB /do/:path] Error getting DO stub: {}", e);
                    return Response::error(format!("Error getting DO stub: {}", e), 500);
                }
            };
            console_log!("[WORKER LIB /do/:path] Got DO stub.");

            let path_param = match route_ctx.param("path") {
                Some(p) => p.to_string(),
                None => {
                    console_log!(
                        "[WORKER LIB /do/:path] 'path' param is None, defaulting to empty string."
                    );
                    String::new()
                }
            };
            console_log!("[WORKER LIB /do/:path] path_param: '{}'", path_param);

            let do_internal_path = format!("/{}", path_param);
            // Use a dummy base URL as required by the Request constructor for stubs
            // The actual hostname doesn't matter as it's not used for routing to the DO.
            let full_do_url = format!("https://durable-object.internal-url{}", do_internal_path);

            // console_log!(
            //     "Worker forwarding request: {} to DO with path: {}",
            //     worker_req.url()?.path(),
            //     do_request_path
            // );

            // Construct a new request to send to the Durable Object.
            // We forward the method. Body handling is important for POST/PUT.
            let mut do_req_init = RequestInit::new();
            do_req_init.with_method(worker_req.method());

            // Forward essential headers if necessary (e.g., Content-Type)
            // For simplicity, we're not forwarding all headers here, but you might need to.
            if let Some(content_type) = worker_req.headers().get("content-type")? {
                let mut do_headers = Headers::new();
                do_headers.set("content-type", &content_type)?;
                do_req_init.with_headers(do_headers);
            }

            // Conditionally set the body for methods like POST, PUT, PATCH
            let method = worker_req.method();
            if method == Method::Post || method == Method::Put || method == Method::Patch {
                // Clone the original request to read its body for forwarding
                let body_bytes = worker_req.clone()?.bytes().await?;
                do_req_init.with_body(Some(body_bytes.into()));
            }
            // For GET, DELETE, OPTIONS, HEAD etc., do not set a body.
            // RequestInit::new() defaults to `body: None`.

            // Construct the request with the full dummy URL
            console_log!(
                "[WORKER LIB] Attempting to create DO request with URL: '{}'",
                full_do_url
            );
            let do_req = Request::new_with_init(&full_do_url, &do_req_init)?;

            // Send the request to the Durable Object instance and return its response.
            stub.fetch_with_request(do_req).await
        })
        .run(req, env)
        .await
}
