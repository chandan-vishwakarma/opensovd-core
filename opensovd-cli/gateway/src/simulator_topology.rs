// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

#![allow(clippy::module_name_repetitions, clippy::too_many_lines)]

//! HPC2 simulator-backed SOVD topology.
//!
//! The SOVD-facing component IDs and data IDs intentionally mirror the old
//! simulator mapping contract. The backend signal IDs are kept separate so that
//! the simulator can later be replaced by a real vehicle signal source without
//! changing the public SOVD API.

use std::sync::Arc;

use async_trait::async_trait;
use opensovd_core::{Component, DataError, Topology};
use opensovd_models::data::DataCategory;
use opensovd_providers::data::{
    BuiltDataProvider, DataProviderBuilder, ReadableDataResource, Value as SovdValue,
};

use crate::signal_backend::{SignalBackend, map_signal_error};
use crate::simulator_backend::SimulatorBackend;

struct ComponentSpec {
    id: &'static str,
    name: &'static str,
    tags: &'static [&'static str],
    resources: &'static [ResourceSpec],
}

struct ResourceSpec {
    data_id: &'static str,
    name: &'static str,
    category: Category,
    groups: &'static [&'static str],
    tags: &'static [&'static str],
    signal_id: &'static str,
    datatype: DataType,
}

#[derive(Clone, Copy)]
enum Category {
    IdentData,
    CurrentData,
    StoredData,
}

impl Category {
    fn as_data_category(self) -> DataCategory {
        match self {
            Self::IdentData => DataCategory::IdentData,
            Self::CurrentData => DataCategory::CurrentData,
            Self::StoredData => DataCategory::StoredData,
        }
    }
}

#[derive(Clone, Copy)]
enum DataType {
    Number,
    Integer,
    String,
}

const LIVE_TAGS: &[&str] = &["live", "simulator"];

const HPC2_RESOURCES: &[ResourceSpec] = &[
    ResourceSpec {
        data_id: "vehicle_speed",
        name: "Vehicle Speed",
        category: Category::CurrentData,
        groups: &["currentTrip", "powertrain"],
        tags: LIVE_TAGS,
        signal_id: "vehicle.speed",
        datatype: DataType::Number,
    },
    ResourceSpec {
        data_id: "total_distance",
        name: "Odometer",
        category: Category::StoredData,
        groups: &["lifetime"],
        tags: LIVE_TAGS,
        signal_id: "vehicle.odometer",
        datatype: DataType::Number,
    },
    ResourceSpec {
        data_id: "trip_time",
        name: "Trip Time",
        category: Category::StoredData,
        groups: &["currentTrip"],
        tags: LIVE_TAGS,
        signal_id: "vehicle.trip_time",
        datatype: DataType::Number,
    },
    ResourceSpec {
        data_id: "propulsion_active_time",
        name: "Propulsion Active Time",
        category: Category::StoredData,
        groups: &["currentTrip"],
        tags: LIVE_TAGS,
        signal_id: "vehicle.propulsion_active_time",
        datatype: DataType::Number,
    },
    ResourceSpec {
        data_id: "accelerator_pedal",
        name: "Accelerator Pedal Position",
        category: Category::CurrentData,
        groups: &["currentTrip", "powertrain"],
        tags: LIVE_TAGS,
        signal_id: "vehicle.accelerator_pedal",
        datatype: DataType::Number,
    },
    ResourceSpec {
        data_id: "motor_speed",
        name: "Motor Speed",
        category: Category::CurrentData,
        groups: &["currentTrip", "powertrain"],
        tags: LIVE_TAGS,
        signal_id: "powertrain.motor_speed",
        datatype: DataType::Number,
    },
    ResourceSpec {
        data_id: "distance_since_fault_clear",
        name: "Distance since fault memory cleared",
        category: Category::StoredData,
        groups: &["lifetime"],
        tags: LIVE_TAGS,
        signal_id: "vehicle.distance_since_fault_clear_km",
        datatype: DataType::Number,
    },
    ResourceSpec {
        data_id: "propulsion_trips_since_fault_clear",
        name: "Propulsion trips since fault memory cleared",
        category: Category::StoredData,
        groups: &["lifetime"],
        tags: LIVE_TAGS,
        signal_id: "vehicle.propulsion_trips_since_fault_clear",
        datatype: DataType::Integer,
    },
    ResourceSpec {
        data_id: "vin",
        name: "VIN",
        category: Category::IdentData,
        groups: &["identification"],
        tags: LIVE_TAGS,
        signal_id: "vehicle.vin",
        datatype: DataType::String,
    },
];

