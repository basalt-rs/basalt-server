use std::{path::PathBuf, sync::Arc};

use bedrock::Config;
use tokio::{sync::mpsc::UnboundedSender, task::JoinSet};

use crate::{
    server::{hooks::events::ServerEvent, AppState},
    storage::SqliteLayer,
};

#[allow(unused_mut)]
pub fn init_state_with_hooks(
    db: SqliteLayer,
    cfg: Config,
    webdir: Option<PathBuf>,
) -> (Arc<AppState>, JoinSet<()>) {
    let mut dispatchers: Vec<UnboundedSender<(ServerEvent, Arc<AppState>)>> = Vec::new();
    #[cfg(feature = "scripting")]
    let (mut hook_handler, hooks_tx) = crate::server::hooks::handler::EventHookHandler::create();
    #[cfg(feature = "scripting")]
    dispatchers.push(hooks_tx);
    #[cfg(feature = "webhooks")]
    let (mut webhook_handler, webhooks_tx) =
        crate::server::hooks::handler::EventWebhookHandler::create();
    #[cfg(feature = "webhooks")]
    dispatchers.push(webhooks_tx);

    let mut jset: tokio::task::JoinSet<()> = tokio::task::JoinSet::new();

    let app_state = Arc::new(AppState::new(db, cfg, dispatchers, webdir));

    #[cfg(feature = "scripting")]
    jset.spawn(async move { hook_handler.start().await });

    #[cfg(feature = "webhooks")]
    jset.spawn(async move { webhook_handler.start().await });

    (app_state, jset)
}
