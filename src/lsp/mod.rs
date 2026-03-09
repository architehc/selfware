#![allow(dead_code, unused_imports, unused_variables)]
//! Language Server Protocol (LSP) client for semantic code intelligence.
//!
//! Provides go-to-definition, find-references, document symbols, hover, and
//! diagnostics by connecting to language servers (rust-analyzer, pyright,
//! typescript-language-server, gopls) over JSON-RPC 2.0 / stdio.

pub mod client;

pub use client::LspClient;
