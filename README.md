[![release](https://github.com/tyrchen/simple-kv/actions/workflows/release.yml/badge.svg)](https://github.com/tyrchen/simple-kv/actions/workflows/release.yml)

[![build](https://github.com/tyrchen/simple-kv/actions/workflows/build.yml/badge.svg?branch=master)](https://github.com/tyrchen/simple-kv/actions/workflows/build.yml)

# Simple KV

一个简单的 KV server 实现，配合 geektime 上我的 Rust 第一课。
非常值得学习。看了 2 遍了。
老师说如果你可以自己写出这个simple_kv 的代码，那么你可以在北美找到年包 300k $ 的工作。
## protobuf
命令定义都在`ab.proto`文件中。
通过`build.rs` 文件会自动编译成对应的结构体。 试用的`prost-build`, 因为是编译阶段 用的。所以需要放在`build-dependencies`中。

`ab.proto`文件中定义了`CommandRequest`和`CommandResponse`两个消息。没啥可说的。

`pb/mod.rs` 定义了一些便捷的函数。和一些类型转换。
这课的代码主要是分层设计。
## 错误设计
库的错误一般都是用`thiserror` 来定义的。参考`src/error.rs`文件。

## 存储层
通过`trait` 来定义存储层的接口。
定义在`src/storage/mod.rs`文件中。
有 2 个实现。
- `MemTable` 内存实现。
- `SledDb` sled 实现。

`MemTable` 是`DashMap` 的封装。
`SledDb` 是`sled` 的封装。

`SledDb` 的`get_full_key` 函数，是把`table` 和`key` 拼接成一个字符串。


## 网络层

看到有tcp和quic 2 个实现。
quic 的实现是基于`s2n-quic` 库的。
quic 其实是基于udp的。

选择 QUIC 当：
需要低延迟、抗网络抖动、移动端友好或强制加密（如 HTTP/3）。
​选择 TCP 当：
追求稳定性、高吞吐量或兼容旧设备。


## 服务层
这里也是业务代码，应用层。

## 使用
生成证书。
```bash
cargo run --bin gen_cert
```

`ps`: 精髓都在老师的第一课里。

## 测试

```bash
cargo test
```

## 基准测试

```bash
cargo bench
```

我的结果
publishing              time:   [4.3689 ms 4.4354 ms 4.5273 ms]

## 性能监控
跑个 all in one 的 jager.

```sh
docker run -d --name jaeger \
  -p 16686:16686 \
  -p 4317:4317 \
  -p 4318:4318 \
  jaegertracing/all-in-one:latest
	```

## ci/cd

# 参考

[X.509](https://zh.wikipedia.org/wiki/X.509)