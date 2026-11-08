//! saga-io: streaming shard writers + readers (blueprint §7).
//!
//! Writers consume the engine's columnar `EventBatch` and shard by size:
//! Parquet (primary), CSV, GraphML. Readers stream SAGA's own output back
//! for `saga stats`/`saga validate`.

pub mod csv;
pub mod graphml;
pub mod parquet;
pub mod read;
pub mod threaded;

pub use csv::CsvSink;
pub use graphml::GraphmlSink;
pub use parquet::ParquetSink;
pub use threaded::ThreadedSink;

use saga_core::{EventBatch, EventSink, NodeBatch};

/// Format-selected writer behind one concrete type.
///
/// The variants differ in size (Parquet carries Arrow builders) but exactly
/// one is constructed per run and lives for the whole run, so the size
/// disparity costs nothing — boxing would only add indirection on the write
/// path.
#[allow(clippy::large_enum_variant)]
pub enum FormatSink {
    Csv(CsvSink),
    Parquet(ParquetSink),
    Graphml(GraphmlSink),
}


/// Per-row end time: `t + duration` where the row's event type declares a
/// duration attribute (seconds) holding a finite non-negative value, else `t`.
pub(crate) fn end_times(b: &saga_core::EventBatch, dur_cols: &[Option<(usize, f64)>]) -> Vec<i64> {
    (0..b.len())
        .map(|i| {
            let t = b.t[i];
            match dur_cols.get(b.event_type[i] as usize).copied().flatten() {
                Some((c, unit)) => {
                    let d = b.attrs[c][i] * unit;
                    if d.is_finite() && d >= 0.0 { t + d.round() as i64 } else { t }
                }
                None => t,
            }
        })
        .collect()
}

/// Resolve per-event-type duration attribute names to union-column indices.
pub(crate) fn resolve_dur_cols(
    durations: &[Option<(String, f64)>],
    attr_names: &[String],
) -> Vec<Option<(usize, f64)>> {
    durations
        .iter()
        .map(|d| {
            d.as_ref()
                .and_then(|(n, unit)| attr_names.iter().position(|a| a == n).map(|c| (c, *unit)))
        })
        .collect()
}

/// Write `labels_nodes.csv`, the node-level ground truth: one row per
/// (anomaly actor, instance), complete (no member cap). `role` is `operator`
/// for the member that runs the scheme (hub centre or fan-in collector),
/// `affected` for a failure's entity, else `member`.
pub fn write_labels_nodes(
    path: &std::path::Path,
    instances: &[saga_core::AnomalyRecord],
) -> anyhow::Result<()> {
    use std::io::Write as _;
    let mut w = std::io::BufWriter::new(std::fs::File::create(path)?);
    writeln!(w, "node_id,intent,kind,anomaly_id,camouflage,community,role")?;
    for inst in instances {
        let cam = if inst.camouflage.is_nan() {
            String::new()
        } else {
            format!("{:.4}", inst.camouflage)
        };
        for &node in &inst.members {
            let role = if inst.kind == "failure" {
                "affected"
            } else if inst.operator == node as i64 {
                "operator"
            } else {
                "member"
            };
            writeln!(
                w,
                "{node},{},{},{},{cam},{},{role}",
                inst.intent, inst.kind, inst.id, inst.community
            )?;
        }
    }
    w.flush()?;
    Ok(())
}

impl FormatSink {
    /// Declare each event type's duration attribute and its unit in seconds
    /// (by event-type index), so
    /// the edges carry an end-time column `t_end`. Absent everywhere (the
    /// default), no `t_end` column is written.
    pub fn set_event_durations(&mut self, durations: Vec<Option<(String, f64)>>) {
        match self {
            FormatSink::Csv(s) => s.set_event_durations(durations),
            FormatSink::Parquet(s) => s.set_event_durations(durations),
            FormatSink::Graphml(_) => {}
        }
    }

    /// `format`: "parquet" | "csv" | "graphml".
    pub fn make(
        format: &str,
        dir: std::path::PathBuf,
        shard_size_mb: u64,
        event_type_names: Vec<String>,
        intent_names: Vec<String>,
        attr_dicts: Vec<(String, Vec<String>)>,
    ) -> anyhow::Result<FormatSink> {
        Ok(match format {
            "parquet" => FormatSink::Parquet(ParquetSink::new(
                dir, shard_size_mb, event_type_names, intent_names, attr_dicts,
            )?),
            "csv" => FormatSink::Csv(CsvSink::new(
                dir, shard_size_mb, event_type_names, intent_names, attr_dicts,
            )?),
            "graphml" => FormatSink::Graphml(GraphmlSink::new(
                dir, event_type_names, intent_names, attr_dicts,
            )?),
            other => anyhow::bail!("unknown format `{other}`: expected parquet, csv or graphml"),
        })
    }
}

impl EventSink for FormatSink {
    fn write_batch(&mut self, batch: &EventBatch) -> anyhow::Result<()> {
        match self {
            FormatSink::Csv(s) => s.write_batch(batch),
            FormatSink::Parquet(s) => s.write_batch(batch),
            FormatSink::Graphml(s) => s.write_batch(batch),
        }
    }
    fn write_nodes(&mut self, batch: &NodeBatch) -> anyhow::Result<()> {
        match self {
            FormatSink::Csv(s) => s.write_nodes(batch),
            FormatSink::Parquet(s) => s.write_nodes(batch),
            FormatSink::Graphml(s) => s.write_nodes(batch),
        }
    }
    fn write_table(&mut self, table: &saga_core::AuxTable) -> anyhow::Result<()> {
        match self {
            FormatSink::Csv(s) => s.write_table(table),
            FormatSink::Parquet(s) => s.write_table(table),
            FormatSink::Graphml(s) => s.write_table(table),
        }
    }
    fn finish(&mut self) -> anyhow::Result<()> {
        match self {
            FormatSink::Csv(s) => s.finish(),
            FormatSink::Parquet(s) => s.finish(),
            FormatSink::Graphml(s) => s.finish(),
        }
    }
}
