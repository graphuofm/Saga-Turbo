//! Sharded Parquet writer — the primary output format (blueprint §12/§13).
//!
//! Layout (out_dir/):
//!   edges_00000.parquet, edges_00001.parquet, …
//!   nodes_<entity_type>.parquet
//!
//! Schema choices for zero-preprocessing loading into PyG/DGL/Neo4j/polars:
//!   src/dst   UInt64
//!   t         Int64 (unix seconds; epoch documented in saga_meta.json)
//!   event_type/label   dictionary-encoded Utf8
//!   numeric attrs      Float64, null where not applicable
//!   categorical attrs  dictionary-encoded Utf8, null where not applicable
//!
//! Parallel encoding (consistency model section 14): the edge file is built
//! row group by row group exactly as `ArrowWriter` would build it (same
//! `max_row_group_row_count` segmentation, same slice sequence fed to the
//! same column writers, same properties), but each column chunk of a row
//! group is encoded (dictionary + ZSTD) on its own thread, streaming as the
//! slices arrive, and the finished chunks are appended to the single file
//! strictly in row-group index order. Row groups and column chunks are
//! independent in Parquet, so the bytes are identical to the serial writer;
//! only the wall clock changes. Memory: the group being filled plus at most
//! `groups_in_flight` sealed groups (backpressure onto the dispatcher).

use std::collections::VecDeque;
use std::fs::File;
use std::path::PathBuf;
use std::sync::mpsc::{channel, sync_channel, Receiver, Sender};
use std::sync::Arc;

use arrow_array::builder::{Float64Builder, StringDictionaryBuilder};
use arrow_array::types::UInt16Type;
use arrow_array::{ArrayRef, Int64Array, RecordBatch, UInt64Array};
use arrow_schema::{DataType, Field, Schema};
use parquet::arrow::arrow_writer::{
    compute_leaves, ArrowColumnChunk, ArrowRowGroupWriterFactory,
};
use parquet::arrow::ArrowWriter;
use parquet::basic::{Compression, ZstdLevel};
use parquet::file::properties::WriterProperties;
use parquet::file::writer::SerializedFileWriter;
use saga_core::{EventBatch, EventSink, NodeBatch, NodeColumn};

pub struct ParquetSink {
    dir: PathBuf,
    rows_per_shard: usize,
    event_type_names: Vec<String>,
    intent_names: Vec<String>,
    /// attr name -> categorical dictionary (None = numeric).
    attr_dicts: Vec<(String, Vec<String>)>,
    schema: Option<Arc<Schema>>,
    /// Per union column: Some(dict) if categorical.
    col_dicts: Option<Vec<Option<Vec<String>>>>,
    writer: Option<ShardWriter>,
    rows_in_shard: usize,
    shard_idx: u32,
    pub shards_written: Vec<PathBuf>,
    /// Bound on sealed row groups still being finished by their encoders
    /// (the one being filled is extra). Memory stays within that many
    /// encoded groups; the file order never depends on it.
    groups_in_flight: usize,
    /// Per event type: the duration attribute, if any (see
    /// `FormatSink::set_event_durations`); resolved to columns with the schema.
    durations: Vec<Option<(String, f64)>>,
    dur_cols: Option<Vec<Option<(usize, f64)>>>,
    /// Open writers of auxiliary tables (`<name>.parquet`), closed in finish.
    aux: Vec<(String, ArrowWriter<File>)>,
}

type ChunkResult = (usize, parquet::errors::Result<ArrowColumnChunk>);

/// The row group currently being filled: one encoder thread per leaf column,
/// each fed every slice as it arrives (streaming, nothing retained on the
/// dispatcher side), reporting its finished column chunk on `rx`.
struct OpenGroup {
    feeds: Vec<Sender<Arc<RecordBatch>>>,
    rx: Receiver<ChunkResult>,
    rows: usize,
}

