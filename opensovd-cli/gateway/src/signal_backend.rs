// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

#![allow(clippy::module_name_repetitions)]

//! Backend-neutral signal access used by OpenSOVD data resources.
//!
//! The gateway-facing SOVD topology should not depend directly on the current
//! simulator. The simulator is one implementation of this trait; a real vehicle
//! signal service can implement the same trait later without changing the SOVD
//! component/resource IDs.

use async_trait::async_trait;
use opensovd_core::DataError;

#[async_trait]
pub(crate) trait SignalBackend: Send + Sync + 'static {
    async fn read_f64(&self, signal_id: &str) -> Result<f64, SignalBackendError>;

    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_possible_wrap,
        clippy::cast_precision_loss,
        clippy::cast_sign_loss
    )]
    async fn read_i64(&self, signal_id: &str) -> Result<i64, SignalBackendError> {
        let value = self.read_f64(signal_id).await?;
        if !value.is_finite() {
            return Err(SignalBackendError::Backend(format!(
                "signal {signal_id} returned non-finite value {value}"
            )));
        }

        let rounded = value.round();
        if rounded < i64::MIN as f64 || rounded > i64::MAX as f64 {
            return Err(SignalBackendError::Backend(format!(
                "signal {signal_id} value {rounded} does not fit into i64"
            )));
        }

        Ok(rounded as i64)
    }

    async fn read_string(&self, signal_id: &str) -> Result<String, SignalBackendError>;
}

#[derive(Debug)]
pub(crate) enum SignalBackendError {
    NotFound(String),
    Backend(String),
}

impl std::fmt::Display for SignalBackendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound(signal_id) => write!(f, "signal not found: {signal_id}"),
            Self::Backend(message) => write!(f, "signal backend error: {message}"),
        }
    }
}

impl std::error::Error for SignalBackendError {}

pub(crate) fn map_signal_error(error: SignalBackendError, data_id: &str) -> DataError {
    match error {
        SignalBackendError::NotFound(_) => DataError::NotFound(data_id.to_owned()),
        SignalBackendError::Backend(message) => DataError::Internal(message),
    }
}
