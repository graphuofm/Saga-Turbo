//! Background-writer sink: runs an inner `EventSink` on its own thread so the
//! expensive encode+write of one window overlaps generation of the next
//! (blueprint §10: I/O dominates at scale — overlap it with compute).
//!
//! The generation thread pays only a memcpy of each batch into the channel;
//! the encode (Parquet/CSV) happens off the critical path. A bounded channel
//! applies backpressure so a slow disk can't blow up memory.
//!
//! Small-batch coalescing (opt-in, `with_coalescing`): the guarded L-block
//! sweep delivers one batch per block, so a small L means thousands of tiny
//! batches (L = 0.01 d over 90 days is 9,000 blocks of ~1k rows at 1e7
//! edges). Parquet pays a fixed per-encode-call cost (RecordBatch assembly,
//! dictionary builders, writer dispatch) per delivered batch, and with tiny
//! batches that fixed cost makes the writer thread the bottleneck. Coalescing
//! accumulates incoming rows into a chunk ON THE GENERATION SIDE (the same
//! one-memcpy-per-row the plain path pays for the channel clone) and sends
//! the chunk as one message once it reaches `chunk_rows`, so the writer
//! thread encodes a few large batches instead of thousands of tiny ones,
//! and the encode of one chunk overlaps generation of the next. Coalescing
//! must NOT be enabled on paths whose output bytes are frozen (the guards-off
//! default path): logical row content and order are unchanged, but batch
//! segmentation, and with it byte-level layout of the written file, may
//! differ. Block boundaries are a semantics concept, not an IO concept.

//!
//! Ownership (consistency model section 15): messages carry an
//! `Arc<EventBatch>`, so a batch the engine gives away (`write_batch_owned`)
//! crosses the channel with NO row copy, and a batch shared with another
//! consumer (the stats thread, [`ThreadedSink::push_shared`]) is encoded from
//! the same allocation both threads read. Only `write_batch(&batch)` (a
//! borrowed batch the caller keeps) still copies.

use std::sync::mpsc::{sync_channel, Receiver, SyncSender};
use std::sync::Arc;
use std::thread::JoinHandle;

use saga_core::{EventBatch, EventSink, NodeBatch};

enum Msg {
    Batch(Arc<EventBatch>),
    Nodes(NodeBatch),
}

pub struct ThreadedSink {
    tx: Option<SyncSender<Msg>>,
    handle: Option<JoinHandle<anyhow::Result<()>>>,
    /// Coalescing target in rows; 0 = off (send every batch through as-is,
    /// the historical behavior).
    chunk_rows: usize,
    /// Rows accumulated but not yet sent (only when `chunk_rows > 0`).
    pending: EventBatch,
}

impl ThreadedSink {
    /// Wrap `inner`, moving it onto a writer thread. `depth` bounds the number
    /// of in-flight batches (backpressure).
    pub fn new<S: EventSink + Send + 'static>(inner: S, depth: usize) -> ThreadedSink {
        Self::build(inner, depth, 0)
    }

    /// Like [`ThreadedSink::new`], but coalesce delivered batches into
    /// `chunk_rows`-row messages before they cross the channel (see the
    /// module docs). 128k rows amortizes the per-encode fixed cost the same
    /// way the default path's ~100k-row day batches do, while keeping the
    /// encode streaming and overlapped.
    pub fn with_coalescing<S: EventSink + Send + 'static>(
        inner: S,
        depth: usize,
        chunk_rows: usize,
    ) -> ThreadedSink {
        Self::build(inner, depth, chunk_rows)
    }

    fn build<S: EventSink + Send + 'static>(
        mut inner: S,
        depth: usize,
        chunk_rows: usize,
    ) -> ThreadedSink {
        let (tx, rx): (SyncSender<Msg>, Receiver<Msg>) = sync_channel(depth.max(1));
        let handle = std::thread::Builder::new()
            .name("saga-writer".into())
            .spawn(move || -> anyhow::Result<()> {
                for msg in rx {
                    match msg {
                        Msg::Batch(b) => inner.write_batch(&b)?,
                        Msg::Nodes(n) => inner.write_nodes(&n)?,
                    }
                }
                inner.finish()
            })
            .expect("spawn writer thread");
        ThreadedSink { tx: Some(tx), handle: Some(handle), chunk_rows, pending: EventBatch::default() }
    }

    /// Send the accumulated chunk (if any) as one message.
    fn flush_pending(&mut self) -> anyhow::Result<()> {
        if self.pending.is_empty() {
            return Ok(());
        }
        let b = std::mem::take(&mut self.pending);
        // Preserve the union schema for subsequent appends.
        self.pending.attr_names = b.attr_names.clone();
        self.pending.attrs = vec![Vec::new(); b.attrs.len()];
        self.send_batch(Arc::new(b))
    }

    fn send_batch(&self, b: Arc<EventBatch>) -> anyhow::Result<()> {
        if let Some(tx) = &self.tx {
            tx.send(Msg::Batch(b)).map_err(|_| anyhow::anyhow!("writer thread stopped early"))?;
        }
        Ok(())
    }

    /// Deliver a batch that another consumer may be reading concurrently (the
    /// stats thread): no copy on the plain path, one copy into the pending
    /// chunk on the coalescing path. Delivery semantics are exactly those of
    /// `write_batch(&batch)`.
    pub fn push_shared(&mut self, batch: Arc<EventBatch>) -> anyhow::Result<()> {
        if self.chunk_rows == 0 {
            return self.send_batch(batch);
        }
        self.write_batch(&batch)
    }

    /// Join the writer thread, propagating any write error.
    fn join(&mut self) -> anyhow::Result<()> {
        let flush = self.flush_pending();
        // Drop the sender so the receiver loop ends.
        self.tx.take();
        let joined = if let Some(h) = self.handle.take() {
            match h.join() {
                Ok(r) => r,
                Err(_) => anyhow::bail!("writer thread panicked"),
            }
        } else {
            Ok(())
        };
        flush.and(joined)
    }
}

