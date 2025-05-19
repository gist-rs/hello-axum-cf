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
                "dokg-memory worker is running. Use /do/... to interact with the KnowledgeGraphDO.",
            )
        })
        .on_async("/do/*path", |worker_req, route_ctx| async move {
            let env = route_ctx.env.clone();
            let durable_object_binding_name = "KNOWLEDGE_GRAPH_DO";

            let namespace = match env.durable_object(durable_object_binding_name) {
                Ok(ns) => ns,
                Err(e) => {
                    console_error!("[WORKER LIB /do/:path] Error getting DO namespace: {}", e);
                    return Response::error(format!("Error getting DO namespace: {}", e), 500);
                }
            };

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

            let stub = match id.get_stub() {
                Ok(s) => s,
                Err(e) => {
                    console_error!("[WORKER LIB /do/:path] Error getting DO stub: {}", e);
                    return Response::error(format!("Error getting DO stub: {}", e), 500);
                }
            };

            let path_param = match route_ctx.param("path") {
                Some(p) => p.to_string(),
                None => String::new(),
            };

            // path_param is the raw path segment from the router (e.g., "nodes/123/related" from an incoming URL like /do/nodes/123/related?k=v).
            // We need to construct the DO's internal request path by prepending "/" and potentially appending the original query string.
            let mut internal_path_for_do = format!("/{}", path_param);

            // Try to get the original query string from worker_req and append it.
            if let Ok(url_obj) = worker_req.url() {
                // Safely access the Url object from the original request
                if let Some(query_str) = url_obj.query() {
                    // Get the query string part (e.g., "k=v")
                    if !query_str.is_empty() {
                        internal_path_for_do.push('?');
                        internal_path_for_do.push_str(query_str); // Append "?<query_string>"
                    }
                }
            }
            // If worker_req.url() fails or there's no query string, internal_path_for_do remains /<path_param>.

            let do_internal_path = internal_path_for_do; // Use this complete path for the DO request.
                                                         // Use a dummy base URL as required by the Request constructor for stubs
                                                         // The actual hostname doesn't matter as it's not used for routing to the DO.
                                                         // do_internal_path now includes the original query string, if present.
            let full_do_url = format!("https://durable-object.internal-url{}", do_internal_path);

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
            let do_req = Request::new_with_init(&full_do_url, &do_req_init)?;

            // Send the request to the Durable Object instance and return its response.
            stub.fetch_with_request(do_req).await
        })
        .run(req, env)
        .await
}
