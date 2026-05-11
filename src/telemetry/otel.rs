use anyhow::{Context, Result, anyhow, bail};
use opentelemetry::{
    KeyValue, metrics::Meter, metrics::MeterProvider as _, trace::TracerProvider as _,
};
use opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::{
    Resource,
    logs::SdkLoggerProvider,
    metrics::{PeriodicReader, SdkMeterProvider},
    trace::SdkTracerProvider,
};
use tracing_subscriber::{EnvFilter, layer::SubscriberExt as _};

use crate::config::TelemetryConfig;

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

        if let Some(logger_provider) = self.logger_provider
            && let Err(error) = logger_provider.shutdown()
        {
            errors.push(format!(
                "failed to shut down OpenTelemetry logs provider: {error}"
            ));
        }

        if let Some(meter_provider) = self.meter_provider
            && let Err(error) = meter_provider.shutdown()
        {
            errors.push(format!(
                "failed to shut down OpenTelemetry metrics provider: {error}"
            ));
        }

        if let Some(tracer_provider) = self.tracer_provider
            && let Err(error) = tracer_provider.shutdown()
        {
            errors.push(format!(
                "failed to shut down OpenTelemetry traces provider: {error}"
            ));
        }

        if errors.is_empty() {
            Ok(())
        } else {
            bail!(errors.join("; "));
        }
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
    let exporter_builder = opentelemetry_otlp::SpanExporter::builder().with_tonic();
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
    let exporter_builder = opentelemetry_otlp::MetricExporter::builder().with_tonic();
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
    let exporter_builder = opentelemetry_otlp::LogExporter::builder().with_tonic();
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