/// Append every row of `src` to `dst` (all batches of one run share the
/// compiled world's union attr schema).
fn extend_batch(dst: &mut EventBatch, src: &EventBatch) {
    if dst.attr_names.is_empty() {
        dst.attr_names = src.attr_names.clone();
        dst.attrs = vec![Vec::new(); src.attrs.len()];
    }
    dst.src.extend_from_slice(&src.src);
    dst.dst.extend_from_slice(&src.dst);
    dst.t.extend_from_slice(&src.t);
    dst.event_type.extend_from_slice(&src.event_type);
    dst.label.extend_from_slice(&src.label);
    dst.anomaly_id.extend_from_slice(&src.anomaly_id);
    for (c, col) in src.attrs.iter().enumerate() {
        dst.attrs[c].extend_from_slice(col);
    }
}

impl EventSink for ThreadedSink {
    fn write_batch(&mut self, batch: &EventBatch) -> anyhow::Result<()> {
        if self.chunk_rows == 0 {
            // Historical path, unchanged (empty batches included): one memcpy
            // of the borrowed batch; encode happens on the thread.
            return self.send_batch(Arc::new(batch.clone()));
        }
        if batch.is_empty() {
            return Ok(());
        }
        // Coalescing: same one copy per row, into the chunk instead of the
        // channel; the chunk moves into the channel when full.
        extend_batch(&mut self.pending, batch);
        if self.pending.len() >= self.chunk_rows {
            self.flush_pending()?;
        }
        Ok(())
    }

    fn write_batch_owned(&mut self, batch: EventBatch) -> anyhow::Result<()> {
        if self.chunk_rows == 0 {
            // The caller gave the batch away: move it across, no copy.
            return self.send_batch(Arc::new(batch));
        }
        if batch.is_empty() {
            return Ok(());
        }
        if self.pending.is_empty() {
            // Nothing accumulated: the batch becomes the pending chunk as is
            // (same rows, same order; flushed by the same size rule).
            self.pending = batch;
        } else {
            extend_batch(&mut self.pending, &batch);
        }
        if self.pending.len() >= self.chunk_rows {
            self.flush_pending()?;
        }
        Ok(())
    }

    fn write_nodes(&mut self, batch: &NodeBatch) -> anyhow::Result<()> {
        // Keep channel messages in delivery order: edges before these nodes.
        self.flush_pending()?;
        if let Some(tx) = &self.tx {
            tx.send(Msg::Nodes(batch.clone()))
                .map_err(|_| anyhow::anyhow!("writer thread stopped early"))?;
        }
        Ok(())
    }

    fn finish(&mut self) -> anyhow::Result<()> {
        self.join()
    }
}

impl Drop for ThreadedSink {
    fn drop(&mut self) {
        let _ = self.join();
    }
}
