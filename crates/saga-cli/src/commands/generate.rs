//! `saga generate` — the main command (blueprint §12).
//!
//! Flow: config (file + flag overrides) -> rule base -> host probe -> cost
//! model GUARD -> [dry-run stops here] -> meta -> engine -> summary.

use std::path::PathBuf;

use clap::Args;
use owo_colors::OwoColorize;
use saga_host::cost::{human_bytes, human_duration, CostModel, Feasibility, RunRequest};
use saga_host::{CostEstimate, ExecutionMode, HostProbe};
use saga_rules::{
    load_builtin_rulebase, GenerationConfig, OutputFormat, Preset, RunMeta,
};

use super::Ctx;

#[derive(Args, Debug)]
pub struct GenerateArgs {
    /// YAML config file (flags below override its fields).
    #[arg(long)]
    pub config: Option<PathBuf>,
    /// Built-in domain id or rule-base path.
    #[arg(long)]
    pub domain: Option<String>,
    /// Size preset: tiny | small | medium | large | huge.
    #[arg(long)]
    pub preset: Option<String>,
    #[arg(long)]
    pub nodes: Option<u64>,
    #[arg(long)]
    pub edges: Option<u64>,
    /// Simulated time span in days.
    #[arg(long = "time-span")]
    pub time_span_days: Option<f64>,
    /// Timestamp granularity in seconds.
    #[arg(long)]
    pub granularity: Option<u64>,
    /// Fraction of nodes participating in anomalous processes [0, 0.5].
    #[arg(long = "anomaly-rate")]
    pub anomaly_rate: Option<f64>,
    /// Anomaly difficulty/camouflage [0, 1].
    #[arg(long)]
    pub difficulty: Option<f64>,
    /// Axis 3 — type mix: per-intent weight overrides, e.g.
    /// `--type-mix structuring=0.6,layering=0.4` (unlisted intents keep their
    /// rule-base weight). Repeatable / comma-separated.
    #[arg(long = "type-mix", value_delimiter = ',')]
    pub type_mix: Vec<String>,
    /// Axis 5 — cascade multiplier on per-process cascade probability.
    #[arg(long)]
    pub cascade: Option<f64>,
    /// Axis 4 — number of communities anomalies cluster into (clustered
    /// placement). Overrides the rule-base community count.
    #[arg(long)]
    pub communities: Option<u32>,
    /// Disable anomaly processes entirely (normal behavior only).
    #[arg(long)]
    pub no_anomalies: bool,
    #[arg(long)]
    pub seed: Option<u64>,
    /// Output directory.
    #[arg(long)]
    pub out: Option<PathBuf>,
    /// parquet | csv | graphml.
    #[arg(long)]
    pub format: Option<String>,
    #[arg(long)]
    pub threads: Option<usize>,
    /// Force-enable the GPU path.
    #[arg(long)]
    pub gpu: bool,
    /// Force-disable the GPU path.
    #[arg(long)]
    pub no_gpu: bool,
    /// RAM budget in GiB.
    #[arg(long = "mem-budget")]
    pub mem_budget_gb: Option<f64>,
    /// Shard size in MiB.
    #[arg(long = "shard-size")]
    pub shard_size_mb: Option<u64>,
    /// Estimate cost and feasibility, then exit without generating.
    #[arg(long)]
    pub dry_run: bool,
    /// Distributed sharding: this shard's index in [0, shard-count).
    #[arg(long, default_value_t = 0)]
    pub shard_index: u64,
    /// Distributed sharding: total number of shards (1 = whole-graph run).
    /// Run the SAME command on each machine with the same seed/config and a
    /// different --shard-index; the disjoint outputs union to the whole graph.
    #[arg(long, default_value_t = 1)]
    pub shard_count: u64,
    /// Staleness/merge block length L in days (consistency model §6, §8).
    /// Behavior reads state as of the block start, so a read is stale by at most
    /// L. Values >= the time span are one block (option A: reads see the initial
    /// state only). Default 1 day.
    #[arg(long = "block-len-days", default_value_t = 1.0)]
    pub block_len_days: f64,
    /// Enable event state guards (e.g. the finance overdraft guard). Output then
    /// depends on L via the guards; with the flag off, output is independent of
    /// L (the RNG/L decoupling proof).
    #[arg(long = "guards")]
    pub guards: bool,
    /// Guard-aware anomaly calibration (consistency model section 17). With
    /// --guards, guarded event types are suppressed differentially (an
    /// anomalous structuring-sized transfer exceeds a typical balance more
    /// often than a normal one), so the realized anomalous-edge fraction
    /// drifts from the planned one. This flag runs PASSES (default 1)
    /// measurement passes of the full guarded pipeline first and rescales
    /// each event type's campaign rate by the measured background-vs-campaign
    /// survival ratio, so the realized fraction lands back on the guards-off
    /// (planned) fraction. Deterministic (a pure function of seed/params;
    /// thread-count independent). Costs one extra generation per pass.
    /// Requires --guards and a whole-graph run (no --shard-count).
    #[arg(long = "calibrate-under-guards", value_name = "PASSES", num_args = 0..=1, default_missing_value = "1")]
    pub calibrate_under_guards: Option<u32>,
    /// Shared directory for the per-block cross-shard state exchange (real
    /// multi-process sharded runs). Give the SAME fresh, empty directory (e.g.
    /// on a shared filesystem) to all N shard processes, started concurrently
    /// with identical seed/params. Required with --guards --shard-count > 1;
    /// with guards off it additionally makes each shard's exported node state
    /// complete (otherwise cross-shard in-edge effects are missing).
    #[arg(long = "exchange-dir")]
    pub exchange_dir: Option<PathBuf>,
    /// Seconds to wait for a peer shard's per-block delta file before failing
    /// with an error instead of hanging (a missing or crashed peer).
    #[arg(long = "exchange-timeout-s", default_value_t = 300)]
    pub exchange_timeout_s: u64,
}

