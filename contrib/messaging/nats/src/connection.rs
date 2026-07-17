use std::time::Duration;

use async_nats::{Client, ConnectOptions};

use crate::Config;

pub(crate) async fn connect(config: &Config) -> Result<Client, async_nats::ConnectError> {
    connect_options(config)
        .connect(config.servers.clone())
        .await
}

pub(crate) fn connect_options(config: &Config) -> ConnectOptions {
    let mut options = ConnectOptions::new()
        .name(config.base.name.clone())
        .connection_timeout(Duration::from_millis(config.connection_timeout))
        .request_timeout(config.base.request_timeout_duration())
        .max_reconnects(config.max_reconnects)
        .client_capacity(config.base.max_in_flight);

    let reconnect_delay = Duration::from_millis(config.reconnect_delay);
    options = options.reconnect_delay_callback(move |_| reconnect_delay);

    if let Some(token) = config.token.as_ref() {
        options = options.token(token.clone());
    }
    if let (Some(username), Some(password)) = (config.username.as_ref(), config.password.as_ref()) {
        options = options.user_and_password(username.clone(), password.clone());
    }

    options
}
