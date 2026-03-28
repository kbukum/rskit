use rskit_errors::{AppResult, ErrorCode};

/// Unwrap an `AppResult`, panicking with a clear message on `Err`.
///
/// Returns the inner value on success.
#[track_caller]
pub fn assert_ok<T>(result: AppResult<T>) -> T {
    match result {
        Ok(v) => v,
        Err(e) => panic!("expected Ok, got Err: {e}"),
    }
}

/// Assert that the result is an error with the expected [`ErrorCode`].
#[track_caller]
pub fn assert_err_code(result: AppResult<impl std::fmt::Debug>, code: ErrorCode) {
    match result {
        Ok(v) => panic!("expected Err({code:?}), got Ok({v:?})"),
        Err(e) => {
            assert_eq!(
                e.code, code,
                "expected error code {code:?}, got {:?}: {e}",
                e.code,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rskit_errors::AppError;

    #[test]
    fn assert_ok_passes() {
        let val: AppResult<i32> = Ok(42);
        assert_eq!(assert_ok(val), 42);
    }

    #[test]
    #[should_panic(expected = "expected Ok")]
    fn assert_ok_panics_on_err() {
        let val: AppResult<i32> = Err(AppError::new(ErrorCode::Internal, "boom"));
        assert_ok(val);
    }

    #[test]
    fn assert_err_code_passes() {
        let val: AppResult<i32> = Err(AppError::not_found("gone", None));
        assert_err_code(val, ErrorCode::NotFound);
    }

    #[test]
    #[should_panic(expected = "expected Err")]
    fn assert_err_code_panics_on_ok() {
        let val: AppResult<i32> = Ok(42);
        assert_err_code(val, ErrorCode::Internal);
    }

    #[test]
    #[should_panic(expected = "expected error code")]
    fn assert_err_code_panics_on_wrong_code() {
        let val: AppResult<i32> = Err(AppError::not_found("nope", None));
        assert_err_code(val, ErrorCode::Internal);
    }
}
