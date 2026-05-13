use anyhow::{Context, Result, anyhow, bail};
use opentelemetry::{
    KeyValue, metrics::Meter, metrics::MeterProvider as _, trace::TracerProvider as _,
};
use opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge;
use opentelemetry_otlp::{
    Protocol, WithExportConfig, WithHttpConfig, WithTonicConfig,
    tonic_types::{metadata::MetadataMap, transport::ClientTlsConfig},
};
use opentelemetry_sdk::{
    Resource,
    logs::SdkLoggerProvider,
    metrics::{PeriodicReader, SdkMeterProvider},
    trace::SdkTracerProvider,
};
use tracing::{debug, warn};
use tracing_subscriber::{EnvFilter, layer::SubscriberExt as _};

use crate::config::{TelemetryConfig, TelemetryProtocol};

#[derive(Debug)]
pub struct TelemetryRuntime {
    service_name: String,
    tracer_provider: Option<SdkTracerProvider>,
    meter_provider: Option<SdkMeterProvider>,
    logger_provider: Option<SdkLoggerProvider>,
    noop_meter_provider: SdkMeterProvider,
}

impl TelemetryRuntime {
    pub fn init(cfg: &TelemetryConfig) -> Result<Self> {
        let noop_meter_provider = SdkMeterProvider::builder().build();
        if !cfg.metrics.enabled && !cfg.traces.enabled && !cfg.logs.enabled {
            return Ok(Self {
                service_name: cfg.service_name.clone(),
                tracer_provider: None,
                meter_provider: None,
                logger_provider: None,
                noop_meter_provider,
            });
        }

        let resource = resource(cfg);
        let meter_provider = if cfg.metrics.enabled {
            Some(
                build_meter_provider(cfg, resource.clone())
                    .context("failed to initialize OpenTelemetry metrics exporter")?,
            )
        } else {
            None
        };
        let tracer_provider = if cfg.traces.enabled {
            Some(
                build_tracer_provider(cfg, resource.clone())
                    .context("failed to initialize OpenTelemetry traces exporter")?,
            )
        } else {
            None
        };
        let logger_provider = if cfg.logs.enabled {
            Some(
                build_logger_provider(cfg, resource)
                    .context("failed to initialize OpenTelemetry logs exporter")?,
            )
        } else {
            None
        };

        Ok(Self {
            service_name: cfg.service_name.clone(),
            tracer_provider,
            meter_provider,
            logger_provider,
            noop_meter_provider,
        })
    }

    pub fn meter(&self, name: &'static str) -> Meter {
        if let Some(provider) = &self.meter_provider {
            provider.meter(name)
        } else {
            self.noop_meter_provider.meter(name)
        }
    }

    pub fn shutdown(self) -> Result<()> {
        let mut errors = Vec::new();

        if let Some(logger_provider) = self.logger_provider {
            debug!("Shutting down OpenTelemetry logs provider");
            match logger_provider.shutdown() {
                Ok(()) => debug!("OpenTelemetry logs provider shut down successfully"),
                Err(error) => {
                    warn!(error = ?error, "OpenTelemetry logs provider shutdown failed");
                    errors.push(format!(
                        "failed to shut down OpenTelemetry logs provider: {error}"
                    ));
                }
            }
        } else {
            debug!(
                "OpenTelemetry logs provider is disabled by configuration; skipping shutdown (this is not an error)"
            );
        }

        if let Some(meter_provider) = self.meter_provider {
            debug!("Shutting down OpenTelemetry metrics provider");
            match meter_provider.shutdown() {
                Ok(()) => debug!("OpenTelemetry metrics provider shut down successfully"),
                Err(error) => {
                    warn!(error = ?error, "OpenTelemetry metrics provider shutdown failed");
                    errors.push(format!(
                        "failed to shut down OpenTelemetry metrics provider: {error}"
                    ));
                }
            }
        } else {
            debug!(
                "OpenTelemetry metrics provider is disabled by configuration; skipping shutdown (this is not an error)"
            );
        }

        if let Some(tracer_provider) = self.tracer_provider {
            debug!("Shutting down OpenTelemetry traces provider");
            match tracer_provider.shutdown() {
                Ok(()) => debug!("OpenTelemetry traces provider shut down successfully"),
                Err(error) => {
                    warn!(error = ?error, "OpenTelemetry traces provider shutdown failed");
                    errors.push(format!(
                        "failed to shut down OpenTelemetry traces provider: {error}"
                    ));
                }
            }
        } else {
            debug!(
                "OpenTelemetry traces provider is disabled by configuration; skipping shutdown (this is not an error)"
            );
        }

        if errors.is_empty() {
            Ok(())
        } else {
            bail!(errors.join("; "));
        }
    }
}

