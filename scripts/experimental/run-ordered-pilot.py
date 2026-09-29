#!/usr/bin/env python3
import argparse
import hashlib
import json
import math
import os
import platform
import shutil
import statistics
import subprocess
from datetime import datetime, timezone
from pathlib import Path


BACKENDS = {
    "btreelmdb": "BTreeLMDB",
    "noaggwindow": "NoAggWindowSliceAELMDB",
    "aelmdb": "AELMDBSlice",
}


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def command(args, log, **kwargs):
    subprocess.run([str(arg) for arg in args], check=True, stdout=log, stderr=log, **kwargs)


def revision(path):
    return subprocess.check_output(["git", "-C", str(path), "rev-parse", "HEAD"], text=True).strip()


def build(sources, output, profile, log):
    for name, sha in profile["source"]["repositories"].items():
        path = sources / name
        if not path.exists():
            command(["git", "init", path], log)
            command(["git", "-C", path, "fetch", "--depth=1",
                     f"https://github.com/amparore/{name}.git", sha], log)
            command(["git", "-C", path, "checkout", "--detach", "FETCH_HEAD"], log)
        if revision(path) != sha:
            raise ValueError(f"wrong revision for {name}")
        command(["git", "-C", path, "diff", "--exit-code", "HEAD"], log)

    build_dir = output / "build"
    build_dir.mkdir()
    bench = sources / "bench-aelmdb"
    source = (bench / "rbsr_aelmdb_advanced_test.cpp").read_text()
    anchor = "static inline void print_row_csv("
    if source.count(anchor) != 1:
        raise ValueError("JSON exporter insertion point changed")
    exporter = Path(__file__).with_name("aelmdb-result.h").read_text()
    source = "#include <cstdlib>\n" + source.replace(anchor, exporter + "\n" + anchor)
    anchor = "                                 const Metrics &m)\n{\n"
    if source.count(anchor) != 1:
        raise ValueError("JSON exporter call insertion point changed")
    source = source.replace(anchor, anchor + "    write_machine_row(sc, backend, exp, m);\n")
    generated = build_dir / "instrumented.cpp"
    generated.write_text(source)
    for stem in ["mdb", "midl"]:
        command(["cc", "-pthread", "-O3", "-DNDEBUG", "-c",
                 sources / "aelmdb" / f"{stem}.c", "-o", build_dir / f"{stem}.o"], log)
    executable = build_dir / "ordered-pilot"
    command(["g++", "-std=c++20", "-O3", "-DNDEBUG",
             "-I" + str(sources / "negentropy-aelmdb/cpp"),
             "-I" + str(sources / "negentropy-aelmdb/cpp/negentropy/storage"),
             "-I" + str(sources / "lmdbxx-aelmdb/include"),
             "-I" + str(sources / "aelmdb"), "-I" + str(bench),
             generated, build_dir / "mdb.o", build_dir / "midl.o",
             "-lcrypto", "-o", executable], log)
    return executable


def validate(row, backend, i, repeats):
    expected = {
        "schema_version": 1, "scenario": f"baseline_dense_{i}",
        "backend": BACKENDS[backend], "repeat_reconcile": repeats,
        "fullA": 1268 * i, "fullB": 1268 * i,
        "sliceA": 68 * i, "sliceB": 68 * i,
        "expected_have": 4 * i, "expected_need": 4 * i,
        "have_count": 4 * i, "need_count": 4 * i,
    }
    for key, value in expected.items():
        if row.get(key) != value:
            raise ValueError(f"unexpected {key}: {row.get(key)} != {value}")
    for key, value in row.items():
        if isinstance(value, (int, float)) and (not math.isfinite(value) or value < 0):
            raise ValueError(f"invalid metric: {key}")


