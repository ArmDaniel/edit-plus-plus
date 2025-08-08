// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! LSP client implementation.

use anyhow::{anyhow, Result};
use log::{debug, error, warn};
use lsp_types::{
    notification::{self, DidChangeTextDocument, DidOpenTextDocument, Notification},
    request::{Completion, Request},
    CompletionItem, CompletionParams, DidChangeTextDocumentParams,
    DidOpenTextDocumentParams, InitializeParams, Position, TextDocumentContentChangeEvent,
    TextDocumentIdentifier, TextDocumentItem, Url, VersionedTextDocumentIdentifier,
    WorkspaceFolder,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, ChildStdout, Command},
    sync::mpsc,
};

#[derive(Debug)]
pub enum LspMessage {
    DidChange(Url, String, i32),
    Completion(Url, Position),
}

#[derive(Debug)]
pub enum LspResponse {
    Completion(Vec<CompletionItem>),
}


#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ServerMessage {
    Response { id: u64, result: Value },
    Notification { method: String, params: Value },
}

pub struct LspClient {
    _server: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_request_id: u64,
    pending_requests: HashMap<u64, mpsc::Sender<Value>>,
}

impl LspClient {
    pub async fn new() -> Result<Self> {
        debug!("Spawning LSP server");
        let mut server = Command::new("rust-analyzer")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()?;

        let stdin = server.stdin.take().unwrap();
        let stdout = BufReader::new(server.stdout.take().unwrap());

        Ok(Self {
            _server: server,
            stdin,
            stdout,
            next_request_id: 1,
            pending_requests: HashMap::new(),
        })
    }

    async fn send_message(&mut self, message: &Value) -> Result<()> {
        let message_str = serde_json::to_string(message)?;
        debug!("Sending message: {}", message_str);
        let content_length = message_str.len();
        let header = format!("Content-Length: {}\r\n\r\n", content_length);
        self.stdin.write_all(header.as_bytes()).await?;
        self.stdin.write_all(message_str.as_bytes()).await?;
        self.stdin.flush().await?;
        Ok(())
    }

