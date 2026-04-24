use serde::{Deserialize, Deserializer};

#[derive(Debug, Deserialize)]
#[serde(bound(deserialize = "T: Deserialize<'de>"))]
pub struct DalResponse<T> {
    pub result: String,
    #[serde(rename = "Object", default)]
    pub object: Vec<T>,
}

impl<T> DalResponse<T> {
    pub fn into_first_object(self) -> Option<T> {
        self.object.into_iter().next()
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize, Default)]
pub struct StatusObject {
    #[serde(rename = "DeviceInfo", default)]
    pub device_info: Option<DeviceInfo>,
    #[serde(rename = "MemoryStatus", default)]
    pub memory_status: Option<MemoryStatus>,
    #[serde(rename = "ProcessStatus", default)]
    pub process_status: Option<ProcessStatus>,
    #[serde(rename = "LanPortInfo", default)]
    pub lan_port_info: Vec<LanPortInfo>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Default)]
pub struct DeviceInfo {
    #[serde(
        rename = "ModelName",
        default,
        deserialize_with = "deserialize_optional_text"
    )]
    pub model_name: Option<String>,
    #[serde(
        rename = "SoftwareVersion",
        default,
        deserialize_with = "deserialize_optional_text"
    )]
    pub software_version: Option<String>,
    #[serde(
        rename = "UpTime",
        default,
        deserialize_with = "deserialize_optional_text"
    )]
    pub up_time: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Default)]
pub struct MemoryStatus {
    #[serde(
        rename = "Total",
        default,
        deserialize_with = "deserialize_optional_u64"
    )]
    pub total: Option<u64>,
    #[serde(
        rename = "Free",
        default,
        deserialize_with = "deserialize_optional_u64"
    )]
    pub free: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Default)]
pub struct ProcessStatus {
    #[serde(
        rename = "CPUUsage",
        default,
        deserialize_with = "deserialize_optional_f64"
    )]
    pub cpu_usage: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Default)]
pub struct LanPortInfo {
    #[serde(
        rename = "Status",
        default,
        deserialize_with = "deserialize_optional_text"
    )]
    pub status: Option<String>,
    #[serde(
        rename = "Name",
        default,
        deserialize_with = "deserialize_optional_text"
    )]
    pub name: Option<String>,
    #[serde(
        rename = "MaxBitRate",
        default,
        deserialize_with = "deserialize_optional_u64"
    )]
    pub max_bit_rate: Option<u64>,
    #[serde(
        rename = "X_ZYXEL_SwitchToWAN",
        default,
        deserialize_with = "deserialize_optional_bool"
    )]
    pub switch_to_wan: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Default)]
pub struct CellWanStatusObject {
    #[serde(
        rename = "INTF_Status",
        default,
        deserialize_with = "deserialize_optional_text"
    )]
    pub intf_status: Option<String>,
    #[serde(
        rename = "INTF_Current_Access_Technology",
        default,
        deserialize_with = "deserialize_optional_text"
    )]
    pub intf_current_access_technology: Option<String>,
    #[serde(
        rename = "INTF_Current_Band",
        default,
        deserialize_with = "deserialize_optional_text"
    )]
    pub intf_current_band: Option<String>,
    #[serde(
        rename = "INTF_RSSI",
        default,
        deserialize_with = "deserialize_optional_f64"
    )]
    pub intf_rssi: Option<f64>,
    #[serde(
        rename = "INTF_RSRP",
        default,
        deserialize_with = "deserialize_optional_f64"
    )]
    pub intf_rsrp: Option<f64>,
    #[serde(
        rename = "INTF_RSRQ",
        default,
        deserialize_with = "deserialize_optional_f64"
    )]
    pub intf_rsrq: Option<f64>,
    #[serde(
        rename = "INTF_SINR",
        default,
        deserialize_with = "deserialize_optional_f64"
    )]
    pub intf_sinr: Option<f64>,
    #[serde(
        rename = "NSA_Band",
        default,
        deserialize_with = "deserialize_optional_text"
    )]
    pub nsa_band: Option<String>,
    #[serde(
        rename = "NSA_RSSI",
        default,
        deserialize_with = "deserialize_optional_f64"
    )]
    pub nsa_rssi: Option<f64>,
    #[serde(
        rename = "NSA_RSRP",
        default,
        deserialize_with = "deserialize_optional_f64"
    )]
    pub nsa_rsrp: Option<f64>,
    #[serde(
        rename = "NSA_RSRQ",
        default,
        deserialize_with = "deserialize_optional_f64"
    )]
    pub nsa_rsrq: Option<f64>,
    #[serde(
        rename = "NSA_SINR",
        default,
        deserialize_with = "deserialize_optional_f64"
    )]
    pub nsa_sinr: Option<f64>,
    #[serde(rename = "SCC_Info", default)]
    pub scc_info: Vec<SccInfo>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Default)]
