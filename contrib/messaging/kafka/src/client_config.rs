use rdkafka::config::ClientConfig;
use rskit_messaging::{CommitStrategy, DeliveryGuarantee};

use crate::{Compression, Config, OffsetReset};

fn base_client_config(config: &Config) -> ClientConfig {
    let mut cfg = ClientConfig::new();
    cfg.set("bootstrap.servers", config.brokers.join(","));
    cfg.set("security.protocol", config.security_protocol.to_string());

    if let Some(ref mechanism) = config.sasl_mechanism {
        cfg.set("sasl.mechanism", mechanism);
    }
    if let Some(ref username) = config.sasl_username {
        cfg.set("sasl.username", username);
    }
    if let Some(ref password) = config.sasl_password {
        cfg.set("sasl.password", password);
    }
    if let Some(timeout) = config.base.request_timeout {
        cfg.set("request.timeout.ms", timeout.to_string());
    }
    cfg.set("retry.backoff.ms", config.base.retry_backoff.to_string());

    cfg
}

pub(crate) fn producer_config(config: &Config) -> ClientConfig {
    let mut cfg = base_client_config(config);
    let compression = match config.compression {
        Compression::None => "none",
        Compression::Gzip => "gzip",
        Compression::Snappy => "snappy",
        Compression::Lz4 => "lz4",
        Compression::Zstd => "zstd",
    };
    cfg.set("compression.type", compression);
    cfg.set("batch.size", config.batch_size.to_string());
    cfg.set("linger.ms", config.linger_ms.to_string());
    cfg.set(
        "queue.buffering.max.messages",
        config.queue_capacity.to_string(),
    );
    cfg.set("message.send.max.retries", config.base.retries.to_string());
    cfg.set(
        "acks",
        match config.base.delivery_guarantee {
            DeliveryGuarantee::AtMostOnce => "0",
            _ => "all",
        },
    );
    cfg.set(
        "max.in.flight.requests.per.connection",
        config.base.max_in_flight.to_string(),
    );
    cfg
}

pub(crate) fn consumer_config(config: &Config) -> ClientConfig {
    let mut cfg = base_client_config(config);
    if let Some(group) = config.effective_group_id() {
        cfg.set("group.id", group);
    }
    let offset = match config.auto_offset_reset {
        OffsetReset::Latest => "latest",
        OffsetReset::Earliest => "earliest",
    };
    cfg.set("auto.offset.reset", offset);
    let auto_commit = matches!(config.base.commit_strategy, CommitStrategy::Auto);
    cfg.set("enable.auto.commit", auto_commit.to_string());
    cfg.set(
        "session.timeout.ms",
        config.session_timeout.as_millis().to_string(),
    );
    cfg
}
