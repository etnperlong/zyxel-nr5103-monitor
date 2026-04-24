use zyxel_nr5103_monitor::telemetry::zyxel::{
    CellWanBandObject, CellWanStatusObject, DalResponse, InterfaceType, RadioKind,
    RouterTelemetrySnapshot, SignalKind, StatusObject, TrafficStatusObject,
};

const STATUS_JSON: &str = r#"
{
  "result": "ZCFG_SUCCESS",
  "Object": [
    {
      "DeviceInfo": {
        "ModelName": "NR5103",
        "SoftwareVersion": "V1.00(ABCD.4)C0",
        "UpTime": "01:01:01",
        "IMEI": "fake-imei"
      },
      "MemoryStatus": {
        "Total": "1024",
        "Free": "256"
      },
      "ProcessStatus": {
        "CPUUsage": "12.5"
      },
      "LanPortInfo": [
        {
          "Status": "Up",
          "Name": "LAN1",
          "MaxBitRate": "1000",
          "X_ZYXEL_SwitchToWAN": "1",
          "MACAddress": "aa:bb:cc:dd:ee:ff"
        },
        {
          "Status": "Down",
          "Name": "LAN2",
          "MaxBitRate": "1000",
          "X_ZYXEL_SwitchToWAN": 0
        }
      ]
    }
  ]
}
"#;

const CELLWAN_STATUS_JSON: &str = r#"
{
  "result": "ZCFG_SUCCESS",
  "Object": [
    {
      "INTF_Status": "Up",
      "INTF_RSSI": "-65",
      "INTF_RSRP": "-95",
      "INTF_RSRQ": "-11",
      "INTF_SINR": "19",
      "NSA_Band": "n78",
      "NSA_RSSI": "-61",
      "NSA_RSRP": "-88",
      "NSA_RSRQ": "-10",
      "NSA_SINR": "20",
      "SCC_Info": [
        {
          "Band": "B1",
          "RSSI": "-70",
          "RSRP": "-97",
          "RSRQ": "-13",
          "SINR": "11",
          "UplinkBandwidth": "15",
          "DownlinkBandwidth": "20",
          "Cell_ID": "12345"
        }
      ],
      "IMSI": "fake-imsi"
    }
  ]
}
"#;

const TRAFFIC_STATUS_JSON: &str = r#"
{
  "result": "ZCFG_SUCCESS",
  "Object": [
    {
      "ipIface": [
        {
          "Name": "Cellular",
          "Status": "Up",
          "X_ZYXEL_IfName": "wwan0",
          "X_ZYXEL_SrvName": "internet",
          "IPAddress": "192.0.2.10"
        },
        {
          "Name": "ShouldBeSkipped",
          "Status": "Down",
          "X_ZYXEL_IfName": "wwan1",
          "X_ZYXEL_SrvName": "backup"
        }
      ],
      "ipIfaceSt": [
        {
          "BytesSent": "1000",
          "BytesReceived": "2000",
          "PacketsSent": "10",
          "PacketsReceived": "20",
          "ErrorsSent": "1",
          "ErrorsReceived": "2",
          "DiscardPacketsSent": "3",
          "DiscardPacketsReceived": "4"
        }
      ],
      "ethIface": [
        {
          "Name": "LAN1",
          "Status": "Up",
          "MaxBitRate": "1000",
          "MACAddress": "11:22:33:44:55:66"
        }
      ],
      "ethIfaceSt": [
        {
          "BytesSent": "3000",
          "BytesReceived": "4000",
          "PacketsSent": "30",
          "PacketsReceived": "40",
          "ErrorsSent": "0",
          "ErrorsReceived": "1",
          "DiscardPacketsSent": "0",
          "DiscardPacketsReceived": "2"
        }
      ]
    }
  ]
}
"#;

const CELLWAN_BAND_JSON: &str = r#"
{
  "result": "ZCFG_SUCCESS",
  "Object": [
    {
      "INTF_Supported_Access_Technologies": "LTE, NR5G-NSA",
      "INTF_Preferred_Access_Technology": "NR5G-NSA",
      "INTF_Current_Access_Technology": "NR5G-NSA",
      "INTF_Supported_Bands": "B3, n78",
      "INTF_Preferred_Bands": "n78",
      "INTF_Current_Band": "B3",
      "SessionKey": 42
    }
  ]
}
"#;

fn parse_response<T>(json: &str) -> DalResponse<T>
where
    T: serde::de::DeserializeOwned,
{
    serde_json::from_str(json).unwrap()
}

