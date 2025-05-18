# hello-world-cloudflare

## Setup (once)
```shell
rustup target add wasm32-unknown-unknown
cargo install cargo-generate
```

## Setup new project (optional)
```shell
# From template
cargo generate cloudflare/workers-rs

# Or
npx wrangler init
```

## Or from existing source
```shell
git clone https://github.com/gist-rs/hello-world-cloudflare
cd hello-world-cloudflare
```

## Run
```shell
npx wrangler dev
```

## Deploy
```shell
npx wrangler login
npx wrangler deploy
```

## Build and Deploy on Cloudflare
```shell
npx wrangler deploy -e production
```

## Try
```shell
http://localhost:8787
```
