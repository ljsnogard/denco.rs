//! 真实 TCP / Unix domain socket 上的 `mp_denco` 集成测试。
//!
//! 写端使用 [`BuffWrite`]，读端使用 [`BuffRead`]；所有用例都包在 5 秒超时里，
//! 避免适配器 park 导致测试挂死。
//!
//! Integration tests on real TCP / Unix domain sockets. Reusable socket rings
//! are consumed through `read_async`, and every case is wrapped in a 5-second
//! timeout.

#![feature(impl_trait_in_assoc_type)]

#[cfg(unix)]
use std::path::{Path, PathBuf};
use std::time::Duration;
#[cfg(unix)]
use std::time::{SystemTime, UNIX_EPOCH};

use abs_buff::{TrBuffRead, TrBuffWrite};
use abs_cancel::{NonCancellableToken, TrCancellationToken, TrMayCancel};
use abs_denco::{de::TrDecodable, ser::TrEncodable};
use buffex_tokio_adapt::{BuffRead, BuffWrite, DefaultAllocConfig};
use cantok_rs::CancellationSource;
use mp_denco::{MpDecoder, MpEncoder, MpError};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    time::timeout,
};

mod support;
use support::{StrVal, TIMEOUT_, U64Val};

/// 编码一个值并写入适配器（不负责收尾）。
///
/// 调用方仍需 `flush_async` / `close_async` 才能把环里的尾部真正送到设备。
///
/// Encode one value into an adapter; the caller is still responsible for the
/// final `flush_async` / `close_async`.
async fn encode_into_<T, W>(value: &T, writer: &mut W) -> Result<(), MpError>
where
    T: TrEncodable,
    W: TrBuffWrite<u8>,
{
    value
        .encode_async(MpEncoder::new(writer))
        .may_cancel_with(NonCancellableToken::new())
        .await
}

/// 从适配器解码一个 owned 值。
///
/// Decode one owned value from the adapter.
async fn decode_from_<'de, T, R>(reader: &mut R) -> Result<T, MpError>
where
    T: TrDecodable<'de>,
    R: TrBuffRead<u8>,
{
    T::decode_async(MpDecoder::new(reader))
        .may_cancel_with(NonCancellableToken::new())
        .await
}

/// 从适配器解码一个 owned 值，并允许外部取消。
///
/// Decode one owned value from the adapter with an external cancellation token.
async fn decode_from_cancellable_<'de, T, R, C>(reader: &mut R, cancel: C) -> Result<T, MpError>
where
    T: TrDecodable<'de>,
    R: TrBuffRead<u8>,
    C: TrCancellationToken,
{
    T::decode_async(MpDecoder::new(reader))
        .may_cancel_with(cancel)
        .await
}

/// 验证 TCP 上的基础往返与收尾语义。
/// Verify the basic TCP roundtrip and flush/close tail delivery.
///
/// - 测试目标 / Goal：`MpEncoder` / `MpDecoder` 能在真实 TCP 连接上工作，
///   且 `flush_async` + `close_async` 会把环内尾部送达对端。
/// - 手段 / Method：回环 `TcpListener`/`TcpStream`；服务端编码一个 `U64Val`
///   后先 flush 再 close，客户端直接解码；整体 5 秒超时。
/// - 判断 / Assert：解码值等于原值，两个方向都没有超时或错误。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tcp_roundtrip_() {
    timeout(TIMEOUT_, async {
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("监听回环地址应当成功");
        let addr = listener.local_addr().expect("应当能取到监听地址");
        let value = U64Val(0x0102_0304_0506_0708);
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
            let got: U64Val = decode_from_(&mut rx).await.expect("TCP 解码应当成功");
            assert_eq!(got, expected, "TCP 往返内容应当一致");
        };

        tokio::join!(server, client);
    })
    .await
    .expect("tcp_roundtrip_ 应当在 5 秒内完成");
}