pub struct SccInfo {
    #[serde(
        rename = "Band",
        default,
        deserialize_with = "deserialize_optional_text"
    )]
    pub band: Option<String>,
    #[serde(
        rename = "RSSI",
        default,
        deserialize_with = "deserialize_optional_f64"
    )]
    pub rssi: Option<f64>,
    #[serde(
        rename = "RSRP",
        default,
        deserialize_with = "deserialize_optional_f64"
    )]
    pub rsrp: Option<f64>,
    #[serde(
        rename = "RSRQ",
        default,
        deserialize_with = "deserialize_optional_f64"
    )]
    pub rsrq: Option<f64>,
    #[serde(
        rename = "SINR",
        default,
        deserialize_with = "deserialize_optional_f64"
    )]
    pub sinr: Option<f64>,
    #[serde(
        rename = "UplinkBandwidth",
        default,
        deserialize_with = "deserialize_optional_u64"
    )]
    pub uplink_bandwidth: Option<u64>,
    #[serde(
        rename = "DownlinkBandwidth",
        default,
        deserialize_with = "deserialize_optional_u64"
    )]
    pub downlink_bandwidth: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Default)]
pub struct CellWanBandObject {
    #[serde(
        rename = "INTF_Supported_Access_Technologies",
        default,
        deserialize_with = "deserialize_optional_text"
    )]
    pub intf_supported_access_technologies: Option<String>,
    #[serde(
        rename = "INTF_Preferred_Access_Technology",
        default,
        deserialize_with = "deserialize_optional_text"
    )]
    pub intf_preferred_access_technology: Option<String>,
    #[serde(
        rename = "INTF_Current_Access_Technology",
        default,
        deserialize_with = "deserialize_optional_text"
    )]
    pub intf_current_access_technology: Option<String>,
    #[serde(
        rename = "INTF_Supported_Bands",
        default,
        deserialize_with = "deserialize_optional_text"
    )]
    pub intf_supported_bands: Option<String>,
    #[serde(
        rename = "INTF_Preferred_Bands",
        default,
        deserialize_with = "deserialize_optional_text"
    )]
    pub intf_preferred_bands: Option<String>,
    #[serde(
        rename = "INTF_Current_Band",
        default,
        deserialize_with = "deserialize_optional_text"
    )]
    pub intf_current_band: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Default)]
pub struct TrafficStatusObject {
    #[serde(rename = "ipIface", default)]
    pub ip_iface: Vec<IpInterface>,
    #[serde(rename = "ipIfaceSt", default)]
    pub ip_iface_st: Vec<InterfaceStats>,
    #[serde(rename = "ethIface", default)]
    pub eth_iface: Vec<EthernetInterface>,
    #[serde(rename = "ethIfaceSt", default)]
    pub eth_iface_st: Vec<InterfaceStats>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Default)]
pub struct IpInterface {
    #[serde(
        rename = "Name",
        default,
        deserialize_with = "deserialize_optional_text"
    )]
    pub name: Option<String>,
    #[serde(
        rename = "Status",
        default,
        deserialize_with = "deserialize_optional_text"
    )]
    pub status: Option<String>,
    #[serde(
        rename = "X_ZYXEL_IfName",
        default,
        deserialize_with = "deserialize_optional_text"
    )]
    pub if_name: Option<String>,
    #[serde(
        rename = "X_ZYXEL_SrvName",
        default,
        deserialize_with = "deserialize_optional_text"
    )]
    pub service_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Default)]
