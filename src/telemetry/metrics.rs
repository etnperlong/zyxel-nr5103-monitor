use anyhow::{Context, Result};
use opentelemetry::{
    KeyValue,
    metrics::{Counter, Gauge, Histogram, Meter},
};
use serde::de::DeserializeOwned;
use std::{collections::HashMap, sync::Arc, time::Duration};

use crate::client::{ZyxelClient, dal::DalOid};

use super::zyxel::{
    CellWanBandObject, CellWanStatusObject, DalResponse, InterfaceCounters, InterfaceType,
    LanPortStatus, RadioKind, RouterTelemetrySnapshot, SignalKind, StatusObject, SystemMetrics,
    TrafficStatusObject,
};

pub struct TelemetryCollector {
    client: Arc<ZyxelClient>,
    recorder: MetricRecorder,
}

impl TelemetryCollector {
    pub fn new(client: Arc<ZyxelClient>, meter: Meter) -> Self {
        Self {
            client,
            recorder: MetricRecorder::new(meter),
        }
    }

    pub async fn collect(&mut self) -> Result<()> {
        let status = self
            .fetch_dal::<DalResponse<StatusObject>>(DalOid::Status)
            .await?;
        let cellwan_status = self
            .fetch_dal::<DalResponse<CellWanStatusObject>>(DalOid::CellWanStatus)
            .await?;
        let traffic_status = self
            .fetch_dal::<DalResponse<TrafficStatusObject>>(DalOid::TrafficStatus)
            .await?;
        let cellwan_band = if needs_cellwan_band(Some(&cellwan_status)) {
            Some(
                self.fetch_dal::<DalResponse<CellWanBandObject>>(DalOid::CellWanBand)
                    .await?,
            )
        } else {
            None
        };

        let snapshot = RouterTelemetrySnapshot::from_dal_responses(
            Some(status),
            Some(cellwan_status),
            Some(traffic_status),
            cellwan_band,
        );
        self.recorder.record_router_snapshot(&snapshot);

        Ok(())
    }

    pub fn record_connectivity_success(&self, rtt: Duration) {
        self.recorder.record_connectivity_success(rtt);
    }

    pub fn record_connectivity_failure(&self) {
        self.recorder.record_connectivity_failure();
    }

    pub fn record_reboot_attempt(&self) {
        self.recorder.record_reboot_attempt();
    }

    pub fn record_reboot_success(&self) {
        self.recorder.record_reboot_success();
    }

    pub fn record_reload_attempt(&self) {
        self.recorder.record_reload_attempt();
    }

    pub fn record_reload_success(&self, duration: Duration) {
        self.recorder.record_reload_success(duration);
    }

    pub fn record_reload_failure(&self, duration: Duration) {
        self.recorder.record_reload_failure(duration);
    }

    pub fn record_signal_degraded(&self, reason: &'static str) {
        self.recorder.record_signal_degraded(reason);
    }

    pub fn record_signal_recovery_attempt(&self) {
        self.recorder.record_signal_recovery_attempt();
    }

    pub fn record_signal_recovery_success(&self) {
        self.recorder.record_signal_recovery_success();
    }

    pub fn record_signal_recovery_failure(&self) {
        self.recorder.record_signal_recovery_failure();
    }

    async fn fetch_dal<T>(&self, oid: DalOid) -> Result<T>
    where
        T: DeserializeOwned,
    {
        self.client
            .get_dal(oid)
            .await
            .with_context(|| format!("failed to fetch DAL {}", oid.as_str()))
    }
}

pub struct MetricRecorder {
    device_uptime_seconds: Gauge<u64>,
    system_cpu_usage_percent: Gauge<f64>,
    system_memory_bytes: Gauge<u64>,
    cellular_signal_dbm: Gauge<f64>,
    cellular_signal_db: Gauge<f64>,
    interface_traffic_bytes: Counter<u64>,
    interface_traffic_packets: Counter<u64>,
    interface_errors: Counter<u64>,
    interface_discards: Counter<u64>,
    lan_port_up: Gauge<u64>,
    monitor_connectivity_rtt_ms: Histogram<f64>,
    monitor_connectivity_failures: Counter<u64>,
    monitor_reboot_attempts: Counter<u64>,
    monitor_reboot_successes: Counter<u64>,
    monitor_reload_attempts: Counter<u64>,
    monitor_reload_successes: Counter<u64>,
    monitor_reload_failures: Counter<u64>,
    monitor_reload_duration_seconds: Histogram<f64>,
    monitor_signal_degraded: Counter<u64>,
    monitor_signal_recovery_attempts: Counter<u64>,
    monitor_signal_recovery_successes: Counter<u64>,
    monitor_signal_recovery_failures: Counter<u64>,
    interface_baselines: HashMap<InterfaceCounterKey, InterfaceCounterBaseline>,
}

