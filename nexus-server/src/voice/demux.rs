//! Removable UDP demux for server-side DTLS voice handshakes.
//!
//! The upstream DTLS listener runs the handshake inside `accept()` and its raw
//! UDP child `close()` is a no-op. This demux keeps Nexus in charge of child
//! lifetime so timed-out DTLS handshakes can be closed and removed.

use std::any::Any;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, Weak};

use tokio::net::UdpSocket;
use tokio::sync::{Notify, Semaphore, mpsc};
use tokio::task::JoinHandle;
use webrtc_util::conn::Conn;
use webrtc_util::{Error as WebRtcError, Result as WebRtcResult};

use crate::constants::{
    ERR_VOICE_DEMUX_ACCEPT_TX_LOCK, ERR_VOICE_DEMUX_CHILDREN_LOCK, ERR_VOICE_DEMUX_READ_TASK_LOCK,
    ERR_VOICE_UDP_CHILD_TX_LOCK,
};

const RECEIVE_MTU: usize = 8192;
const DTLS_HANDSHAKE_CONTENT_TYPE: u8 = 22;
const DTLS_CLIENT_HELLO_TYPE: u8 = 1;
const DTLS_RECORD_HEADER_LEN: usize = 13;
const DTLS_HANDSHAKE_HEADER_TYPE_OFFSET: usize = DTLS_RECORD_HEADER_LEN;

pub(super) const DEFAULT_PENDING_HANDSHAKE_LIMIT: usize = 128;
pub(super) const DEFAULT_CHILD_PACKET_QUEUE: usize = 128;

pub(super) struct VoiceUdpListener {
    inner: Arc<VoiceUdpInner>,
}

pub(super) struct PendingVoiceConnection {
    conn: VoiceUdpConnHandle,
    remote_addr: SocketAddr,
    _pending_permit: tokio::sync::OwnedSemaphorePermit,
}

#[derive(Clone)]
pub(super) struct VoiceUdpConnHandle {
    conn: Arc<VoiceUdpConn>,
}

impl PendingVoiceConnection {
    pub(super) fn remote_addr(&self) -> SocketAddr {
        self.remote_addr
    }

    pub(super) fn into_parts(
        self,
    ) -> (
        Arc<dyn Conn + Send + Sync>,
        VoiceUdpConnHandle,
        tokio::sync::OwnedSemaphorePermit,
    ) {
        let conn = self.conn.as_conn();
        (conn, self.conn, self._pending_permit)
    }

    pub(super) async fn close(self) {
        self.conn.close().await;
    }
}

impl VoiceUdpConnHandle {
    fn new(conn: Arc<VoiceUdpConn>) -> Self {
        Self { conn }
    }

    pub(super) fn as_conn(&self) -> Arc<dyn Conn + Send + Sync> {
        self.conn.clone()
    }

    pub(super) async fn close(&self) {
        let _ = self.conn.close().await;
    }

    fn close_sync(&self) {
        self.conn.close_sync();
    }

    #[cfg(test)]
    fn weak(&self) -> Weak<VoiceUdpConn> {
        Arc::downgrade(&self.conn)
    }
}

struct VoiceUdpInner {
    socket: Arc<UdpSocket>,
    accept_tx: Mutex<Option<mpsc::Sender<PendingVoiceConnection>>>,
    accept_rx: tokio::sync::Mutex<mpsc::Receiver<PendingVoiceConnection>>,
    children: Mutex<HashMap<SocketAddr, VoiceUdpChildEntry>>,
    pending_handshakes: Arc<Semaphore>,
    next_child_id: AtomicU64,
    child_queue_capacity: usize,
    closed: AtomicBool,
    read_task: Mutex<Option<JoinHandle<()>>>,
}

struct VoiceUdpChildEntry {
    id: u64,
    conn: Arc<VoiceUdpConn>,
}

struct VoiceUdpConn {
    inner: Weak<VoiceUdpInner>,
    socket: Arc<UdpSocket>,
    remote_addr: SocketAddr,
    id: u64,
    packet_tx: Mutex<Option<mpsc::Sender<Vec<u8>>>>,
    packet_rx: tokio::sync::Mutex<mpsc::Receiver<Vec<u8>>>,
    closed: AtomicBool,
    closed_notify: Notify,
}