pub struct EthernetInterface {
    #[serde(
        rename = "Name",
        default,
        deserialize_with = "deserialize_optional_text"
    )]
    pub name: Option<String>,
    #[serde(
        rename = "Status",
        default,
        deserialize_with = "deserialize_optional_text"
    )]
    pub status: Option<String>,
    #[serde(
        rename = "MaxBitRate",
        default,
        deserialize_with = "deserialize_optional_u64"
    )]
    pub max_bit_rate: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Default)]
pub struct InterfaceStats {
    #[serde(
        rename = "BytesSent",
        default,
        deserialize_with = "deserialize_optional_u64"
    )]
    pub bytes_sent: Option<u64>,
    #[serde(
        rename = "BytesReceived",
        default,
        deserialize_with = "deserialize_optional_u64"
    )]
    pub bytes_received: Option<u64>,
    #[serde(
        rename = "PacketsSent",
        default,
        deserialize_with = "deserialize_optional_u64"
    )]
    pub packets_sent: Option<u64>,
    #[serde(
        rename = "PacketsReceived",
        default,
        deserialize_with = "deserialize_optional_u64"
    )]
    pub packets_received: Option<u64>,
    #[serde(
        rename = "ErrorsSent",
        default,
        deserialize_with = "deserialize_optional_u64"
    )]
    pub errors_sent: Option<u64>,
    #[serde(
        rename = "ErrorsReceived",
        default,
        deserialize_with = "deserialize_optional_u64"
    )]
    pub errors_received: Option<u64>,
    #[serde(
        rename = "DiscardPacketsSent",
        default,
        deserialize_with = "deserialize_optional_u64"
    )]
    pub discard_packets_sent: Option<u64>,
    #[serde(
        rename = "DiscardPacketsReceived",
        default,
        deserialize_with = "deserialize_optional_u64"
    )]
    pub discard_packets_received: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct RouterTelemetrySnapshot {
    pub system: Option<SystemMetrics>,
    pub cellular: Option<CellularMetrics>,
    pub traffic: Vec<InterfaceCounters>,
    pub lan_ports: Vec<LanPortStatus>,
}

