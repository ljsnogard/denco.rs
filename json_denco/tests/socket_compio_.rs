//! `json_denco` 在 compio TCP socket 上的集成测试。
//!
//! 使用 `buffex_compio_adapt` 的后台泵适配器；每个 `#[compio::test]` 自带
//! thread-local 运行时，所有用例都包在 5 秒超时里。

#![feature(impl_trait_in_assoc_type)]

use buffex_compio_adapt::{BuffRead, BuffWrite, DefaultAllocConfig};
use compio::{
    net::{TcpListener, TcpStream},
    runtime::{ResumeUnwind, spawn},
    time::timeout,
};
use json_denco::JsonError;

mod support;
use support::{
    StrVal, TIMEOUT_, U64Val, Wire, decode_from_, decode_next_, encode_into_, sample_wire_,
    write_raw_,
};

/// 验证 compio 后端 TCP 上的复合结构体往返。
/// Verify a composite payload round-trips over compio-backed TCP.
///
/// - 测试目标 / Goal：`JsonEncoder` / `JsonDecoder` 在 compio 后台泵适配器上
///   仍只依赖 `TrBuffWrite` / `TrBuffRead`。
/// - 手段 / Method：回环连接后把服务端编码任务 spawn 出去，客户端同时解码。
/// - 判断 / Assert：解码值与原值完全相等，服务端任务正常退出。
#[compio::test]
async fn tcp_payload_roundtrip_() {
    timeout(TIMEOUT_, async {
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("监听回环地址应当成功");
        let addr = listener.local_addr().expect("应当能取到监听地址");
        let client_stream = TcpStream::connect(addr).await.expect("连接服务端应当成功");
        let (server_stream, _) = listener.accept().await.expect("应当接受一个连接");

        let value = sample_wire_();
        let expected = value.clone();
        let server = spawn(async move {
            let mut tx = BuffWrite::<_, DefaultAllocConfig>::try_new(
                server_stream,
                4096,
                DefaultAllocConfig,
            )
            .expect("写端容量合法");
            encode_into_(&value, &mut tx)
                .await
                .expect("compio TCP 编码应当成功");
            tx.flush_async().await.expect("flush 应当成功");
            tx.close_async().await.expect("close 应当成功");
        });

        let mut rx =
            BuffRead::<_, DefaultAllocConfig>::try_new(client_stream, 4096, DefaultAllocConfig)
                .expect("读端容量合法");
        let got: Wire = decode_from_(&mut rx)
            .await
            .expect("compio TCP 解码应当成功");
        server.await.resume_unwind().expect("服务端任务不应失败");
        assert_eq!(got, expected, "compio TCP 往返内容应当一致");
    })
    .await
    .expect("tcp_payload_roundtrip_ 应当在 5 秒内完成");
}

/// 验证 compio 后端 TCP 上的多消息流。
/// Verify a multi-message stream over compio-backed TCP.
///
/// - 测试目标 / Goal：复用同一个解码器连续解码多个数值消息。
/// - 手段 / Method：服务端任务发送 3 个 `U64Val`（每个值后写空格），客户端
///   复用同一个 `JsonDecoder` 解码，再验证第 4 次为 EOF。
/// - 判断 / Assert：3 个值内容与顺序正确；第 4 次返回 `Err`。
#[compio::test]
async fn tcp_multi_message_stream_() {
    timeout(TIMEOUT_, async {
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("监听回环地址应当成功");
        let addr = listener.local_addr().expect("应当能取到监听地址");
        let client_stream = TcpStream::connect(addr).await.expect("连接服务端应当成功");
        let (server_stream, _) = listener.accept().await.expect("应当接受一个连接");

        let values = [U64Val(3), U64Val(700), U64Val(u64::MAX)];
        let server_values = values.clone();
        let server = spawn(async move {
            let mut tx = BuffWrite::<_, DefaultAllocConfig>::try_new(
                server_stream,
                4096,
                DefaultAllocConfig,
            )
            .expect("写端容量合法");
            for value in &server_values {
                encode_into_(value, &mut tx)
                    .await
                    .expect("多消息编码应当成功");
                write_raw_(&mut tx, b" ").await.expect("分隔符写入应当成功");
            }
            tx.close_async().await.expect("close 应当成功");
        });

        let mut rx =
            BuffRead::<_, DefaultAllocConfig>::try_new(client_stream, 4096, DefaultAllocConfig)
                .expect("读端容量合法");
        let mut decoder = json_denco::JsonDecoder::new(&mut rx);
        for expected in &values {
            let got: U64Val = decode_next_(&mut decoder)
                .await
                .expect("多消息流中的值应当解码成功");
            assert_eq!(&got, expected, "消息顺序应当保持不变");
        }
        let eof: Result<U64Val, JsonError> = decode_next_(&mut decoder).await;
        assert!(eof.is_err(), "连接关闭后继续解码应返回 Err");
        server.await.resume_unwind().expect("服务端任务不应失败");
    })
    .await
    .expect("tcp_multi_message_stream_ 应当在 5 秒内完成");
}

/// 验证 compio 后端上的大字符串跨多个环段传输。
/// Verify a large string crosses several ring segments on the compio backend.
///
/// - 测试目标 / Goal：解码器的内部缓冲与长字符串路径在 compio 适配器上不丢字节。
/// - 手段 / Method：环容量 4 KiB，服务端任务发送 64 KiB 字符串，客户端同时解码。
/// - 判断 / Assert：解码内容与发送内容完全相等。
#[compio::test]
async fn tcp_large_payload_() {
    timeout(TIMEOUT_, async {
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("监听回环地址应当成功");
        let addr = listener.local_addr().expect("应当能取到监听地址");
        let client_stream = TcpStream::connect(addr).await.expect("连接服务端应当成功");
        let (server_stream, _) = listener.accept().await.expect("应当接受一个连接");

        let value = StrVal("c".repeat(64 * 1024));
        let expected = value.clone();
        let server = spawn(async move {
            let mut tx = BuffWrite::<_, DefaultAllocConfig>::try_new(
                server_stream,
                4096,
                DefaultAllocConfig,
            )
            .expect("写端容量合法");
            encode_into_(&value, &mut tx)
                .await
                .expect("大 payload 编码应当成功");
            tx.close_async().await.expect("close 应当成功");
        });

        let mut rx =
            BuffRead::<_, DefaultAllocConfig>::try_new(client_stream, 4096, DefaultAllocConfig)
                .expect("读端容量合法");
        let got: StrVal = decode_from_(&mut rx)
            .await
            .expect("大 payload 解码应当成功");
        server.await.resume_unwind().expect("服务端任务不应失败");
        assert_eq!(got, expected);
    })
    .await
    .expect("tcp_large_payload_ 应当在 5 秒内完成");
}
