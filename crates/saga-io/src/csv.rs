//! Sharded CSV writer for the event stream + node tables.
//!
//! Output layout (out_dir/):
//!   edges_00000.csv, edges_00001.csv, …   sharded by target size
//!   nodes.csv                              one row per node
//! Headers are written per shard; numeric attrs render empty when NaN
//! (not applicable for that event type).

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::PathBuf;

use saga_core::{EventBatch, EventSink, NodeBatch};

pub struct CsvSink {
    dir: PathBuf,
    shard_bytes_target: u64,
    event_type_names: Vec<String>,
    intent_names: Vec<String>,
    /// attr name -> category values (decode codes to strings on write).
    attr_dicts: Vec<(String, Vec<String>)>,
    /// Per union-column dictionary, resolved on first batch.
    col_dicts: Option<Vec<Option<Vec<String>>>>,
    shard_idx: u32,
    bytes_in_shard: u64,
    writer: Option<BufWriter<File>>,
    header: Option<String>,
    pub shards_written: Vec<PathBuf>,
    /// Per event type: the duration attribute, if any; resolved with the header.
    durations: Vec<Option<(String, f64)>>,
    dur_cols: Option<Vec<Option<(usize, f64)>>>,
}

impl CsvSink {
    pub fn new(
        dir: PathBuf,
        shard_size_mb: u64,
        event_type_names: Vec<String>,
        intent_names: Vec<String>,
        attr_dicts: Vec<(String, Vec<String>)>,
    ) -> anyhow::Result<CsvSink> {
        std::fs::create_dir_all(&dir)?;
        Ok(CsvSink {
            dir,
            shard_bytes_target: shard_size_mb * 1024 * 1024,
            event_type_names,
            intent_names,
            attr_dicts,
            col_dicts: None,
            shard_idx: 0,
            bytes_in_shard: 0,
            writer: None,
            header: None,
            shards_written: Vec::new(),
            durations: Vec::new(),
            dur_cols: None,
        })
    }

    /// See [`crate::FormatSink::set_event_durations`].
    pub fn set_event_durations(&mut self, durations: Vec<Option<(String, f64)>>) {
        self.durations = durations;
    }

    fn roll_shard(&mut self) -> anyhow::Result<&mut BufWriter<File>> {
        if self.writer.is_none() || self.bytes_in_shard >= self.shard_bytes_target {
            if let Some(mut w) = self.writer.take() {
                w.flush()?;
            }
            let path = self.dir.join(format!("edges_{:05}.csv", self.shard_idx));
            self.shard_idx += 1;
            self.bytes_in_shard = 0;
            let mut w = BufWriter::with_capacity(1 << 20, File::create(&path)?);
            if let Some(h) = &self.header {
                w.write_all(h.as_bytes())?;
            }
            self.shards_written.push(path);
            self.writer = Some(w);
        }
        Ok(self.writer.as_mut().expect("just ensured"))
    }
}

impl EventSink for CsvSink {
    fn write_batch(&mut self, batch: &EventBatch) -> anyhow::Result<()> {
        if batch.is_empty() {
            return Ok(());
        }
        if self.header.is_none() {
            let mut h = String::from("src,dst,t,event_type,label,anomaly_id");
            for a in &batch.attr_names {
                h.push(',');
                h.push_str(a);
            }
            if self.durations.iter().any(Option::is_some) {
                h.push_str(",t_end");
                self.dur_cols = Some(crate::resolve_dur_cols(&self.durations, &batch.attr_names));
            }
            h.push('\n');
            self.header = Some(h);
        }
        // Resolve per-column dictionaries against the union schema once.
        if self.col_dicts.is_none() {
            self.col_dicts = Some(
                batch
                    .attr_names
                    .iter()
                    .map(|name| {
                        self.attr_dicts
                            .iter()
                            .find(|(n, _)| n == name)
                            .map(|(_, vals)| vals.clone())
                    })
                    .collect(),
            );
        }
        let col_dicts = self.col_dicts.clone().expect("just set");
        let t_end = self.dur_cols.as_ref().map(|dc| crate::end_times(batch, dc));
        // Render rows into a reusable buffer, then write once.
        let mut buf = String::with_capacity(batch.len() * 48);
        for i in 0..batch.len() {
            use std::fmt::Write as _;
            let et = &self.event_type_names[batch.event_type[i] as usize];
            let label = &self.intent_names[batch.label[i] as usize];
            write!(
                buf,
                "{},{},{},{et},{label},{}",
                batch.src[i], batch.dst[i], batch.t[i], batch.anomaly_id[i]
            )
            .unwrap();
            for (c, col) in batch.attrs.iter().enumerate() {
                let v = col[i];
                if v.is_nan() {
                    buf.push(',');
                } else if let Some(dict) = &col_dicts[c] {
                    let code = (v as usize).min(dict.len().saturating_sub(1));
                    write!(buf, ",{}", dict[code]).unwrap();
                } else if v == v.trunc() && v.abs() < 1e15 {
                    write!(buf, ",{}", v as i64).unwrap();
                } else {
                    write!(buf, ",{v:.4}").unwrap();
                }
            }
            if let Some(te) = &t_end {
                write!(buf, ",{}", te[i]).unwrap();
            }
            buf.push('\n');
        }
        let w = self.roll_shard()?;
        w.write_all(buf.as_bytes())?;
        self.bytes_in_shard += buf.len() as u64;
        Ok(())
    }

