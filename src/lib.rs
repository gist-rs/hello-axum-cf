mod solana;
use axum::extract::Query;
use axum::http::StatusCode;
use axum::{routing::get, Router};
use axum_macros::debug_handler;
use currency_rs::CurrencyOpts;
use rand::Rng;
use serde::Deserialize;
use tower_service::Service;
use worker::*;

fn router() -> Router {
    Router::new().route("/", get(root))
}

#[derive(Deserialize)]
pub struct WalletQuery {
    wallet_address: Option<String>,
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

#[debug_handler]
pub async fn root(
    Query(params): Query<WalletQuery>,
) -> std::result::Result<String, (StatusCode, String)> {
    match params.wallet_address {
        Some(wallet_address) => {
            let options = solana::GetBalanceOptions {
                rpc_url: "https://rpc.ankr.com/solana",
                id: rand::thread_rng().gen_range(0u32..u32::MAX),
                currency_opts: Some(
                    CurrencyOpts::new()
                        .set_precision(2)
                        .set_symbol("")
                        .set_separator(",")
                        .set_decimal("."),
                ),
            };
            solana::get_balance(wallet_address.to_string(), options)
                .await
                .map_err(|e| {
                    eprintln!("Error fetching balance: {}", e);
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("Failed to get balance: {}", e),
                    )
                })
                .map(|balance| format!("Balance for {}: {}", wallet_address, balance.ui_lamports))
        }
        None => Ok(
            "Please provide a wallet_address query parameter, e.g., /?wallet_address=YOUR_ADDRESS"
                .to_string(),
        )
    }
}