def summarize(rows):
    result = []
    for scenario, backend in sorted({(r["scenario"], r["backend"]) for r in rows}):
        samples = [r for r in rows if (r["scenario"], r["backend"]) == (scenario, backend)]
        summary = {"scenario": scenario, "backend": backend, "process_trials": len(samples)}
        for metric in ["prep_total_ms", "reconcile_ms", "total_bench_ms", "rss_kb_after"]:
            values = [r[metric] for r in samples]
            summary[metric] = {
                "median": statistics.median(values), "min": min(values), "max": max(values)
            }
        summary["protocol_bytes"] = samples[0]["bytes_a_to_b"] + samples[0]["bytes_b_to_a"]
        summary["allocated_disk_bytes"] = samples[0]["A_alloc_bytes"] + samples[0]["B_alloc_bytes"]
        result.append(summary)
    return result


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("output", type=Path)
    parser.add_argument("--sources", type=Path)
    parser.add_argument("--smoke", action="store_true")
    args = parser.parse_args()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    root = Path(__file__).resolve().parents[1]
    manifest = root / "rbsr-research/benches/fixtures/datastore-workloads/ordered.aelmdb.base_dense.json"
    profile = json.loads(manifest.read_text())
    sources = args.sources.resolve() if args.sources else output / "sources"
    sources.mkdir(parents=True, exist_ok=True)
    instances = [1] if args.smoke else profile["family"]["i"]
    trials = 1 if args.smoke else profile["measurements"]["independent_process_trials"]
    repeats = profile["measurements"]["reconciliations_averaged_per_process"]
    metadata = {
        "schema_version": 1, "pilot_id": profile["pilot_id"],
        "status": "running", "smoke": args.smoke,
        "started_utc": datetime.now(timezone.utc).isoformat(),
        "manifest_sha256": digest(manifest), "repositories": profile["source"]["repositories"],
        "runner_sha256": digest(Path(__file__)),
        "exporter_sha256": digest(Path(__file__).with_name("aelmdb-result.h")),
        "platform": platform.platform(), "machine": platform.machine(),
        "cpu_model": next((line.split(":", 1)[1].strip() for line in
                           Path("/proc/cpuinfo").read_text().splitlines()
                           if line.startswith("model name")), "unknown"),
        "affinity_cpus": len(os.sched_getaffinity(0)),
        "compiler": subprocess.check_output(["g++", "--version"], text=True),
        "flags": "cc -pthread -O3 -DNDEBUG; g++ -std=c++20 -O3 -DNDEBUG -lcrypto",
        "instances": instances, "independent_process_trials": trials,
        "reconciliations_averaged_per_process": repeats,
        "limitations": [
            "Local sequential protocol execution; no network transport or repair.",
            "Fresh processes after preparation; no page-cache eviction or host exclusivity.",
            "Source total_bench_ms is open + build + mean reconciliation, not one critical-path timer.",
            "Source preparation subphases overlap; do not sum them or equate prep_total to addon build.",
            "RSS is a process snapshot, not a per-peer allocation or peak measure.",
            "CPU and I/O remain unmeasured. Equal protocol byte counts do not prove transcript identity.",
            "Source msg_count counts request/response iterations, not individual one-way messages.",
            "Source checks exact analytic HAVE/NEED on the first reconciliation of each process.",
        ],
    }
    metadata_path = output / "manifest.json"
    metadata_path.write_text(json.dumps(metadata, indent=2) + "\n")
    rows = []
    try:
        with (output / "build.log").open("w") as log:
            executable = build(sources, output, profile, log)
        metadata["executable_sha256"] = digest(executable)
        with (output / "observations.jsonl").open("x") as observations, (output / "execution.log").open("w") as log:
            for i in instances:
                shapes = set()
                for trial in range(trials):
                    order = list(BACKENDS)
                    offset = trial % len(order)
                    order = order[offset:] + order[:offset]
                    for backend in order:
                        result = output / "current-result.json"
                        result.unlink(missing_ok=True)
                        database = output / "database"
                        env = dict(os.environ, RBSR_MACHINE_OUTPUT=str(result))
                        options = ["--magnitude", i, "--scenario", f"baseline_dense_{i}",
                                   "--backend", backend, "--mapsize-mb", "2048",
                                   "--repeat-reconcile", repeats, "--root", database]
                        command([executable, "--mode", "init", *options], log)
                        command([executable, "--mode", "bench", *options], log, env=env)
                        row = json.loads(result.read_text())
                        validate(row, backend, i, repeats)
                        row["trial"] = trial
                        rows.append(row)
                        observations.write(json.dumps(row, separators=(",", ":")) + "\n")
                        observations.flush()
                        shapes.add((row["msg_count"], row["bytes_a_to_b"], row["bytes_b_to_a"]))
                        shutil.rmtree(database)
                if len(shapes) != 1:
                    raise ValueError(f"protocol counts differ for instance {i}")
        (output / "summary.json").write_text(json.dumps(summarize(rows), indent=2) + "\n")
        metadata["status"] = "completed"
        metadata["observation_count"] = len(rows)
        metadata["observations_sha256"] = digest(output / "observations.jsonl")
        metadata["summary_sha256"] = digest(output / "summary.json")
    except Exception as error:
        metadata["status"] = "failed"
        metadata["error"] = str(error)
        raise
    finally:
        metadata["finished_utc"] = datetime.now(timezone.utc).isoformat()
        metadata_path.write_text(json.dumps(metadata, indent=2) + "\n")


if __name__ == "__main__":
    main()
