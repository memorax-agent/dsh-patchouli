mod transport;

use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

use patchouli_backend::{
    BackendEngine, BackendError, BackendErrorReason, BackendService, ClientIdentity,
    ControlShutdownResult, ControlShutdownResultData, ControlStatusResult, ControlStatusResultData,
    CreateEntityParams, DeleteEntityParams, EmptyData, HandshakeParams, HandshakeResult, Meta,
    PROTOCOL_VERSION, ProtocolErrorData, ProtocolErrorReason, ReadEntityParams, RpcParams,
    RpcResult, ServerIdentity, ServerLimits, SubscribeChangesParams, UnsubscribeChangesParams,
    UpdateEntityParams, error_codes, methods,
};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use thiserror::Error;
use tokio::{
    io::{
        AsyncBufReadExt, AsyncWrite, AsyncWriteExt, BufReader, Lines, ReadHalf, WriteHalf, split,
    },
    sync::watch,
};

const SERVER_CAPABILITIES: &[&str] = &["control.status", "control.shutdown"];
const MAX_REQUEST_BYTES: usize = 1024 * 1024;

#[derive(Clone, Debug)]
pub struct ServerOptions {
    pub endpoint: String,
    pub node_id: String,
    pub cluster_id: String,
}

#[derive(Debug, Error)]
pub enum IpcError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid JSON response: {0}")]
    Json(#[from] serde_json::Error),
    #[error("daemon closed the connection before replying")]
    ConnectionClosed,
    #[error("daemon returned JSON-RPC error {code}: {message}")]
    Rpc { code: i64, message: String },
    #[error("daemon response id does not match request id")]
    ResponseIdMismatch,
}

pub struct LocalServer {
    listener: transport::Listener,
    options: ServerOptions,
    started_at_unix_ms: u64,
    active_connections: Arc<AtomicU64>,
    shutdown_tx: watch::Sender<bool>,
    engine: Arc<BackendEngine>,
}

impl LocalServer {
    pub async fn bind(
        options: ServerOptions,
        engine: Arc<BackendEngine>,
    ) -> Result<Self, IpcError> {
        let listener = transport::Listener::bind(&options.endpoint).await?;
        let (shutdown_tx, _) = watch::channel(false);
        Ok(Self {
            listener,
            options,
            started_at_unix_ms: unix_time_ms(),
            active_connections: Arc::new(AtomicU64::new(0)),
            shutdown_tx,
            engine,
        })
    }

    pub async fn run(mut self) -> Result<(), IpcError> {
        let mut shutdown_rx = self.shutdown_tx.subscribe();
        loop {
            tokio::select! {
                result = self.listener.accept() => {
                    let stream = result?;
                    let connection = ConnectionState::new(
                        self.options.clone(),
                        self.started_at_unix_ms,
                        Arc::clone(&self.active_connections),
                        self.shutdown_tx.clone(),
                        Arc::clone(&self.engine),
                    );
                    tokio::spawn(async move {
                        let _ = connection.serve(stream).await;
                    });
                }
                result = shutdown_rx.changed() => {
                    if result.is_err() || *shutdown_rx.borrow() {
                        break;
                    }
                }
                result = tokio::signal::ctrl_c() => {
                    result?;
                    break;
                }
            }
        }
        Ok(())
    }
}

struct ConnectionState {
    options: ServerOptions,
    started_at_unix_ms: u64,
    active_connections: Arc<AtomicU64>,
    shutdown_tx: watch::Sender<bool>,
    engine: Arc<BackendEngine>,
}

impl ConnectionState {
    fn new(
        options: ServerOptions,
        started_at_unix_ms: u64,
        active_connections: Arc<AtomicU64>,
        shutdown_tx: watch::Sender<bool>,
        engine: Arc<BackendEngine>,
    ) -> Self {
        Self {
            options,
            started_at_unix_ms,
            active_connections,
            shutdown_tx,
            engine,
        }
    }

    async fn serve(self, stream: transport::Stream) -> Result<(), IpcError> {
        self.active_connections.fetch_add(1, Ordering::Relaxed);
        let _connection_guard = ConnectionGuard(Arc::clone(&self.active_connections));
        let (read_half, mut write_half) = split(stream);
        let mut lines = BufReader::new(read_half).lines();
        let mut handshaken = false;

        while let Some(line) = lines.next_line().await? {
            let (response, shutdown) = self.dispatch(&line, &mut handshaken).await;
            write_json_line(&mut write_half, &response).await?;
            if shutdown {
                let _ = self.shutdown_tx.send(true);
                break;
            }
        }
        Ok(())
    }