pub fn log_debug_configuration(cfg: &TelemetryConfig) {
    let endpoint = cfg.endpoint.as_deref();
    debug!(
        service_name = %cfg.service_name,
        endpoint = endpoint.unwrap_or("<default from OTLP env>"),
        protocol = cfg.protocol.as_str(),
        tls_enabled_for_configured_endpoint = endpoint_needs_tls(endpoint),
        endpoint_contains_path = endpoint_has_path(endpoint),
        export_interval_seconds = cfg.export_interval.as_secs(),
        metrics_enabled = cfg.metrics.enabled,
        traces_enabled = cfg.traces.enabled,
        logs_enabled = cfg.logs.enabled,
        authorization_configured = cfg.authorization.is_some(),
        authorization_scheme = authorization_scheme(cfg.authorization.as_deref()).unwrap_or("<none>"),
        otel_exporter_otlp_endpoint_env = env_var_state("OTEL_EXPORTER_OTLP_ENDPOINT"),
        otel_exporter_otlp_metrics_endpoint_env = env_var_state("OTEL_EXPORTER_OTLP_METRICS_ENDPOINT"),
        otel_exporter_otlp_headers_env = env_var_state("OTEL_EXPORTER_OTLP_HEADERS"),
        otel_exporter_otlp_metrics_headers_env = env_var_state("OTEL_EXPORTER_OTLP_METRICS_HEADERS"),
        "OpenTelemetry configuration"
    );

    debug!(
        metrics_provider_configured = cfg.metrics.enabled,
        traces_provider_configured = cfg.traces.enabled,
        logs_provider_configured = cfg.logs.enabled,
        "OpenTelemetry provider plan"
    );

    if endpoint_has_path(endpoint) {
        debug!(
            endpoint = endpoint.unwrap_or_default(),
            protocol = cfg.protocol.as_str(),
            "Configured OTLP endpoint includes a path"
        );
    }

    if cfg.authorization.is_some() && std::env::var_os("OTEL_EXPORTER_OTLP_HEADERS").is_some() {
        debug!(
            "Both telemetry.authorization and OTEL_EXPORTER_OTLP_HEADERS are configured; exporter metadata will include both sources, and duplicate authorization headers may confuse collectors"
        );
    }
}

pub fn install_subscriber(log_level: &str, telemetry: &TelemetryRuntime) -> Result<()> {
    let filter = EnvFilter::try_new(log_level).unwrap_or_else(|_| EnvFilter::new("info"));
    let trace_layer = telemetry.tracer_provider.as_ref().map(|provider| {
        tracing_opentelemetry::layer().with_tracer(provider.tracer(telemetry.service_name.clone()))
    });
    let log_layer = telemetry
        .logger_provider
        .as_ref()
        .map(OpenTelemetryTracingBridge::new);

    tracing::subscriber::set_global_default(
        tracing_subscriber::registry()
            .with(filter)
            .with(tracing_subscriber::fmt::layer())
            .with(trace_layer)
            .with(log_layer),
    )
    .map_err(|error| anyhow!(error))
}

