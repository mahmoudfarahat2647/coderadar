//! Concrete implementations of Layer 2 ports.
//!
//! These exist to prove, at compile time, that the outermost circle depends
//! inward on the use-case ports and domain entities — never the reverse.

use std::sync::Mutex;

use coderadar_domain::{CodeChunk, DomainError};
use coderadar_usecases::ChunkSink;

/// A [`ChunkSink`] that buffers every chunk it receives in memory.
///
/// Used by tests and by the Phase 2 CLI until the real streaming pipeline
/// lands.
#[derive(Debug, Default)]
pub struct CollectingChunkSink {
    chunks: Mutex<Vec<CodeChunk>>,
}

impl CollectingChunkSink {
    /// Create an empty sink.
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of buffered chunks.
    pub fn len(&self) -> usize {
        self.chunks.lock().map_or(0, |guard| guard.len())
    }

    /// `true` when no chunks have been buffered.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Drain and return every buffered chunk.
    pub fn drain(&self) -> Vec<CodeChunk> {
        self.chunks
            .lock()
            .map(|mut guard| std::mem::take(&mut *guard))
            .unwrap_or_default()
    }
}

impl ChunkSink for CollectingChunkSink {
    fn send_chunk(&self, chunk: CodeChunk) -> Result<(), DomainError> {
        let mut guard = self
            .chunks
            .lock()
            .map_err(|_| DomainError::External("chunk sink lock poisoned".to_string()))?;
        guard.push(chunk);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::CollectingChunkSink;
    use coderadar_domain::CodeChunk;
    use coderadar_usecases::ChunkSink;

    #[test]
    fn sink_collects_and_drains_chunks() {
        let sink = CollectingChunkSink::new();
        assert!(sink.is_empty());

        sink.send_chunk(CodeChunk::new("a.ts", "typescript", 1, 3, "let a = 1;"))
            .unwrap();
        sink.send_chunk(CodeChunk::new("b.ts", "typescript", 1, 3, "let b = 2;"))
            .unwrap();

        assert_eq!(sink.len(), 2);
        let drained = sink.drain();
        assert_eq!(drained.len(), 2);
        assert!(sink.is_empty());
    }
}
