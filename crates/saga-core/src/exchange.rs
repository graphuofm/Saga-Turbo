//! Phase 4 (consistency model §4, §10, §12, §16): the REAL multi-process
//! per-block state exchange for sharded generation, over a shared filesystem
//! directory.
//!
//! Protocol, per L-block `b` (one barrier round per block, identical block
//! schedule on every shard because it is derived from params only):
//!   1. shard `k` generates its block-`b` events against its own `S_{b-1}`
//!      slice, then writes its outbound cross-shard dst-effect deltas to
//!      `<dir>/r<b>/b<b>_from<k>_to<j>.bin`, one file per peer `j` (possibly
//!      empty). The round directory `r<b>` is created by whoever arrives
//!      first. Files are written to a hidden temp name and atomically
//!      renamed, so a partial file is never observable under its final name.
//!   2. shard `k` waits (bounded by a timeout) until
//!      `b<b>_from<j>_to<k>.bin` is present for every peer `j`, reads each,
//!      and deletes it (each file has exactly one reader, so consumed deltas
//!      never accumulate). Discovery is by LISTING the round directory, not
//!      by opening a predicted name: see [`ExchangeFs`] and §16 of the
//!      consistency model (NFS negative-dentry caching made the old
//!      predicted-name `open()` poll stall for the attribute-cache timeout).
//!      Whoever consumes the last file of a round removes `r<b>`.
//!   3. shard `k` merges its local effects on owned nodes with the received
//!      deltas in the canonical order (`sim::fold_canonical`) to form its
//!      `S_b` slice, and proceeds to block `b+1`.
//!
//! A missing peer becomes a clear timeout error, never a hang. The wire
//! format is a compact fixed-width record; the receiving shard reconstructs
//! the effect (entity/var/kind/target) from `(event_type, effect_index)` in
//! its own compiled world, so only the canonical key and the resolved amount
//! travel.

use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Fixed wire size of one delta record, little-endian, in field order:
/// t (i64) + src (u64) + dst (u64) + event_type (u16) + effect_index (u8) +
/// amount (f64).
pub const WIRE_RECORD_BYTES: usize = 8 + 8 + 8 + 2 + 1 + 8;

/// One shipped effect delta. `(t, src, dst, event_type)` is the canonical
/// merge key; `effect_index` selects the effect inside the event type's
/// compiled effect list (which yields entity, var, kind and src/dst target);
/// `amount` is the resolved operand (attr value for Add/Sub, 1.0 for
/// Increment, the constant for Set).
#[derive(Clone, Copy, Debug)]
pub struct WireDelta {
    pub t: i64,
    pub src: u64,
    pub dst: u64,
    pub event_type: u16,
    pub effect_index: u8,
    pub amount: f64,
}

impl WireDelta {
    fn encode(&self, buf: &mut Vec<u8>) {
        buf.extend_from_slice(&self.t.to_le_bytes());
        buf.extend_from_slice(&self.src.to_le_bytes());
        buf.extend_from_slice(&self.dst.to_le_bytes());
        buf.extend_from_slice(&self.event_type.to_le_bytes());
        buf.push(self.effect_index);
        buf.extend_from_slice(&self.amount.to_le_bytes());
    }

    fn decode(rec: &[u8]) -> WireDelta {
        WireDelta {
            t: i64::from_le_bytes(rec[0..8].try_into().unwrap()),
            src: u64::from_le_bytes(rec[8..16].try_into().unwrap()),
            dst: u64::from_le_bytes(rec[16..24].try_into().unwrap()),
            event_type: u16::from_le_bytes(rec[24..26].try_into().unwrap()),
            effect_index: rec[26],
            amount: f64::from_le_bytes(rec[27..35].try_into().unwrap()),
        }
    }
}

