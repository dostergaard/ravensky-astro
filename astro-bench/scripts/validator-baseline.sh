#!/bin/sh
# Run from the workspace root after cargo build --release -p astro-bench.
# Outputs are exclusive: use a fresh report directory for every measurement matrix.
set -eu
if [ "$#" -lt 3 ]; then
    echo "Usage: sh astro-bench/scripts/validator-baseline.sh OUTPUT_DIR SCRATCH_DIR ENVIRONMENT_NOTE [BINARY]" >&2
    exit 2
fi
report_dir=$1
scratch_dir=$2
environment_note=$3
benchmark_binary=${4:-./target/release/astro-bench}
mkdir -p "$report_dir"
for encoding in fits xisf zlib zstd; do
    for pattern in noise gradient; do
        for size in 8 32; do
            if [ "$size" = 8 ]; then
                dimension=2048
                frames=8
            else
                dimension=4096
                frames=4
            fi
            workloads=full
            if [ "$pattern" = noise ] && [ "$size" = 8 ]; then
                workloads="read structural full"
            fi
            for workload in $workloads; do
                "$benchmark_binary" run \
                    --encoding "$encoding" --pattern "$pattern" --workload "$workload" \
                    --width "$dimension" --height "$dimension" --frames "$frames" \
                    --workers 1,2,4 --repeats 3 --disk-mib 512 --timeout-seconds 120 \
                    --scratch "$scratch_dir" --note "$environment_note" \
                    --output "$report_dir/$encoding-$pattern-${size}mib-$workload.json"
            done
        done
    done
done