impl VoiceUdpListener {
    pub(super) async fn bind(addr: SocketAddr) -> WebRtcResult<Self> {
        Self::bind_with_limits(
            addr,
            DEFAULT_PENDING_HANDSHAKE_LIMIT,
            DEFAULT_CHILD_PACKET_QUEUE,
        )
        .await
    }

    async fn bind_with_limits(
        addr: SocketAddr,
        pending_handshake_limit: usize,
        child_queue_capacity: usize,
    ) -> WebRtcResult<Self> {
        let pending_handshake_limit = pending_handshake_limit.max(1);
        let child_queue_capacity = child_queue_capacity.max(1);
        let socket = Arc::new(UdpSocket::bind(addr).await?);
        let (accept_tx, accept_rx) = mpsc::channel(pending_handshake_limit);

        let inner = Arc::new(VoiceUdpInner {
            socket,
            accept_tx: Mutex::new(Some(accept_tx)),
            accept_rx: tokio::sync::Mutex::new(accept_rx),
            children: Mutex::new(HashMap::new()),
            pending_handshakes: Arc::new(Semaphore::new(pending_handshake_limit)),
            next_child_id: AtomicU64::new(1),
            child_queue_capacity,
            closed: AtomicBool::new(false),
            read_task: Mutex::new(None),
        });

        let read_inner = inner.clone();
        let read_task = tokio::spawn(async move {
            read_inner.read_loop().await;
        });
        *inner
            .read_task
            .lock()
            .expect(ERR_VOICE_DEMUX_READ_TASK_LOCK) = Some(read_task);

        Ok(Self { inner })
    }

    pub(super) async fn accept(&self) -> WebRtcResult<PendingVoiceConnection> {
        let mut accept_rx = self.inner.accept_rx.lock().await;
        accept_rx
            .recv()
            .await
            .ok_or(WebRtcError::ErrClosedListenerAcceptCh)
    }

    #[cfg(test)]
    pub(super) fn local_addr(&self) -> WebRtcResult<SocketAddr> {
        self.inner.socket.local_addr().map_err(WebRtcError::from)
    }

    #[cfg(test)]
    fn child_count(&self) -> usize {
        self.inner
            .children
            .lock()
            .expect(ERR_VOICE_DEMUX_CHILDREN_LOCK)
            .len()
    }

    #[cfg(test)]
    fn pending_permits(&self) -> usize {
        self.inner.pending_handshakes.available_permits()
    }
}

impl Drop for VoiceUdpListener {
    fn drop(&mut self) {
        self.inner.close_sync();
    }
}

impl VoiceUdpInner {
    async fn read_loop(self: Arc<Self>) {
        let mut buf = vec![0u8; RECEIVE_MTU];

        loop {
            if self.closed.load(Ordering::SeqCst) {
                break;
            }

            match self.socket.recv_from(&mut buf).await {
                Ok((len, remote_addr)) => self.route_datagram(remote_addr, &buf[..len]),
                Err(_) => {
                    self.close_sync();
                    break;
                }
            }
        }
    }

    fn route_datagram(self: &Arc<Self>, remote_addr: SocketAddr, packet: &[u8]) {
        if self.closed.load(Ordering::SeqCst) {
            return;
        }

        if let Some(conn) = self.get_child(remote_addr) {
            conn.enqueue(packet);
            return;
        }

        if !is_dtls_client_hello(packet) {
            return;
        }

        let Ok(pending_permit) = self.pending_handshakes.clone().try_acquire_owned() else {
            return;
        };

        let id = self.next_child_id.fetch_add(1, Ordering::Relaxed);
        let conn = Arc::new(VoiceUdpConn::new(
            Arc::downgrade(self),
            self.socket.clone(),
            remote_addr,
            id,
            self.child_queue_capacity,
        ));

        if !conn.enqueue(packet) {
            conn.close_sync();
            return;
        }

        self.children
            .lock()
            .expect(ERR_VOICE_DEMUX_CHILDREN_LOCK)
            .insert(
                remote_addr,
                VoiceUdpChildEntry {
                    id,
                    conn: conn.clone(),
                },
            );

        let pending = PendingVoiceConnection {
            conn: VoiceUdpConnHandle::new(conn),
            remote_addr,
            _pending_permit: pending_permit,
        };

        let send_result = self
            .accept_tx
            .lock()
            .expect(ERR_VOICE_DEMUX_ACCEPT_TX_LOCK)
            .as_ref()
            .map(|tx| tx.try_send(pending));

        match send_result {
            Some(Ok(())) => {}
            Some(Err(err)) => {
                let pending = err.into_inner();
                pending.conn.close_sync();
            }
            None => {
                self.remove_child(remote_addr, id);
            }
        }
    }