pub fn run(ctx: &Ctx, args: GenerateArgs) -> anyhow::Result<()> {
    // Fail fast on a nonsensical staleness bound (0, negative, NaN, inf):
    // the engine refuses it too, but catching it here means no output
    // directory or meta file is ever created for a doomed run.
    if !(args.block_len_days.is_finite() && args.block_len_days > 0.0) {
        anyhow::bail!(
            "--block-len-days must be a finite value > 0 (got {}); values >= \
             the time span mean one whole-run block (option A), values below \
             one second floor at the 1 s engine granularity",
            args.block_len_days
        );
    }
    // Correctness rails (fail fast, before any output is created); the engine
    // enforces the same rules (saga-core sim::run).
    //   * Guarded sharded runs need the per-block cross-shard state exchange:
    //     without it each shard reads state missing the dst-effects of other
    //     shards' edges and would silently produce wrong edges and wrong node
    //     state.
    //   * The exchange only makes sense between shard processes.
    if args.guards && args.shard_count > 1 && args.exchange_dir.is_none() {
        anyhow::bail!(
            "--guards with --shard-count {} requires --exchange-dir <shared \
             directory>: the N shard processes exchange their per-block \
             cross-shard state deltas through it (give the SAME fresh directory \
             to every shard process and start them concurrently). Without the \
             exchange, guarded generation would silently produce wrong edges and \
             wrong node state. Alternatively drop --guards for edge-only sharded \
             generation.",
            args.shard_count
        );
    }
    if args.exchange_dir.is_some() && args.shard_count <= 1 {
        anyhow::bail!(
            "--exchange-dir requires --shard-count > 1: the exchange is the \
             cross-process protocol between shard processes; a whole-graph run \
             has no peers to exchange with"
        );
    }
    let calibrate_passes = args.calibrate_under_guards.unwrap_or(0);
    if calibrate_passes > 0 && !args.guards {
        anyhow::bail!(
            "--calibrate-under-guards requires --guards: the calibration measures how \
             the state guards suppress anomalous vs normal candidates; with guards \
             off the output already realizes the planned anomaly rate"
        );
    }
    if calibrate_passes > 0 && args.shard_count > 1 {
        anyhow::bail!(
            "--calibrate-under-guards requires a whole-graph run (no --shard-count): a \
             shard measures only its own slice of the stream, so per-shard \
             calibrations would disagree and the shards would no longer union to \
             one graph"
        );
    }
    let mut cfg = resolve_config(&args)?;
    cfg.validate()?;

    // Rule base must load before anything else (fail fast on bad domain).
    let rulebase = load_builtin_rulebase(&cfg.domain)?;

    // Probe + guard (§11a: never a silent OOM or disk-full crash).
    let probe = HostProbe::probe(&cfg.output.path);
    let est = CostModel::default().estimate(
        &RunRequest {
            nodes: cfg.scale.nodes,
            edges: cfg.scale.edges,
            threads: cfg.runtime.threads,
            bytes_per_edge_disk: cfg.output.format.bytes_per_edge(),
            mem_budget: cfg
                .runtime
                .mem_budget_gb
                .map(|g| (g * 1024.0 * 1024.0 * 1024.0) as u64),
        },
        &probe,
    );

    if args.dry_run {
        return print_dry_run(ctx, &cfg, &est);
    }

    match &est.feasibility {
        Feasibility::Infeasible(reason) => {
            anyhow::bail!("this run does not fit this host: {reason} (see `saga generate --dry-run`)")
        }
        Feasibility::Warn(ws) => {
            for w in ws {
                ctx.say(format!("{} {w}", "warning:".yellow()));
            }
        }
        Feasibility::Ok => {}
    }

    // Under sharding, each shard writes into its own subdirectory so files
    // never collide; the union of shard_*/edges_* is the whole edge set.
    if args.shard_count > 1 {
        if args.shard_index >= args.shard_count {
            anyhow::bail!(
                "--shard-index {} must be < --shard-count {}",
                args.shard_index,
                args.shard_count
            );
        }
        cfg.output.path = cfg.output.path.join(format!("shard_{:02}", args.shard_index));
        ctx.say(format!(
            "shard   {}/{} -> {}",
            args.shard_index,
            args.shard_count,
            cfg.output.path.display()
        ));
    }

    // Reproducibility record before the run starts.
    let mut meta = RunMeta::new(&cfg, serde_json::to_value(&probe)?);
    let meta_path = meta.write(&cfg.output.path)?;
    ctx.say(format!("meta    {}", meta_path.display()));

    let params = saga_core::GenParams {
        rulebase,
        nodes: cfg.scale.nodes,
        target_edges: cfg.scale.edges,
        span_days: cfg.time.span_days,
        granularity_s: cfg.time.granularity_s,
        epoch_unix: cfg.time.epoch_unix,
        seed: cfg.seed,
        threads: est.threads,
        block_len_days: args.block_len_days,
        guards_enabled: args.guards,
        anomaly_rate: cfg.anomaly.rate,
        anomaly_difficulty: cfg.anomaly.difficulty,
        anomaly_type_mix: parse_type_mix(&args.type_mix)?,
        anomaly_cascade: args.cascade,
        anomaly_communities: args.communities,
        anomalies_disabled: cfg.anomaly.disabled,
        shard_index: args.shard_index,
        shard_count: args.shard_count.max(1),
        exchange_dir: args.exchange_dir.clone(),
        exchange_timeout_s: args.exchange_timeout_s,
        calibrate_passes,
    };

    // Categorical event-attr dictionaries (decode codes to strings on write).
    // Must be the engine's own union dictionaries — same codes, same order.
    let attr_dicts = saga_core::world::union_attr_dictionaries(&params.rulebase);
    let event_type_names: Vec<String> =
        params.rulebase.event_types.iter().map(|e| e.name.clone()).collect();
    let intent_names: Vec<String> = {
        let mut v = vec!["normal".to_string()];
        v.extend(params.rulebase.adversaries.iter().map(|a| a.intent.clone()));
        v.extend(params.rulebase.failures.iter().map(|f| f.intent.clone()));
        v
    };
    let format_str = match cfg.output.format {
        OutputFormat::Parquet => "parquet",
        OutputFormat::Csv => "csv",
        OutputFormat::Graphml => "graphml",
    };
    let writer = saga_io::FormatSink::make(
        format_str,
        cfg.output.path.clone(),
        cfg.output.shard_size_mb,
        event_type_names.clone(),
        intent_names.clone(),
        attr_dicts.clone(),
    )?;
    // The guarded L-block sweep delivers one batch per block; a small L means
    // thousands of tiny batches, and Parquet pays a fixed per-encode cost per
    // delivered batch (measured: +23% wall at L = 0.01 vs L = 1 at 1e7
    // edges, while CSV moves ~2%). Coalesce tiny batches into 128k-row
    // chunks BEFORE the writer channel (saga_io::ThreadedSink docs) so the
    // writer thread encodes a few large batches that overlap generation.
    // Block boundaries are a semantics concept, not an IO concept; the
    // logical row stream is unchanged. Gated to guarded parquet runs only:
    // the guards-off default path's output bytes are frozen (byte-compat
    // reference hashes) and stay on the historical write path, and CSV /
    // GraphML have no per-batch encode cost worth the reshaping.
    let coalesce_rows = if args.guards && matches!(cfg.output.format, OutputFormat::Parquet) {
        1usize << 17
    } else {
        0
    };
    // Tee: streaming stats (single pass §11b) on its own thread + the file
    // writer on a background thread, both fed the SAME batch allocation
    // (consistency model section 15), so the engine's main thread does neither
    // the per-row statistics fold nor a per-consumer row copy; the engine
    // gives each batch away and moves on to the next day.
    let stats = saga_introspect::StatsCollector::new(
        cfg.scale.nodes,
        event_type_names,
        intent_names,
        attr_dicts,
    );
    let mut sink = SharedTee {
        stats: saga_introspect::ThreadedStats::new(stats, 3),
        writer: saga_io::ThreadedSink::with_coalescing(writer, 3, coalesce_rows),
    };

    // Live progress from the engine's own telemetry (§12).
    let started = std::time::Instant::now();
    let quiet = ctx.quiet || ctx.json;
    let mut last_print = std::time::Instant::now();
    let mut on_progress = move |edges: u64, frac: f64| {
        if quiet || last_print.elapsed().as_millis() < 500 {
            return;
        }
        last_print = std::time::Instant::now();
        let elapsed = started.elapsed().as_secs_f64();
        let rate = edges as f64 / elapsed.max(1e-9);
        let eta = if frac > 0.0 { elapsed / frac * (1.0 - frac) } else { 0.0 };
        eprint!(
            "\r  generating  {:>3.0}%  {} edges  {:.2}M ev/s  ETA {}   ",
            frac * 100.0,
            edges,
            rate / 1e6,
            human_duration(eta)
        );
    };

    let summary = saga_core::generate(&params, &mut sink, Some(&mut on_progress))?;
    if !quiet {
        eprintln!();
    }

    // The omniscient ground-truth record: one entry per anomaly instance
    // (campaign/incident) with its typology, difficulty, community, window and
    // member nodes. Edges join to it via their `anomaly_id` column. This is
    // multi-attribute ground truth — free & exact because the simulator IS the
    // mechanism (§3, §11b) — and what a real-data curator can never provide.
    std::fs::write(
        cfg.output.path.join("ground_truth.json"),
        serde_json::to_string_pretty(&serde_json::json!({
            "intent_names": summary.intent_names,
            "edges_per_intent": summary.intent_names.iter().cloned()
                .zip(summary.edges_per_intent.iter().copied()).collect::<Vec<_>>(),
            "instances": summary.ground_truth,
        }))?,
    )?;
    // Node-level ground truth: one row per (anomaly actor, instance), so a
    // detector benchmark has per-NODE labels too (not just per-edge). Rings
    // are small (≤ the rule base's ring_size), so the per-instance member
    // list is complete in practice.
    {
        use std::io::Write as _;
        let path = cfg.output.path.join("labels_nodes.csv");
        let mut w = std::io::BufWriter::new(std::fs::File::create(&path)?);
        writeln!(w, "node_id,intent,kind,anomaly_id,camouflage,community")?;
        for inst in &summary.ground_truth {
            for &node in &inst.members {
                let cam = if inst.camouflage.is_nan() {
                    String::new()
                } else {
                    format!("{:.4}", inst.camouflage)
                };
                writeln!(
                    w,
                    "{node},{},{},{},{cam},{}",
                    inst.intent, inst.kind, inst.id, inst.community
                )?;
            }
        }
        w.flush()?;
    }

    // Exclude the (potentially large) per-instance records from the summary
    // embedded in meta; they live in ground_truth.json.
    let mut summary_for_meta = summary.clone();
    summary_for_meta.ground_truth = Vec::new();
    meta.result = Some(serde_json::to_value(&summary_for_meta)?);
    meta.write(&cfg.output.path)?;

    // Introspection artifacts: stats JSON + Prometheus exposition (§11b).
    let report = sink.stats.report()?;
    std::fs::write(
        cfg.output.path.join("saga_stats.json"),
        serde_json::to_string_pretty(&report)?,
    )?;
    std::fs::write(
        cfg.output.path.join("metrics.prom"),
        prometheus_exposition(&summary, &report),
    )?;

    if ctx.json {
        println!("{}", serde_json::to_string_pretty(&summary)?);
    } else {
        print_summary(ctx, &summary);
    }
    Ok(())
}

