//! Readers for SAGA's own output (used by `saga stats`-on-demand and
//! `saga validate`). Streaming: shards are visited in name order and yielded
//! as columnar mini-batches, so validation runs on outputs larger than RAM.

use std::path::{Path, PathBuf};

use anyhow::Context;

/// One decoded read batch (event_type/label decoded to strings; numeric attr
/// columns by name, NaN = absent; categorical attrs by name as codes-as-text).
pub struct ReadBatch {
    pub src: Vec<u64>,
    pub dst: Vec<u64>,
    pub t: Vec<i64>,
    pub event_type: Vec<String>,
    pub label: Vec<String>,
    pub numeric_attrs: Vec<(String, Vec<f64>)>,
}

/// Find edge shards (parquet or csv) in an output directory, name-ordered.
pub fn edge_shards(dir: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let mut shards: Vec<PathBuf> = std::fs::read_dir(dir)
        .with_context(|| format!("cannot read `{}`", dir.display()))?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            name.starts_with("edges_") && (name.ends_with(".parquet") || name.ends_with(".csv"))
        })
        .collect();
    shards.sort();
    anyhow::ensure!(
        !shards.is_empty(),
        "no edge shards (edges_*.parquet / edges_*.csv) found in `{}`",
        dir.display()
    );
    Ok(shards)
}

/// Stream one shard, invoking `f` per batch.
pub fn read_shard(path: &Path, f: &mut dyn FnMut(ReadBatch) -> anyhow::Result<()>) -> anyhow::Result<()> {
    match path.extension().and_then(|e| e.to_str()) {
        Some("parquet") => read_parquet(path, f),
        Some("csv") => read_csv(path, f),
        other => anyhow::bail!("unsupported shard extension {other:?}"),
    }
}

fn read_parquet(path: &Path, f: &mut dyn FnMut(ReadBatch) -> anyhow::Result<()>) -> anyhow::Result<()> {
    use arrow_array::cast::AsArray;
    use arrow_array::types::{Float64Type, Int64Type, UInt16Type, UInt64Type};
    use arrow_array::Array as _;
    use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;

    let file = std::fs::File::open(path)?;
    let reader = ParquetRecordBatchReaderBuilder::try_new(file)?.with_batch_size(65_536).build()?;
    for rb in reader {
        let rb = rb?;
        let n = rb.num_rows();
        let get_dict_strings = |name: &str| -> Vec<String> {
            let col = rb.column_by_name(name).expect("schema fixed");
            let dict = col.as_dictionary::<UInt16Type>();
            let values = dict.values().as_string::<i32>();
            dict.keys()
                .iter()
                .map(|k| values.value(k.expect("non-null") as usize).to_string())
                .collect()
        };
        let mut numeric_attrs = Vec::new();
        for field in rb.schema().fields() {
            if field.data_type() == &arrow_schema::DataType::Float64
                && !["src", "dst", "t"].contains(&field.name().as_str())
            {
                let col = rb.column_by_name(field.name()).expect("by name");
                let arr = col.as_primitive::<Float64Type>();
                let vals: Vec<f64> = (0..n)
                    .map(|i| if arr.is_null(i) { f64::NAN } else { arr.value(i) })
                    .collect();
                numeric_attrs.push((field.name().clone(), vals));
            }
        }
        f(ReadBatch {
            src: rb.column_by_name("src").expect("src").as_primitive::<UInt64Type>().values().to_vec(),
            dst: rb.column_by_name("dst").expect("dst").as_primitive::<UInt64Type>().values().to_vec(),
            t: rb.column_by_name("t").expect("t").as_primitive::<Int64Type>().values().to_vec(),
            event_type: get_dict_strings("event_type"),
            label: get_dict_strings("label"),
            numeric_attrs,
        })?;
    }
    Ok(())
}

fn read_csv(path: &Path, f: &mut dyn FnMut(ReadBatch) -> anyhow::Result<()>) -> anyhow::Result<()> {
    use std::io::BufRead;
    let file = std::fs::File::open(path)?;
    let mut lines = std::io::BufReader::with_capacity(1 << 20, file).lines();
    let header = lines
        .next()
        .ok_or_else(|| anyhow::anyhow!("`{}` is empty", path.display()))??;
    let cols: Vec<String> = header.split(',').map(str::to_string).collect();
    anyhow::ensure!(
        cols.len() >= 6 && cols[..6] == ["src", "dst", "t", "event_type", "label", "anomaly_id"],
        "`{}` does not look like a SAGA edges CSV",
        path.display()
    );
    let attr_cols: Vec<String> = cols[6..].to_vec();

    const BATCH: usize = 65_536;
    let mut batch = new_batch(&attr_cols);
    for line in lines {
        let line = line?;
        let mut it = line.split(',');
        batch.src.push(it.next().unwrap_or("0").parse().unwrap_or(u64::MAX));
        batch.dst.push(it.next().unwrap_or("0").parse().unwrap_or(u64::MAX));
        batch.t.push(it.next().unwrap_or("0").parse().unwrap_or(i64::MIN));
        batch.event_type.push(it.next().unwrap_or("").to_string());
        batch.label.push(it.next().unwrap_or("").to_string());
        let _ = it.next(); // anomaly_id column (not needed for validation)
        for (a, _) in attr_cols.iter().enumerate() {
            let raw = it.next().unwrap_or("");
            batch.numeric_attrs[a].1.push(raw.parse::<f64>().unwrap_or(f64::NAN));
        }
        if batch.src.len() >= BATCH {
            f(std::mem::replace(&mut batch, new_batch(&attr_cols)))?;
        }
    }
    if !batch.src.is_empty() {
        f(batch)?;
    }
    Ok(())
}

