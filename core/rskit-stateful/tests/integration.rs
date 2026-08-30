use std::sync::Arc;
use std::time::Duration;

use rskit_stateful::{
    AccumulatorConfig, ByteSizeMeasurer, ByteSizeTrigger, Manager, MemoryStore, SizeTrigger,
};

#[tokio::test]
async fn accumulator_eviction_and_flush_work_together() {
    let manager = Manager::new(
        AccumulatorConfig::new()
            .with_max_size(2)
            .with_trigger(Arc::new(SizeTrigger::new(2))),
        || Box::new(MemoryStore::<i32>::new()),
    );

    assert!(manager.append("numbers", 1).unwrap().is_none());
    assert_eq!(manager.append("numbers", 2).unwrap(), Some(vec![1, 2]));
    assert!(manager.append("numbers", 3).unwrap().is_none());
    assert_eq!(manager.flush(&"numbers").unwrap(), Some(vec![3]));
}

#[tokio::test]
async fn byte_size_trigger_uses_custom_measurer() {
    let manager = Manager::new(
        AccumulatorConfig::new()
            .with_measurer(Arc::new(ByteSizeMeasurer))
            .with_trigger(Arc::new(ByteSizeTrigger::new(5))),
        || Box::new(MemoryStore::<Vec<u8>>::new()),
    );

    assert!(manager.append("bytes", b"ab".to_vec()).unwrap().is_none());
    assert_eq!(
        manager.append("bytes", b"cde".to_vec()).unwrap(),
        Some(vec![b"ab".to_vec(), b"cde".to_vec()])
    );
}

#[tokio::test]
async fn ttl_cleanup_removes_idle_accumulators() {
    tokio::time::pause();
    let manager = Manager::new(
        AccumulatorConfig::new().with_ttl(Duration::from_secs(10)),
        || Box::new(MemoryStore::<i32>::new()),
    );

    manager.append("session", 7).unwrap();
    tokio::time::advance(Duration::from_secs(11)).await;
    assert_eq!(
        manager.cleanup_expired().into_result().unwrap(),
        vec!["session"]
    );
    assert!(manager.keys().is_empty());
}