impl RouterTelemetrySnapshot {
    pub fn from_dal_responses(
        status: Option<DalResponse<StatusObject>>,
        cellwan_status: Option<DalResponse<CellWanStatusObject>>,
        traffic_status: Option<DalResponse<TrafficStatusObject>>,
        cellwan_band: Option<DalResponse<CellWanBandObject>>,
    ) -> Self {
        let status = status.and_then(DalResponse::into_first_object);
        let cellwan_status = cellwan_status.and_then(DalResponse::into_first_object);
        let traffic_status = traffic_status.and_then(DalResponse::into_first_object);
        let cellwan_band = cellwan_band.and_then(DalResponse::into_first_object);

        Self {
            system: system_metrics(status.as_ref()),
            cellular: cellular_metrics(cellwan_status.as_ref(), cellwan_band.as_ref()),
            traffic: interface_counters(traffic_status.as_ref()),
            lan_ports: lan_port_statuses(status.as_ref()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SystemMetrics {
    pub model_name: Option<String>,
    pub software_version: Option<String>,
    pub uptime_seconds: Option<u64>,
    pub memory_total_bytes: Option<u64>,
    pub memory_free_bytes: Option<u64>,
    pub cpu_usage_percent: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct CellularMetrics {
    pub status: Option<String>,
    pub access_technology: Option<String>,
    pub current_band: Option<String>,
    pub preferred_access_technology: Option<String>,
    pub supported_access_technologies: Vec<String>,
    pub supported_bands: Vec<String>,
    pub preferred_bands: Vec<String>,
    pub signals: Vec<SignalSample>,
    pub carriers: Vec<CellularCarrier>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RadioKind {
    Lte,
    NrNsa,
    Scc,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalKind {
    Rssi,
    Rsrp,
    Rsrq,
    Sinr,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SignalSample {
    pub radio: RadioKind,
    pub signal_kind: SignalKind,
    pub value: f64,
    pub band: Option<String>,
    pub carrier_index: Option<usize>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CellularCarrier {
    pub carrier_index: usize,
    pub band: Option<String>,
    pub uplink_bandwidth_mhz: Option<u64>,
    pub downlink_bandwidth_mhz: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InterfaceType {
    Ip,
    Ethernet,
}

#[derive(Debug, Clone, PartialEq)]
pub struct InterfaceCounters {
    pub interface_type: InterfaceType,
    pub interface_name: String,
    pub status: Option<String>,
    pub max_bit_rate_mbps: Option<u64>,
    pub bytes_sent: Option<u64>,
    pub bytes_received: Option<u64>,
    pub packets_sent: Option<u64>,
    pub packets_received: Option<u64>,
    pub errors_sent: Option<u64>,
    pub errors_received: Option<u64>,
    pub discards_sent: Option<u64>,
    pub discards_received: Option<u64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LanPortStatus {
    pub port_name: String,
    pub status: Option<String>,
    pub max_bit_rate_mbps: Option<u64>,
    pub switch_to_wan: Option<bool>,
}

fn system_metrics(status: Option<&StatusObject>) -> Option<SystemMetrics> {
    let status = status?;

    let metrics = SystemMetrics {
        model_name: status
            .device_info
            .as_ref()
            .and_then(|device| trimmed_non_empty(device.model_name.as_deref())),
        software_version: status
            .device_info
            .as_ref()
            .and_then(|device| trimmed_non_empty(device.software_version.as_deref())),
        uptime_seconds: status
            .device_info
            .as_ref()
            .and_then(|device| device.up_time.as_deref())
            .and_then(parse_uptime_seconds),
        memory_total_bytes: status
            .memory_status
            .as_ref()
            .and_then(|memory| memory.total),
        memory_free_bytes: status.memory_status.as_ref().and_then(|memory| memory.free),
        cpu_usage_percent: status
            .process_status
            .as_ref()
            .and_then(|process| process.cpu_usage),
    };

    (!metrics.is_empty()).then_some(metrics)
}

fn cellular_metrics(
    cellwan_status: Option<&CellWanStatusObject>,
    cellwan_band: Option<&CellWanBandObject>,
) -> Option<CellularMetrics> {
    let access_technology = cellwan_status
        .and_then(|status| status.intf_current_access_technology.as_deref())
        .and_then(canonicalize_label)
        .or_else(|| {
            cellwan_band
                .and_then(|band| band.intf_current_access_technology.as_deref())
                .and_then(canonicalize_label)
        })
        .or_else(|| {
            cellwan_band
                .and_then(|band| band.intf_preferred_access_technology.as_deref())
                .and_then(canonicalize_label)
        });
    let current_band = cellwan_status
        .and_then(|status| status.intf_current_band.as_deref())
        .and_then(canonicalize_label)
        .or_else(|| {
            cellwan_band
                .and_then(|band| band.intf_current_band.as_deref())
                .and_then(canonicalize_label)
        });
    let preferred_access_technology = cellwan_band
        .and_then(|band| band.intf_preferred_access_technology.as_deref())
        .and_then(canonicalize_label);
    let supported_access_technologies = cellwan_band
        .and_then(|band| band.intf_supported_access_technologies.as_deref())
        .map(parse_label_list)
        .unwrap_or_default();
    let supported_bands = cellwan_band
        .and_then(|band| band.intf_supported_bands.as_deref())
        .map(parse_label_list)
        .unwrap_or_default();
    let preferred_bands = cellwan_band
        .and_then(|band| band.intf_preferred_bands.as_deref())
        .map(parse_label_list)
        .unwrap_or_default();

    let mut signals = Vec::new();
    let mut carriers = Vec::new();

    if let Some(status) = cellwan_status {
        push_signal(
            &mut signals,
            RadioKind::Lte,
            SignalKind::Rssi,
            status.intf_rssi,
            current_band.clone(),
            None,
        );
        push_signal(
            &mut signals,
            RadioKind::Lte,
            SignalKind::Rsrp,
            status.intf_rsrp,
            current_band.clone(),
            None,
        );
        push_signal(
            &mut signals,
            RadioKind::Lte,
            SignalKind::Rsrq,
            status.intf_rsrq,
            current_band.clone(),
            None,
        );
        push_signal(
            &mut signals,
            RadioKind::Lte,
            SignalKind::Sinr,
            status.intf_sinr,
            current_band.clone(),
            None,
        );

        let nsa_band = status.nsa_band.as_deref().and_then(canonicalize_label);
        push_signal(
            &mut signals,
            RadioKind::NrNsa,
            SignalKind::Rssi,
            status.nsa_rssi,
            nsa_band.clone(),
            None,
        );
        push_signal(
            &mut signals,
            RadioKind::NrNsa,
            SignalKind::Rsrp,
            status.nsa_rsrp,
            nsa_band.clone(),
            None,
        );
        push_signal(
            &mut signals,
            RadioKind::NrNsa,
            SignalKind::Rsrq,
            status.nsa_rsrq,
            nsa_band.clone(),
            None,
        );
        push_signal(
            &mut signals,
            RadioKind::NrNsa,
            SignalKind::Sinr,
            status.nsa_sinr,
            nsa_band,
            None,
        );

        for (carrier_index, carrier) in status.scc_info.iter().enumerate() {
            let band = carrier.band.as_deref().and_then(canonicalize_label);
            push_signal(
                &mut signals,
                RadioKind::Scc,
                SignalKind::Rssi,
                carrier.rssi,
                band.clone(),
                Some(carrier_index),
            );
            push_signal(
                &mut signals,
                RadioKind::Scc,
                SignalKind::Rsrp,
                carrier.rsrp,
                band.clone(),
                Some(carrier_index),
            );
            push_signal(
                &mut signals,
                RadioKind::Scc,
                SignalKind::Rsrq,
                carrier.rsrq,
                band.clone(),
                Some(carrier_index),
            );
            push_signal(
                &mut signals,
                RadioKind::Scc,
                SignalKind::Sinr,
                carrier.sinr,
                band.clone(),
                Some(carrier_index),
            );

            let carrier = CellularCarrier {
                carrier_index,
                band,
                uplink_bandwidth_mhz: carrier.uplink_bandwidth,
                downlink_bandwidth_mhz: carrier.downlink_bandwidth,
            };

            if !carrier.is_empty() {
                carriers.push(carrier);
            }
        }
    }

    let metrics = CellularMetrics {
        status: cellwan_status
            .and_then(|status| status.intf_status.as_deref())
            .and_then(canonicalize_label),
        access_technology,
        current_band,
        preferred_access_technology,
        supported_access_technologies,
        supported_bands,
        preferred_bands,
        signals,
        carriers,
    };

    (!metrics.is_empty()).then_some(metrics)
}

fn interface_counters(traffic_status: Option<&TrafficStatusObject>) -> Vec<InterfaceCounters> {
    let Some(traffic_status) = traffic_status else {
        return Vec::new();
    };

    let mut counters = Vec::new();

    for (metadata, stats) in traffic_status
        .ip_iface
        .iter()
        .zip(&traffic_status.ip_iface_st)
    {
        if let Some(interface) = ip_interface_counters(metadata, stats) {
            counters.push(interface);
        }
    }

    for (metadata, stats) in traffic_status
        .eth_iface
        .iter()
        .zip(&traffic_status.eth_iface_st)
    {
        if let Some(interface) = ethernet_interface_counters(metadata, stats) {
            counters.push(interface);
        }
    }

    counters
}

fn lan_port_statuses(status: Option<&StatusObject>) -> Vec<LanPortStatus> {
    let Some(status) = status else {
        return Vec::new();
    };

    status
        .lan_port_info
        .iter()
        .filter_map(|port| {
            let port_name = port.name.as_deref().and_then(sanitize_safe_name)?;

            Some(LanPortStatus {
                port_name,
                status: port.status.as_deref().and_then(canonicalize_label),
                max_bit_rate_mbps: port.max_bit_rate,
                switch_to_wan: port.switch_to_wan,
            })
        })
        .collect()
}

fn ip_interface_counters(
    metadata: &IpInterface,
    stats: &InterfaceStats,
) -> Option<InterfaceCounters> {
    let interface_name = metadata
        .if_name
        .as_deref()
        .or(metadata.name.as_deref())
        .and_then(sanitize_safe_name)?;

    Some(InterfaceCounters {
        interface_type: InterfaceType::Ip,
        interface_name,
        status: metadata.status.as_deref().and_then(canonicalize_label),
        max_bit_rate_mbps: None,
        bytes_sent: stats.bytes_sent,
        bytes_received: stats.bytes_received,
        packets_sent: stats.packets_sent,
        packets_received: stats.packets_received,
        errors_sent: stats.errors_sent,
        errors_received: stats.errors_received,
        discards_sent: stats.discard_packets_sent,
        discards_received: stats.discard_packets_received,
    })
}

fn ethernet_interface_counters(
    metadata: &EthernetInterface,
    stats: &InterfaceStats,
) -> Option<InterfaceCounters> {
    let interface_name = metadata.name.as_deref().and_then(sanitize_safe_name)?;

    Some(InterfaceCounters {
        interface_type: InterfaceType::Ethernet,
        interface_name,
        status: metadata.status.as_deref().and_then(canonicalize_label),
        max_bit_rate_mbps: metadata.max_bit_rate,
        bytes_sent: stats.bytes_sent,
        bytes_received: stats.bytes_received,
        packets_sent: stats.packets_sent,
        packets_received: stats.packets_received,
        errors_sent: stats.errors_sent,
        errors_received: stats.errors_received,
        discards_sent: stats.discard_packets_sent,
        discards_received: stats.discard_packets_received,
    })
}

fn push_signal(
    signals: &mut Vec<SignalSample>,
    radio: RadioKind,
    signal_kind: SignalKind,
    value: Option<f64>,
    band: Option<String>,
    carrier_index: Option<usize>,
) {
    if let Some(value) = value {
        signals.push(SignalSample {
            radio,
            signal_kind,
            value,
            band,
            carrier_index,
        });
    }
}

fn parse_label_list(value: &str) -> Vec<String> {
    value.split(',').filter_map(canonicalize_label).collect()
}

fn canonicalize_label(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }

    let mut normalized = String::with_capacity(trimmed.len());
    let mut last_was_separator = false;

    for character in trimmed.chars() {
        if character.is_ascii_alphanumeric() {
            normalized.push(character.to_ascii_lowercase());
            last_was_separator = false;
        } else if !last_was_separator {
            normalized.push('_');
            last_was_separator = true;
        }
    }

    while normalized.ends_with('_') {
        normalized.pop();
    }

    (!normalized.is_empty()).then_some(normalized)
}

fn sanitize_safe_name(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty()
        || looks_like_ipv4(trimmed)
        || looks_like_ipv6(trimmed)
        || looks_like_mac(trimmed)
    {
        return None;
    }

    canonicalize_label(trimmed)
}

fn looks_like_ipv4(value: &str) -> bool {
    let mut segments = value.split('.');
    let segment_count = segments.clone().count();

    segment_count == 4 && segments.all(|segment| segment.parse::<u8>().is_ok())
}

fn looks_like_ipv6(value: &str) -> bool {
    value.contains(':')
        && value
            .chars()
            .all(|character| character.is_ascii_hexdigit() || character == ':')
        && value
            .split(':')
            .filter(|segment| !segment.is_empty())
            .count()
            >= 2
}

fn looks_like_mac(value: &str) -> bool {
    [':', '-'].into_iter().any(|separator| {
        let mut segments = value.split(separator);
        let segment_count = segments.clone().count();

        segment_count == 6
            && segments.all(|segment| {
                segment.len() == 2
                    && segment
                        .chars()
                        .all(|character| character.is_ascii_hexdigit())
            })
    })
}

fn trimmed_non_empty(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn parse_uptime_seconds(value: &str) -> Option<u64> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }

    parse_u64_text(trimmed).or_else(|| {
        let parts: Vec<_> = trimmed.split(':').collect();
        match parts.as_slice() {
            [hours, minutes, seconds] => Some(
                parse_u64_text(hours)? * 3_600
                    + parse_u64_text(minutes)? * 60
                    + parse_u64_text(seconds)?,
            ),
            [days, hours, minutes, seconds] => Some(
                parse_u64_text(days)? * 86_400
                    + parse_u64_text(hours)? * 3_600
                    + parse_u64_text(minutes)? * 60
                    + parse_u64_text(seconds)?,
            ),
            _ => None,
        }
    })
}

fn deserialize_optional_text<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(Option::<FlexibleValue>::deserialize(deserializer)?
        .and_then(FlexibleValue::into_text)
        .and_then(|value| trimmed_non_empty(Some(&value))))
}

fn deserialize_optional_u64<'de, D>(deserializer: D) -> Result<Option<u64>, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(Option::<FlexibleValue>::deserialize(deserializer)?.and_then(FlexibleValue::into_u64))
}

fn deserialize_optional_f64<'de, D>(deserializer: D) -> Result<Option<f64>, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(Option::<FlexibleValue>::deserialize(deserializer)?.and_then(FlexibleValue::into_f64))
}

fn deserialize_optional_bool<'de, D>(deserializer: D) -> Result<Option<bool>, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(Option::<FlexibleValue>::deserialize(deserializer)?.and_then(FlexibleValue::into_bool))
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum FlexibleValue {
    String(String),
    Unsigned(u64),
    Signed(i64),
    Float(f64),
    Bool(bool),
}

impl FlexibleValue {
    fn into_text(self) -> Option<String> {
        match self {
            Self::String(value) => Some(value),
            Self::Unsigned(value) => Some(value.to_string()),
            Self::Signed(value) => Some(value.to_string()),
            Self::Float(value) => Some(value.to_string()),
            Self::Bool(value) => Some(value.to_string()),
        }
    }

    fn into_u64(self) -> Option<u64> {
        match self {
            Self::Unsigned(value) => Some(value),
            Self::Signed(value) => u64::try_from(value).ok(),
            Self::Float(value) if value >= 0.0 && value.fract() == 0.0 => Some(value as u64),
            Self::Float(_) => None,
            Self::String(value) => parse_u64_text(&value),
            Self::Bool(value) => Some(u64::from(value)),
        }
    }

    fn into_f64(self) -> Option<f64> {
        match self {
            Self::Unsigned(value) => Some(value as f64),
            Self::Signed(value) => Some(value as f64),
            Self::Float(value) => Some(value),
            Self::String(value) => parse_f64_text(&value),
            Self::Bool(value) => Some(if value { 1.0 } else { 0.0 }),
        }
    }

    fn into_bool(self) -> Option<bool> {
        match self {
            Self::Bool(value) => Some(value),
            Self::Unsigned(value) => Some(value != 0),
            Self::Signed(value) => Some(value != 0),
            Self::Float(value) => Some(value != 0.0),
            Self::String(value) => parse_bool_text(&value),
        }
    }
}

fn parse_u64_text(value: &str) -> Option<u64> {
    let normalized = value.trim().trim_end_matches('%').replace(',', "");
    let candidate = normalized.split_whitespace().next()?;
    candidate.parse().ok()
}

fn parse_f64_text(value: &str) -> Option<f64> {
    let normalized = value.trim().trim_end_matches('%').replace(',', "");
    let candidate = normalized.split_whitespace().next()?;
    candidate.parse().ok()
}

fn parse_bool_text(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" | "up" => Some(true),
        "0" | "false" | "no" | "off" | "down" => Some(false),
        _ => None,
    }
}

impl SystemMetrics {
    fn is_empty(&self) -> bool {
        self.model_name.is_none()
            && self.software_version.is_none()
            && self.uptime_seconds.is_none()
            && self.memory_total_bytes.is_none()
            && self.memory_free_bytes.is_none()
            && self.cpu_usage_percent.is_none()
    }
}

impl CellularMetrics {
    fn is_empty(&self) -> bool {
        self.status.is_none()
            && self.access_technology.is_none()
            && self.current_band.is_none()
            && self.preferred_access_technology.is_none()
            && self.supported_access_technologies.is_empty()
            && self.supported_bands.is_empty()
            && self.preferred_bands.is_empty()
            && self.signals.is_empty()
            && self.carriers.is_empty()
    }
}

impl CellularCarrier {
    fn is_empty(&self) -> bool {
        self.band.is_none()
            && self.uplink_bandwidth_mhz.is_none()
            && self.downlink_bandwidth_mhz.is_none()
    }
}
