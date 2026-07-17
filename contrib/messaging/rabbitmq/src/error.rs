use std::fmt;

use rskit_errors::{AppError, ErrorCode};

fn rabbitmq_external_error(context: &str, error: impl fmt::Display) -> AppError {
    AppError::new(
        ErrorCode::ExternalService,
        format!("RabbitMQ {context}: {error}"),
    )
}

pub(crate) fn channel_failed(error: lapin::Error) -> AppError {
    rabbitmq_external_error("channel failed", error)
}

pub(crate) fn publish_failed(error: lapin::Error) -> AppError {
    rabbitmq_external_error("publish failed", error)
}

pub(crate) fn publish_confirm_failed(error: lapin::Error) -> AppError {
    rabbitmq_external_error("publish confirm failed", error)
}

pub(crate) fn channel_close_failed(error: lapin::Error) -> AppError {
    rabbitmq_external_error("channel close failed", error)
}

pub(crate) fn connection_close_failed(error: lapin::Error) -> AppError {
    rabbitmq_external_error("connection close failed", error)
}

pub(crate) fn consume_failed(error: lapin::Error) -> AppError {
    rabbitmq_external_error("consume failed", error)
}

pub(crate) fn connect_timed_out(error: impl fmt::Display) -> AppError {
    rabbitmq_external_error("connect timed out", error)
}

pub(crate) fn connect_failed(error: lapin::Error) -> AppError {
    rabbitmq_external_error("connect failed", error)
}

pub(crate) fn qos_configuration_failed(error: lapin::Error) -> AppError {
    rabbitmq_external_error("qos configuration failed", error)
}

pub(crate) fn queue_declare_failed(error: lapin::Error) -> AppError {
    rabbitmq_external_error("queue declare failed", error)
}
