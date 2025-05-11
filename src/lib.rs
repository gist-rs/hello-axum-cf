mod solana;

use currency_rs::CurrencyOpts;
use rand::Rng;
use worker::*;

async fn handle_balance_request(req: Request, _ctx: RouteContext<()>) -> Result<Response> {
    // Extract wallet_address from query parameters
    let url = req.url()?;
    let mut wallet_address_opt: Option<String> = None;
    for (key, value) in url.query_pairs() {
        if key == "wallet_address" {
            wallet_address_opt = Some(value.into_owned());
            break;
        }
    }

    match wallet_address_opt {
        Some(wallet_address) => {
            let options = solana::GetBalanceOptions {
                rpc_url: "https://api.mainnet-beta.solana.com",
                id: rand::thread_rng().gen_range(0u32..u32::MAX),
                currency_opts: Some(
                    CurrencyOpts::new()
                        .set_precision(2)
                        .set_symbol("")
                        .set_separator(",")
                        .set_decimal("."),
                ),
            };

            // solana::get_balance returns anyhow::Result<solana::UiBalance>
            // We need to map this to worker::Result<worker::Response>
            match solana::get_balance(wallet_address.clone(), options).await {
                Ok(balance_info) => {
                    let response_string = format!(
                        "Balance for {}: {}",
                        wallet_address, balance_info.ui_lamports
                    );
                    Response::ok(response_string)
                }
                Err(e) => {
                    console_error!(
                        "Error fetching balance from Solana for wallet {}: {}",
                        wallet_address,
                        e.to_string()
                    );
                    // Return a user-friendly error response
                    Response::error(format!("Failed to get balance: {}", e), 500)
                }
            }
        }
        None => Response::ok(
            "Please provide a wallet_address query parameter, e.g., /?wallet_address=YOUR_ADDRESS",
        ),
    }
}

#[event(fetch)]
pub async fn main(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    // Set up panic hook for better error messages in Cloudflare console

    let router = Router::new();

    router
        .get_async("/", handle_balance_request)
        .run(req, env)
        .await
}
