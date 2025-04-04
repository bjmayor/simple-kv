use anyhow::Result;
use criterion::{Criterion, criterion_group, criterion_main};
use futures::StreamExt;
use rand::prelude::*;
use simple_kv::{
    AppStream, ClientConfig, CommandRequest, ServerConfig, StorageConfig, YamuxCtrl,
    start_server_with_config, start_yamux_client_with_config,
};
use std::time::Duration;
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

    // 创建一个通道来通知服务器已启动
    let (tx, mut rx) = tokio::sync::mpsc::channel(1);

    // 启动服务器任务
    tokio::spawn(async move {
        // 启动服务器
        let result = start_server_with_config(&config).await;

        // 通知服务器已启动或失败
        let _ = tx.send(result).await;
    });

    // 等待服务器启动信号
    rx.recv()
        .await
        .ok_or_else(|| anyhow::anyhow!("Server failed to start"))??;

    // 给服务器一些时间完全启动
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    Ok(())
}

async fn connect() -> Result<YamuxCtrl<TlsStream<TcpStream>>> {
    let addr = "127.0.0.1:9999";
    let mut config: ClientConfig = toml::from_str(include_str!("../fixtures/client.conf"))?;
    config.general.addr = addr.into();

    Ok(start_yamux_client_with_config(&config).await?)
}

async fn start_subscribers(topic: &'static str) -> Result<()> {
    let mut ctrl = connect().await?;
    let stream = ctrl.open_stream().await?;
    info!("C(subscriber): stream opened");
    let cmd = CommandRequest::new_subscribe(topic.to_string());
    tokio::spawn(async move {
        let mut stream = stream.execute_streaming(&cmd).await.unwrap();
        while let Some(Ok(data)) = stream.next().await {
            drop(data);
        }
    });

    Ok(())
}

async fn start_publishers(topic: &'static str, values: &'static [&'static str]) -> Result<()> {
    let mut rng = rand::rng();
    let v = values.choose(&mut rng).unwrap();

    let mut ctrl = connect().await.unwrap();
    let mut stream = ctrl.open_stream().await.unwrap();
    info!("C(publisher): stream opened");

    let cmd = CommandRequest::new_publish(topic.to_string(), vec![(*v).into()]);
    stream.execute_unary(&cmd).await.unwrap();

    Ok(())
}

fn pubsub(c: &mut Criterion) {
    let tracer = opentelemetry_jaeger::new_pipeline()
        .with_service_name("kv-bench")
        .install_simple()
        .unwrap();
    let opentelemetry = tracing_opentelemetry::layer().with_tracer(tracer);

    tracing_subscriber::registry()
        .with(EnvFilter::from_default_env())
        .with(opentelemetry)
        .init();

    let root = span!(tracing::Level::INFO, "app_start", work_units = 2);
    let _enter = root.enter();
    // 创建 Tokio runtime
    let runtime = Builder::new_multi_thread()
        .worker_threads(4)
        .thread_name("pubsub")
        .enable_all()
        .build()
        .unwrap();

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

    // 运行服务器和 100 个 subscriber，为测试准备
    runtime.block_on(async {
        eprint!("preparing server and subscribers");
        start_server().await.unwrap();
        time::sleep(Duration::from_millis(50)).await;
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
