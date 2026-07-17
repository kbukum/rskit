use std::time::Duration;

use lapin::options::{BasicQosOptions, QueueDeclareOptions};
use lapin::types::FieldTable;
use lapin::{Connection, ConnectionProperties};
use rskit_errors::AppResult;

use crate::Config;
use crate::error::{
    connect_failed, connect_timed_out, qos_configuration_failed, queue_declare_failed,
};

pub(crate) async fn connect(config: &Config) -> AppResult<Connection> {
    tokio::time::timeout(
        Duration::from_millis(config.connection_timeout),
        Connection::connect(&config.uri, ConnectionProperties::default()),
    )
    .await
    .map_err(connect_timed_out)?
    .map_err(connect_failed)
}

pub(crate) async fn configure_qos(channel: &lapin::Channel, prefetch_count: u16) -> AppResult<()> {
    channel
        .basic_qos(prefetch_count, BasicQosOptions::default())
        .await
        .map_err(qos_configuration_failed)
}

pub(crate) async fn declare_queue(
    channel: &lapin::Channel,
    queue: &str,
    durable: bool,
) -> AppResult<()> {
    channel
        .queue_declare(
            queue.into(),
            QueueDeclareOptions {
                durable,
                ..QueueDeclareOptions::default()
            },
            FieldTable::default(),
        )
        .await
        .map_err(queue_declare_failed)?;
    Ok(())
}