const BMS_RESOURCES: &[ResourceSpec] = &[
    ResourceSpec {
        data_id: "hv_battery_soc",
        name: "Battery SOC",
        category: Category::CurrentData,
        groups: &["hvBattery"],
        tags: LIVE_TAGS,
        signal_id: "bms.soc",
        datatype: DataType::Number,
    },
    ResourceSpec {
        data_id: "hv_battery_voltage",
        name: "Battery Voltage",
        category: Category::CurrentData,
        groups: &["hvBattery"],
        tags: LIVE_TAGS,
        signal_id: "bms.voltage",
        datatype: DataType::Number,
    },
    ResourceSpec {
        data_id: "hv_battery_current",
        name: "Battery Current",
        category: Category::CurrentData,
        groups: &["hvBattery"],
        tags: LIVE_TAGS,
        signal_id: "bms.current",
        datatype: DataType::Number,
    },
    ResourceSpec {
        data_id: "hv_battery_power",
        name: "Battery Power",
        category: Category::CurrentData,
        groups: &["hvBattery"],
        tags: LIVE_TAGS,
        signal_id: "bms.power",
        datatype: DataType::Number,
    },
    ResourceSpec {
        data_id: "hv_battery_system_voltage",
        name: "Battery System Voltage",
        category: Category::CurrentData,
        groups: &["hvBattery"],
        tags: LIVE_TAGS,
        signal_id: "bms.system_voltage",
        datatype: DataType::Number,
    },
    ResourceSpec {
        data_id: "hv_battery_max_cell_v",
        name: "Max Cell Voltage",
        category: Category::CurrentData,
        groups: &["hvBattery"],
        tags: LIVE_TAGS,
        signal_id: "bms.max_cell_voltage",
        datatype: DataType::Number,
    },
    ResourceSpec {
        data_id: "hv_battery_min_cell_v",
        name: "Min Cell Voltage",
        category: Category::CurrentData,
        groups: &["hvBattery"],
        tags: LIVE_TAGS,
        signal_id: "bms.min_cell_voltage",
        datatype: DataType::Number,
    },
    ResourceSpec {
        data_id: "hv_battery_current_1s",
        name: "Battery Current (1s)",
        category: Category::CurrentData,
        groups: &["hvBattery"],
        tags: LIVE_TAGS,
        signal_id: "bms.current_1s",
        datatype: DataType::Number,
    },
    ResourceSpec {
        data_id: "hv_battery_energy_1s",
        name: "Battery Energy (1s)",
        category: Category::CurrentData,
        groups: &["hvBattery"],
        tags: LIVE_TAGS,
        signal_id: "bms.energy_1s_wh",
        datatype: DataType::Number,
    },
    ResourceSpec {
        data_id: "hv_battery_consumption_1s",
        name: "Consumption (1s)",
        category: Category::CurrentData,
        groups: &["hvBattery"],
        tags: LIVE_TAGS,
        signal_id: "bms.consumption_1s_wh",
        datatype: DataType::Number,
    },
    ResourceSpec {
        data_id: "hv_battery_soh",
        name: "Battery SOH",
        category: Category::StoredData,
        groups: &["hvBattery", "health"],
        tags: LIVE_TAGS,
        signal_id: "bms.soh",
        datatype: DataType::Number,
    },
    ResourceSpec {
        data_id: "distance_since_soh_reset",
        name: "Distance since SOH reset",
        category: Category::StoredData,
        groups: &["hvBattery", "health"],
        tags: LIVE_TAGS,
        signal_id: "bms.distance_since_soh_reset_km",
        datatype: DataType::Number,
    },
];

const HYDRAULICS_RESOURCES: &[ResourceSpec] = &[
    ResourceSpec {
        data_id: "main_pressure",
        name: "Main Hydraulic Pressure",
        category: Category::CurrentData,
        groups: &["hydraulics"],
        tags: LIVE_TAGS,
        signal_id: "hydraulic.main_pressure",
        datatype: DataType::Number,
    },
    ResourceSpec {
        data_id: "hydraulic_pump_speed",
        name: "Hydraulic Pump Speed",
        category: Category::CurrentData,
        groups: &["hydraulics"],
        tags: LIVE_TAGS,
        signal_id: "hydraulic.pump_speed",
        datatype: DataType::Number,
    },
    ResourceSpec {
        data_id: "hydraulic_valve_position",
        name: "Hydraulic Valve Position",
        category: Category::CurrentData,
        groups: &["hydraulics"],
        tags: LIVE_TAGS,
        signal_id: "hydraulic.valve_position",
        datatype: DataType::Number,
    },
];

