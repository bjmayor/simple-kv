use anyhow::Result;
use criterion::{Criterion, criterion_group, criterion_main};
use futures::StreamExt;
use opentelemetry::trace::{Tracer, TracerProvider as _};
use opentelemetry_otlp::{SpanExporter, WithExportConfig};
use opentelemetry_sdk::trace::{Sampler, SdkTracerProvider};
use rand::prelude::*;
use simple_kv::{
    AppStream, ClientConfig, CommandRequest, ServerConfig, StorageConfig, YamuxCtrl,
    start_server_with_config, start_yamux_client_with_config,
};
use std::{env, time::Duration};
use tokio::net::TcpStream;
use tokio::runtime::Builder;
use tokio::time;
use tokio_rustls::client::TlsStream;
use tracing::{info, span};
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, prelude::*};

async fn start_server() -> Result<()> {
    let addr = "127.0.0.1:9999";
    let mut config: ServerConfig = toml::from_str(include_str!("../fixtures/server.conf"))?;
    config.general.addr = addr.into();
    config.storage = StorageConfig::MemTable;

    info!(addr, "starting server");
    // 启动服务器任务
    tokio::spawn(async move {
        start_server_with_config(&config).await.unwrap();
    });

    Ok(())
}

async fn connect() -> Result<YamuxCtrl<TlsStream<TcpStream>>> {
    let addr = "127.0.0.1:9999";
    let mut config: ClientConfig = toml::from_str(include_str!("../fixtures/client.conf"))?;
    config.general.addr = addr.into();

    info!(addr, "connecting to server");
    Ok(start_yamux_client_with_config(&config).await?)
}

async fn start_subscribers(topic: &'static str) -> Result<()> {
    let mut ctrl = connect().await?;
    let stream = ctrl.open_stream().await?;
    info!(%topic, "subscriber stream opened");
    let cmd = CommandRequest::new_subscribe(topic.to_string());
    tokio::spawn(async move {
        let mut stream = stream.execute_streaming(&cmd).await.unwrap();
        while let Some(Ok(data)) = stream.next().await {
            info!(%topic, "received data");
            drop(data);
        }
    });

    Ok(())
}

async fn start_publishers(topic: &'static str, values: &'static [&'static str]) -> Result<()> {
    let mut rng = rand::rng();
    let v = values.choose(&mut rng).unwrap();

    let mut ctrl: YamuxCtrl<TlsStream<TcpStream>> = connect().await.unwrap();
    let mut stream = ctrl.open_stream().await.unwrap();
    info!(%topic, len = v.len(), "publisher stream opened");

    let cmd = CommandRequest::new_publish(topic.to_string(), vec![(*v).into()]);
    stream.execute_unary(&cmd).await.unwrap();
    info!(%topic, "published data");

    Ok(())
}

fn pubsub(c: &mut Criterion) {
    // 创建 Tokio runtime
    let runtime = Builder::new_multi_thread()
        .worker_threads(4)
        .thread_name("pubsub")
        .enable_all()
        .build()
        .unwrap();

    // 在 runtime 上下文中初始化 OTLP
    let _guard = runtime.block_on(async {
        unsafe {
            // 设置 OTLP 端点
            env::set_var("OTEL_EXPORTER_OTLP_ENDPOINT", "http://localhost:4317");
            // 设置服务名称
            env::set_var("OTEL_SERVICE_NAME", "kv-bench");
        }

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
        let tracer = provider.tracer("kv-bench");
        let telemetry = tracing_opentelemetry::layer().with_tracer(tracer);

        tracing_subscriber::registry()
            .with(EnvFilter::from_default_env())
            .with(telemetry)
            .init();

        provider
    });

    let root = span!(tracing::Level::INFO, "benchmark_start", work_units = 2);
    let _enter = root.enter();

    let base_str = include_str!("../fixtures/server.conf"); // 891 bytes

    let values: &'static [&'static str] = Box::leak(
        vec![
            &base_str[..64],
            &base_str[..128],
            &base_str[..256],
            &base_str[..512],
        ]
        .into_boxed_slice(),
    );
    let topic = "lobby";

    // 运行服务器和 10 个 subscriber，为测试准备
    runtime.block_on(async {
        eprintln!("preparing server and subscribers");
        start_server().await.unwrap();
        time::sleep(Duration::from_millis(100)).await;
        for _ in 0..1000 {
            start_subscribers(topic).await.unwrap();
            eprint!(".");
        }
        eprintln!("Done!");
    });

    // 进行 benchmark
    c.bench_function("publishing", move |b| {
        b.to_async(&runtime)
            .iter(|| async { start_publishers(topic, values).await })
    });
}

criterion_group! {
    name = benches;
    config = Criterion::default().sample_size(10);
    targets = pubsub
}
criterion_main!(benches);