#[test]
fn router_telemetry_snapshot_from_dal_responses_extracts_safe_metrics() {
    let snapshot = RouterTelemetrySnapshot::from_dal_responses(
        Some(parse_response::<StatusObject>(STATUS_JSON)),
        Some(parse_response::<CellWanStatusObject>(CELLWAN_STATUS_JSON)),
        Some(parse_response::<TrafficStatusObject>(TRAFFIC_STATUS_JSON)),
        Some(parse_response::<CellWanBandObject>(CELLWAN_BAND_JSON)),
    );

    let system = snapshot.system.expect("system metrics should be present");
    assert_eq!(system.model_name.as_deref(), Some("NR5103"));
    assert_eq!(system.software_version.as_deref(), Some("V1.00(ABCD.4)C0"));
    assert_eq!(system.uptime_seconds, Some(3661));
    assert_eq!(system.memory_total_bytes, Some(1024));
    assert_eq!(system.memory_free_bytes, Some(256));
    assert_eq!(system.cpu_usage_percent, Some(12.5));

    let cellular = snapshot
        .cellular
        .expect("cellular metrics should be present");
    assert_eq!(cellular.status.as_deref(), Some("up"));
    assert_eq!(cellular.access_technology.as_deref(), Some("nr5g_nsa"));
    assert_eq!(cellular.current_band.as_deref(), Some("b3"));
    assert_eq!(
        cellular.preferred_access_technology.as_deref(),
        Some("nr5g_nsa")
    );
    assert_eq!(
        cellular.supported_access_technologies,
        vec!["lte", "nr5g_nsa"]
    );
    assert_eq!(cellular.supported_bands, vec!["b3", "n78"]);
    assert_eq!(cellular.preferred_bands, vec!["n78"]);
    assert!(cellular.signals.iter().any(|signal| {
        signal.radio == RadioKind::Lte
            && signal.signal_kind == SignalKind::Rsrp
            && signal.value == -95.0
            && signal.band.as_deref() == Some("b3")
            && signal.carrier_index.is_none()
    }));
    assert!(cellular.signals.iter().any(|signal| {
        signal.radio == RadioKind::NrNsa
            && signal.signal_kind == SignalKind::Sinr
            && signal.value == 20.0
            && signal.band.as_deref() == Some("n78")
    }));
    assert!(cellular.signals.iter().any(|signal| {
        signal.radio == RadioKind::Scc
            && signal.signal_kind == SignalKind::Rssi
            && signal.value == -70.0
            && signal.band.as_deref() == Some("b1")
            && signal.carrier_index == Some(0)
    }));
    assert_eq!(cellular.carriers.len(), 1);
    assert_eq!(cellular.carriers[0].band.as_deref(), Some("b1"));
    assert_eq!(cellular.carriers[0].uplink_bandwidth_mhz, Some(15));
    assert_eq!(cellular.carriers[0].downlink_bandwidth_mhz, Some(20));

    assert_eq!(snapshot.traffic.len(), 2);
    assert_eq!(snapshot.traffic[0].interface_type, InterfaceType::Ip);
    assert_eq!(snapshot.traffic[0].interface_name, "wwan0");
    assert_eq!(snapshot.traffic[0].status.as_deref(), Some("up"));
    assert_eq!(snapshot.traffic[0].bytes_sent, Some(1000));
    assert_eq!(snapshot.traffic[0].bytes_received, Some(2000));
    assert_eq!(snapshot.traffic[0].packets_sent, Some(10));
    assert_eq!(snapshot.traffic[0].packets_received, Some(20));
    assert_eq!(snapshot.traffic[0].errors_sent, Some(1));
    assert_eq!(snapshot.traffic[0].errors_received, Some(2));
    assert_eq!(snapshot.traffic[0].discards_sent, Some(3));
    assert_eq!(snapshot.traffic[0].discards_received, Some(4));
    assert_eq!(snapshot.traffic[1].interface_type, InterfaceType::Ethernet);
    assert_eq!(snapshot.traffic[1].interface_name, "lan1");
    assert_eq!(snapshot.traffic[1].status.as_deref(), Some("up"));
    assert_eq!(snapshot.traffic[1].max_bit_rate_mbps, Some(1000));
    assert!(
        !snapshot
            .traffic
            .iter()
            .any(|interface| interface.interface_name == "wwan1")
    );

    assert_eq!(snapshot.lan_ports.len(), 2);
    assert_eq!(snapshot.lan_ports[0].port_name, "lan1");
    assert_eq!(snapshot.lan_ports[0].status.as_deref(), Some("up"));
    assert_eq!(snapshot.lan_ports[0].max_bit_rate_mbps, Some(1000));
    assert_eq!(snapshot.lan_ports[0].switch_to_wan, Some(true));
    assert_eq!(snapshot.lan_ports[1].port_name, "lan2");
    assert_eq!(snapshot.lan_ports[1].status.as_deref(), Some("down"));
    assert_eq!(snapshot.lan_ports[1].switch_to_wan, Some(false));
}

#[test]
fn router_telemetry_snapshot_from_dal_responses_allows_missing_optional_endpoints() {
    let snapshot = RouterTelemetrySnapshot::from_dal_responses(
        Some(parse_response::<StatusObject>(STATUS_JSON)),
        None,
        Some(parse_response::<TrafficStatusObject>(TRAFFIC_STATUS_JSON)),
        None,
    );

    assert!(snapshot.system.is_some());
    assert!(snapshot.cellular.is_none());
    assert_eq!(snapshot.traffic.len(), 2);
    assert_eq!(snapshot.lan_ports.len(), 2);
}