fn resource(cfg: &TelemetryConfig) -> Resource {
    Resource::builder()
        .with_service_name(cfg.service_name.clone())
        .with_attributes([KeyValue::new("service.version", env!("CARGO_PKG_VERSION"))])
        .build()
}

fn build_tracer_provider(cfg: &TelemetryConfig, resource: Resource) -> Result<SdkTracerProvider> {
    if cfg.protocol == TelemetryProtocol::HttpProtobuf {
        let mut exporter_builder = opentelemetry_otlp::SpanExporter::builder()
            .with_http()
            .with_protocol(Protocol::HttpBinary);
        if let Some(headers) = authorization_headers(cfg) {
            exporter_builder = exporter_builder.with_headers(headers);
        }
        if let Some(endpoint) = http_signal_endpoint(cfg.endpoint.as_deref(), "traces") {
            exporter_builder = exporter_builder.with_endpoint(endpoint);
        }
        let exporter = exporter_builder
            .build()
            .with_context(|| trace_exporter_error(cfg.endpoint.as_deref()))?;

        return Ok(SdkTracerProvider::builder()
            .with_resource(resource)
            .with_batch_exporter(exporter)
            .build());
    }

    let mut exporter_builder = opentelemetry_otlp::SpanExporter::builder().with_tonic();
    if let Some(metadata) = authorization_metadata(cfg)? {
        exporter_builder = exporter_builder.with_metadata(metadata);
    }
    if endpoint_needs_tls(cfg.endpoint.as_deref()) {
        debug!(
            signal = "traces",
            "Enabling TLS for HTTPS OTLP/gRPC endpoint"
        );
        exporter_builder =
            exporter_builder.with_tls_config(ClientTlsConfig::new().with_enabled_roots());
    }
    let exporter = if let Some(endpoint) = cfg.endpoint.as_deref() {
        exporter_builder.with_endpoint(endpoint).build()
    } else {
        exporter_builder.build()
    }
    .with_context(|| trace_exporter_error(cfg.endpoint.as_deref()))?;

    Ok(SdkTracerProvider::builder()
        .with_resource(resource)
        .with_batch_exporter(exporter)
        .build())
}

fn build_meter_provider(cfg: &TelemetryConfig, resource: Resource) -> Result<SdkMeterProvider> {
    if cfg.protocol == TelemetryProtocol::HttpProtobuf {
        let mut exporter_builder = opentelemetry_otlp::MetricExporter::builder()
            .with_http()
            .with_protocol(Protocol::HttpBinary);
        if let Some(headers) = authorization_headers(cfg) {
            exporter_builder = exporter_builder.with_headers(headers);
        }
        if let Some(endpoint) = http_signal_endpoint(cfg.endpoint.as_deref(), "metrics") {
            exporter_builder = exporter_builder.with_endpoint(endpoint);
        }
        let exporter = exporter_builder
            .build()
            .with_context(|| metric_exporter_error(cfg.endpoint.as_deref()))?;

        let reader = PeriodicReader::builder(exporter)
            .with_interval(cfg.export_interval)
            .build();

        return Ok(SdkMeterProvider::builder()
            .with_resource(resource)
            .with_reader(reader)
            .build());
    }

    let mut exporter_builder = opentelemetry_otlp::MetricExporter::builder().with_tonic();
    if let Some(metadata) = authorization_metadata(cfg)? {
        exporter_builder = exporter_builder.with_metadata(metadata);
    }
    if endpoint_needs_tls(cfg.endpoint.as_deref()) {
        debug!(
            signal = "metrics",
            "Enabling TLS for HTTPS OTLP/gRPC endpoint"
        );
        exporter_builder =
            exporter_builder.with_tls_config(ClientTlsConfig::new().with_enabled_roots());
    }
    let exporter = if let Some(endpoint) = cfg.endpoint.as_deref() {
        exporter_builder.with_endpoint(endpoint).build()
    } else {
        exporter_builder.build()
    }
    .with_context(|| metric_exporter_error(cfg.endpoint.as_deref()))?;

    let reader = PeriodicReader::builder(exporter)
        .with_interval(cfg.export_interval)
        .build();

    Ok(SdkMeterProvider::builder()
        .with_resource(resource)
        .with_reader(reader)
        .build())
}

