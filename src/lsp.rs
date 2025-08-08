// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! LSP client implementation.

use anyhow::{anyhow, Result};
use lsp_types::notification::{DidChangeTextDocument, DidOpenTextDocument, Notification};
use lsp_types::request::{Completion, Request};
use lsp_types::{
    CompletionItem, CompletionParams, DidChangeTextDocumentParams, DidOpenTextDocumentParams,
    InitializeParams, Position, TextDocumentContentChangeEvent, TextDocumentIdentifier,
    TextDocumentItem, Url, VersionedTextDocumentIdentifier, WorkspaceFolder,
};
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};

pub enum LspMessage {
    DidChange(Url, String, i32),
    Completion(Url, Position),
}

pub enum LspResponse {
    Completion(Vec<CompletionItem>),
}

pub struct LspClient {
    server: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl LspClient {
    pub async fn new() -> Result<Self> {
        let mut server = Command::new("rust-analyzer")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()?;

        let stdin = server.stdin.take().unwrap();
        let stdout = BufReader::new(server.stdout.take().unwrap());

        Ok(Self {
            server,
            stdin,
            stdout,
        })
    }

    pub async fn send_message(&mut self, message: &Value) -> Result<()> {
        let message_str = serde_json::to_string(message)?;
        let content_length = message_str.len();

        let header = format!("Content-Length: {}\r\n\r\n", content_length);

        self.stdin.write_all(header.as_bytes()).await?;
        self.stdin.write_all(message_str.as_bytes()).await?;
        self.stdin.flush().await?;

        Ok(())
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

        let request = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": params,
        });

        self.send_message(&request).await?;
        let _response = self.read_message().await?;

        // TODO: Handle the response

        Ok(())
    }

    pub async fn did_open(&mut self, uri: Url, text: &str) -> Result<()> {
        let params = DidOpenTextDocumentParams {
            text_document: TextDocumentItem::new(
                uri,
                "rust".to_string(), // TODO: Make this dynamic
                1,
                text.to_string(),
            ),
        };

        let notification = json!({
            "jsonrpc": "2.0",
            "method": DidOpenTextDocument::METHOD,
            "params": params,
        });

        self.send_message(&notification).await
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

        let notification = json!({
            "jsonrpc": "2.0",
            "method": DidChangeTextDocument::METHOD,
            "params": params,
        });

        self.send_message(&notification).await
    }

    pub async fn completion(&mut self, uri: Url, position: Position) -> Result<Option<Value>> {
        let params = CompletionParams {
            text_document_position: lsp_types::TextDocumentPositionParams {
                text_document: TextDocumentIdentifier::new(uri),
                position,
            },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
            context: None,
        };

        let request = json!({
            "jsonrpc": "2.0",
            "id": 2, // TODO: Use a proper request ID
            "method": Completion::METHOD,
            "params": params,
        });

        self.send_message(&request).await?;
        self.read_message().await
    }

    pub async fn read_message(&mut self) -> Result<Option<Value>> {
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
            let message = serde_json::from_str(&message_str)?;
            Ok(Some(message))
        } else {
            Ok(None)
        }
    }
}
