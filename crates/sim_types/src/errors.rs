//! Strongly-typed domain error types for deterministic math and conversions.

use thiserror::Error;

/// Errors arising during deterministic mathematical evaluation.
#[derive(Error, Debug, Clone, Copy, PartialEq, Eq)]
pub enum MathError {
    /// Attempted to compute square root of a negative fixed-point value.
    #[error("domain error: square root of negative value ({value:?})")]
    NegativeSquareRoot {
        /// The negative raw value passed into `sqrt`.
        value: i64,
    },

    /// Division by zero in fixed-point arithmetic.
    #[error("division by zero in fixed-point operation")]
    DivisionByZero,

    /// Arithmetic overflow in checked arithmetic operation.
    #[error("arithmetic overflow during fixed-point evaluation")]
    Overflow,

    /// Normalization failed because the vector length is zero or below epsilon.
    #[error("cannot normalize zero-length vector")]
    ZeroLengthVector,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_math_error_display() {
        let err1 = MathError::NegativeSquareRoot { value: -42 };
        assert_eq!(
            err1.to_string(),
            "domain error: square root of negative value (-42)"
        );

        let err2 = MathError::DivisionByZero;
        assert_eq!(
            err2.to_string(),
            "division by zero in fixed-point operation"
        );

        let err3 = MathError::Overflow;
        assert_eq!(
            err3.to_string(),
            "arithmetic overflow during fixed-point evaluation"
        );

        let err4 = MathError::ZeroLengthVector;
        assert_eq!(err4.to_string(), "cannot normalize zero-length vector");
    }
}