/// A row group whose feeds are closed; its leaves arrive on `rx` in
/// completion order and are collected back into leaf order.
struct InFlight {
    rx: Receiver<ChunkResult>,
    done: Vec<Option<ArrowColumnChunk>>,
    remaining: usize,
}

/// One open edge shard: the file writer, the row group being filled and the
/// row groups whose encoders are finishing (FIFO = index order).
struct ShardWriter {
    file: SerializedFileWriter<File>,
    factory: Arc<ArrowRowGroupWriterFactory>,
    max_rows: usize,
    current: Option<OpenGroup>,
    in_flight: VecDeque<InFlight>,
    next_rg_index: usize,
}

impl ShardWriter {
    fn open(path: &PathBuf, schema: Arc<Schema>, props: WriterProperties) -> anyhow::Result<ShardWriter> {
        let max_rows = props
            .max_row_group_row_count()
            .expect("default writer properties set a row-group row limit");
        // Build through ArrowWriter so the file carries exactly the metadata
        // (ARROW:schema key-value, writer version, created_by) the serial
        // writer wrote, then drop to the row-group API.
        let aw = ArrowWriter::try_new(File::create(path)?, schema, Some(props))?;
        let (file, factory) = aw.into_serialized_writer()?;
        Ok(ShardWriter {
            file,
            factory: Arc::new(factory),
            max_rows,
            current: None,
            in_flight: VecDeque::new(),
            next_rg_index: 0,
        })
    }

    /// Mirror of `ArrowWriter::write`: split at the row-group row limit and
    /// close the group once it reaches the limit. Same slice boundaries, so
    /// each column writer sees the same mini-batch sequence as the serial
    /// writer and produces the same pages.
    fn push(&mut self, rb: RecordBatch, k: usize) -> anyhow::Result<()> {
        if rb.num_rows() == 0 {
            return Ok(());
        }
        let rows = self.current.as_ref().map_or(0, |g| g.rows);
        if rows + rb.num_rows() > self.max_rows {
            let to_write = self.max_rows - rows;
            let a = rb.slice(0, to_write);
            let b = rb.slice(to_write, rb.num_rows() - to_write);
            self.push(a, k)?;
            return self.push(b, k);
        }
        if self.current.is_none() {
            self.current = Some(self.open_group()?);
        }
        let g = self.current.as_mut().expect("opened");
        let rb = Arc::new(rb);
        for feed in &g.feeds {
            // A closed feed means that encoder already failed; its error
            // surfaces when the group is collected.
            let _ = feed.send(Arc::clone(&rb));
        }
        g.rows += rb.num_rows();
        if g.rows >= self.max_rows {
            self.seal_group(k)?;
        }
        // Append whatever has finished without blocking, so encoded groups
        // never accumulate.
        self.drain_ready()
    }

    /// Start the encoders of the next row group: column writers created here
    /// exactly as ArrowWriter creates them, one thread per leaf column.
    fn open_group(&mut self) -> anyhow::Result<OpenGroup> {
        let rg_index = self.next_rg_index;
        self.next_rg_index += 1;
        let writers = self.factory.create_column_writers(rg_index)?;
        let n_leaves = writers.len();
        let (tx, rx) = sync_channel::<ChunkResult>(n_leaves);
        let mut feeds = Vec::with_capacity(n_leaves);
        // Every column here is a single-leaf primitive/dictionary type, so
        // leaf index == field index.
        for (leaf_idx, mut w) in writers.into_iter().enumerate() {
            let (feed_tx, feed_rx) = channel::<Arc<RecordBatch>>();
            let tx = tx.clone();
            std::thread::Builder::new()
                .name(format!("saga-enc-{leaf_idx}"))
                .spawn(move || {
                    let res = (|| -> parquet::errors::Result<ArrowColumnChunk> {
                        for rb in feed_rx {
                            let schema = rb.schema();
                            let field = &schema.fields()[leaf_idx];
                            for leaf in compute_leaves(field.as_ref(), &rb.columns()[leaf_idx])? {
                                w.write(&leaf)?;
                            }
                        }
                        w.close()
                    })();
                    // Receiver gone = shard already failed; nothing to report.
                    let _ = tx.send((leaf_idx, res));
                })?;
            feeds.push(feed_tx);
        }
        Ok(OpenGroup { feeds, rx, rows: 0 })
    }

