mod api;
mod bundle;
mod error;
mod inputs;
mod runner;
mod summary;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter("info")
        .with_target(false)
        .init();

    let exit_code = runner::run().await;
    std::process::exit(exit_code);
}
