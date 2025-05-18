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
        .get_async("/do/*path", |worker_req, route_ctx| async move {
            let env = route_ctx.env.clone();
            // This name MUST match the binding name in your wrangler.toml
            let durable_object_binding_name = "KNOWLEDGE_GRAPH_DO";

            let namespace = env.durable_object(durable_object_binding_name)?;

            // For simplicity in this example, using a fixed name for the DO instance.
            // In a real application, this might come from user authentication,
            // a path parameter for a specific graph, or a query parameter.
            let do_id_name = "default_knowledge_graph"; // You can make this dynamic
            let id = namespace.id_from_name(do_id_name)?;
            let stub = id.get_stub()?;

            // Extract the path intended for the Durable Object.
            // If the worker request is GET /do/nodes/some-id, the path for the DO will be /nodes/some-id.
            let path_param = route_ctx
                .param("path")
                .map(|s| s.to_string())
                .unwrap_or_default(); // Default to empty string if no path, DO will handle /

            let do_request_path = format!("/{}", path_param);

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

            // Clone the body if it's a POST or PUT request
            let body = if worker_req.method() == Method::Post || worker_req.method() == Method::Put
            {
                worker_req.clone()?.bytes().await?
            } else {
                Vec::new() // Empty body for GET, DELETE etc.
            };

            do_req_init.with_body(Some(body.into()));

            let do_req = Request::new_with_init(&do_request_path, &do_req_init)?;

            // Send the request to the Durable Object instance and return its response.
            stub.fetch_with_request(do_req).await
        })
        .run(req, env)
        .await
}
