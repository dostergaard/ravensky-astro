#!/bin/sh
# Current bounded GZIP validator baseline: 24 configurations, 216 isolated samples.
set -eu
if [ "$#" -lt 3 ]; then
    echo "Usage: sh astro-bench/scripts/fits-gzip-baseline.sh OUTPUT_DIR SCRATCH_DIR ENVIRONMENT_NOTE [BINARY]" >&2
    exit 2
fi
report_dir=$1
scratch_dir=$2
environment_note=$3
benchmark_binary=${4:-./target/release/astro-bench}
mkdir -p "$report_dir"
for encoding in fits-gzip fits-gzip2; do
    for tile in row image; do
        for pattern in noise gradient; do
            for size in 8 32; do
                if [ "$size" = 8 ]; then
                    dimension=2048
                    frames=8
                else
                    dimension=4096
                    frames=4
                fi
                if [ "$tile" = row ]; then
                    tile_rows=1
                else
                    tile_rows=$dimension
                fi
                workloads=full
                if [ "$pattern" = noise ] && [ "$size" = 8 ]; then
                    workloads="read structural full"
                fi
                for workload in $workloads; do
                    "$benchmark_binary" run \
                        --encoding "$encoding" --tile-rows "$tile_rows" \
                        --pattern "$pattern" --workload "$workload" \
                        --width "$dimension" --height "$dimension" --frames "$frames" \
                        --workers 1,2,4 --repeats 3 --disk-mib 512 --timeout-seconds 120 \
                        --scratch "$scratch_dir" --note "$environment_note" \
                        --output "$report_dir/$encoding-$tile-$pattern-${size}mib-$workload.json"
                done
            done
        done
    done
done