/// 验证 TCP 双向并发收发不会死锁。
/// Verify concurrent bidirectional TCP traffic cannot deadlock.
///
/// - 测试目标 / Goal：一条连接两端的读写适配器可以同时工作。
/// - 手段 / Method：两端各 `split` 出读写半部，分别包成 `BuffRead` /
///   `BuffWrite`；两端用 `tokio::join!` 同时发送 `U64Val` 与 `StrVal`。
/// - 判断 / Assert：两个方向都解出正确值，整体在 5 秒内结束。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tcp_bidirectional_concurrent_() {
    timeout(TIMEOUT_, async {
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("监听回环地址应当成功");
        let addr = listener.local_addr().expect("应当能取到监听地址");
        let (accepted, connected) = tokio::join!(listener.accept(), TcpStream::connect(addr),);
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

        let server_send = async {
            encode_into_(&U64Val(11), &mut server_tx)
                .await
                .expect("服务端发送应当成功");
            server_tx
                .close_async()
                .await
                .expect("服务端写方向关闭应当成功");
        };
        let server_recv = async {
            let got: StrVal = decode_from_(&mut server_rx)
                .await
                .expect("服务端接收应当成功");
            assert_eq!(got, StrVal(String::from("from-client")));
        };
        let client_send = async {
            encode_into_(&StrVal(String::from("from-client")), &mut client_tx)
                .await
                .expect("客户端发送应当成功");
            client_tx
                .close_async()
                .await
                .expect("客户端写方向关闭应当成功");
        };
        let client_recv = async {
            let got: U64Val = decode_from_(&mut client_rx)
                .await
                .expect("客户端接收应当成功");
            assert_eq!(got, U64Val(11));
        };

        tokio::join!(server_send, server_recv, client_send, client_recv);
    })
    .await
    .expect("tcp_bidirectional_concurrent_ 应当在 5 秒内完成");
}

/// 验证小容量适配器环导致多次 park 时仍能正确解码。
/// Verify decoding still succeeds when a small adapter ring parks repeatedly.
///
/// - 测试目标 / Goal：写端环满、读端环空时的 park/wake 在真实 socket 上可用。
/// - 手段 / Method：两端适配器环都只有 64 字节，发送 2 KiB 字符串。
/// - 判断 / Assert：解码内容完全相等，整体在 5 秒内结束。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tcp_small_ring_multi_park_() {
    timeout(TIMEOUT_, async {
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("监听回环地址应当成功");
        let addr = listener.local_addr().expect("应当能取到监听地址");
        let value = StrVal("f".repeat(2048));
        let expected = value.clone();

        let server = async {
            let (stream, _) = listener.accept().await.expect("应当接受一个连接");
            let mut tx =
                BuffWrite::<_, DefaultAllocConfig>::try_new(stream, 64, DefaultAllocConfig)
                    .expect("写端容量合法");
            encode_into_(&value, &mut tx)
                .await
                .expect("小环编码应当成功");
            tx.close_async().await.expect("close 应当成功");
        };
        let client = async {
            let stream = TcpStream::connect(addr).await.expect("连接服务端应当成功");
            let mut rx =
                BuffRead::<_, DefaultAllocConfig>::try_new(stream, 64, DefaultAllocConfig)
                    .expect("读端容量合法");
            let got: StrVal = decode_from_(&mut rx).await.expect("小环解码应当成功");
            assert_eq!(got, expected);
        };

        tokio::join!(server, client);
    })
    .await
    .expect("tcp_small_ring_multi_park_ 应当在 5 秒内完成");
}

/// 验证超过适配器环容量的大 payload 不被截断。
/// Verify a payload larger than the adapter ring is delivered intact.
///
/// - 测试目标 / Goal：设备侧分多次搬运时，长值 owned 路径不丢字节。
/// - 手段 / Method：适配器环 4 KiB，发送 64 KiB 字符串。
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

