use std::time::Duration;

use zyxel_nr5103_monitor::config::{TelemetryConfig, TelemetryProtocol, TelemetrySignalConfig};
use zyxel_nr5103_monitor::telemetry::otel::TelemetryRuntime;

fn telemetry_config(endpoint: &str) -> TelemetryConfig {
    TelemetryConfig {
        service_name: "zyxel-nr5103-monitor-test".to_string(),
        endpoint: Some(endpoint.to_string()),
        authorization: None,
        protocol: TelemetryProtocol::Grpc,
        export_interval: Duration::from_secs(1),
        metrics: TelemetrySignalConfig::default(),
        traces: TelemetrySignalConfig::default(),
        logs: TelemetrySignalConfig::default(),
    }
}

#[test]
fn telemetry_disabled_does_not_build_exporters() {
    let runtime = TelemetryRuntime::init(&telemetry_config("http://[::1"))
        .expect("disabled telemetry should not attempt exporter setup");
    let meter = runtime.meter("telemetry-runtime-test");
    let counter = meter.u64_counter("telemetry_runtime_test_counter").build();

    counter.add(1, &[]);

    runtime
        .shutdown()
        .expect("disabled telemetry shutdown should be a no-op");
}

#[test]
fn telemetry_enabled_invalid_endpoint_errors() {
    let mut config = telemetry_config("http://[::1");
    config.traces.enabled = true;

    let error =
        TelemetryRuntime::init(&config).expect_err("enabled telemetry should validate exporters");

    assert!(
        error.to_string().contains("OpenTelemetry"),
        "unexpected error: {error:#}"
    );
}