/// The generate command's sink: one shared `Arc<EventBatch>` per delivered
/// batch, fanned out in order to the stats thread and the writer thread.
/// Each consumer sees exactly the batch sequence `TeeSink` would have given
/// it; only the copies and the thread doing the stats fold changed.
struct SharedTee {
    stats: saga_introspect::ThreadedStats,
    writer: saga_io::ThreadedSink,
}

impl SharedTee {
    fn push(&mut self, b: std::sync::Arc<saga_core::EventBatch>) -> anyhow::Result<()> {
        self.stats.push_shared(b.clone())?;
        self.writer.push_shared(b)
    }
}

impl saga_core::EventSink for SharedTee {
    fn write_batch(&mut self, batch: &saga_core::EventBatch) -> anyhow::Result<()> {
        self.push(std::sync::Arc::new(batch.clone()))
    }
    fn write_batch_owned(&mut self, batch: saga_core::EventBatch) -> anyhow::Result<()> {
        self.push(std::sync::Arc::new(batch))
    }
    fn write_nodes(&mut self, batch: &saga_core::NodeBatch) -> anyhow::Result<()> {
        self.stats.write_nodes(batch)?;
        self.writer.write_nodes(batch)
    }
    fn finish(&mut self) -> anyhow::Result<()> {
        self.stats.finish()?;
        self.writer.finish()
    }
}

