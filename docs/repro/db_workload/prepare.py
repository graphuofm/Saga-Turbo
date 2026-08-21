#!/usr/bin/env python3
"""Canonicalize a dataset into the db_workload schema using DuckDB:

  nodes.csv : id INT64, type STRING, attrs STRING   (attrs = 'k=v;k=v;...')
  edges.csv : src, dst, t INT64 (unix s), event_type, label, amount DOUBLE

Modes:
  prepare.py saga <agora_out_dir> <canon_dir>
      nodes.csv (id, entity_type, ...) -> (id, type, attrs); the agora nodes.csv is
      ragged (merchant rows have fewer columns), hence null_padding
      edges_*.csv (src,dst,t,event_type,label,anomaly_id,amount,...) -> 6 cols
  prepare.py ldbc <social_network_dir> <canon_dir>
      LDBC SNB Interactive v1 CsvBasic (LongDateFormatter, '|' separated):
      nodes = person, post, comment (dense ids, type-prefixed offsets)
      edges = person-knows-person (t = knows.creationDate)
              person-hasCreator->post / comment ("creation" edges,
              t = post/comment creationDate), event_type = knows|post|comment.
      Timestamps are epoch milliseconds in the source; mapped to unix seconds.
      label is NULL (no ground-truth labels), amount = post/comment length.
"""
import glob
import os
import sys
import time

import duckdb


def saga(src, out):
    os.makedirs(out, exist_ok=True)
    con = duckdb.connect()
    cols = [c[0] for c in con.execute(f"DESCRIBE SELECT * FROM read_csv('{src}/nodes.csv', header=true, null_padding=true, all_varchar=true)").fetchall()]
    rest = [c for c in cols if c not in ("id", "entity_type")]
    attrs = " || ';' || ".join([f"'{c}=' || coalesce(cast({c} as varchar), '')" for c in rest])
    con.execute(f"""COPY (SELECT id, entity_type AS type, {attrs} AS attrs
                    FROM read_csv('{src}/nodes.csv', header=true, null_padding=true, all_varchar=true) ORDER BY id)
                    TO '{out}/nodes.csv' (HEADER, DELIMITER ',')""")
    con.execute(f"""COPY (SELECT src, dst, cast(t as bigint) AS t, event_type, label, amount
                    FROM read_csv('{src}/edges_*.csv', header=true, null_padding=true, union_by_name=true))
                    TO '{out}/edges.csv' (HEADER, DELIMITER ',')""")


def ldbc(src, out):
    os.makedirs(out, exist_ok=True)
    con = duckdb.connect()
    con.execute("SET threads = 4")
    dyn = os.path.join(src, "dynamic")
    def rc(name, cols):
        files = sorted(glob.glob(os.path.join(dyn, f"{name}_[0-9]*.csv")))
        assert files, name
        names = list(cols)
        return (f"read_csv({files!r}, delim='|', header=true, max_line_size=10000000, "
                f"names={names!r}, types={list(cols.values())!r})")
    steps = [
        ("person", f"CREATE TABLE person AS SELECT id, creationDate, firstName, gender, browserUsed FROM {rc('person', {'id':'BIGINT','firstName':'VARCHAR','lastName':'VARCHAR','gender':'VARCHAR','birthday':'BIGINT','creationDate':'BIGINT','locationIP':'VARCHAR','browserUsed':'VARCHAR'})}"),
        ("post", f"CREATE TABLE post AS SELECT id, creationDate, browserUsed, language, length FROM {rc('post', {'id':'BIGINT','imageFile':'VARCHAR','creationDate':'BIGINT','locationIP':'VARCHAR','browserUsed':'VARCHAR','language':'VARCHAR','content':'VARCHAR','length':'BIGINT'})}"),
        ("comment", f"CREATE TABLE comment AS SELECT id, creationDate, browserUsed, length FROM {rc('comment', {'id':'BIGINT','creationDate':'BIGINT','locationIP':'VARCHAR','browserUsed':'VARCHAR','content':'VARCHAR','length':'BIGINT'})}"),
        ("knows", f"CREATE TABLE knows AS SELECT * FROM {rc('person_knows_person', {'p1':'BIGINT','p2':'BIGINT','creationDate':'BIGINT'})}"),
        ("post_creator", f"CREATE TABLE post_creator AS SELECT * FROM {rc('post_hasCreator_person', {'post':'BIGINT','person':'BIGINT'})}"),
        ("comment_creator", f"CREATE TABLE comment_creator AS SELECT * FROM {rc('comment_hasCreator_person', {'comment':'BIGINT','person':'BIGINT'})}"),
    ]
    for name, sql in steps:
        t = time.time()
        con.execute(sql)
        print(name, con.execute(f"SELECT count(*) FROM {name}").fetchone()[0], f"{time.time()-t:.1f}s", flush=True)
    # dense node ids: person [0, P), post [P, P+Q), comment [P+Q, ...)
    con.execute("CREATE TABLE pid AS SELECT id AS raw, row_number() OVER (ORDER BY id) - 1 AS nid FROM person")
    P = con.execute("SELECT count(*) FROM pid").fetchone()[0]
    con.execute(f"CREATE TABLE qid AS SELECT id AS raw, {P} + row_number() OVER (ORDER BY id) - 1 AS nid FROM post")
    Q = con.execute("SELECT count(*) FROM qid").fetchone()[0]
    con.execute(f"CREATE TABLE cid AS SELECT id AS raw, {P + Q} + row_number() OVER (ORDER BY id) - 1 AS nid FROM comment")
    t = time.time()
    con.execute(f"""COPY (
        SELECT nid AS id, 'person' AS type, 'firstName=' || p.firstName || ';gender=' || p.gender || ';browser=' || p.browserUsed AS attrs
          FROM person p JOIN pid ON pid.raw = p.id
        UNION ALL
        SELECT nid, 'post', 'browser=' || q.browserUsed || ';lang=' || coalesce(q.language, '') FROM post q JOIN qid ON qid.raw = q.id
        UNION ALL
        SELECT nid, 'comment', 'browser=' || c.browserUsed FROM comment c JOIN cid ON cid.raw = c.id
        ORDER BY 1) TO '{out}/nodes.csv' (HEADER, DELIMITER ',')""")
    print(f"nodes.csv {time.time()-t:.1f}s", flush=True)
    t = time.time()
    con.execute(f"""COPY (
        SELECT a.nid AS src, b.nid AS dst, k.creationDate // 1000 AS t, 'knows' AS event_type, NULL AS label, NULL AS amount
          FROM knows k JOIN pid a ON a.raw = k.p1 JOIN pid b ON b.raw = k.p2
        UNION ALL
        SELECT a.nid, b.nid, q.creationDate // 1000, 'post', NULL, q.length
          FROM post_creator pc JOIN post q ON q.id = pc.post JOIN pid a ON a.raw = pc.person JOIN qid b ON b.raw = pc.post
        UNION ALL
        SELECT a.nid, b.nid, c.creationDate // 1000, 'comment', NULL, c.length
          FROM comment_creator cc JOIN comment c ON c.id = cc.comment JOIN pid a ON a.raw = cc.person JOIN cid b ON b.raw = cc.comment
        ) TO '{out}/edges.csv' (HEADER, DELIMITER ',')""")
    print(f"edges.csv {time.time()-t:.1f}s", flush=True)


if __name__ == "__main__":
    t0 = time.time()
    {"saga": saga, "ldbc": ldbc}[sys.argv[1]](sys.argv[2], sys.argv[3])
    print(f"prepare {sys.argv[1]} wall={time.time()-t0:.1f}s")