impl MetricRecorder {
    pub fn new(meter: Meter) -> Self {
        Self {
            device_uptime_seconds: meter
                .u64_gauge("zyxel.device.uptime.seconds")
                .with_unit("s")
                .build(),
            system_cpu_usage_percent: meter
                .f64_gauge("zyxel.system.cpu.usage.percent")
                .with_unit("%")
                .build(),
            system_memory_bytes: meter
                .u64_gauge("zyxel.system.memory.bytes")
                .with_unit("By")
                .build(),
            cellular_signal_dbm: meter
                .f64_gauge("zyxel.cellular.signal.dbm")
                .with_unit("dBm")
                .build(),
            cellular_signal_db: meter
                .f64_gauge("zyxel.cellular.signal.db")
                .with_unit("dB")
                .build(),
            interface_traffic_bytes: meter
                .u64_counter("zyxel.interface.traffic.bytes")
                .with_unit("By")
                .build(),
            interface_traffic_packets: meter
                .u64_counter("zyxel.interface.traffic.packets")
                .with_unit("{packet}")
                .build(),
            interface_errors: meter
                .u64_counter("zyxel.interface.errors")
                .with_unit("{error}")
                .build(),
            interface_discards: meter
                .u64_counter("zyxel.interface.discards")
                .with_unit("{packet}")
                .build(),
            lan_port_up: meter.u64_gauge("zyxel.lan.port.up").build(),
            monitor_connectivity_rtt_ms: meter
                .f64_histogram("zyxel.monitor.connectivity.rtt.ms")
                .with_unit("ms")
                .build(),
            monitor_connectivity_failures: meter
                .u64_counter("zyxel.monitor.connectivity.failures")
                .build(),
            monitor_reboot_attempts: meter.u64_counter("zyxel.monitor.reboot.attempts").build(),
            monitor_reboot_successes: meter.u64_counter("zyxel.monitor.reboot.successes").build(),
            monitor_reload_attempts: meter
                .u64_counter("zyxel.monitor.reload.attempts")
                .build(),
            monitor_reload_successes: meter
                .u64_counter("zyxel.monitor.reload.successes")
                .build(),
            monitor_reload_failures: meter
                .u64_counter("zyxel.monitor.reload.failures")
                .build(),
            monitor_reload_duration_seconds: meter
                .f64_histogram("zyxel.monitor.reload.duration.seconds")
                .with_unit("s")
                .build(),
            monitor_signal_degraded: meter
                .u64_counter("zyxel.monitor.signal.degraded")
                .build(),
            monitor_signal_recovery_attempts: meter
                .u64_counter("zyxel.monitor.signal.recovery.attempts")
                .build(),
            monitor_signal_recovery_successes: meter
                .u64_counter("zyxel.monitor.signal.recovery.successes")
                .build(),
            monitor_signal_recovery_failures: meter
                .u64_counter("zyxel.monitor.signal.recovery.failures")
                .build(),
            interface_baselines: HashMap::new(),
        }
    }

    pub fn record_router_snapshot(&mut self, snapshot: &RouterTelemetrySnapshot) {
        if let Some(system) = snapshot.system.as_ref() {
            self.record_system(system);
        }

        if let Some(cellular) = snapshot.cellular.as_ref() {
            self.record_cellular(cellular);
        }

        for interface in &snapshot.traffic {
            self.record_interface_counters(interface);
        }

        for lan_port in &snapshot.lan_ports {
            self.record_lan_port(lan_port);
        }
    }

    pub fn record_connectivity_success(&self, rtt: Duration) {
        self.monitor_connectivity_rtt_ms
            .record(rtt.as_secs_f64() * 1_000.0, &[]);
    }

    pub fn record_connectivity_failure(&self) {
        self.monitor_connectivity_failures.add(1, &[]);
    }

    pub fn record_reboot_attempt(&self) {
        self.monitor_reboot_attempts.add(1, &[]);
    }

    pub fn record_reboot_success(&self) {
        self.monitor_reboot_successes.add(1, &[]);
    }

    pub fn record_reload_attempt(&self) {
        self.monitor_reload_attempts.add(1, &[]);
    }

    pub fn record_reload_success(&self, duration: Duration) {
        self.monitor_reload_successes.add(1, &[]);
        self.monitor_reload_duration_seconds
            .record(duration.as_secs_f64(), &[]);
    }

    pub fn record_reload_failure(&self, duration: Duration) {
        self.monitor_reload_failures.add(1, &[]);
        self.monitor_reload_duration_seconds
            .record(duration.as_secs_f64(), &[]);
    }

    pub fn record_signal_degraded(&self, reason: &'static str) {
        self.monitor_signal_degraded
            .add(1, &[KeyValue::new("reason", reason)]);
    }

    pub fn record_signal_recovery_attempt(&self) {
        self.monitor_signal_recovery_attempts.add(1, &[]);
    }

    pub fn record_signal_recovery_success(&self) {
        self.monitor_signal_recovery_successes.add(1, &[]);
    }

    pub fn record_signal_recovery_failure(&self) {
        self.monitor_signal_recovery_failures.add(1, &[]);
    }