/// 验证同一 TCP 连接上的多消息流与关闭后的 EOF 错误。
/// Verify a multi-message stream on one TCP connection and the EOF error after close.
///
/// - 测试目标 / Goal：同一个 `BuffRead` 可以逐值解码，连接关闭后解码返回 `Err`。
/// - 手段 / Method：服务端连续编码 3 个 `U64Val` 后 close；客户端依次解码，
///   第 4 次解码应看到 EOF。
/// - 判断 / Assert：3 个值顺序与内容正确；第 4 次返回 `MpError`，整体 5 秒内结束。
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
            }
            tx.close_async().await.expect("close 应当成功");
        };
        let client = async {
            let stream = TcpStream::connect(addr).await.expect("连接服务端应当成功");
            let mut rx =
                BuffRead::<_, DefaultAllocConfig>::try_new(stream, 4096, DefaultAllocConfig)
                    .expect("读端容量合法");
            for expected in &values {
                let got: U64Val = decode_from_(&mut rx)
                    .await
                    .expect("多消息流中的值应当解码成功");
                assert_eq!(&got, expected, "消息顺序应当保持不变");
            }
            let eof: Result<U64Val, MpError> = decode_from_(&mut rx).await;
            assert!(eof.is_err(), "连接关闭后继续解码应返回 Err");
        };

        tokio::join!(server, client);
    })
    .await
    .expect("tcp_multi_message_stream_ 应当在 5 秒内完成");
}

/// 验证底层 socket 写错误会映射成 `MpError`。
/// Verify a socket write error is mapped to `MpError`.
///
/// - 测试目标 / Goal：对端被 RST 后编码/收尾不会 panic，而是给出可读错误。
/// - 手段 / Method：客户端设置 `SO_LINGER=0` 后立即关闭，服务端确认重置后
///   再编码 64 KiB 字符串；错误可能出现在编码或 close 阶段。
/// - 判断 / Assert：返回 `MpError`，`Display` 包含 “buff write failed”。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tcp_write_error_maps_to_mp_error_() {
    timeout(TIMEOUT_, async {
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("监听回环地址应当成功");
        let addr = listener.local_addr().expect("应当能取到监听地址");
        let (accepted, connected) = tokio::join!(listener.accept(), TcpStream::connect(addr),);
        let (mut server_stream, _) = accepted.expect("应当接受一个连接");
        let client_stream = connected.expect("连接服务端应当成功");

        // `SO_LINGER=0`：关闭时内核发送 RST，而不是普通 FIN。
        {
            let sock = socket2::SockRef::from(&client_stream);
            sock.set_linger(Some(Duration::ZERO))
                .expect("设置 SO_LINGER 应当成功");
        }
        drop(client_stream);

        // 先读一次确认对端已重置，再开始编码。
        let mut probe = [0u8; 1];
        let _ = server_stream.read(&mut probe).await;

        let mut tx =
            BuffWrite::<_, DefaultAllocConfig>::try_new(server_stream, 4096, DefaultAllocConfig)
                .expect("写端容量合法");
        let value = StrVal("e".repeat(64 * 1024));
        let err = match encode_into_(&value, &mut tx).await {
            Err(err) => err,
            Ok(()) => {
                let close_err = tx
                    .close_async()
                    .await
                    .expect_err("向已重置的对端收尾应当失败");
                MpError::buff_write(close_err)
            }
        };
        assert!(
            err.to_string().contains("buff write failed"),
            "错误信息应当包含底层写错误：{err}"
        );
        assert!(tx.take_error().is_some(), "适配器应当保留底层设备错误");
    })
    .await
    .expect("tcp_write_error_maps_to_mp_error_ 应当在 5 秒内完成");
}

#[cfg(unix)]
use tokio::net::{UnixListener, UnixStream};

/// 临时 Unix domain socket 路径；drop 时自动清理文件。
///
/// A temporary Unix socket path that removes its file on drop.
#[cfg(unix)]
struct TempSockPath_ {
    path_: PathBuf,
}

#[cfg(unix)]
impl TempSockPath_ {
    /// 在系统临时目录下生成一个进程内唯一的 socket 路径。
    ///
    /// Build a process-unique socket path under the system temp directory.
    fn new_(tag: &str) -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("系统时间应当晚于 UNIX_EPOCH")
            .as_nanos();
        let mut path_ = std::env::temp_dir();
        path_.push(format!(
            "mp_denco_{tag}_{}_{nanos}.sock",
            std::process::id()
        ));
        Self { path_ }
    }

    /// 返回 socket 路径。
    ///
    /// Return the socket path.
    fn as_path_(&self) -> &Path {
        self.path_.as_path()
    }
}