    /// Close the current group's feeds (its encoders finish and report) and
    /// queue it for the ordered append; block if more than `k` are pending.
    fn seal_group(&mut self, k: usize) -> anyhow::Result<()> {
        let Some(g) = self.current.take() else { return Ok(()) };
        let n_leaves = g.feeds.len();
        drop(g.feeds);
        let mut done = Vec::with_capacity(n_leaves);
        done.resize_with(n_leaves, || None);
        self.in_flight.push_back(InFlight { rx: g.rx, done, remaining: n_leaves });
        while self.in_flight.len() > k.max(1) {
            self.append_oldest()?;
        }
        Ok(())
    }

    /// Non-blocking: pull finished leaves into the oldest slot and append
    /// every fully encoded row group at the head of the queue, in order.
    fn drain_ready(&mut self) -> anyhow::Result<()> {
        loop {
            let Some(front) = self.in_flight.front_mut() else { return Ok(()) };
            while front.remaining > 0 {
                match front.rx.try_recv() {
                    Ok((idx, res)) => {
                        front.done[idx] = Some(res?);
                        front.remaining -= 1;
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => return Ok(()),
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        anyhow::bail!("parquet encoder thread died")
                    }
                }
            }
            self.append_oldest()?;
        }
    }

    /// Blocking: wait for the oldest row group and append it.
    fn append_oldest(&mut self) -> anyhow::Result<()> {
        let Some(mut slot) = self.in_flight.pop_front() else { return Ok(()) };
        while slot.remaining > 0 {
            let (idx, res) = slot
                .rx
                .recv()
                .map_err(|_| anyhow::anyhow!("parquet encoder thread died"))?;
            slot.done[idx] = Some(res?);
            slot.remaining -= 1;
        }
        let mut rg = self.file.next_row_group()?;
        for c in slot.done {
            c.expect("all leaves collected").append_to_row_group(&mut rg)?;
        }
        rg.close()?;
        Ok(())
    }

    fn close(mut self, k: usize) -> anyhow::Result<()> {
        self.seal_group(k)?;
        while !self.in_flight.is_empty() {
            self.append_oldest()?;
        }
        self.file.close()?;
        Ok(())
    }
}

impl ParquetSink {
    pub fn new(
        dir: PathBuf,
        shard_size_mb: u64,
        event_type_names: Vec<String>,
        intent_names: Vec<String>,
        attr_dicts: Vec<(String, Vec<String>)>,
    ) -> anyhow::Result<ParquetSink> {
        std::fs::create_dir_all(&dir)?;
        // ~24 B/edge on disk after compression (cost-model constant).
        let rows_per_shard = ((shard_size_mb * 1024 * 1024) / 24).max(1024) as usize;
        Ok(ParquetSink {
            dir,
            rows_per_shard,
            event_type_names,
            intent_names,
            attr_dicts,
            schema: None,
            col_dicts: None,
            writer: None,
            rows_in_shard: 0,
            shard_idx: 0,
            shards_written: Vec::new(),
            groups_in_flight: 2,
            aux: Vec::new(),
            durations: Vec::new(),
            dur_cols: None,
        })
    }

    fn props() -> WriterProperties {
        WriterProperties::builder()
            .set_compression(Compression::ZSTD(ZstdLevel::try_new(1).expect("level 1 valid")))
            .build()
    }

    /// See [`crate::FormatSink::set_event_durations`].
    pub fn set_event_durations(&mut self, durations: Vec<Option<(String, f64)>>) {
        self.durations = durations;
    }

