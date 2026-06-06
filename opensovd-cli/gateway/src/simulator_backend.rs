// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

#![allow(clippy::module_name_repetitions)]

//! `SignalBackend` implementation backed by the standalone HPC2 simulator.

use async_trait::async_trait;
use http::StatusCode;
use opensovd_client::Client;
use serde::Deserialize;

use crate::signal_backend::{SignalBackend, SignalBackendError};

#[derive(Clone)]
pub(crate) struct SimulatorBackend {
    client: Client,
}

#[derive(Deserialize)]
struct SignalValue {
    value: f64,
}

#[derive(Deserialize)]
struct VinResponse {
    vin: String,
}

impl SimulatorBackend {
    pub(crate) fn connect(base_url: &str) -> anyhow::Result<Self> {
        Ok(Self {
            client: Client::connect(&simulator_v1_url(base_url))?,
        })
    }
}

#[async_trait]
impl SignalBackend for SimulatorBackend {
    async fn read_f64(&self, signal_id: &str) -> Result<f64, SignalBackendError> {
        let path = format!("/values/{signal_id}");
        let item = self
            .client
            .get::<SignalValue>(&path, &[])
            .await
            .map_err(|error| map_client_error(error, signal_id))?;

        Ok(item.value)
    }

    async fn read_string(&self, signal_id: &str) -> Result<String, SignalBackendError> {
        if signal_id == "vehicle.vin" {
            let response = self
                .client
                .get::<VinResponse>("/ident/vin", &[])
                .await
                .map_err(|error| map_client_error(error, signal_id))?;
            return Ok(response.vin);
        }

        Err(SignalBackendError::Backend(format!(
            "simulator has no string endpoint for signal {signal_id}"
        )))
    }
}

fn simulator_v1_url(base_url: &str) -> String {
    let trimmed = base_url.trim_end_matches('/');
    if trimmed.ends_with("/v1") {
        trimmed.to_owned()
    } else {
        format!("{trimmed}/v1")
    }
}

fn map_client_error(error: opensovd_client::Error, signal_id: &str) -> SignalBackendError {
    match error {
        opensovd_client::Error::ApiError { status, .. } if status == StatusCode::NOT_FOUND => {
            SignalBackendError::NotFound(signal_id.to_owned())
        }
        other => SignalBackendError::Backend(format!(
            "failed to read simulator signal {signal_id}: {other}"
        )),
    }
}