    async fn send_request(
        &mut self,
        method: &str,
        params: Value,
    ) -> Result<mpsc::Receiver<Value>> {
        let id = self.next_request_id;
        self.next_request_id += 1;

        let request = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });

        let (tx, rx) = mpsc::channel(1);
        self.pending_requests.insert(id, tx);
        self.send_message(&request).await?;
        Ok(rx)
    }

    async fn send_notification(&mut self, method: &str, params: Value) -> Result<()> {
        let notification = json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });
        self.send_message(&notification).await
    }

    pub async fn initialize(&mut self) -> Result<()> {
        let params = InitializeParams {
            process_id: Some(std::process::id()),
            workspace_folders: Some(vec![WorkspaceFolder {
                uri: Url::from_directory_path(std::env::current_dir()?)
                    .map_err(|_| anyhow!("Failed to get current directory"))?,
                name: "edit".to_string(),
            }]),
            ..Default::default()
        };

        let mut response_rx = self
            .send_request("initialize", serde_json::to_value(params)?)
            .await?;

        tokio::spawn(async move {
            if let Some(_response) = response_rx.recv().await {
                debug!("LSP server initialized");
            }
        });

        Ok(())
    }

    pub async fn did_open(&mut self, uri: Url, text: &str) -> Result<()> {
        let params = DidOpenTextDocumentParams {
            text_document: TextDocumentItem::new(uri, "rust".to_string(), 1, text.to_string()),
        };
        self.send_notification(
            DidOpenTextDocument::METHOD,
            serde_json::to_value(params)?,
        )
        .await
    }

    pub async fn did_change(&mut self, uri: Url, text: &str, version: i32) -> Result<()> {
        let params = DidChangeTextDocumentParams {
            text_document: VersionedTextDocumentIdentifier::new(uri, version),
            content_changes: vec![TextDocumentContentChangeEvent {
                range: None,
                range_length: None,
                text: text.to_string(),
            }],
        };
        self.send_notification(
            DidChangeTextDocument::METHOD,
            serde_json::to_value(params)?,
        )
        .await
    }

    pub async fn completion(
        &mut self,
        uri: Url,
        position: Position,
    ) -> Result<Option<Vec<CompletionItem>>> {
        let params = CompletionParams {
            text_document_position: lsp_types::TextDocumentPositionParams {
                text_document: TextDocumentIdentifier::new(uri),
                position,
            },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
            context: None,
        };

        let mut response_rx = self
            .send_request(Completion::METHOD, serde_json::to_value(params)?)
            .await?;

        if let Some(response) = response_rx.recv().await {
            let completion_response: lsp_types::CompletionResponse =
                serde_json::from_value(response)?;
            match completion_response {
                lsp_types::CompletionResponse::Array(items) => Ok(Some(items)),
                lsp_types::CompletionResponse::List(list) => Ok(Some(list.items)),
            }
        } else {
            Ok(None)
        }
    }

    pub async fn run(
        &mut self,
        mut rx: mpsc::Receiver<LspMessage>,
        response_tx: mpsc::Sender<LspResponse>,
    ) {
        loop {
            tokio::select! {
                Some(msg) = rx.recv() => {
                    if let Err(e) = self.handle_client_message(msg, &response_tx).await {
                        error!("Error handling LSP message: {}", e);
                    }
                }
                Ok(Some(msg)) = self.read_message() => {
                    if let Err(e) = self.handle_server_message(msg, &response_tx).await {
                        error!("Error handling LSP server message: {}", e);
                    }
                }
                else => {
                    debug!("LSP server closed connection");
                    break;
                }
            }
        }
    }

    async fn handle_server_message(
        &mut self,
        message: ServerMessage,
        _response_tx: &mpsc::Sender<LspResponse>,
    ) -> Result<()> {
        match message {
            ServerMessage::Response { id, result } => {
                if let Some(tx) = self.pending_requests.remove(&id) {
                    tx.send(result).await?;
                } else {
                    warn!("Received response for unknown request id: {}", id);
                }
            }
            ServerMessage::Notification { method, params } => {
                self.handle_notification(&method, params).await?;
            }
        }
        Ok(())
    }

    async fn handle_client_message(
        &mut self,
        message: LspMessage,
        response_tx: &mpsc::Sender<LspResponse>,
    ) -> Result<()> {
        match message {
            LspMessage::DidChange(uri, text, version) => {
                self.did_change(uri, &text, version).await?;
            }
            LspMessage::Completion(uri, position) => {
                if let Some(items) = self.completion(uri, position).await? {
                    response_tx.send(LspResponse::Completion(items)).await?;
                }
            }
        }
        Ok(())
    }


    async fn handle_notification(&mut self, method: &str, params: Value) -> Result<()> {
        match method {
            notification::LogMessage::METHOD => {
                let log_params: lsp_types::LogMessageParams = serde_json::from_value(params)?;
                debug!("[LSP] {:?}: {}", log_params.typ, log_params.message);
            }
            _ => {
                debug!("Unhandled notification: {}", method);
            }
        }
        Ok(())
    }

    async fn read_message(&mut self) -> Result<Option<ServerMessage>> {
        let mut content_length = None;
        let mut line = String::new();

        loop {
            line.clear();
            if self.stdout.read_line(&mut line).await? == 0 {
                return Ok(None);
            }
            if line.trim().is_empty() {
                break;
            }
            let parts: Vec<&str> = line.trim().splitn(2, ": ").collect();
            if parts.len() == 2 && parts[0] == "Content-Length" {
                content_length = Some(parts[1].parse::<usize>()?);
            }
        }

        if let Some(content_length) = content_length {
            let mut content = vec![0; content_length];
            self.stdout.read_exact(&mut content).await?;
            let message_str = String::from_utf8(content)?;
            debug!("Received message: {}", message_str);
            let message = serde_json::from_str(&message_str)?;
            Ok(Some(message))
        } else {
            Ok(None)
        }
    }
}
