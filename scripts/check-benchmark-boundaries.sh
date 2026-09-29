#!/usr/bin/env bash
set -euo pipefail

cargo metadata --format-version 1 --no-deps |
python3 -c '
import json
import sys

metadata = json.load(sys.stdin)
owned = {"rsos", "rbsr"}
banned = {
    "async-trait",
    "bincode",
    "chrono",
    "gossip",
    "hyper",
    "ipnet",
    "lww-register",
    "mio",
    "reconcile",
    "reconcile-gossip",
    "reqwest",
    "socket2",
    "tokio",
    "tokio-util",
}

violations = []
for package in metadata["packages"]:
    if package["name"] not in owned:
        continue
    for dependency in package["dependencies"]:
        package_name = dependency.get("package") or dependency["name"]
        if package_name in banned or dependency["name"] in banned:
            kind = dependency.get("kind") or "normal"
            violations.append(
                f"{package['name']}: forbidden {kind} dependency "
                f"{dependency['name']} ({package_name})"
            )

if violations:
    print(
        "RSOS/RBSR own intrinsic algorithm/data-structure benchmarks. "
        "Runtime, wire, clock, and transport dependencies belong downstream or in rbsr-research.",
        file=sys.stderr,
    )
    for violation in violations:
        print(f"  - {violation}", file=sys.stderr)
    raise SystemExit(1)
'

if grep -R -nE 'std::net|UdpSocket|TcpStream' rsos/benches rbsr/benches; then
  echo "Intrinsic RSOS/RBSR benchmarks must not open or model network transports." >&2
  exit 1
fi

echo "benchmark ownership boundary: ok"
