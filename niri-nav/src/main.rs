mod button;
mod input;
mod ipc_server;
mod modifiers;
mod profile;
mod profiles;
mod runtime;

use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let profile = profiles::ActiveProfile::new();
    runtime::run(profile).await
}