const INFOTAINMENT_RESOURCES: &[ResourceSpec] = &[
    ResourceSpec {
        data_id: "hu_uptime",
        name: "System Uptime",
        category: Category::CurrentData,
        groups: &["system", "lifecycle"],
        tags: LIVE_TAGS,
        signal_id: "hu.system.uptime",
        datatype: DataType::Number,
    },
    ResourceSpec {
        data_id: "hu_restart_counter",
        name: "Restart Counter",
        category: Category::StoredData,
        groups: &["lifetime", "system"],
        tags: LIVE_TAGS,
        signal_id: "hu.system.restart_counter",
        datatype: DataType::Integer,
    },
    ResourceSpec {
        data_id: "hu_cpu_usage",
        name: "CPU Load",
        category: Category::CurrentData,
        groups: &["system", "performance"],
        tags: LIVE_TAGS,
        signal_id: "hu.system.cpu_usage",
        datatype: DataType::Number,
    },
    ResourceSpec {
        data_id: "hu_memory_usage",
        name: "Memory Utilization",
        category: Category::CurrentData,
        groups: &["system", "performance"],
        tags: LIVE_TAGS,
        signal_id: "hu.system.memory_usage",
        datatype: DataType::Number,
    },
    ResourceSpec {
        data_id: "hu_temperature",
        name: "SoC Temperature",
        category: Category::CurrentData,
        groups: &["system", "thermal"],
        tags: LIVE_TAGS,
        signal_id: "hu.hardware.temperature",
        datatype: DataType::Number,
    },
    ResourceSpec {
        data_id: "hu_supply_voltage",
        name: "KL30 Supply Voltage",
        category: Category::CurrentData,
        groups: &["hardware", "electrical"],
        tags: LIVE_TAGS,
        signal_id: "hu.hardware.supply_voltage",
        datatype: DataType::Number,
    },
    ResourceSpec {
        data_id: "hu_reboot_in_progress",
        name: "Reboot In Progress",
        category: Category::CurrentData,
        groups: &["system", "lifecycle"],
        tags: LIVE_TAGS,
        signal_id: "hu.system.reboot_in_progress",
        datatype: DataType::Number,
    },
    ResourceSpec {
        data_id: "hu_undervoltage_active",
        name: "Undervoltage Active",
        category: Category::CurrentData,
        groups: &["hardware", "electrical"],
        tags: LIVE_TAGS,
        signal_id: "hu.hardware.undervoltage_active",
        datatype: DataType::Number,
    },
    ResourceSpec {
        data_id: "hu_thermal_protection_active",
        name: "Thermal Protection Active",
        category: Category::CurrentData,
        groups: &["hardware", "thermal"],
        tags: LIVE_TAGS,
        signal_id: "hu.hardware.thermal_protection_active",
        datatype: DataType::Number,
    },
    ResourceSpec {
        data_id: "hu_low_memory_active",
        name: "Low Memory Active",
        category: Category::CurrentData,
        groups: &["system", "performance"],
        tags: LIVE_TAGS,
        signal_id: "hu.system.low_memory_active",
        datatype: DataType::Number,
    },
    ResourceSpec {
        data_id: "hu_ui_stall_active",
        name: "UI Stall Active",
        category: Category::CurrentData,
        groups: &["system", "performance"],
        tags: LIVE_TAGS,
        signal_id: "hu.system.ui_stall_active",
        datatype: DataType::Number,
    },
    ResourceSpec {
        data_id: "hu_telemetry_stale",
        name: "Telemetry Stale",
        category: Category::CurrentData,
        groups: &["system", "network"],
        tags: LIVE_TAGS,
        signal_id: "hu.system.telemetry_stale",
        datatype: DataType::Number,
    },
];

const COMPONENTS: &[ComponentSpec] = &[
    // Keep the insertion order aligned with the old sovd-playground response.
    ComponentSpec {
        id: "hydraulics",
        name: "Hydraulic Control System",
        tags: &["sim", "hydraulics"],
        resources: HYDRAULICS_RESOURCES,
    },
    ComponentSpec {
        id: "hpc2",
        name: "HPC2 Simulated Powertrain",
        tags: &["sim", "powertrain"],
        resources: HPC2_RESOURCES,
    },
    ComponentSpec {
        id: "infotainment",
        name: "Head Unit Infotainment",
        tags: &["sim", "hu", "infotainment", "opensovd"],
        resources: INFOTAINMENT_RESOURCES,
    },
    ComponentSpec {
        id: "bms",
        name: "Battery Management System",
        tags: &["sim", "battery"],
        resources: BMS_RESOURCES,
    },
];