fn build_logger_provider(cfg: &TelemetryConfig, resource: Resource) -> Result<SdkLoggerProvider> {
    if cfg.protocol == TelemetryProtocol::HttpProtobuf {
        let mut exporter_builder = opentelemetry_otlp::LogExporter::builder()
            .with_http()
            .with_protocol(Protocol::HttpBinary);
        if let Some(headers) = authorization_headers(cfg) {
            exporter_builder = exporter_builder.with_headers(headers);
        }
        if let Some(endpoint) = http_signal_endpoint(cfg.endpoint.as_deref(), "logs") {
            exporter_builder = exporter_builder.with_endpoint(endpoint);
        }
        let exporter = exporter_builder
            .build()
            .with_context(|| log_exporter_error(cfg.endpoint.as_deref()))?;

        return Ok(SdkLoggerProvider::builder()
            .with_resource(resource)
            .with_batch_exporter(exporter)
            .build());
    }

    let mut exporter_builder = opentelemetry_otlp::LogExporter::builder().with_tonic();
    if let Some(metadata) = authorization_metadata(cfg)? {
        exporter_builder = exporter_builder.with_metadata(metadata);
    }
    if endpoint_needs_tls(cfg.endpoint.as_deref()) {
        debug!(signal = "logs", "Enabling TLS for HTTPS OTLP/gRPC endpoint");
        exporter_builder =
            exporter_builder.with_tls_config(ClientTlsConfig::new().with_enabled_roots());
    }
    let exporter = if let Some(endpoint) = cfg.endpoint.as_deref() {
        exporter_builder.with_endpoint(endpoint).build()
    } else {
        exporter_builder.build()
    }
    .with_context(|| log_exporter_error(cfg.endpoint.as_deref()))?;

    Ok(SdkLoggerProvider::builder()
        .with_resource(resource)
        .with_batch_exporter(exporter)
        .build())
}

fn authorization_metadata(cfg: &TelemetryConfig) -> Result<Option<MetadataMap>> {
    let Some(authorization) = cfg.authorization.as_deref() else {
        return Ok(None);
    };

    let mut metadata = MetadataMap::new();
    metadata.insert(
        "authorization",
        authorization
            .parse()
            .context("invalid telemetry authorization header value")?,
    );

    Ok(Some(metadata))
}

fn authorization_headers(
    cfg: &TelemetryConfig,
) -> Option<std::collections::HashMap<String, String>> {
    let authorization = cfg.authorization.as_deref()?;
    let mut headers = std::collections::HashMap::new();
    headers.insert("authorization".to_string(), authorization.to_string());
    Some(headers)
}

fn authorization_scheme(authorization: Option<&str>) -> Option<&str> {
    authorization?.split_ascii_whitespace().next()
}

fn env_var_state(name: &str) -> &'static str {
    if std::env::var_os(name).is_some() {
        "set"
    } else {
        "unset"
    }
}

fn endpoint_needs_tls(endpoint: Option<&str>) -> bool {
    endpoint
        .map(|endpoint| endpoint.starts_with("https://"))
        .unwrap_or(false)
}

fn endpoint_has_path(endpoint: Option<&str>) -> bool {
    endpoint
        .and_then(|endpoint| endpoint.split_once("://").map(|(_, rest)| rest))
        .and_then(|rest| rest.split_once('/').map(|(_, path)| path))
        .map(|path| !path.is_empty())
        .unwrap_or(false)
}

fn http_signal_endpoint(endpoint: Option<&str>, signal: &str) -> Option<String> {
    let endpoint = endpoint?;
    let signal_path = format!("/v1/{signal}");
    if endpoint.ends_with(&signal_path) {
        Some(endpoint.to_string())
    } else {
        Some(format!("{}{}", endpoint.trim_end_matches('/'), signal_path))
    }
}