/// The filesystem primitives the exchange needs, as a trait so tests can
/// inject a view with NFS-like caching pathologies (a listing that shows a
/// name whose `open()` still fails, negative lookups that stick). The real
/// implementation is [`RealFs`]; nothing else is ever used in production.
pub trait ExchangeFs: Send + Sync {
    /// Create `dir` (and missing parents). Succeeds if it already exists.
    /// Must never stat the directory after an `AlreadyExists` (a stale
    /// negative dentry could make that stat lie); `AlreadyExists` is success.
    fn create_dir(&self, dir: &Path) -> io::Result<()>;
    /// Write `bytes` to `tmp`, then atomically rename `tmp` to `dst`.
    fn publish(&self, tmp: &Path, dst: &Path, bytes: &[u8]) -> io::Result<()>;
    /// The entry names of `dir` (readdir); hidden temp/nudge names included,
    /// the caller filters.
    fn list(&self, dir: &Path) -> io::Result<Vec<OsString>>;
    /// Open and read a whole file.
    fn read(&self, path: &Path) -> io::Result<Vec<u8>>;
    fn remove_file(&self, path: &Path) -> io::Result<()>;
    /// Remove an EMPTY directory (`NotEmpty`/`NotFound` are tolerated by the
    /// caller: the last consumer of a round wins the race).
    fn remove_dir(&self, dir: &Path) -> io::Result<()>;
    /// Revalidation nudge: a cheap directory-mutating operation on `dir`
    /// (create and unlink a reader-private hidden file) that forces an NFS
    /// client to refresh the directory's attribute cache and drop stale
    /// negative dentries. Best effort, errors ignored.
    fn nudge(&self, dir: &Path, name: &str);
}

/// The production filesystem.
pub struct RealFs;

impl ExchangeFs for RealFs {
    fn create_dir(&self, dir: &Path) -> io::Result<()> {
        match std::fs::create_dir(dir) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => Ok(()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                if let Some(parent) = dir.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                match std::fs::create_dir(dir) {
                    Ok(()) => Ok(()),
                    Err(e) if e.kind() == io::ErrorKind::AlreadyExists => Ok(()),
                    Err(e) => Err(e),
                }
            }
            Err(e) => Err(e),
        }
    }
    fn publish(&self, tmp: &Path, dst: &Path, bytes: &[u8]) -> io::Result<()> {
        std::fs::write(tmp, bytes)?;
        std::fs::rename(tmp, dst)
    }
    fn list(&self, dir: &Path) -> io::Result<Vec<OsString>> {
        let mut names = Vec::new();
        for ent in std::fs::read_dir(dir)? {
            names.push(ent?.file_name());
        }
        Ok(names)
    }
    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        std::fs::read(path)
    }
    fn remove_file(&self, path: &Path) -> io::Result<()> {
        std::fs::remove_file(path)
    }
    fn remove_dir(&self, dir: &Path) -> io::Result<()> {
        std::fs::remove_dir(dir)
    }
    fn nudge(&self, dir: &Path, name: &str) {
        let p = dir.join(name);
        let _ = std::fs::File::create(&p);
        let _ = std::fs::remove_file(&p);
    }
}

/// Initial poll backoff (reset whenever a round makes progress).
const BACKOFF_MIN: Duration = Duration::from_millis(1);
/// Backoff cap while waiting for a late peer.
const BACKOFF_MAX: Duration = Duration::from_millis(32);
/// How long a round may wait before the per-poll revalidation nudge starts.
/// Below this the listing alone is retried (keeps local filesystems on the
/// cheapest path: a round whose peers are on time never nudges).
const NUDGE_AFTER: Duration = Duration::from_millis(4);

/// The per-shard endpoint of the filesystem exchange. One instance lives for
/// the whole run; [`Exchanger::round`] performs one per-block barrier.
pub struct Exchanger {
    dir: PathBuf,
    shard_index: u64,
    shard_count: u64,
    timeout: Duration,
    fs: Box<dyn ExchangeFs>,
    // Telemetry (reported in GenSummary).
    pub rounds: u64,
    pub deltas_sent: u64,
    pub bytes_sent: u64,
    pub deltas_received: u64,
    pub bytes_received: u64,
    /// Poll iterations that found nothing new (latency diagnostics).
    pub idle_polls: u64,
    /// Revalidation nudges performed (zero when every peer is on time).
    pub nudges: u64,
    /// Opens that failed with NotFound right after a listing showed the name
    /// (the NFS cache-skew case), retried rather than treated as absent.
    pub skewed_opens: u64,
}

impl Exchanger {
    pub fn new(
        dir: PathBuf,
        shard_index: u64,
        shard_count: u64,
        timeout_s: u64,
    ) -> anyhow::Result<Exchanger> {
        Exchanger::with_fs(dir, shard_index, shard_count, timeout_s, Box::new(RealFs))
    }