    async fn dispatch(&self, line: &str, handshaken: &mut bool) -> (Value, bool) {
        if line.len() > MAX_REQUEST_BYTES {
            return (
                rpc_error(Value::Null, -32600, "request exceeds server limit"),
                false,
            );
        }

        let request: Value = match serde_json::from_str(line) {
            Ok(request) => request,
            Err(_) => return (rpc_error(Value::Null, -32700, "parse error"), false),
        };
        let Some(object) = request.as_object() else {
            return (rpc_error(Value::Null, -32600, "invalid request"), false);
        };
        let id = object.get("id").cloned().unwrap_or(Value::Null);
        if object.get("jsonrpc") != Some(&Value::String("2.0".to_owned()))
            || !matches!(id, Value::Number(_) | Value::String(_))
        {
            return (rpc_error(id, -32600, "invalid request"), false);
        }
        let Some(method) = object.get("method").and_then(Value::as_str) else {
            return (rpc_error(id, -32600, "invalid request"), false);
        };
        let params = object.get("params").cloned().unwrap_or_else(|| json!({}));

        if method == methods::HANDSHAKE {
            let params: HandshakeParams = match serde_json::from_value(params) {
                Ok(params) => params,
                Err(error) => return (rpc_error(id, -32602, &error.to_string()), false),
            };
            if !params.protocol_versions.contains(&PROTOCOL_VERSION) {
                return (
                    rpc_error(id, -32006, "protocol version 1 is required"),
                    false,
                );
            }
            if let Some(capability) = params
                .capabilities
                .iter()
                .find(|capability| !SERVER_CAPABILITIES.contains(&capability.as_str()))
            {
                return (
                    rpc_error(
                        id,
                        -32006,
                        &format!("unsupported capability {capability:?}"),
                    ),
                    false,
                );
            }
            *handshaken = true;
            let result = HandshakeResult {
                protocol_version: PROTOCOL_VERSION,
                server: ServerIdentity {
                    version: env!("CARGO_PKG_VERSION").to_owned(),
                    cluster_id: self.options.cluster_id.clone(),
                    node_id: self.options.node_id.clone(),
                },
                capabilities: SERVER_CAPABILITIES
                    .iter()
                    .map(|value| (*value).to_owned())
                    .collect(),
                limits: ServerLimits {
                    max_request_bytes: MAX_REQUEST_BYTES as u64,
                    max_result_items: 1,
                    idempotency_retention_seconds: 1,
                    change_retention_seconds: 1,
                },
            };
            return (rpc_success(id, result), false);
        }

        if !*handshaken {
            return (rpc_error(id, -32001, "handshake is required"), false);
        }

        match method {
            methods::CONTROL_STATUS => {
                if let Err(error) = serde_json::from_value::<RpcParams<EmptyData>>(params) {
                    return (rpc_error(id, -32602, &error.to_string()), false);
                }
                let result: ControlStatusResult = RpcResult {
                    meta: Meta::new(),
                    data: ControlStatusResultData {
                        ready: true,
                        provider: self.engine.provider_kind().to_owned(),
                        pid: std::process::id(),
                        started_at_unix_ms: self.started_at_unix_ms,
                        active_connections: self.active_connections.load(Ordering::Relaxed),
                    },
                };
                (rpc_success(id, result), false)
            }
            methods::CONTROL_SHUTDOWN => {
                if let Err(error) = serde_json::from_value::<RpcParams<EmptyData>>(params) {
                    return (rpc_error(id, -32602, &error.to_string()), false);
                }
                let result: ControlShutdownResult = RpcResult {
                    meta: Meta::new(),
                    data: ControlShutdownResultData { accepted: true },
                };
                (rpc_success(id, result), true)
            }
            methods::ENTITY_CREATE => {
                let params = match serde_json::from_value::<CreateEntityParams>(params) {
                    Ok(params) => params,
                    Err(error) => return (rpc_error(id, -32602, &error.to_string()), false),
                };
                match self.engine.create(params).await {
                    Ok(result) => (rpc_success(id, result), false),
                    Err(error) => (rpc_backend_error(id, error), false),
                }
            }
            methods::ENTITY_READ => {
                let params = match serde_json::from_value::<ReadEntityParams>(params) {
                    Ok(params) => params,
                    Err(error) => return (rpc_error(id, -32602, &error.to_string()), false),
                };
                match self.engine.read(params).await {
                    Ok(result) => (rpc_success(id, result), false),
                    Err(error) => (rpc_backend_error(id, error), false),
                }
            }
            methods::ENTITY_UPDATE => {
                let params = match serde_json::from_value::<UpdateEntityParams>(params) {
                    Ok(params) => params,
                    Err(error) => return (rpc_error(id, -32602, &error.to_string()), false),
                };
                match self.engine.update(params).await {
                    Ok(result) => (rpc_success(id, result), false),
                    Err(error) => (rpc_backend_error(id, error), false),
                }
            }
            methods::ENTITY_DELETE => {
                let params = match serde_json::from_value::<DeleteEntityParams>(params) {
                    Ok(params) => params,
                    Err(error) => return (rpc_error(id, -32602, &error.to_string()), false),
                };
                match self.engine.delete(params).await {
                    Ok(result) => (rpc_success(id, result), false),
                    Err(error) => (rpc_backend_error(id, error), false),
                }
            }
            methods::CHANGES_SUBSCRIBE => {
                let params = match serde_json::from_value::<SubscribeChangesParams>(params) {
                    Ok(params) => params,
                    Err(error) => return (rpc_error(id, -32602, &error.to_string()), false),
                };
                match self.engine.subscribe(params).await {
                    Ok(_) => (
                        rpc_error(id, -32603, "subscription delivery is not wired"),
                        false,
                    ),
                    Err(error) => (rpc_backend_error(id, error), false),
                }
            }
            methods::CHANGES_UNSUBSCRIBE => {
                if let Err(error) = serde_json::from_value::<UnsubscribeChangesParams>(params) {
                    return (rpc_error(id, -32602, &error.to_string()), false);
                }
                (
                    rpc_protocol_error(
                        id,
                        error_codes::UNSUPPORTED_CAPABILITY,
                        "change subscription is not implemented by the backend engine",
                        ProtocolErrorReason::UnsupportedCapability,
                        None,
                    ),
                    false,
                )
            }
            _ => (rpc_error(id, -32601, "method not found"), false),
        }
    }
}