/// Create the complete HPC2 simulator topology.
///
/// This exposes the SOVD-facing component/resource contract from the previous
/// simulator mapping while using an external simulator as the current backend.
///
/// # Errors
///
/// Returns an error when the simulator URL is invalid or a data provider cannot
/// be built.
pub(crate) async fn create_hpc2_simulator_topology(simulator_url: &str) -> anyhow::Result<Topology> {
    let backend: Arc<dyn SignalBackend> = Arc::new(SimulatorBackend::connect(simulator_url)?);
    let topology = Topology::default();

    {
        let mut topo = topology.write().await;
        for component in COMPONENTS {
            topo.add_component(create_component(component, Arc::clone(&backend))?);
        }
    }

    Ok(topology)
}

fn create_component(
    spec: &ComponentSpec,
    backend: Arc<dyn SignalBackend>,
) -> anyhow::Result<Component> {
    let provider = create_provider(spec.resources, backend)?;
    Ok(Component::new(spec.id, spec.name)
        .with_tags(spec.tags.iter().map(ToString::to_string).collect())
        .with_data_provider(provider))
}

fn create_provider(
    resources: &[ResourceSpec],
    backend: Arc<dyn SignalBackend>,
) -> anyhow::Result<BuiltDataProvider> {
    let mut builder = DataProviderBuilder::new();

    for resource in resources {
        let category = resource.category.as_data_category();
        builder = match resource.datatype {
            DataType::Number => builder.read_data(
                resource.data_id,
                resource.name,
                &category,
                F64SignalResource::new(Arc::clone(&backend), resource.data_id, resource.signal_id),
            ),
            DataType::Integer => builder.read_data(
                resource.data_id,
                resource.name,
                &category,
                I64SignalResource::new(Arc::clone(&backend), resource.data_id, resource.signal_id),
            ),
            DataType::String => builder.read_data(
                resource.data_id,
                resource.name,
                &category,
                StringSignalResource::new(
                    Arc::clone(&backend),
                    resource.data_id,
                    resource.signal_id,
                ),
            ),
        }
        .groups(resource.groups.iter().copied())
        .tags(resource.tags.iter().copied());
    }

    builder.build().map_err(Into::into)
}

#[derive(Clone)]
struct F64SignalResource {
    backend: Arc<dyn SignalBackend>,
    data_id: &'static str,
    signal_id: &'static str,
}

impl F64SignalResource {
    fn new(
        backend: Arc<dyn SignalBackend>,
        data_id: &'static str,
        signal_id: &'static str,
    ) -> Self {
        Self {
            backend,
            data_id,
            signal_id,
        }
    }
}

#[async_trait]
impl ReadableDataResource for F64SignalResource {
    type Value = SovdValue<f64>;

    async fn read(&self) -> Result<Self::Value, DataError> {
        self.backend
            .read_f64(self.signal_id)
            .await
            .map(SovdValue::new)
            .map_err(|error| map_signal_error(error, self.data_id))
    }
}

#[derive(Clone)]
struct I64SignalResource {
    backend: Arc<dyn SignalBackend>,
    data_id: &'static str,
    signal_id: &'static str,
}

impl I64SignalResource {
    fn new(
        backend: Arc<dyn SignalBackend>,
        data_id: &'static str,
        signal_id: &'static str,
    ) -> Self {
        Self {
            backend,
            data_id,
            signal_id,
        }
    }
}

#[async_trait]
impl ReadableDataResource for I64SignalResource {
    type Value = SovdValue<i64>;

    async fn read(&self) -> Result<Self::Value, DataError> {
        self.backend
            .read_i64(self.signal_id)
            .await
            .map(SovdValue::new)
            .map_err(|error| map_signal_error(error, self.data_id))
    }
}

#[derive(Clone)]
struct StringSignalResource {
    backend: Arc<dyn SignalBackend>,
    data_id: &'static str,
    signal_id: &'static str,
}

impl StringSignalResource {
    fn new(
        backend: Arc<dyn SignalBackend>,
        data_id: &'static str,
        signal_id: &'static str,
    ) -> Self {
        Self {
            backend,
            data_id,
            signal_id,
        }
    }
}

#[async_trait]
impl ReadableDataResource for StringSignalResource {
    type Value = SovdValue<String>;

    async fn read(&self) -> Result<Self::Value, DataError> {
        self.backend
            .read_string(self.signal_id)
            .await
            .map(SovdValue::new)
            .map_err(|error| map_signal_error(error, self.data_id))
    }
}
