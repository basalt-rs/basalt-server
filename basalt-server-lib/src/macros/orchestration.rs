#[macro_export]
macro_rules! init_state_with_hooks {
    ($db:expr, $config:expr, $webdir:expr) => {{
        #[cfg(feature = "scripting")]
        let (mut hook_handler, hooks_tx) =
            basalt_server_lib::server::hooks::handler::EventHookHandler::create();
        #[cfg(feature = "webhooks")]
        let (mut webhook_handler, webhooks_tx) =
            basalt_server_lib::server::hooks::handler::EventWebhookHandler::create();

        let hook_dispatcher = EventDispatcherService::new(
            #[cfg(feature = "scripting")]
            hooks_tx,
            #[cfg(feature = "webhooks")]
            webhooks_tx,
        );

        let mut jset: tokio::task::JoinSet<()> = tokio::task::JoinSet::new();

        let app_state = Arc::new(AppState::new($db, $config, hook_dispatcher, $webdir));

        #[cfg(feature = "scripting")]
        jset.spawn({
            let app_state = app_state.clone();
            async move { hook_handler.start(app_state).await }
        });

        #[cfg(feature = "webhooks")]
        jset.spawn({
            let app_state = app_state.clone();
            async move { webhook_handler.start(app_state).await }
        });

        (app_state, jset)
    }};
}