    /// Like [`Exchanger::new`] with an injected filesystem view (tests).
    pub fn with_fs(
        dir: PathBuf,
        shard_index: u64,
        shard_count: u64,
        timeout_s: u64,
        fs: Box<dyn ExchangeFs>,
    ) -> anyhow::Result<Exchanger> {
        anyhow::ensure!(shard_count > 1, "the exchange needs shard_count > 1");
        anyhow::ensure!(
            shard_index < shard_count,
            "shard_index {shard_index} must be < shard_count {shard_count}"
        );
        fs.create_dir(&dir).map_err(|e| {
            anyhow::anyhow!("cannot create exchange dir `{}`: {e}", dir.display())
        })?;
        Ok(Exchanger {
            dir,
            shard_index,
            shard_count,
            timeout: Duration::from_secs(timeout_s.max(1)),
            fs,
            rounds: 0,
            deltas_sent: 0,
            bytes_sent: 0,
            deltas_received: 0,
            bytes_received: 0,
            idle_polls: 0,
            nudges: 0,
            skewed_opens: 0,
        })
    }

    /// The round directory of `block`.
    fn round_dir(&self, block: i64) -> PathBuf {
        self.dir.join(format!("r{block}"))
    }

    fn file_name(block: i64, from: u64, to: u64) -> String {
        format!("b{block}_from{from}_to{to}.bin")
    }

    /// One barrier round for `block`: publish `outbound[j]` to every peer `j`
    /// (the slot for this shard itself is ignored), then wait for, read and
    /// delete every peer's inbound file. Returns the received deltas grouped
    /// by origin shard in ascending shard order (arrival order never matters).
    pub fn round(
        &mut self,
        block: i64,
        outbound: &[Vec<WireDelta>],
    ) -> anyhow::Result<Vec<Vec<WireDelta>>> {
        let k = self.shard_index;
        let n = self.shard_count;
        let rdir = self.round_dir(block);
        self.fs.create_dir(&rdir).map_err(|e| {
            anyhow::anyhow!("cannot create exchange round dir `{}`: {e}", rdir.display())
        })?;

        // 1. Publish outbound files (write hidden temp, then atomic rename).
        for j in 0..n {
            if j == k {
                continue;
            }
            let final_path = rdir.join(Self::file_name(block, k, j));
            let tmp_path = rdir.join(format!(".b{block}_from{k}_to{j}.tmp"));
            let deltas = &outbound[j as usize];
            let mut buf = Vec::with_capacity(deltas.len() * WIRE_RECORD_BYTES);
            for d in deltas {
                d.encode(&mut buf);
            }
            self.fs.publish(&tmp_path, &final_path, &buf).map_err(|e| {
                anyhow::anyhow!(
                    "cannot publish exchange file `{}`: {e}",
                    final_path.display()
                )
            })?;
            self.deltas_sent += deltas.len() as u64;
            self.bytes_sent += buf.len() as u64;
        }

        // 2. Barrier: discover every peer's inbound file by listing the round
        //    directory, read it, delete it. Names are only ever opened after
        //    a listing returned them; an open that still fails with NotFound
        //    (cache skew) is retried, never treated as absence.
        let start = Instant::now();
        let deadline = start + self.timeout;
        let mut inbound: Vec<Vec<WireDelta>> = (0..n).map(|_| Vec::new()).collect();
        let mut pending: Vec<(u64, OsString)> = (0..n)
            .filter(|&j| j != k)
            .map(|j| (j, OsString::from(Self::file_name(block, j, k))))
            .collect();
        let mut backoff = BACKOFF_MIN;
        let nudge_name = format!(".nudge{k}");
        while !pending.is_empty() {
            let names = self.fs.list(&rdir).map_err(|e| {
                anyhow::anyhow!("cannot list exchange round dir `{}`: {e}", rdir.display())
            })?;
            let mut progressed = false;
            let mut i = 0;
            while i < pending.len() {
                let (j, ref name) = pending[i];
                if !names.iter().any(|x| x == name) {
                    i += 1;
                    continue;
                }
                let path = rdir.join(name);
                match self.fs.read(&path) {
                    Ok(buf) => {
                        anyhow::ensure!(
                            buf.len() % WIRE_RECORD_BYTES == 0,
                            "corrupt exchange file `{}`: {} bytes is not a multiple of \
                             the {WIRE_RECORD_BYTES}-byte record (mixed runs sharing one \
                             --exchange-dir?)",
                            path.display(),
                            buf.len()
                        );
                        let deltas: Vec<WireDelta> = buf
                            .chunks_exact(WIRE_RECORD_BYTES)
                            .map(WireDelta::decode)
                            .collect();
                        self.deltas_received += deltas.len() as u64;
                        self.bytes_received += buf.len() as u64;
                        inbound[j as usize] = deltas;
                        // Consumed: this shard is the file's only reader.
                        let _ = self.fs.remove_file(&path);
                        pending.swap_remove(i);
                        progressed = true;
                    }
                    Err(e) if e.kind() == io::ErrorKind::NotFound => {
                        // Listed but not openable yet on this client: retry.
                        self.skewed_opens += 1;
                        i += 1;
                    }
                    Err(e) => {
                        anyhow::bail!("cannot read exchange file `{}`: {e}", path.display());
                    }
                }
            }
            if progressed {
                backoff = BACKOFF_MIN;
                continue;
            }
            let now = Instant::now();
            if now >= deadline {
                let (j, ref name) = pending[0];
                anyhow::bail!(
                    "shard {k}/{n}: timed out after {t}s waiting for the \
                     block-{block} delta file `{p}` from peer shard {j}. All \
                     {n} shard processes must run concurrently with identical \
                     seed/params and the same --exchange-dir; start the \
                     missing peer or raise --exchange-timeout-s.",
                    t = self.timeout.as_secs(),
                    p = rdir.join(name).display(),
                );
            }
            self.idle_polls += 1;
            if now - start >= NUDGE_AFTER {
                // Slow path only: force the client to revalidate the round
                // dir (NFS attribute cache / negative dentries), as the
                // cluster sidecar did.
                self.fs.nudge(&rdir, &nudge_name);
                self.nudges += 1;
            }
            std::thread::sleep(backoff.min(deadline.saturating_duration_since(now)));
            backoff = (backoff * 2).min(BACKOFF_MAX);
        }
        // 3. Whoever consumed the last file of the round removes its dir
        //    (fails harmlessly with NotEmpty for everyone else).
        let _ = self.fs.remove_dir(&rdir);
        self.rounds += 1;
        Ok(inbound)
    }