    fn get_child(&self, remote_addr: SocketAddr) -> Option<Arc<VoiceUdpConn>> {
        self.children
            .lock()
            .expect(ERR_VOICE_DEMUX_CHILDREN_LOCK)
            .get(&remote_addr)
            .map(|entry| entry.conn.clone())
    }

    fn remove_child(&self, remote_addr: SocketAddr, id: u64) {
        let mut children = self.children.lock().expect(ERR_VOICE_DEMUX_CHILDREN_LOCK);
        if children
            .get(&remote_addr)
            .is_some_and(|entry| entry.id == id)
        {
            children.remove(&remote_addr);
        }
    }

    fn close_sync(&self) {
        if self.closed.swap(true, Ordering::SeqCst) {
            return;
        }

        self.accept_tx
            .lock()
            .expect(ERR_VOICE_DEMUX_ACCEPT_TX_LOCK)
            .take();

        if let Some(read_task) = self
            .read_task
            .lock()
            .expect(ERR_VOICE_DEMUX_READ_TASK_LOCK)
            .take()
        {
            read_task.abort();
        }

        let children = {
            let mut children = self.children.lock().expect(ERR_VOICE_DEMUX_CHILDREN_LOCK);
            children
                .drain()
                .map(|(_, entry)| entry.conn)
                .collect::<Vec<_>>()
        };

        for child in children {
            child.close_sync();
        }
    }
}

impl VoiceUdpConn {
    fn new(
        inner: Weak<VoiceUdpInner>,
        socket: Arc<UdpSocket>,
        remote_addr: SocketAddr,
        id: u64,
        queue_capacity: usize,
    ) -> Self {
        let (packet_tx, packet_rx) = mpsc::channel(queue_capacity);

        Self {
            inner,
            socket,
            remote_addr,
            id,
            packet_tx: Mutex::new(Some(packet_tx)),
            packet_rx: tokio::sync::Mutex::new(packet_rx),
            closed: AtomicBool::new(false),
            closed_notify: Notify::new(),
        }
    }

    fn enqueue(&self, packet: &[u8]) -> bool {
        if self.closed.load(Ordering::SeqCst) {
            return false;
        }

        let packet_tx = self.packet_tx.lock().expect(ERR_VOICE_UDP_CHILD_TX_LOCK);
        let Some(packet_tx) = packet_tx.as_ref() else {
            return false;
        };

        packet_tx.try_send(packet.to_vec()).is_ok()
    }

    fn close_sync(&self) {
        if self.closed.swap(true, Ordering::SeqCst) {
            return;
        }

        self.packet_tx
            .lock()
            .expect(ERR_VOICE_UDP_CHILD_TX_LOCK)
            .take();
        self.closed_notify.notify_waiters();

        if let Some(inner) = self.inner.upgrade() {
            inner.remove_child(self.remote_addr, self.id);
        }
    }
}

impl Drop for VoiceUdpConn {
    fn drop(&mut self) {
        self.close_sync();
    }
}

#[async_trait::async_trait]
impl Conn for VoiceUdpConn {
    async fn connect(&self, _addr: SocketAddr) -> WebRtcResult<()> {
        Ok(())
    }

