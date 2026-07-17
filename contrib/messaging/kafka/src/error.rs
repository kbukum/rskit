use rdkafka::error::KafkaError;
use rskit_errors::{AppError, ErrorCode};

pub(crate) fn kafka_producer_creation_error(error: &KafkaError) -> AppError {
    AppError::new(
        ErrorCode::Internal,
        format!("failed to create Kafka producer: {error}"),
    )
}

pub(crate) fn kafka_send_error(error: &KafkaError) -> AppError {
    let code = if matches!(
        error,
        KafkaError::MessageProduction(rdkafka::types::RDKafkaErrorCode::QueueFull)
    ) {
        ErrorCode::RateLimited
    } else {
        ErrorCode::ExternalService
    };
    AppError::new(code, format!("Kafka send failed: {error}"))
}

pub(crate) fn kafka_flush_error(error: &KafkaError) -> AppError {
    AppError::new(
        ErrorCode::ExternalService,
        format!("Kafka flush failed: {error}"),
    )
}

pub(crate) fn kafka_consumer_creation_error(error: &KafkaError) -> AppError {
    AppError::new(
        ErrorCode::Internal,
        format!("failed to create Kafka consumer: {error}"),
    )
}

pub(crate) fn kafka_subscribe_error(error: &KafkaError) -> AppError {
    AppError::new(
        ErrorCode::ExternalService,
        format!("Kafka subscribe failed: {error}"),
    )
}

pub(crate) fn kafka_stream_ended_error() -> AppError {
    AppError::new(
        ErrorCode::ExternalService,
        "Kafka stream ended unexpectedly",
    )
}

pub(crate) fn kafka_receive_error(error: &KafkaError) -> AppError {
    AppError::new(
        ErrorCode::ExternalService,
        format!("Kafka receive error: {error}"),
    )
}