fn new_batch(attr_cols: &[String]) -> ReadBatch {
    ReadBatch {
        src: Vec::new(),
        dst: Vec::new(),
        t: Vec::new(),
        event_type: Vec::new(),
        label: Vec::new(),
        numeric_attrs: attr_cols.iter().map(|c| (c.clone(), Vec::new())).collect(),
    }
}

/// Read one numeric column of an exported node table: the FINAL value of
/// attribute/state variable `var` for every node of `entity_type`, in id
/// order as written. Looks for `nodes_<entity_type>.parquet` first, then the
/// rows of `nodes.csv` whose `entity_type` matches. `Ok(None)` when no node
/// table exists or the column is absent (e.g. an edge-only sharded run).
/// CSV blanks (NaN at write time) read back as NaN.
///
/// `position` is the 0-based index of `var` within the entity type's
/// exported column list (declared attributes first, then state variables, as
/// `World::node_batches` writes them). `nodes.csv` carries ONE header (that
/// of the first entity type written), so for any other type the column can
/// only be located positionally; parquet node tables are resolved by name.
pub fn node_column(
    dir: &Path,
    entity_type: &str,
    var: &str,
    position: Option<usize>,
) -> anyhow::Result<Option<Vec<f64>>> {
    let pq = dir.join(format!("nodes_{entity_type}.parquet"));
    if pq.exists() {
        return node_column_parquet(&pq, var);
    }
    let csv = dir.join("nodes.csv");
    if csv.exists() {
        return node_column_csv(&csv, entity_type, var, position);
    }
    Ok(None)
}

fn node_column_parquet(path: &Path, var: &str) -> anyhow::Result<Option<Vec<f64>>> {
    use arrow_array::cast::AsArray;
    use arrow_array::types::Float64Type;
    use arrow_array::Array as _;
    use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;

    let file = std::fs::File::open(path).with_context(|| format!("open `{}`", path.display()))?;
    let builder = ParquetRecordBatchReaderBuilder::try_new(file)?;
    let Some(idx) = builder.schema().fields().iter().position(|f| f.name() == var) else {
        return Ok(None);
    };
    if builder.schema().field(idx).data_type() != &arrow_schema::DataType::Float64 {
        return Ok(None);
    }
    let mask = parquet::arrow::ProjectionMask::roots(builder.parquet_schema(), [idx]);
    let reader = builder.with_projection(mask).with_batch_size(65_536).build()?;
    let mut out = Vec::new();
    for rb in reader {
        let rb = rb?;
        let arr = rb.column(0).as_primitive::<Float64Type>();
        out.extend((0..rb.num_rows()).map(|i| if arr.is_null(i) { f64::NAN } else { arr.value(i) }));
    }
    Ok(Some(out))
}

fn node_column_csv(
    path: &Path,
    entity_type: &str,
    var: &str,
    position: Option<usize>,
) -> anyhow::Result<Option<Vec<f64>>> {
    use std::io::BufRead;
    let file = std::fs::File::open(path).with_context(|| format!("open `{}`", path.display()))?;
    let mut lines = std::io::BufReader::with_capacity(1 << 20, file).lines();
    let header = lines
        .next()
        .ok_or_else(|| anyhow::anyhow!("`{}` is empty", path.display()))??;
    let cols: Vec<&str> = header.split(',').collect();
    anyhow::ensure!(
        cols.len() >= 2 && cols[..2] == ["id", "entity_type"],
        "`{}` does not look like a SAGA nodes CSV",
        path.display()
    );
    // nodes.csv is appended per entity type under the FIRST type's header;
    // prefer the caller's positional index, fall back to the header name.
    let ci = match position {
        Some(p) => p + 2,
        None => match cols.iter().position(|c| *c == var) {
            Some(ci) => ci,
            None => return Ok(None),
        },
    };
    let mut out = Vec::new();
    let mut seen_type = false;
    for line in lines {
        let line = line?;
        let mut it = line.split(',');
        let _id = it.next();
        if it.next() != Some(entity_type) {
            continue;
        }
        seen_type = true;
        let raw = it.nth(ci - 2).unwrap_or("");
        out.push(raw.parse::<f64>().unwrap_or(f64::NAN));
    }
    Ok(if seen_type { Some(out) } else { None })
}
