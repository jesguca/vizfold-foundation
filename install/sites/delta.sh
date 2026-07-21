#!/bin/bash
# NCSA Delta. Reached from ../../install.sh, or run directly from a checkout.
set -euo pipefail

REPO=${VIZFOLD_OPENFOLD_HOME:-$(cd "$(dirname "${BASH_SOURCE[0]}")" && until [ -f setup.py ] || [ "$PWD" = / ]; do cd ..; done; pwd)}
. "$REPO/install/interactive.sh"

# Project space is /work/nvme/<allocation>/<user>, and it names the account too,
# so a wrong guess bills the wrong project. Only asked for when it is needed.
allocation() {
    local found=()
    for dir in /work/nvme/*/"$USER"; do
        [ -d "$dir" ] && found+=("$(basename "$(dirname "$dir")")")
    done
    interactive::choose VIZFOLD_ALLOCATION allocation "${found[@]}"
}

if [ -n "${VIZFOLD_PREFIX:-}" ]; then
    PREFIX=$VIZFOLD_PREFIX
else
    ALLOCATION=$(allocation)
    PREFIX=$(interactive::resolve VIZFOLD_PREFIX "install prefix" "/work/nvme/$ALLOCATION/$USER/vizfold")
fi

export VIZFOLD_PREFIX=$PREFIX VIZFOLD_OPENFOLD_HOME=$REPO
export VIZFOLD_GPU_PARTITION=${VIZFOLD_GPU_PARTITION:-gpuA100x4-interactive}
SETUP=$REPO/install/setup.sh
mkdir -p "$PREFIX"

if [ -n "${SLURM_STEP_ID:-}" ]; then
    LAUNCH=(bash)                                     # already on the node
elif [ -n "${SLURM_JOB_ID:-}" ]; then
    LAUNCH=(srun --ntasks=1)                          # salloc leaves you off it
else
    ALLOCATION=${ALLOCATION:-$(allocation)}
    ACCOUNT=$(interactive::resolve VIZFOLD_ACCOUNT "slurm account" "$ALLOCATION-delta-cpu")
    PARTITION=$(interactive::resolve VIZFOLD_PARTITION "slurm partition" cpu)
    LAUNCH=(
        sbatch --job-name=vizfold-install
        --account="$ACCOUNT" --partition="$PARTITION"
        --nodes=1 --ntasks=1 --cpus-per-task=4 --mem=16G --time=01:00:00
        --output="$PREFIX/install-%j.log" --export=ALL
    )
fi
# Only for the smoke test setup.sh prints; unknown here is fine.
if [ -n "${ALLOCATION:-}" ]; then export VIZFOLD_GPU_ACCOUNT=$ALLOCATION-delta-gpu; fi

echo "${LAUNCH[0]} $SETUP"
exec "${LAUNCH[@]}" "$SETUP"