    async fn recv(&self, buf: &mut [u8]) -> WebRtcResult<usize> {
        if self.closed.load(Ordering::SeqCst) {
            return Err(WebRtcError::ErrUseClosedNetworkConn);
        }

        let mut packet_rx = self.packet_rx.lock().await;

        let packet = tokio::select! {
            _ = self.closed_notify.notified() => return Err(WebRtcError::ErrUseClosedNetworkConn),
            packet = packet_rx.recv() => packet,
        };

        if self.closed.load(Ordering::SeqCst) {
            return Err(WebRtcError::ErrUseClosedNetworkConn);
        }

        let packet = packet.ok_or(WebRtcError::ErrUseClosedNetworkConn)?;
        if packet.len() > buf.len() {
            return Err(WebRtcError::ErrBufferShort);
        }

        buf[..packet.len()].copy_from_slice(&packet);
        Ok(packet.len())
    }

    async fn recv_from(&self, buf: &mut [u8]) -> WebRtcResult<(usize, SocketAddr)> {
        let len = self.recv(buf).await?;
        Ok((len, self.remote_addr))
    }

    async fn send(&self, buf: &[u8]) -> WebRtcResult<usize> {
        if self.closed.load(Ordering::SeqCst) {
            return Err(WebRtcError::ErrUseClosedNetworkConn);
        }

        self.socket
            .send_to(buf, self.remote_addr)
            .await
            .map_err(WebRtcError::from)
    }

    async fn send_to(&self, buf: &[u8], target: SocketAddr) -> WebRtcResult<usize> {
        if self.closed.load(Ordering::SeqCst) {
            return Err(WebRtcError::ErrUseClosedNetworkConn);
        }

        self.socket
            .send_to(buf, target)
            .await
            .map_err(WebRtcError::from)
    }

    fn local_addr(&self) -> WebRtcResult<SocketAddr> {
        self.socket.local_addr().map_err(WebRtcError::from)
    }

    fn remote_addr(&self) -> Option<SocketAddr> {
        Some(self.remote_addr)
    }

    async fn close(&self) -> WebRtcResult<()> {
        self.close_sync();
        Ok(())
    }

    fn as_any(&self) -> &(dyn Any + Send + Sync) {
        self
    }
}

fn is_dtls_client_hello(packet: &[u8]) -> bool {
    if packet.len() <= DTLS_HANDSHAKE_HEADER_TYPE_OFFSET {
        return false;
    }

    if packet[0] != DTLS_HANDSHAKE_CONTENT_TYPE {
        return false;
    }

    if packet[1] != 0xfe {
        return false;
    }

    let record_len = u16::from_be_bytes([packet[11], packet[12]]) as usize;
    if record_len == 0 || DTLS_RECORD_HEADER_LEN + record_len > packet.len() {
        return false;
    }

    packet[DTLS_HANDSHAKE_HEADER_TYPE_OFFSET] == DTLS_CLIENT_HELLO_TYPE
}

#[cfg(test)]
mod tests {
    use std::sync::Weak;
    use std::time::{Duration, Instant};

    use dtls::config::Config as DtlsConfig;
    use dtls::conn::DTLSConn;
    use dtls::crypto::Certificate;
    use tokio::time::timeout;

    use super::*;

    const TEST_TIMEOUT: Duration = Duration::from_millis(500);

    fn install_crypto_provider() {
        let _ = tokio_rustls::rustls::crypto::ring::default_provider().install_default();
    }

    fn client_hello_probe() -> Vec<u8> {
        vec![22, 0xfe, 0xfd, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1]
    }

    async fn udp_client(listener: &VoiceUdpListener) -> UdpSocket {
        let client = UdpSocket::bind("127.0.0.1:0")
            .await
            .expect("bind UDP client");
        client
            .connect(listener.local_addr().expect("listener addr"))
            .await
            .expect("connect UDP client");
        client
    }

    async fn accept_pending(listener: &VoiceUdpListener) -> PendingVoiceConnection {
        timeout(TEST_TIMEOUT, listener.accept())
            .await
            .expect("accept should complete")
            .expect("accept should succeed")
    }