type ClientReader = Lines<BufReader<ReadHalf<transport::Stream>>>;
type ClientWriter = WriteHalf<transport::Stream>;

pub struct LocalClient {
    lines: ClientReader,
    writer: ClientWriter,
    next_id: i64,
}

impl LocalClient {
    pub async fn connect(
        endpoint: &str,
        client_name: &str,
        client_version: &str,
    ) -> Result<Self, IpcError> {
        let stream = transport::connect(endpoint).await?;
        let (read_half, writer) = split(stream);
        let mut client = Self {
            lines: BufReader::new(read_half).lines(),
            writer,
            next_id: 1,
        };
        let instance_id = format!("{}-{}", std::process::id(), unix_time_ms());
        client
            .call::<_, HandshakeResult>(
                methods::HANDSHAKE,
                &HandshakeParams {
                    client: ClientIdentity {
                        name: client_name.to_owned(),
                        version: client_version.to_owned(),
                        instance_id,
                    },
                    protocol_versions: vec![PROTOCOL_VERSION],
                    capabilities: Vec::new(),
                },
            )
            .await?;
        Ok(client)
    }

    pub async fn status(&mut self) -> Result<ControlStatusResult, IpcError> {
        self.call(
            methods::CONTROL_STATUS,
            &RpcParams {
                meta: Meta::new(),
                data: EmptyData::default(),
            },
        )
        .await
    }

    pub async fn shutdown(&mut self) -> Result<ControlShutdownResult, IpcError> {
        self.call(
            methods::CONTROL_SHUTDOWN,
            &RpcParams {
                meta: Meta::new(),
                data: EmptyData::default(),
            },
        )
        .await
    }