    /// Best-effort end-of-run tidy: remove the exchange dir if it is already
    /// empty (every shard tries; whoever finishes last succeeds).
    pub fn tidy(&self) {
        let _ = self.fs.remove_dir(&self.dir);
    }
}

#[cfg(test)]
mod tests {
    //! The NFS pathology, injected (§16). `CachingFs` is one client's view of
    //! a shared directory with the two defects measured on the cluster:
    //!   * negative-lookup caching: once `open()` on a name fails, this
    //!     client keeps answering NotFound for `neg_ttl`, even after the
    //!     peer's rename landed (the `acdirmin=30` stall: 15 s per round);
    //!   * listing/open skew: a name that a listing shows may still fail to
    //!     open on this client for `open_lag` after it first appeared (the
    //!     `ls` sees it, `wc -c` does not, observation on itiger03).
    //! A nudge (create+unlink in the dir) clears this client's negative
    //! entries for that dir, as the measurement sidecar did. Each shard gets
    //! its own `CachingFs` (its own client cache) over the same real dir.
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    struct CachingFs {
        real: RealFs,
        neg_ttl: Duration,
        open_lag: Duration,
        neg: Mutex<HashMap<PathBuf, Instant>>,
        first_seen: Mutex<HashMap<PathBuf, Instant>>,
    }

    impl CachingFs {
        fn new(neg_ttl: Duration, open_lag: Duration) -> CachingFs {
            CachingFs {
                real: RealFs,
                neg_ttl,
                open_lag,
                neg: Mutex::new(HashMap::new()),
                first_seen: Mutex::new(HashMap::new()),
            }
        }
        fn open_visible(&self, path: &Path) -> io::Result<()> {
            let now = Instant::now();
            if let Some(&until) = self.neg.lock().unwrap().get(path) {
                if now < until {
                    return Err(io::Error::from(io::ErrorKind::NotFound));
                }
            }
            if !path.exists() {
                self.neg
                    .lock()
                    .unwrap()
                    .insert(path.to_path_buf(), now + self.neg_ttl);
                return Err(io::Error::from(io::ErrorKind::NotFound));
            }
            let seen = *self
                .first_seen
                .lock()
                .unwrap()
                .entry(path.to_path_buf())
                .or_insert(now);
            if now < seen + self.open_lag {
                return Err(io::Error::from(io::ErrorKind::NotFound));
            }
            Ok(())
        }
    }

