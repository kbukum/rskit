/// Assert that a `Result` is `Ok`, printing the error on failure.
#[macro_export]
macro_rules! assert_ok {
    ($result:expr) => {
        match $result {
            Ok(v) => v,
            Err(e) => panic!("expected Ok, got Err: {:?}", e),
        }
    };
}

/// Assert that a `Result` is `Err`.
#[macro_export]
macro_rules! assert_err {
    ($result:expr) => {
        match $result {
            Err(e) => e,
            Ok(v) => panic!("expected Err, got Ok: {:?}", v),
        }
    };
}