    fn ensure_schema(&mut self, batch: &EventBatch) {
        if self.schema.is_some() {
            return;
        }
        let dict_ty = DataType::Dictionary(Box::new(DataType::UInt16), Box::new(DataType::Utf8));
        let mut fields = vec![
            Field::new("src", DataType::UInt64, false),
            Field::new("dst", DataType::UInt64, false),
            Field::new("t", DataType::Int64, false),
            Field::new("event_type", dict_ty.clone(), false),
            Field::new("label", dict_ty.clone(), false),
            Field::new("anomaly_id", DataType::Int64, false),
        ];
        let col_dicts: Vec<Option<Vec<String>>> = batch
            .attr_names
            .iter()
            .map(|name| {
                self.attr_dicts
                    .iter()
                    .find(|(n, _)| n == name)
                    .map(|(_, vals)| vals.clone())
            })
            .collect();
        for (i, name) in batch.attr_names.iter().enumerate() {
            let ty = if col_dicts[i].is_some() { dict_ty.clone() } else { DataType::Float64 };
            fields.push(Field::new(name, ty, true));
        }
        if self.durations.iter().any(Option::is_some) {
            fields.push(Field::new("t_end", DataType::Int64, false));
            self.dur_cols = Some(crate::resolve_dur_cols(&self.durations, &batch.attr_names));
        }
        self.schema = Some(Arc::new(Schema::new(fields)));
        self.col_dicts = Some(col_dicts);
    }

    fn roll_writer(&mut self) -> anyhow::Result<()> {
        if let Some(w) = self.writer.take() {
            w.close(self.groups_in_flight)?;
        }
        let path = self.dir.join(format!("edges_{:05}.parquet", self.shard_idx));
        self.shard_idx += 1;
        self.rows_in_shard = 0;
        self.writer = Some(ShardWriter::open(
            &path,
            self.schema.clone().expect("schema set before roll"),
            Self::props(),
        )?);
        self.shards_written.push(path);
        Ok(())
    }

    fn to_record_batch(&self, b: &EventBatch) -> anyhow::Result<RecordBatch> {
        let col_dicts = self.col_dicts.as_ref().expect("set with schema");
        let n = b.len();
        let mut cols: Vec<ArrayRef> = vec![
            Arc::new(UInt64Array::from(b.src.clone())),
            Arc::new(UInt64Array::from(b.dst.clone())),
            Arc::new(Int64Array::from(b.t.clone())),
            dict_col(&b.event_type, &self.event_type_names, n),
            dict_col(&b.label, &self.intent_names, n),
            Arc::new(Int64Array::from(b.anomaly_id.clone())),
        ];
        for (c, col) in b.attrs.iter().enumerate() {
            match &col_dicts[c] {
                Some(dict) => {
                    let mut builder = StringDictionaryBuilder::<UInt16Type>::new();
                    for &v in col {
                        if v.is_nan() {
                            builder.append_null();
                        } else {
                            let code = (v as usize).min(dict.len().saturating_sub(1));
                            builder.append_value(&dict[code]);
                        }
                    }
                    cols.push(Arc::new(builder.finish()));
                }
                None => {
                    let mut builder = Float64Builder::with_capacity(n);
                    for &v in col {
                        if v.is_nan() {
                            builder.append_null();
                        } else {
                            builder.append_value(v);
                        }
                    }
                    cols.push(Arc::new(builder.finish()));
                }
            }
        }
        if let Some(dc) = &self.dur_cols {
            cols.push(Arc::new(Int64Array::from(crate::end_times(b, dc))));
        }
        Ok(RecordBatch::try_new(self.schema.clone().expect("set"), cols)?)
    }
}

fn dict_col(codes: &[u16], names: &[String], n: usize) -> ArrayRef {
    let mut builder = StringDictionaryBuilder::<UInt16Type>::with_capacity(n, names.len(), 1024);
    for &c in codes {
        builder.append_value(&names[c as usize]);
    }
    Arc::new(builder.finish())
}