    impl ExchangeFs for CachingFs {
        fn create_dir(&self, dir: &Path) -> io::Result<()> {
            self.real.create_dir(dir)
        }
        fn publish(&self, tmp: &Path, dst: &Path, bytes: &[u8]) -> io::Result<()> {
            self.real.publish(tmp, dst, bytes)
        }
        fn list(&self, dir: &Path) -> io::Result<Vec<OsString>> {
            let names = self.real.list(dir)?;
            let now = Instant::now();
            let mut fs = self.first_seen.lock().unwrap();
            for n in &names {
                fs.entry(dir.join(n)).or_insert(now);
            }
            Ok(names)
        }
        fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
            self.open_visible(path)?;
            self.real.read(path)
        }
        fn remove_file(&self, path: &Path) -> io::Result<()> {
            self.real.remove_file(path)
        }
        fn remove_dir(&self, dir: &Path) -> io::Result<()> {
            self.real.remove_dir(dir)
        }
        fn nudge(&self, dir: &Path, name: &str) {
            self.real.nudge(dir, name);
            self.neg
                .lock()
                .unwrap()
                .retain(|p, _| p.parent() != Some(dir));
        }
    }

    fn fresh_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "agora_xchg_unit_{tag}_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    fn delta(t: i64, src: u64, dst: u64) -> WireDelta {
        WireDelta { t, src, dst, event_type: 1, effect_index: 0, amount: src as f64 + 0.5 }
    }

    /// N shards, each with its own caching client view, over `rounds`
    /// rounds; shard `j` publishes `j + 1` deltas to every peer and a
    /// per-shard publish delay staggers arrivals. Returns the wall time.
    fn run_shards(
        n: u64,
        rounds: i64,
        neg_ttl: Duration,
        open_lag: Duration,
        stagger: Duration,
        timeout_s: u64,
    ) -> (Duration, Vec<(u64, u64, u64)>) {
        let dir = fresh_dir(&format!("n{n}"));
        let t0 = Instant::now();
        let handles: Vec<_> = (0..n)
            .map(|k| {
                let dir = dir.clone();
                std::thread::spawn(move || -> anyhow::Result<(u64, u64, u64)> {
                    let fs = Box::new(CachingFs::new(neg_ttl, open_lag));
                    let mut ex = Exchanger::with_fs(dir, k, n, timeout_s, fs)?;
                    for b in 0..rounds {
                        std::thread::sleep(stagger * k as u32);
                        let outbound: Vec<Vec<WireDelta>> = (0..n)
                            .map(|j| {
                                if j == k {
                                    Vec::new()
                                } else {
                                    (0..=k).map(|i| delta(b * 100 + i as i64, k, j)).collect()
                                }
                            })
                            .collect();
                        let inbound = ex.round(b, &outbound)?;
                        for j in 0..n {
                            let expect = if j == k { 0 } else { j as usize + 1 };
                            anyhow::ensure!(
                                inbound[j as usize].len() == expect,
                                "shard {k} round {b}: got {} deltas from {j}, expected {expect}",
                                inbound[j as usize].len()
                            );
                            for (i, d) in inbound[j as usize].iter().enumerate() {
                                anyhow::ensure!(d.src == j && d.dst == k && d.t == b * 100 + i as i64);
                                anyhow::ensure!(d.amount.to_bits() == (j as f64 + 0.5).to_bits());
                            }
                        }
                    }
                    ex.tidy();
                    Ok((ex.nudges, ex.skewed_opens, ex.idle_polls))
                })
            })
            .collect();
        let stats: Vec<_> = handles
            .into_iter()
            .map(|h| h.join().unwrap().expect("shard failed"))
            .collect();
        let wall = t0.elapsed();
        assert!(!dir.exists(), "exchange dir not removed: {}", dir.display());
        (wall, stats)
    }

    /// The OLD primitive (predicted-name open() poll every 2 ms) against the
    /// same client view, for one file that the writer publishes shortly
    /// after the reader's first poll: the first NotFound installs a negative
    /// entry and the reader is blind for `neg_ttl`.
    fn old_predicted_name_poll(neg_ttl: Duration, publish_after: Duration) -> Duration {
        let dir = fresh_dir("old");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("b0_from1_to0.bin");
        let writer = {
            let path = path.clone();
            std::thread::spawn(move || {
                std::thread::sleep(publish_after);
                RealFs.publish(&path.with_extension("tmp"), &path, &[]).unwrap();
            })
        };
        let fs = CachingFs::new(neg_ttl, Duration::ZERO);
        let t0 = Instant::now();
        loop {
            match fs.read(&path) {
                Ok(_) => break,
                Err(e) if e.kind() == io::ErrorKind::NotFound => {
                    std::thread::sleep(Duration::from_millis(2))
                }
                Err(e) => panic!("{e}"),
            }
        }
        let waited = t0.elapsed();
        writer.join().unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        waited
    }

    #[test]
    fn listing_discovery_is_immune_to_negative_lookup_caching() {
        // Negative entries stick for 3 s (the cluster: up to acdirmin=30 s).
        let neg_ttl = Duration::from_secs(3);
        // The old poll: one file, published 20 ms after the first poll,
        // takes the whole TTL to be seen.
        let old = old_predicted_name_poll(neg_ttl, Duration::from_millis(20));
        assert!(old >= neg_ttl, "old primitive should have stalled for the TTL, took {old:?}");
        // The new primitive, 3 shards x 6 rounds with staggered arrivals
        // (up to 20 ms late), on clients with the same negative caching:
        // no round waits for the TTL.
        let rounds = 6;
        let (wall, stats) = run_shards(3, rounds, neg_ttl, Duration::ZERO, Duration::from_millis(10), 30);
        assert!(
            wall < Duration::from_secs(2),
            "new primitive stalled: {wall:?} for {rounds} rounds (old: {old:?})"
        );
        eprintln!("neg-cache: old one-file poll {old:?}; new 3x{rounds} rounds {wall:?}; (nudges, skewed_opens, idle_polls) per shard {stats:?}");
    }

    #[test]
    fn listed_name_that_fails_to_open_is_retried_not_treated_as_absent() {
        // Cache skew: a listed name opens only 40 ms after it first appears
        // on this client, and a failed open would stick for 3 s.
        let open_lag = Duration::from_millis(40);
        let (wall, stats) =
            run_shards(2, 5, Duration::from_secs(3), open_lag, Duration::ZERO, 30);
        let skewed: u64 = stats.iter().map(|s| s.1).sum();
        assert!(skewed > 0, "the skew path was not exercised: {stats:?}");
        assert!(wall < Duration::from_secs(2), "skewed opens stalled: {wall:?}");
        eprintln!("skew: 2x5 rounds {wall:?}; (nudges, skewed_opens, idle_polls) per shard {stats:?}");
    }

    #[test]
    fn on_time_peers_take_the_fast_path_and_round_dirs_are_removed() {
        let (wall, stats) = run_shards(4, 20, Duration::ZERO, Duration::ZERO, Duration::ZERO, 30);
        assert!(wall < Duration::from_secs(5), "{wall:?}");
        eprintln!("fast path: 4x20 rounds {wall:?}; (nudges, skewed_opens, idle_polls) per shard {stats:?}");
    }

    #[test]
    fn missing_peer_times_out_naming_the_round_file() {
        let dir = fresh_dir("timeout");
        let mut ex = Exchanger::new(dir.clone(), 0, 2, 1).unwrap();
        let t0 = Instant::now();
        let err = ex.round(0, &[Vec::new(), vec![delta(1, 0, 1)]]).unwrap_err().to_string();
        let took = t0.elapsed();
        assert!(err.contains("timed out") && err.contains("peer shard 1"), "{err}");
        assert!(err.contains("r0") && err.contains("b0_from1_to0.bin"), "{err}");
        assert!(took >= Duration::from_secs(1) && took < Duration::from_secs(3), "{took:?}");
        // Leftovers: only this shard's own outbound file; no nudge file.
        let names = RealFs.list(&dir.join("r0")).unwrap();
        assert_eq!(names, vec![OsString::from("b0_from0_to1.bin")], "{names:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
