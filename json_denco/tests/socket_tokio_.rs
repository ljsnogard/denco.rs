//! `json_denco` 在 tokio TCP socket 上的集成测试。
//!
//! 服务端用 `BuffWrite` 编码发送，客户端用 `BuffRead` 边接收边解码；所有用例
//! 都包在 5 秒超时里，避免适配器 park 导致测试挂死。

#![feature(impl_trait_in_assoc_type)]

use std::time::Duration;

use buffex_tokio_adapt::{BuffRead, BuffWrite, DefaultAllocConfig};
use cantok_rs::CancellationSource;
use json_denco::JsonError;
use tokio::{
    net::{TcpListener, TcpStream},
    time::timeout,
};

mod support;
use support::{
    StrVal, TIMEOUT_, U64Val, Wire, decode_from_, decode_from_cancellable_, decode_next_,
    encode_into_, sample_wire_, write_raw_,
};

/// 验证 TCP 上的复合结构体往返。
/// Verify a composite payload round-trips over TCP.
///
/// - 测试目标 / Goal：`JsonEncoder` / `JsonDecoder` 能在真实 tokio TCP 连接上
///   处理包含字符串、数组、Option、char、浮点与枚举的复合值。
/// - 手段 / Method：回环 `TcpListener` / `TcpStream`；服务端编码后 flush + close，
///   客户端解码；整体 5 秒超时。
/// - 判断 / Assert：解码值与原值完全相等，两个方向都没有超时或错误。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tcp_payload_roundtrip_() {
    timeout(TIMEOUT_, async {
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("监听回环地址应当成功");
        let addr = listener.local_addr().expect("应当能取到监听地址");
        let value = sample_wire_();
        let expected = value.clone();

        let server = async {
            let (stream, _) = listener.accept().await.expect("应当接受一个连接");
            let mut tx =
                BuffWrite::<_, DefaultAllocConfig>::try_new(stream, 4096, DefaultAllocConfig)
                    .expect("写端容量合法");
            encode_into_(&value, &mut tx)
                .await
                .expect("TCP 编码应当成功");
            tx.flush_async().await.expect("flush 应当成功");
            tx.close_async().await.expect("close 应当成功");
        };
        let client = async {
            let stream = TcpStream::connect(addr).await.expect("连接服务端应当成功");
            let mut rx =
                BuffRead::<_, DefaultAllocConfig>::try_new(stream, 4096, DefaultAllocConfig)
                    .expect("读端容量合法");
            let got: Wire = decode_from_(&mut rx).await.expect("TCP 解码应当成功");
            assert_eq!(got, expected, "TCP 往返内容应当一致");
        };

        tokio::join!(server, client);
    })
    .await
    .expect("tcp_payload_roundtrip_ 应当在 5 秒内完成");
}

/// 验证 TCP 双向并发编码/解码不会死锁。
/// Verify concurrent bidirectional TCP encode/decode cannot deadlock.
///
/// - 测试目标 / Goal：同一条连接的两个方向可以同时编码与解码。
/// - 手段 / Method：两端各 split 出读写半部，分别包成 `BuffRead` / `BuffWrite`；
///   服务端发 `Wire`、客户端发 `U64Val`，用 `tokio::join!` 同时进行。
/// - 判断 / Assert：两个方向都解出正确值，整体在 5 秒内结束。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tcp_bidirectional_concurrent_() {
    timeout(TIMEOUT_, async {
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("监听回环地址应当成功");
        let addr = listener.local_addr().expect("应当能取到监听地址");
        let (accepted, connected) = tokio::join!(listener.accept(), TcpStream::connect(addr));
        let (mut server_stream, _) = accepted.expect("应当接受一个连接");
        let mut client_stream = connected.expect("连接服务端应当成功");

        let (server_read, server_write) = server_stream.split();
        let (client_read, client_write) = client_stream.split();
        let mut server_tx =
            BuffWrite::<_, DefaultAllocConfig>::try_new(server_write, 1024, DefaultAllocConfig)
                .expect("写端容量合法");
        let mut server_rx =
            BuffRead::<_, DefaultAllocConfig>::try_new(server_read, 1024, DefaultAllocConfig)
                .expect("读端容量合法");
        let mut client_tx =
            BuffWrite::<_, DefaultAllocConfig>::try_new(client_write, 1024, DefaultAllocConfig)
                .expect("写端容量合法");
        let mut client_rx =
            BuffRead::<_, DefaultAllocConfig>::try_new(client_read, 1024, DefaultAllocConfig)
                .expect("读端容量合法");

        let server_wire = sample_wire_();
        let client_wire = server_wire.clone();
        let server_send = async {
            encode_into_(&server_wire, &mut server_tx)
                .await
                .expect("服务端发送应当成功");
            server_tx
                .close_async()
                .await
                .expect("服务端写方向关闭应当成功");
        };
        let server_recv = async {
            let got: U64Val = decode_from_(&mut server_rx)
                .await
                .expect("服务端接收应当成功");
            assert_eq!(got, U64Val(11));
        };
        let client_send = async {
            encode_into_(&U64Val(11), &mut client_tx)
                .await
                .expect("客户端发送应当成功");
            client_tx
                .close_async()
                .await
                .expect("客户端写方向关闭应当成功");
        };
        let client_recv = async {
            let got: Wire = decode_from_(&mut client_rx)
                .await
                .expect("客户端接收应当成功");
            assert_eq!(got, client_wire);
        };

        tokio::join!(server_send, server_recv, client_send, client_recv);
    })
    .await
    .expect("tcp_bidirectional_concurrent_ 应当在 5 秒内完成");
}

