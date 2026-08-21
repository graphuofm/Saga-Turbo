"""CLI for the SAGA fidelity-evaluation harness (M7).

    # compute the fidelity scorecard of a SAGA output vs a real dataset
    python3 -m saga_eval compare --synth <saga_out_dir> --real <real_path> \
        [--real-format elliptic|snap|csv] [--t-scale 86400] [--json out.json]

    # just dump the statistics of one graph (real or synth)
    python3 -m saga_eval stats --synth <saga_out_dir>
    python3 -m saga_eval stats --real <path> [--t-scale ...]
"""
from __future__ import annotations

import argparse
import json
import sys
from typing import Optional

from . import compare as cmp
from . import load
from . import stats as st


def _load_any(synth: Optional[str], real: Optional[str], args):
    if synth:
        return load.load_saga(synth, event_type=getattr(args, "event_type", None)), synth
    return (
        load.load_real(
            real,
            src_col=args.src_col,
            dst_col=args.dst_col,
            t_col=args.t_col,
            has_header=args.header,
            t_scale=args.t_scale,
        ),
        real,
    )


def main(argv=None) -> int:
    p = argparse.ArgumentParser(prog="saga_eval")
    sub = p.add_subparsers(dest="cmd", required=True)

    def add_common(sp):
        sp.add_argument("--synth", help="SAGA output directory")
        sp.add_argument("--real", help="real dataset file or directory")
        sp.add_argument("--src-col", default=None)
        sp.add_argument("--dst-col", default=None)
        sp.add_argument("--t-col", default=None)
        sp.add_argument("--header", action="store_true", default=None, help="force header present")
        sp.add_argument("--t-scale", type=float, default=1.0, help="multiply timestamps into seconds")
        sp.add_argument("--event-type", default=None,
                        help="restrict the SAGA side to one event type (like-for-like vs a single-relation real dataset)")

    c = sub.add_parser("compare", help="fidelity scorecard: synth vs real")
    add_common(c)
    c.add_argument("--json", help="write the full result as JSON")
    c.add_argument("--advanced", action="store_true",
                   help="also compute EVAL.md layers 2-4 (energy/AD, temporal motifs, discriminative C2ST)")
    c.add_argument("--delta", type=float, default=None,
                   help="δ window (seconds) for temporal motifs (default: min(1h, span/100))")

    s = sub.add_parser("stats", help="print statistics of one graph")
    add_common(s)

    b = sub.add_parser("baselines", help="compare SAGA vs classical generators, both vs real")
    add_common(b)
    b.add_argument("--json", help="write the full comparison as JSON")

    args = p.parse_args(argv)

    if args.cmd == "baselines":
        if not (args.synth and args.real):
            p.error("baselines needs both --synth (SAGA out) and --real")
        from . import baselines as bl
        (ssrc, sdst, stt), sname = _load_any(args.synth, None, args)
        (rsrc, rdst, rtt), rname = _load_any(None, args.real, args)
        rows = bl.compare_baselines((rsrc, rdst, rtt), (ssrc, sdst, stt))
        print(bl.format_table(rows, rname))
        if args.json:
            with open(args.json, "w") as fh:
                json.dump(rows, fh, indent=2, default=lambda o: None)
            print(f"[eval] wrote {args.json}")
        return 0

    if args.cmd == "stats":
        (src, dst, t), name = _load_any(args.synth, args.real, args)
        gs = st.compute_stats(src, dst, t)
        print(f"=== stats: {name} ===")
        for k, v in gs.scalar_summary().items():
            print(f"  {k:<18} {v}")
        print(f"  span               {gs.t_min:.0f} … {gs.t_max:.0f}")
        return 0

    if args.cmd == "compare":
        if not (args.synth and args.real):
            p.error("compare needs both --synth and --real")
        (ssrc, sdst, stt), sname = _load_any(args.synth, None, args)
        (rsrc, rdst, rtt), rname = _load_any(None, args.real, args)
        print(f"[eval] loaded synth: {ssrc.size:,} edges; real: {rsrc.size:,} edges")
        synth = st.compute_stats(ssrc, sdst, stt)
        real = st.compute_stats(rsrc, rdst, rtt)
        result = cmp.compare(real, synth)
        print(cmp.format_scorecard(result, rname, sname))
        if getattr(args, "advanced", False):
            from . import metrics_advanced as adv
            rep = adv.advanced_report(
                (rsrc, rdst, rtt), (ssrc, sdst, stt), real, synth,
                delta_s=args.delta,
            )
            print(adv.format_advanced(rep))
            result["advanced"] = rep
        if args.json:
            with open(args.json, "w") as fh:
                json.dump(result, fh, indent=2, default=lambda o: None)
            print(f"[eval] wrote {args.json}")
        return 0
    return 1


if __name__ == "__main__":
    sys.exit(main())