/// Parse `intent=weight` pairs from the `--type-mix` flag into (intent, weight)
/// tuples, with actionable errors (§13).
fn parse_type_mix(items: &[String]) -> anyhow::Result<Vec<(String, f64)>> {
    items
        .iter()
        .map(|s| {
            let (intent, w) = s.split_once('=').ok_or_else(|| {
                anyhow::anyhow!("--type-mix entry `{s}` must be `intent=weight`")
            })?;
            let weight: f64 = w
                .trim()
                .parse()
                .map_err(|_| anyhow::anyhow!("--type-mix `{s}`: `{w}` is not a number"))?;
            anyhow::ensure!(weight >= 0.0, "--type-mix `{s}`: weight must be ≥ 0");
            Ok((intent.trim().to_string(), weight))
        })
        .collect()
}

/// Merge precedence: defaults < preset < config file < explicit flags.
fn resolve_config(args: &GenerateArgs) -> anyhow::Result<GenerationConfig> {
    let mut cfg = match &args.config {
        Some(p) => GenerationConfig::load(p)?,
        None => {
            let domain = args.domain.clone().unwrap_or_else(|| "finance".into());
            let preset = match &args.preset {
                Some(s) => Preset::parse(s).ok_or_else(|| {
                    anyhow::anyhow!(
                        "unknown preset `{s}`: expected one of {}",
                        Preset::ALL.map(|p| p.name()).join(", ")
                    )
                })?,
                None => Preset::Small,
            };
            GenerationConfig::scaffold(&domain, preset)
        }
    };
    if args.config.is_some() {
        if let Some(s) = &args.preset {
            let preset = Preset::parse(s)
                .ok_or_else(|| anyhow::anyhow!("unknown preset `{s}`"))?;
            let (n, e, d) = preset.baseline();
            cfg.scale.nodes = n;
            cfg.scale.edges = e;
            cfg.time.span_days = d;
        }
        if let Some(d) = &args.domain {
            cfg.domain = d.clone();
        }
    }
    if let Some(v) = args.nodes {
        cfg.scale.nodes = v;
    }
    if let Some(v) = args.edges {
        cfg.scale.edges = v;
    }
    if let Some(v) = args.time_span_days {
        cfg.time.span_days = v;
    }
    if let Some(v) = args.granularity {
        cfg.time.granularity_s = v;
    }
    if let Some(v) = args.anomaly_rate {
        cfg.anomaly.rate = Some(v);
    }
    if let Some(v) = args.difficulty {
        cfg.anomaly.difficulty = Some(v);
    }
    if args.no_anomalies {
        cfg.anomaly.disabled = true;
    }
    if let Some(v) = args.seed {
        cfg.seed = v;
    }
    if let Some(v) = &args.out {
        cfg.output.path = v.clone();
    }
    if let Some(f) = &args.format {
        cfg.output.format = match f.as_str() {
            "parquet" => OutputFormat::Parquet,
            "csv" => OutputFormat::Csv,
            "graphml" => OutputFormat::Graphml,
            other => anyhow::bail!("unknown format `{other}`: expected parquet, csv or graphml"),
        };
    }
    if let Some(v) = args.threads {
        cfg.runtime.threads = Some(v);
    }
    if args.gpu && args.no_gpu {
        anyhow::bail!("--gpu and --no-gpu are mutually exclusive");
    }
    if args.gpu {
        cfg.runtime.gpu = Some(true);
    }
    if args.no_gpu {
        cfg.runtime.gpu = Some(false);
    }
    if let Some(v) = args.mem_budget_gb {
        cfg.runtime.mem_budget_gb = Some(v);
    }
    if let Some(v) = args.shard_size_mb {
        cfg.output.shard_size_mb = v;
    }
    Ok(cfg)
}