#[cfg(unix)]
impl Drop for TempSockPath_ {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path_);
    }
}

/// 验证 Unix domain socket 上的基础往返与 socket 文件清理。
/// Verify the basic Unix domain socket roundtrip and socket-file cleanup.
///
/// - 测试目标 / Goal：`BuffRead` / `BuffWrite` 不绑定 TCP，UDS 上同样可用。
/// - 手段 / Method：在临时路径上监听并连接，服务端编码 `StrVal` 后 close，
///   客户端直接解码；路径由 `TempSockPath_` 在 drop 时清理。
/// - 判断 / Assert：解码值等于原值；drop 后 socket 文件不存在。
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn uds_roundtrip_() {
    timeout(TIMEOUT_, async {
        let sock_path = TempSockPath_::new_("roundtrip");
        let keep_path = sock_path.as_path_().to_path_buf();
        let listener = UnixListener::bind(sock_path.as_path_()).expect("绑定 UDS 应当成功");
        let value = StrVal(String::from("unix-domain"));
        let expected = value.clone();

        let server = async {
            let (stream, _) = listener.accept().await.expect("应当接受一个连接");
            let mut tx =
                BuffWrite::<_, DefaultAllocConfig>::try_new(stream, 4096, DefaultAllocConfig)
                    .expect("写端容量合法");
            encode_into_(&value, &mut tx)
                .await
                .expect("UDS 编码应当成功");
            tx.close_async().await.expect("close 应当成功");
        };
        let client = async {
            let stream = UnixStream::connect(sock_path.as_path_())
                .await
                .expect("连接 UDS 应当成功");
            let mut rx =
                BuffRead::<_, DefaultAllocConfig>::try_new(stream, 4096, DefaultAllocConfig)
                    .expect("读端容量合法");
            let got: StrVal = decode_from_(&mut rx).await.expect("UDS 解码应当成功");
            assert_eq!(got, expected);
        };

        tokio::join!(server, client);
        drop(listener);
        drop(sock_path);
        assert!(!keep_path.exists(), "UDS 文件不应残留");
    })
    .await
    .expect("uds_roundtrip_ 应当在 5 秒内完成");
}

/// 验证 UDS 上只收到部分消息头时取消解码不会挂起。
/// Verify cancelling a decoder that saw only a partial UDS header does not hang.
///
/// - 测试目标 / Goal：真实设备上数据不足导致的 park 能被取消令牌终止。
/// - 手段 / Method：服务端只写 `str16` 的头 3 字节并保持连接，客户端解码
///   `StrVal` 并在 20ms 后取消；tokio 设备适配器把设备读与取消信号竞速。
/// - 判断 / Assert：解码 future 以 `Err` 结束，整体 5 秒内完成。
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn uds_cancel_partial_header_() {
    timeout(TIMEOUT_, async {
        let sock_path = TempSockPath_::new_("cancel");
        let listener = UnixListener::bind(sock_path.as_path_()).expect("绑定 UDS 应当成功");
        let (accepted, connected) =
            tokio::join!(listener.accept(), UnixStream::connect(sock_path.as_path_()),);
        let (mut server_stream, _) = accepted.expect("应当接受一个连接");
        let client_stream = connected.expect("连接 UDS 应当成功");

        // 450 字节字符串的头部：0xda + u16 长度；只发头部，不发负载。
        let server = async {
            let partial = [0xdau8, 0x01, 0xc2];
            server_stream
                .write_all(&partial)
                .await
                .expect("写部分头应当成功");
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
                let out: Result<StrVal, MpError> =
                    decode_from_cancellable_(&mut rx, token).await;
                out
            };
            let cancel = async {
                tokio::time::sleep(Duration::from_millis(20)).await;
                source.cancel();
            };
            let (out, ()) = tokio::join!(decode, cancel);
            let err = out.expect_err("部分头 park 被取消后应返回 Err");
            let message = err.to_string();
            assert!(
                message.to_ascii_lowercase().contains("cancel"),
                "错误应来自取消而不是 EOF：{message}"
            );
        };

        tokio::join!(server, client);
    })
    .await
    .expect("uds_cancel_partial_header_ 应当在 5 秒内完成");
}
