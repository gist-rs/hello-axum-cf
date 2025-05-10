use axum::{routing::get, Router};
use axum::extract::Query;
use tower_service::Service;
use worker::*;
use serde::Deserialize;

fn router() -> Router {
    Router::new().route("/", get(root))
}

#[derive(Deserialize)]
struct WalletQuery {
    wallet_address: String,
}

#[event(fetch)]
async fn fetch(
    req: HttpRequest,
    _env: Env,
    _ctx: Context,
) -> Result<axum::http::Response<axum::body::Body>> {
    console_error_panic_hook::set_once();
    Ok(router().call(req).await?)
}

pub async fn root(Query(params): Query<WalletQuery>) -> String {
    format!("Hello Axum! Wallet Address: {}", params.wallet_address)
}