fn trace_exporter_error(endpoint: Option<&str>) -> String {
    exporter_error("traces", endpoint)
}

fn metric_exporter_error(endpoint: Option<&str>) -> String {
    exporter_error("metrics", endpoint)
}

fn log_exporter_error(endpoint: Option<&str>) -> String {
    exporter_error("logs", endpoint)
}

fn exporter_error(signal: &str, endpoint: Option<&str>) -> String {
    match endpoint {
        Some(endpoint) => format!(
            "OpenTelemetry {signal} exporter could not be configured for endpoint {endpoint}"
        ),
        None => format!(
            "OpenTelemetry {signal} exporter could not be configured from the default OTLP environment"
        ),
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::config::TelemetrySignalConfig;

    fn telemetry_config(authorization: Option<&str>) -> TelemetryConfig {
        TelemetryConfig {
            service_name: "zyxel-nr5103-monitor-test".to_string(),
            endpoint: Some("http://localhost:4317".to_string()),
            authorization: authorization.map(str::to_string),
            protocol: crate::config::TelemetryProtocol::Grpc,
            export_interval: Duration::from_secs(1),
            metrics: TelemetrySignalConfig::default(),
            traces: TelemetrySignalConfig::default(),
            logs: TelemetrySignalConfig::default(),
        }
    }

    #[test]
    fn authorization_metadata_sets_authorization_header() {
        let metadata = authorization_metadata(&telemetry_config(Some("Bearer glc_test_token")))
            .expect("valid authorization header should be accepted")
            .expect("authorization metadata should be present");

        assert_eq!(
            metadata
                .get("authorization")
                .expect("authorization header should be set"),
            "Bearer glc_test_token"
        );
    }

    #[test]
    fn authorization_metadata_rejects_invalid_header_values() {
        let error = authorization_metadata(&telemetry_config(Some("Bearer invalid\nvalue")))
            .expect_err("newline in authorization header should be rejected");

        assert!(
            error.to_string().contains("authorization"),
            "unexpected error: {error:#}"
        );
        assert!(
            !error.to_string().contains("invalid\nvalue"),
            "authorization value should not be included in error text: {error:#}"
        );
    }

    #[test]
    fn authorization_scheme_reports_only_prefix_without_secret() {
        assert_eq!(
            authorization_scheme(Some("Bearer glc_secret_token")),
            Some("Bearer")
        );
        assert_eq!(
            authorization_scheme(Some("Basic encoded_secret")),
            Some("Basic")
        );
        assert_eq!(authorization_scheme(None), None);
    }

    #[test]
    fn endpoint_needs_tls_only_for_https_endpoints() {
        assert!(endpoint_needs_tls(Some(
            "https://otlp-gateway-prod-eu-west-0.grafana.net/otlp"
        )));
        assert!(!endpoint_needs_tls(Some("http://localhost:4317")));
        assert!(!endpoint_needs_tls(None));
    }

    #[test]
    fn endpoint_has_path_detects_non_empty_uri_path() {
        assert!(endpoint_has_path(Some(
            "https://otlp-gateway-prod-eu-west-0.grafana.net/otlp"
        )));
        assert!(!endpoint_has_path(Some(
            "https://otlp-gateway-prod-eu-west-0.grafana.net"
        )));
        assert!(!endpoint_has_path(Some("http://localhost:4317")));
        assert!(!endpoint_has_path(None));
    }

    #[test]
    fn http_signal_endpoint_appends_signal_path_to_generic_endpoint() {
        assert_eq!(
            http_signal_endpoint(Some("https://example.com/otlp"), "metrics"),
            Some("https://example.com/otlp/v1/metrics".to_string())
        );
        assert_eq!(
            http_signal_endpoint(Some("https://example.com/otlp/v1/traces"), "traces"),
            Some("https://example.com/otlp/v1/traces".to_string())
        );
        assert_eq!(http_signal_endpoint(None, "metrics"), None);
    }
}
