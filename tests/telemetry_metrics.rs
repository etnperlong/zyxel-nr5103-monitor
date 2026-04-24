use std::collections::BTreeMap;

use opentelemetry::{KeyValue, metrics::MeterProvider as _};
use opentelemetry_sdk::metrics::{InMemoryMetricExporter, SdkMeterProvider, data::MetricData};
use zyxel_nr5103_monitor::telemetry::{
    metrics::MetricRecorder,
    zyxel::{InterfaceCounters, InterfaceType, RouterTelemetrySnapshot},
};

fn metric_harness() -> (MetricRecorder, SdkMeterProvider, InMemoryMetricExporter) {
    let exporter = InMemoryMetricExporter::default();
    let meter_provider = SdkMeterProvider::builder()
        .with_periodic_exporter(exporter.clone())
        .build();
    let recorder = MetricRecorder::new(meter_provider.meter("telemetry-metrics-test"));

    (recorder, meter_provider, exporter)
}

fn traffic_snapshot(bytes_sent: u64) -> RouterTelemetrySnapshot {
    RouterTelemetrySnapshot {
        traffic: vec![InterfaceCounters {
            interface_type: InterfaceType::Ip,
            interface_name: "wan1".to_string(),
            status: Some("up".to_string()),
            max_bit_rate_mbps: None,
            bytes_sent: Some(bytes_sent),
            bytes_received: None,
            packets_sent: None,
            packets_received: None,
            errors_sent: None,
            errors_received: None,
            discards_sent: None,
            discards_received: None,
        }],
        ..RouterTelemetrySnapshot::default()
    }
}

fn force_flush(
    meter_provider: &SdkMeterProvider,
    exporter: &InMemoryMetricExporter,
) -> Vec<opentelemetry_sdk::metrics::data::ResourceMetrics> {
    meter_provider.force_flush().unwrap();
    exporter.get_finished_metrics().unwrap()
}

fn traffic_byte_points(
    metrics: &[opentelemetry_sdk::metrics::data::ResourceMetrics],
) -> Vec<(u64, BTreeMap<String, String>)> {
    metrics
        .iter()
        .flat_map(|resource| resource.scope_metrics())
        .flat_map(|scope| scope.metrics())
        .filter(|metric| metric.name() == "zyxel.interface.traffic.bytes")
        .flat_map(|metric| match metric.data() {
            opentelemetry_sdk::metrics::data::AggregatedMetrics::U64(MetricData::Sum(sum)) => sum
                .data_points()
                .map(|point| (point.value(), attributes_map(point.attributes())))
                .collect::<Vec<_>>(),
            _ => Vec::new(),
        })
        .collect()
}

fn attributes_map<'a>(attributes: impl Iterator<Item = &'a KeyValue>) -> BTreeMap<String, String> {
    attributes
        .map(|attribute| {
            (
                attribute.key.as_str().to_string(),
                attribute.value.to_string(),
            )
        })
        .collect()
}

#[test]
fn telemetry_metrics_first_sample_establishes_baseline() {
    let (mut recorder, meter_provider, exporter) = metric_harness();

    recorder.record_router_snapshot(&traffic_snapshot(42));

    let metrics = force_flush(&meter_provider, &exporter);

    assert!(traffic_byte_points(&metrics).is_empty());
}

#[test]
fn telemetry_metrics_second_sample_emits_delta() {
    let (mut recorder, meter_provider, exporter) = metric_harness();

    recorder.record_router_snapshot(&traffic_snapshot(42));
    recorder.record_router_snapshot(&traffic_snapshot(67));

    let metrics = force_flush(&meter_provider, &exporter);
    let points = traffic_byte_points(&metrics);

    assert_eq!(points.len(), 1);
    assert_eq!(points[0].0, 25);
    assert_eq!(
        points[0].1,
        BTreeMap::from([
            ("direction".to_string(), "sent".to_string()),
            ("interface_name".to_string(), "wan1".to_string()),
            ("interface_type".to_string(), "ip".to_string()),
        ])
    );
}

#[test]
fn telemetry_metrics_counter_reset_emits_no_negative_delta() {
    let (mut recorder, meter_provider, exporter) = metric_harness();

    recorder.record_router_snapshot(&traffic_snapshot(67));
    recorder.record_router_snapshot(&traffic_snapshot(12));
    recorder.record_router_snapshot(&traffic_snapshot(20));

    let metrics = force_flush(&meter_provider, &exporter);
    let points = traffic_byte_points(&metrics);

    assert_eq!(points.len(), 1);
    assert_eq!(points[0].0, 8);
}