    async fn wait_until_gone(weak: Weak<VoiceUdpConn>) {
        let deadline = Instant::now() + TEST_TIMEOUT;
        while Instant::now() < deadline {
            if weak.upgrade().is_none() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("voice UDP child still has strong references");
    }

    fn server_config() -> DtlsConfig {
        DtlsConfig {
            certificates: vec![
                Certificate::generate_self_signed(vec!["localhost".to_owned()])
                    .expect("generate server certificate"),
            ],
            ..Default::default()
        }
    }

    fn client_config() -> DtlsConfig {
        DtlsConfig {
            insecure_skip_verify: true,
            ..Default::default()
        }
    }

    struct ConnectedUdpConn {
        socket: UdpSocket,
        remote_addr: SocketAddr,
    }

    #[async_trait::async_trait]
    impl Conn for ConnectedUdpConn {
        async fn connect(&self, _addr: SocketAddr) -> WebRtcResult<()> {
            Ok(())
        }

        async fn recv(&self, buf: &mut [u8]) -> WebRtcResult<usize> {
            self.socket.recv(buf).await.map_err(WebRtcError::from)
        }

        async fn recv_from(&self, buf: &mut [u8]) -> WebRtcResult<(usize, SocketAddr)> {
            let len = self.recv(buf).await?;
            Ok((len, self.remote_addr))
        }

        async fn send(&self, buf: &[u8]) -> WebRtcResult<usize> {
            self.socket.send(buf).await.map_err(WebRtcError::from)
        }

        async fn send_to(&self, buf: &[u8], target: SocketAddr) -> WebRtcResult<usize> {
            self.socket
                .send_to(buf, target)
                .await
                .map_err(WebRtcError::from)
        }

        fn local_addr(&self) -> WebRtcResult<SocketAddr> {
            self.socket.local_addr().map_err(WebRtcError::from)
        }

        fn remote_addr(&self) -> Option<SocketAddr> {
            Some(self.remote_addr)
        }

        async fn close(&self) -> WebRtcResult<()> {
            Ok(())
        }

        fn as_any(&self) -> &(dyn Any + Send + Sync) {
            self
        }
    }

    #[tokio::test]
    async fn non_client_hello_does_not_create_child() {
        let listener = VoiceUdpListener::bind_with_limits("127.0.0.1:0".parse().unwrap(), 2, 2)
            .await
            .expect("bind listener");
        let remote_addr = "127.0.0.1:40000".parse().unwrap();

        listener.inner.route_datagram(remote_addr, &[0, 1, 2, 3]);

        assert_eq!(listener.child_count(), 0);
        assert_eq!(listener.pending_permits(), 2);
    }

    #[tokio::test]
    async fn client_hello_creates_pending_child() {
        let listener = VoiceUdpListener::bind_with_limits("127.0.0.1:0".parse().unwrap(), 2, 2)
            .await
            .expect("bind listener");
        let client = udp_client(&listener).await;

        client
            .send(&client_hello_probe())
            .await
            .expect("send client hello");

        let pending = accept_pending(&listener).await;
        assert_eq!(
            pending.remote_addr(),
            client.local_addr().expect("client addr")
        );
        assert_eq!(listener.child_count(), 1);
        assert_eq!(listener.pending_permits(), 1);

        pending.close().await;
        assert_eq!(listener.child_count(), 0);
        assert_eq!(listener.pending_permits(), 2);
    }

    #[tokio::test]
    async fn pending_cap_is_enforced_at_child_creation() {
        let listener = VoiceUdpListener::bind_with_limits("127.0.0.1:0".parse().unwrap(), 1, 2)
            .await
            .expect("bind listener");
        let first_addr = "127.0.0.1:40001".parse().unwrap();
        let second_addr = "127.0.0.1:40002".parse().unwrap();

        listener
            .inner
            .route_datagram(first_addr, &client_hello_probe());
        let pending = accept_pending(&listener).await;
        assert_eq!(pending.remote_addr(), first_addr);

        listener
            .inner
            .route_datagram(second_addr, &client_hello_probe());

        assert_eq!(listener.child_count(), 1);
        assert_eq!(listener.pending_permits(), 0);

        pending.close().await;
        assert_eq!(listener.child_count(), 0);
        assert_eq!(listener.pending_permits(), 1);
    }

    #[tokio::test]
    async fn child_close_wakes_blocked_recv() {
        let listener = VoiceUdpListener::bind_with_limits("127.0.0.1:0".parse().unwrap(), 1, 2)
            .await
            .expect("bind listener");
        let client = udp_client(&listener).await;

        client
            .send(&client_hello_probe())
            .await
            .expect("send client hello");
        let pending = accept_pending(&listener).await;
        let conn = pending.conn.as_conn();
        let mut first_packet = [0u8; 32];
        conn.recv(&mut first_packet)
            .await
            .expect("drain initial client hello");

        let recv_conn = conn.clone();
        let recv_task = tokio::spawn(async move {
            let mut buf = [0u8; 32];
            recv_conn.recv(&mut buf).await
        });

        let _ = conn.close().await;

        let result = timeout(TEST_TIMEOUT, recv_task)
            .await
            .expect("recv should wake")
            .expect("recv task should not panic");

        assert_eq!(result, Err(WebRtcError::ErrUseClosedNetworkConn));
        assert_eq!(listener.child_count(), 0);
    }

    #[tokio::test]
    async fn remove_is_generation_safe() {
        let listener = VoiceUdpListener::bind_with_limits("127.0.0.1:0".parse().unwrap(), 2, 2)
            .await
            .expect("bind listener");
        let remote_addr: SocketAddr = "127.0.0.1:40000".parse().unwrap();

        let old = Arc::new(VoiceUdpConn::new(
            Arc::downgrade(&listener.inner),
            listener.inner.socket.clone(),
            remote_addr,
            1,
            2,
        ));
        let new = Arc::new(VoiceUdpConn::new(
            Arc::downgrade(&listener.inner),
            listener.inner.socket.clone(),
            remote_addr,
            2,
            2,
        ));

        listener
            .inner
            .children
            .lock()
            .expect("children lock")
            .insert(
                remote_addr,
                VoiceUdpChildEntry {
                    id: 2,
                    conn: new.clone(),
                },
            );

        old.close_sync();

        let current = listener
            .inner
            .children
            .lock()
            .expect("children lock")
            .get(&remote_addr)
            .expect("new child should remain")
            .conn
            .clone();

        assert!(Arc::ptr_eq(&current, &new));
    }

    #[tokio::test]
    async fn demux_child_supports_real_dtls_handshake() {
        install_crypto_provider();

        let listener = VoiceUdpListener::bind_with_limits("127.0.0.1:0".parse().unwrap(), 2, 16)
            .await
            .expect("bind listener");
        let client_socket = udp_client(&listener).await;
        let remote_addr = listener.local_addr().expect("listener addr");
        let client_conn = Arc::new(ConnectedUdpConn {
            socket: client_socket,
            remote_addr,
        });

        let client_task =
            tokio::spawn(
                async move { DTLSConn::new(client_conn, client_config(), true, None).await },
            );

        let pending = accept_pending(&listener).await;
        let server_conn = pending.conn.as_conn();
        drop(pending);

        let server = timeout(
            TEST_TIMEOUT,
            DTLSConn::new(server_conn, server_config(), false, None),
        )
        .await
        .expect("server handshake should not time out")
        .expect("server handshake should succeed");

        let client = timeout(TEST_TIMEOUT, client_task)
            .await
            .expect("client task should not time out")
            .expect("client task should not panic")
            .expect("client handshake should succeed");

        server.close().await.expect("close server DTLS");
        client.close().await.expect("close client DTLS");
    }

    #[tokio::test]
    async fn timed_out_dtls_handshake_close_releases_child() {
        install_crypto_provider();

        let listener = VoiceUdpListener::bind_with_limits("127.0.0.1:0".parse().unwrap(), 1, 16)
            .await
            .expect("bind listener");
        let client = udp_client(&listener).await;

        client
            .send(&client_hello_probe())
            .await
            .expect("send client hello");
        let pending = accept_pending(&listener).await;
        let child = pending.conn.clone();
        let weak = child.weak();
        let server_conn = child.as_conn();

        let handshake_result = timeout(
            Duration::from_millis(50),
            DTLSConn::new(server_conn, server_config(), false, None),
        )
        .await;

        assert!(handshake_result.is_err(), "handshake should time out");

        child.close().await;
        drop(child);
        drop(pending);

        assert_eq!(listener.child_count(), 0);
        assert_eq!(listener.pending_permits(), 1);
        wait_until_gone(weak).await;
    }
}