impl EventSink for ParquetSink {
    fn write_batch(&mut self, batch: &EventBatch) -> anyhow::Result<()> {
        if batch.is_empty() {
            return Ok(());
        }
        self.ensure_schema(batch);
        if self.writer.is_none() || self.rows_in_shard >= self.rows_per_shard {
            self.roll_writer()?;
        }
        let rb = self.to_record_batch(batch)?;
        let k = self.groups_in_flight;
        self.writer.as_mut().expect("rolled").push(rb, k)?;
        self.rows_in_shard += batch.len();
        Ok(())
    }

    fn write_nodes(&mut self, batch: &NodeBatch) -> anyhow::Result<()> {
        let path = self.dir.join(format!("nodes_{}.parquet", batch.entity_type));
        let mut fields = vec![Field::new("id", DataType::UInt64, false)];
        let dict_ty = DataType::Dictionary(Box::new(DataType::UInt16), Box::new(DataType::Utf8));
        for (i, name) in batch.attr_names.iter().enumerate() {
            let ty = match &batch.attrs[i] {
                NodeColumn::Numeric(_) => DataType::Float64,
                NodeColumn::Category { .. } => dict_ty.clone(),
            };
            fields.push(Field::new(name, ty, false));
        }
        let schema = Arc::new(Schema::new(fields));
        let mut cols: Vec<ArrayRef> = vec![Arc::new(UInt64Array::from(batch.ids.clone()))];
        for col in &batch.attrs {
            match col {
                NodeColumn::Numeric(v) => {
                    cols.push(Arc::new(arrow_array::Float64Array::from(v.clone())))
                }
                NodeColumn::Category { codes, names } => cols.push(dict_col(codes, names, codes.len())),
            }
        }
        let rb = RecordBatch::try_new(schema.clone(), cols)?;
        let mut w = ArrowWriter::try_new(File::create(&path)?, schema, Some(Self::props()))?;
        w.write(&rb)?;
        w.close()?;
        Ok(())
    }

    fn write_table(&mut self, table: &saga_core::AuxTable) -> anyhow::Result<()> {
        use saga_core::TableColumn;
        if table.is_empty() {
            return Ok(());
        }
        let dict_ty = DataType::Dictionary(Box::new(DataType::UInt16), Box::new(DataType::Utf8));
        let mut fields = Vec::new();
        let mut cols: Vec<ArrayRef> = Vec::new();
        for (name, col) in &table.columns {
            let (ty, arr): (DataType, ArrayRef) = match col {
                TableColumn::U64(v) => (DataType::UInt64, Arc::new(UInt64Array::from(v.clone()))),
                TableColumn::I64(v) => (DataType::Int64, Arc::new(Int64Array::from(v.clone()))),
                TableColumn::F64(v) => {
                    (DataType::Float64, Arc::new(arrow_array::Float64Array::from(v.clone())))
                }
                TableColumn::Category { codes, names } => {
                    (dict_ty.clone(), dict_col(codes, names, codes.len()))
                }
            };
            fields.push(Field::new(name, ty, false));
            cols.push(arr);
        }
        let schema = Arc::new(Schema::new(fields));
        let rb = RecordBatch::try_new(schema.clone(), cols)?;
        let pos = match self.aux.iter().position(|(n, _)| n == &table.name) {
            Some(p) => p,
            None => {
                let path = self.dir.join(format!("{}.parquet", table.name));
                let w = ArrowWriter::try_new(File::create(&path)?, schema, Some(Self::props()))?;
                self.aux.push((table.name.clone(), w));
                self.aux.len() - 1
            }
        };
        self.aux[pos].1.write(&rb)?;
        Ok(())
    }

    fn finish(&mut self) -> anyhow::Result<()> {
        if let Some(w) = self.writer.take() {
            w.close(self.groups_in_flight)?;
        }
        for (_, w) in self.aux.drain(..) {
            w.close()?;
        }
        Ok(())
    }
}
