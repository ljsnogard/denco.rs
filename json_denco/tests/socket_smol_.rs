//! `json_denco` 在 smol / async-net TCP socket 上的集成测试。
//!
//! 使用 `buffex_smol_adapt` 的按需驱动适配器，`futures_lite::future::block_on`
//! 驱动整个场景；所有用例都带 5 秒超时。

#![feature(impl_trait_in_assoc_type)]

use core::future::Future;

use async_net::{TcpListener, TcpStream};
use buffex_smol_adapt::{BuffRead, BuffWrite, DefaultAllocConfig};
use futures_lite::future::{block_on, or, zip};
use json_denco::JsonError;
use smol::Timer;

mod support;
use support::{
    StrVal, TIMEOUT_, U64Val, Wire, decode_from_, decode_next_, encode_into_, sample_wire_,
    write_raw_,
};

/// 在 5 秒内执行 `future`；超时直接 panic。
fn with_timeout_<F>(future: F)
where
    F: Future<Output = ()>,
{
    block_on(or(future, async {
        Timer::after(TIMEOUT_).await;
        panic!("smol socket 用例未能在 {TIMEOUT_:?} 内结束");
    }));
}

/// 验证 smol 后端 TCP 上的复合结构体往返。
/// Verify a composite payload round-trips over smol-backed TCP.
///
/// - 测试目标 / Goal：`JsonEncoder` / `JsonDecoder` 只依赖 `TrBuffWrite` /
///   `TrBuffRead`，在 smol 适配器上同样可用。
/// - 手段 / Method：`async_net` 回环连接，服务端编码后 flush + close，客户端解码。
/// - 判断 / Assert：解码值与原值完全相等，整体 5 秒内结束。
#[test]
fn tcp_payload_roundtrip_() {
    with_timeout_(async {
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("监听回环地址应当成功");
        let addr = listener.local_addr().expect("应当能取到监听地址");
        let value = sample_wire_();
        let expected = value.clone();

        let (accepted, connected) = zip(listener.accept(), TcpStream::connect(addr)).await;
        let (stream, _) = accepted.expect("应当接受一个连接");
        let client_stream = connected.expect("连接服务端应当成功");

        let server = async {
            let mut tx =
                BuffWrite::<_, DefaultAllocConfig>::try_new(stream, 4096, DefaultAllocConfig)
                    .expect("写端容量合法");
            encode_into_(&value, &mut tx)
                .await
                .expect("smol TCP 编码应当成功");
            tx.flush_async().await.expect("flush 应当成功");
            tx.close_async().await.expect("close 应当成功");
        };
        let client = async {
            let mut rx =
                BuffRead::<_, DefaultAllocConfig>::try_new(client_stream, 4096, DefaultAllocConfig)
                    .expect("读端容量合法");
            let got: Wire = decode_from_(&mut rx).await.expect("smol TCP 解码应当成功");
            assert_eq!(got, expected, "smol TCP 往返内容应当一致");
        };

        let ((), ()) = zip(server, client).await;
    });
}

/// 验证 smol 后端 TCP 上每个消息新建解码器也不会丢数据。
/// Verify a fresh decoder per message loses no data over smol-backed TCP.
///
/// - 测试目标 / Goal：解码器不再持有预读缓冲，逐消息新建解码器不会吞掉后续
///   消息。
/// - 手段 / Method：服务端依次发送 3 个 `U64Val`，每个值后写空格；客户端每解
///   一个值都新建 `JsonDecoder`，再验证下一次解码为 EOF。
/// - 判断 / Assert：3 个值内容与顺序正确；第 4 次返回 `Err`。
#[test]
fn tcp_multi_message_stream_() {
    with_timeout_(async {
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("监听回环地址应当成功");
        let addr = listener.local_addr().expect("应当能取到监听地址");
        let values = [U64Val(2), U64Val(500), U64Val(u64::MAX)];
        let (accepted, connected) = zip(listener.accept(), TcpStream::connect(addr)).await;
        let (stream, _) = accepted.expect("应当接受一个连接");
        let client_stream = connected.expect("连接服务端应当成功");

        let server = async {
            let mut tx =
                BuffWrite::<_, DefaultAllocConfig>::try_new(stream, 4096, DefaultAllocConfig)
                    .expect("写端容量合法");
            for value in &values {
                encode_into_(value, &mut tx)
                    .await
                    .expect("多消息编码应当成功");
                write_raw_(&mut tx, b" ").await.expect("分隔符写入应当成功");
            }
            tx.close_async().await.expect("close 应当成功");
        };
        let client = async {
            let mut rx =
                BuffRead::<_, DefaultAllocConfig>::try_new(client_stream, 4096, DefaultAllocConfig)
                    .expect("读端容量合法");
            for expected in &values {
                let mut decoder = json_denco::JsonDecoder::new(&mut rx);
                let got: U64Val = decode_next_(&mut decoder)
                    .await
                    .expect("多消息流中的值应当解码成功");
                assert_eq!(&got, expected, "消息顺序应当保持不变");
            }
            let mut decoder = json_denco::JsonDecoder::new(&mut rx);
            let eof: Result<U64Val, JsonError> = decode_next_(&mut decoder).await;
            assert!(eof.is_err(), "连接关闭后继续解码应返回 Err");
        };

        let ((), ()) = zip(server, client).await;
    });
}

/// 验证 smol 后端上的大字符串跨多个环段传输。
/// Verify a large string crosses several ring segments on the smol backend.
///
/// - 测试目标 / Goal：解码器的内部缓冲与长字符串路径在 smol 适配器上不丢字节。
/// - 手段 / Method：环容量 4 KiB，发送 64 KiB 字符串。
/// - 判断 / Assert：解码内容与发送内容完全相等。
#[test]
fn tcp_large_payload_() {
    with_timeout_(async {
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("监听回环地址应当成功");
        let addr = listener.local_addr().expect("应当能取到监听地址");
        let value = StrVal("s".repeat(64 * 1024));
        let expected = value.clone();
        let (accepted, connected) = zip(listener.accept(), TcpStream::connect(addr)).await;
        let (stream, _) = accepted.expect("应当接受一个连接");
        let client_stream = connected.expect("连接服务端应当成功");

        let server = async {
            let mut tx =
                BuffWrite::<_, DefaultAllocConfig>::try_new(stream, 4096, DefaultAllocConfig)
                    .expect("写端容量合法");
            encode_into_(&value, &mut tx)
                .await
                .expect("大 payload 编码应当成功");
            tx.close_async().await.expect("close 应当成功");
        };
        let client = async {
            let mut rx =
                BuffRead::<_, DefaultAllocConfig>::try_new(client_stream, 4096, DefaultAllocConfig)
                    .expect("读端容量合法");
            let got: StrVal = decode_from_(&mut rx)
                .await
                .expect("大 payload 解码应当成功");
            assert_eq!(got, expected);
        };

        let ((), ()) = zip(server, client).await;
    });
}
