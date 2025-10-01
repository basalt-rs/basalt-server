use std::{path::PathBuf, sync::Arc};

use bedrock::Config;
use tokio::{sync::mpsc::UnboundedSender, task::JoinSet};

use crate::{
    server::{hooks::events::ServerEvent, AppState},
    storage::SqliteLayer,
};

pub async fn init_state_with_hooks(
    db: SqliteLayer,
    cfg: Config,
    webdir: Option<PathBuf>,
) -> anyhow::Result<(Arc<AppState>, JoinSet<()>)> {
    let mut dispatchers: Vec<UnboundedSender<(ServerEvent, Arc<AppState>)>> = Vec::new();
    #[cfg(feature = "scripting")]
    let (mut hook_handler, hooks_tx) = crate::server::hooks::handlers::EventHookHandler::create();
    #[cfg(feature = "scripting")]
    dispatchers.push(hooks_tx);
    #[cfg(feature = "webhooks")]
    let (mut webhook_handler, webhooks_tx) =
        crate::server::hooks::webhooks::EventWebhookHandler::create();
    #[cfg(feature = "webhooks")]
    dispatchers.push(webhooks_tx);

    let mut jset: tokio::task::JoinSet<()> = tokio::task::JoinSet::new();

    let mut app_state = AppState::new(db, cfg, dispatchers, webdir);
    app_state.init().await?;
    let app_state = Arc::new(app_state);

    #[cfg(feature = "scripting")]
    jset.spawn(async move { hook_handler.start().await });

    #[cfg(feature = "webhooks")]
    jset.spawn(async move { webhook_handler.start().await });

    Ok((app_state, jset))
}