fn print_dry_run(ctx: &Ctx, cfg: &GenerationConfig, est: &CostEstimate) -> anyhow::Result<()> {
    if ctx.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "config": cfg, "estimate": est,
            }))?
        );
        return Ok(());
    }
    println!("{}", "saga generate --dry-run (nothing will be generated)".bold());
    println!("  domain      {}", cfg.domain);
    println!("  scale       {} nodes, {} edges over {} days", cfg.scale.nodes, cfg.scale.edges, cfg.time.span_days);
    println!("  est. time   ~{}", human_duration(est.est_wall_time_s).bold());
    println!("  est. RAM    ~{} peak", human_bytes(est.peak_ram_bytes));
    println!("  est. disk   ~{} ({:?} format)", human_bytes(est.disk_bytes), cfg.output.format);
    println!("  threads     {}", est.threads);
    println!(
        "  mode        {}",
        match est.mode {
            ExecutionMode::InRam => "in-RAM (output buffered, written at end)",
            ExecutionMode::StreamToDisk => "stream-to-disk (sharded as generated)",
        }
    );
    for n in &est.notes {
        println!("  note        {n}");
    }
    match &est.feasibility {
        Feasibility::Ok => println!("  verdict     {}", "fits this host".green().bold()),
        Feasibility::Warn(ws) => {
            for w in ws {
                println!("  warning     {}", w.yellow());
            }
            println!("  verdict     {}", "fits, with warnings".yellow().bold());
        }
        Feasibility::Infeasible(r) => {
            println!("  verdict     {} — {r}", "DOES NOT FIT".red().bold());
        }
    }
    Ok(())
}