    fn record_system(&self, system: &SystemMetrics) {
        if let Some(uptime_seconds) = system.uptime_seconds {
            self.device_uptime_seconds.record(uptime_seconds, &[]);
        }

        if let Some(cpu_usage_percent) = system.cpu_usage_percent {
            self.system_cpu_usage_percent.record(cpu_usage_percent, &[]);
        }

        if let Some(memory_total_bytes) = system.memory_total_bytes {
            self.system_memory_bytes
                .record(memory_total_bytes, &[KeyValue::new("state", "total")]);
        }

        if let Some(memory_free_bytes) = system.memory_free_bytes {
            self.system_memory_bytes
                .record(memory_free_bytes, &[KeyValue::new("state", "free")]);
        }
    }

    fn record_cellular(&self, cellular: &super::zyxel::CellularMetrics) {
        for signal in &cellular.signals {
            let attributes = [
                KeyValue::new("radio", radio_attribute(signal.radio)),
                KeyValue::new("kind", signal_kind_attribute(signal.signal_kind)),
            ];

            match signal.signal_kind {
                SignalKind::Rssi | SignalKind::Rsrp => {
                    self.cellular_signal_dbm.record(signal.value, &attributes);
                }
                SignalKind::Rsrq | SignalKind::Sinr => {
                    self.cellular_signal_db.record(signal.value, &attributes);
                }
            }
        }
    }

    fn record_interface_counters(&mut self, interface: &InterfaceCounters) {
        self.record_interface_direction_counters(
            interface,
            "sent",
            interface.bytes_sent,
            interface.packets_sent,
            interface.errors_sent,
            interface.discards_sent,
        );
        self.record_interface_direction_counters(
            interface,
            "received",
            interface.bytes_received,
            interface.packets_received,
            interface.errors_received,
            interface.discards_received,
        );
    }

    fn record_interface_direction_counters(
        &mut self,
        interface: &InterfaceCounters,
        direction: &'static str,
        bytes: Option<u64>,
        packets: Option<u64>,
        errors: Option<u64>,
        discards: Option<u64>,
    ) {
        let interface_type = interface_type_attribute(interface.interface_type);
        let attributes = [
            KeyValue::new("interface_type", interface_type),
            KeyValue::new("interface_name", interface.interface_name.clone()),
            KeyValue::new("direction", direction),
        ];
        let baseline = self
            .interface_baselines
            .entry(InterfaceCounterKey {
                interface_type,
                interface_name: interface.interface_name.clone(),
                direction,
            })
            .or_default();

        record_counter_delta(
            &self.interface_traffic_bytes,
            bytes,
            &mut baseline.bytes,
            &attributes,
        );
        record_counter_delta(
            &self.interface_traffic_packets,
            packets,
            &mut baseline.packets,
            &attributes,
        );
        record_counter_delta(
            &self.interface_errors,
            errors,
            &mut baseline.errors,
            &attributes,
        );
        record_counter_delta(
            &self.interface_discards,
            discards,
            &mut baseline.discards,
            &attributes,
        );
    }

    fn record_lan_port(&self, lan_port: &LanPortStatus) {
        let is_up = u64::from(lan_port.status.as_deref() == Some("up"));
        self.lan_port_up.record(
            is_up,
            &[KeyValue::new("port_name", lan_port.port_name.clone())],
        );
    }
}

#[derive(Default)]
struct InterfaceCounterBaseline {
    bytes: Option<u64>,
    packets: Option<u64>,
    errors: Option<u64>,
    discards: Option<u64>,
}

#[derive(Default, Hash, PartialEq, Eq)]
struct InterfaceCounterKey {
    interface_type: &'static str,
    interface_name: String,
    direction: &'static str,
}

fn needs_cellwan_band(cellwan_status: Option<&DalResponse<CellWanStatusObject>>) -> bool {
    cellwan_status
        .and_then(DalResponse::first_object)
        .is_none_or(CellWanStatusObject::needs_band_metadata_lookup)
}

fn record_counter_delta(
    counter: &Counter<u64>,
    current: Option<u64>,
    previous: &mut Option<u64>,
    attributes: &[KeyValue],
) {
    let Some(current) = current else {
        return;
    };

    match previous {
        Some(last) if current > *last => {
            counter.add(current - *last, attributes);
            *previous = Some(current);
        }
        Some(last) if current <= *last => {
            *previous = Some(current);
        }
        None => {
            *previous = Some(current);
        }
        _ => {}
    }
}

fn interface_type_attribute(interface_type: InterfaceType) -> &'static str {
    match interface_type {
        InterfaceType::Ip => "ip",
        InterfaceType::Ethernet => "ethernet",
    }
}

fn radio_attribute(radio: RadioKind) -> &'static str {
    match radio {
        RadioKind::Lte => "lte",
        RadioKind::NrNsa => "nr_nsa",
        RadioKind::Scc => "scc",
    }
}

fn signal_kind_attribute(signal_kind: SignalKind) -> &'static str {
    match signal_kind {
        SignalKind::Rssi => "rssi",
        SignalKind::Rsrp => "rsrp",
        SignalKind::Rsrq => "rsrq",
        SignalKind::Sinr => "sinr",
    }
}
