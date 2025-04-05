use std::{env, str::FromStr};

use anyhow::Result;
use opentelemetry::global;
use opentelemetry::trace::{Tracer, TracerProvider as _};
use opentelemetry_otlp::{SpanExporter, WithExportConfig};
use opentelemetry_sdk::trace::{Sampler, SdkTracerProvider};
use simple_kv::{RotationConfig, ServerConfig, start_server_with_config};
use tokio::fs;
use tracing::span;
use tracing_subscriber::{
    EnvFilter, filter,
    fmt::{self, format},
    layer::SubscriberExt,
    prelude::*,
};

#[tokio::main]
async fn main() -> Result<()> {
    let config = match env::var("KV_SERVER_CONFIG") {
        Ok(path) => fs::read_to_string(&path).await?,
        Err(_) => include_str!("../fixtures/quic_server.conf").to_string(),
    };
    let config: ServerConfig = toml::from_str(&config)?;
    let log = &config.log;

    unsafe {
        env::set_var("RUST_LOG", &log.log_level);
        // 设置 OTLP 端点
        env::set_var("OTEL_EXPORTER_OTLP_ENDPOINT", "http://localhost:4317");
        // 设置服务名称
        env::set_var("OTEL_SERVICE_NAME", "kv-server");
    }

    let stdout_log = fmt::layer().compact();

    // 初始化 OTLP tracer
    let exporter = SpanExporter::builder()
        .with_tonic()
        .with_protocol(opentelemetry_otlp::Protocol::Grpc)
        .build()
        .unwrap();
    let provider = SdkTracerProvider::builder()
        .with_sampler(Sampler::ParentBased(Box::new(Sampler::TraceIdRatioBased(
            1.0,
        ))))
        .with_batch_exporter(exporter)
        .build();
    let tracer = provider.tracer("kv-server");

    // 创建 OpenTelemetry 层
    let telemetry = tracing_opentelemetry::layer().with_tracer(tracer);

    let file_appender = match log.rotation {
        RotationConfig::Hourly => tracing_appender::rolling::hourly(&log.path, "server.log"),
        RotationConfig::Daily => tracing_appender::rolling::daily(&log.path, "server.log"),
        RotationConfig::Never => tracing_appender::rolling::never(&log.path, "server.log"),
    };

    let (non_blocking, _guard1) = tracing_appender::non_blocking(file_appender);
    let fmt_layer = fmt::layer()
        .event_format(format().compact())
        .with_writer(non_blocking);

    let level = filter::LevelFilter::from_str(&log.log_level)?;

    let log_file_level = match log.enable_log_file {
        true => level,
        false => filter::LevelFilter::OFF,
    };

    tracing_subscriber::registry()
        .with(EnvFilter::from_default_env())
        .with(stdout_log)
        .with(fmt_layer.with_filter(log_file_level))
        .with(telemetry)
        .init();

    let root = span!(tracing::Level::INFO, "app_start");
    let _enter = root.enter();

    start_server_with_config(&config).await?;

    Ok(())
}