    pub async fn create(
        &mut self,
        params: &CreateEntityParams,
    ) -> Result<patchouli_backend::MutationResult, IpcError> {
        self.call(methods::ENTITY_CREATE, params).await
    }

    pub async fn read(
        &mut self,
        params: &ReadEntityParams,
    ) -> Result<patchouli_backend::ReadEntityResult, IpcError> {
        self.call(methods::ENTITY_READ, params).await
    }

    pub async fn update(
        &mut self,
        params: &UpdateEntityParams,
    ) -> Result<patchouli_backend::MutationResult, IpcError> {
        self.call(methods::ENTITY_UPDATE, params).await
    }

    pub async fn delete(
        &mut self,
        params: &DeleteEntityParams,
    ) -> Result<patchouli_backend::MutationResult, IpcError> {
        self.call(methods::ENTITY_DELETE, params).await
    }

    async fn call<TParams: Serialize, TResult: DeserializeOwned>(
        &mut self,
        method: &str,
        params: &TParams,
    ) -> Result<TResult, IpcError> {
        let id = self.next_id;
        self.next_id += 1;
        let request = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });
        write_json_line(&mut self.writer, &request).await?;

        let line = self
            .lines
            .next_line()
            .await?
            .ok_or(IpcError::ConnectionClosed)?;
        let response: Value = serde_json::from_str(&line)?;
        if response.get("id") != Some(&json!(id)) {
            return Err(IpcError::ResponseIdMismatch);
        }
        if let Some(error) = response.get("error") {
            return Err(IpcError::Rpc {
                code: error.get("code").and_then(Value::as_i64).unwrap_or(-32603),
                message: error
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown JSON-RPC error")
                    .to_owned(),
            });
        }
        serde_json::from_value(response.get("result").cloned().unwrap_or(Value::Null))
            .map_err(IpcError::from)
    }
}

async fn write_json_line(
    writer: &mut (impl AsyncWrite + Unpin),
    value: &Value,
) -> Result<(), IpcError> {
    let mut encoded = serde_json::to_vec(value)?;
    encoded.push(b'\n');
    writer.write_all(&encoded).await?;
    writer.flush().await?;
    Ok(())
}

fn rpc_success(id: Value, result: impl Serialize) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn rpc_error(id: Value, code: i64, message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message },
    })
}

fn rpc_backend_error(id: Value, error: BackendError) -> Value {
    let (code, reason) = match error.reason {
        BackendErrorReason::Cancelled => (error_codes::CANCELLED, ProtocolErrorReason::Cancelled),
        BackendErrorReason::CursorExpired => (
            error_codes::CURSOR_EXPIRED,
            ProtocolErrorReason::CursorExpired,
        ),
        BackendErrorReason::DeadlineExceeded => (
            error_codes::DEADLINE_EXCEEDED,
            ProtocolErrorReason::DeadlineExceeded,
        ),
        BackendErrorReason::IdempotencyConflict => (
            error_codes::IDEMPOTENCY_CONFLICT,
            ProtocolErrorReason::IdempotencyConflict,
        ),
        BackendErrorReason::NotFound => (error_codes::NOT_FOUND, ProtocolErrorReason::NotFound),
        BackendErrorReason::Overloaded => {
            (error_codes::OVERLOADED, ProtocolErrorReason::Overloaded)
        }
        BackendErrorReason::UnsupportedCapability => (
            error_codes::UNSUPPORTED_CAPABILITY,
            ProtocolErrorReason::UnsupportedCapability,
        ),
        BackendErrorReason::VersionConflict => (
            error_codes::VERSION_CONFLICT,
            ProtocolErrorReason::VersionConflict,
        ),
    };
    let current_versions = (!error.current_versions.is_empty()).then_some(error.current_versions);
    rpc_protocol_error(id, code, &error.message, reason, current_versions)
}

fn rpc_protocol_error(
    id: Value,
    code: i32,
    message: &str,
    reason: ProtocolErrorReason,
    current_versions: Option<Vec<String>>,
) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": code,
            "message": message,
            "data": ProtocolErrorData { reason, current_versions },
        },
    })
}

fn unix_time_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock must be after the Unix epoch")
        .as_millis() as u64
}

struct ConnectionGuard(Arc<AtomicU64>);

impl Drop for ConnectionGuard {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::Relaxed);
    }
}