/// 验证同一 TCP 连接上的多消息流与关闭后的 EOF。
/// Verify a multi-message stream on one TCP connection and EOF after close.
///
/// - 测试目标 / Goal：同一个 `JsonDecoder` 可以连续解码多个值；数字之间由空白
///   分隔，解码器预读的字节不会丢失。
/// - 手段 / Method：服务端依次编码 3 个 `U64Val`，每个值后写一个空格再 close；
///   客户端复用同一个解码器逐个解码，第 4 次应看到 EOF。
/// - 判断 / Assert：3 个值顺序与内容正确；第 4 次返回 `Err`。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tcp_multi_message_stream_() {
    timeout(TIMEOUT_, async {
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("监听回环地址应当成功");
        let addr = listener.local_addr().expect("应当能取到监听地址");
        let values = [U64Val(1), U64Val(300), U64Val(u64::MAX)];

        let server = async {
            let (stream, _) = listener.accept().await.expect("应当接受一个连接");
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
            let stream = TcpStream::connect(addr).await.expect("连接服务端应当成功");
            let mut rx =
                BuffRead::<_, DefaultAllocConfig>::try_new(stream, 4096, DefaultAllocConfig)
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
        };

        tokio::join!(server, client);
    })
    .await
    .expect("tcp_multi_message_stream_ 应当在 5 秒内完成");
}

/// 验证超过适配器环容量的大字符串不被截断。
/// Verify a string larger than the adapter ring is not truncated.
///
/// - 测试目标 / Goal：设备侧分多次搬运时，解码器无预读缓冲的逐字节读取与
///   长字符串 owned 路径都不丢字节。
/// - 手段 / Method：适配器环 4 KiB，发送 64 KiB 字符串，服务端 close 收尾。
/// - 判断 / Assert：解码长度与内容完全相等，整体在 5 秒内结束。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tcp_large_payload_beyond_ring_capacity_() {
    timeout(TIMEOUT_, async {
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("监听回环地址应当成功");
        let addr = listener.local_addr().expect("应当能取到监听地址");
        let value = StrVal("z".repeat(64 * 1024));
        let expected = value.clone();

        let server = async {
            let (stream, _) = listener.accept().await.expect("应当接受一个连接");
            let mut tx =
                BuffWrite::<_, DefaultAllocConfig>::try_new(stream, 4096, DefaultAllocConfig)
                    .expect("写端容量合法");
            encode_into_(&value, &mut tx)
                .await
                .expect("大 payload 编码应当成功");
            tx.close_async().await.expect("close 应当成功");
        };
        let client = async {
            let stream = TcpStream::connect(addr).await.expect("连接服务端应当成功");
            let mut rx =
                BuffRead::<_, DefaultAllocConfig>::try_new(stream, 4096, DefaultAllocConfig)
                    .expect("读端容量合法");
            let got: StrVal = decode_from_(&mut rx)
                .await
                .expect("大 payload 解码应当成功");
            assert_eq!(got, expected);
        };

        tokio::join!(server, client);
    })
    .await
    .expect("tcp_large_payload_beyond_ring_capacity_ 应当在 5 秒内完成");
}

/// 验证只收到部分 JSON 时取消解码不会挂起。
/// Verify cancelling a decoder that saw only a partial JSON value does not hang.
///
/// - 测试目标 / Goal：真实 socket 上数据不足导致的 park 能被取消令牌终止。
/// - 手段 / Method：服务端只写 `{"name":"` 后保持连接，客户端解码 `Wire` 并在
///   20ms 后取消。
/// - 判断 / Assert：解码 future 以 `Err` 结束且错误来自取消，整体 5 秒内完成。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tcp_cancel_partial_json_() {
    timeout(TIMEOUT_, async {
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("监听回环地址应当成功");
        let addr = listener.local_addr().expect("应当能取到监听地址");
        let (accepted, connected) = tokio::join!(listener.accept(), TcpStream::connect(addr));
        let (stream, _) = accepted.expect("应当接受一个连接");
        let client_stream = connected.expect("连接服务端应当成功");

        let server = async {
            let mut tx =
                BuffWrite::<_, DefaultAllocConfig>::try_new(stream, 4096, DefaultAllocConfig)
                    .expect("写端容量合法");
            write_raw_(&mut tx, br#"{"name":"#)
                .await
                .expect("写部分 JSON 应当成功");
            tx.flush_async().await.expect("flush 应当成功");
            // 保持连接打开，让客户端确实停在「等设备可读」的 park 上。
            tokio::time::sleep(Duration::from_millis(300)).await;
        };
        let client = async {
            let mut rx =
                BuffRead::<_, DefaultAllocConfig>::try_new(client_stream, 4096, DefaultAllocConfig)
                    .expect("读端容量合法");
            let source = CancellationSource::new();
            let token = source.child_token();
            let decode = async {
                let out: Result<Wire, JsonError> = decode_from_cancellable_(&mut rx, token).await;
                out
            };
            let cancel = async {
                tokio::time::sleep(Duration::from_millis(20)).await;
                eprintln!("TEST: cancelling");
                source.cancel();
                eprintln!("TEST: cancelled");
            };
            let (out, ()) = tokio::join!(decode, cancel);
            eprintln!("TEST: decode returned {:?}", out.as_ref().err().map(|e| e.to_string()));
            let err = out.expect_err("部分 JSON park 被取消后应返回 Err");
            let message = err.to_string();
            assert!(
                message.to_ascii_lowercase().contains("cancel"),
                "错误应来自取消而不是 EOF：{message}"
            );
        };

        tokio::join!(server, client);
    })
    .await
    .expect("tcp_cancel_partial_json_ 应当在 5 秒内完成");
}