    fn finish(&mut self) -> anyhow::Result<()> {
        if let Some(mut w) = self.writer.take() {
            w.flush()?;
        }
        Ok(())
    }

    fn write_nodes(&mut self, batch: &NodeBatch) -> anyhow::Result<()> {
        let path = self.dir.join("nodes.csv");
        let exists = path.exists();
        let file = std::fs::OpenOptions::new().create(true).append(true).open(&path)?;
        let mut w = BufWriter::with_capacity(1 << 20, file);
        if !exists {
            let mut h = String::from("id,entity_type");
            for a in &batch.attr_names {
                h.push(',');
                h.push_str(a);
            }
            h.push('\n');
            w.write_all(h.as_bytes())?;
        }
        let mut buf = String::with_capacity(batch.ids.len() * 32);
        for i in 0..batch.ids.len() {
            use std::fmt::Write as _;
            write!(buf, "{},{}", batch.ids[i], batch.entity_type).unwrap();
            for col in &batch.attrs {
                match col {
                    saga_core::NodeColumn::Numeric(v) => {
                        let x = v[i];
                        if x.is_nan() {
                            buf.push(',');
                        } else if x == x.trunc() && x.abs() < 1e15 {
                            write!(buf, ",{}", x as i64).unwrap();
                        } else {
                            write!(buf, ",{x:.4}").unwrap();
                        }
                    }
                    saga_core::NodeColumn::Category { codes, names } => {
                        write!(buf, ",{}", names[codes[i] as usize]).unwrap();
                    }
                }
            }
            buf.push('\n');
        }
        w.write_all(buf.as_bytes())?;
        w.flush()?;
        Ok(())
    }

    fn write_table(&mut self, table: &saga_core::AuxTable) -> anyhow::Result<()> {
        use saga_core::TableColumn;
        use std::fmt::Write as _;
        if table.is_empty() {
            return Ok(());
        }
        let path = self.dir.join(format!("{}.csv", table.name));
        let exists = path.exists();
        let file = std::fs::OpenOptions::new().create(true).append(true).open(&path)?;
        let mut w = BufWriter::with_capacity(1 << 20, file);
        if !exists {
            let h: Vec<&str> = table.columns.iter().map(|(n, _)| n.as_str()).collect();
            w.write_all(h.join(",").as_bytes())?;
            w.write_all(b"\n")?;
        }
        let mut buf = String::with_capacity(table.len() * 32);
        for i in 0..table.len() {
            for (ci, (_, col)) in table.columns.iter().enumerate() {
                if ci > 0 {
                    buf.push(',');
                }
                match col {
                    TableColumn::U64(v) => write!(buf, "{}", v[i]).unwrap(),
                    TableColumn::I64(v) => write!(buf, "{}", v[i]).unwrap(),
                    TableColumn::F64(v) => {
                        if !v[i].is_nan() {
                            write!(buf, "{}", v[i]).unwrap()
                        }
                    }
                    TableColumn::Category { codes, names } => {
                        buf.push_str(&names[codes[i] as usize])
                    }
                }
            }
            buf.push('\n');
        }
        w.write_all(buf.as_bytes())?;
        w.flush()?;
        Ok(())
    }
}