/// System self-metrics in Prometheus exposition format (§11b).
fn prometheus_exposition(
    s: &saga_core::GenSummary,
    r: &saga_introspect::StatsReport,
) -> String {
    let mut out = String::new();
    use std::fmt::Write as _;
    let _ = writeln!(out, "# TYPE saga_edges_total counter\nsaga_edges_total {}", s.edges_written);
    let _ = writeln!(
        out,
        "# TYPE saga_anomalous_edges_total counter\nsaga_anomalous_edges_total {}",
        s.anomalous_edges
    );
    let _ = writeln!(
        out,
        "# TYPE saga_events_per_second gauge\nsaga_events_per_second {:.0}",
        s.events_per_sec
    );
    let _ = writeln!(
        out,
        "# TYPE saga_wall_time_seconds gauge\nsaga_wall_time_seconds {:.3}",
        s.wall_time_s
    );
    for (intent, count) in &r.label_introspection {
        let _ = writeln!(out, "saga_edges_by_intent{{intent=\"{intent}\"}} {count}");
    }
    for (et, count) in &r.events_per_event_type {
        let _ = writeln!(out, "saga_edges_by_event_type{{event_type=\"{et}\"}} {count}");
    }
    out
}

fn print_summary(ctx: &Ctx, s: &saga_core::GenSummary) {
    ctx.say(format!(
        "done    {} edges ({} anomalous, {:.3}%) in {} — {:.2}M events/s",
        s.edges_written,
        s.anomalous_edges,
        100.0 * s.anomalous_edges as f64 / s.edges_written.max(1) as f64,
        human_duration(s.wall_time_s),
        s.events_per_sec / 1e6,
    ));
    for (i, name) in s.intent_names.iter().enumerate().skip(1) {
        ctx.say(format!("  intent  {:<18} {}", name, s.edges_per_intent.get(i).copied().unwrap_or(0)));
    }
    if s.overdraft_guarded > 0 {
        ctx.say(format!(
            "  guard   overdraft {}/{} emitted transfers overdrew on a stale read ({:.4}%)",
            s.overdraft_violations,
            s.overdraft_guarded,
            100.0 * s.overdraft_violations as f64 / s.overdraft_guarded as f64,
        ));
        // Planned vs realized under guards (section 17): the guards suppress
        // anomalous and normal candidates at different rates, so the realized
        // fraction can drift from the planned one unless calibrated.
        let realized = s.anomalous_edges as f64 / s.edges_written.max(1) as f64;
        if s.planned_anomalous_fraction.is_finite() {
            ctx.say(format!(
                "  anomaly planned {:.3}% vs realized {:.3}% under guards{}",
                100.0 * s.planned_anomalous_fraction,
                100.0 * realized,
                if s.calibration_passes == 0 {
                    " (uncalibrated; pass --calibrate-under-guards to close the gap)"
                } else {
                    ""
                },
            ));
        }
        if s.calibration_passes > 0 {
            ctx.say(format!(
                "  calib   {} pass(es): guards-off target {:.3}%, uncorrected {:.3}%, corrected {:.3}%",
                s.calibration_passes,
                100.0 * s.calibration_target_fraction,
                100.0 * s.calibration_uncorrected_fraction,
                100.0 * realized,
            ));
            for (ev, g) in &s.calibration_multipliers {
                ctx.say(format!("  calib   {:<18} x{:.4}", ev, g));
            }
        }
    }
    if s.exchange_rounds > 0 {
        ctx.say(format!(
            "  exchange {} barrier rounds; sent {} deltas ({} B), received {} deltas ({} B)",
            s.exchange_rounds,
            s.exchange_deltas_sent,
            s.exchange_bytes_sent,
            s.exchange_deltas_received,
            s.exchange_bytes_received,
        ));
        ctx.say(format!(
            "  exchange discovery: {} idle polls, {} nudges, {} skewed opens",
            s.exchange_idle_polls, s.exchange_nudges, s.exchange_skewed_opens,
        ));
    }
}
