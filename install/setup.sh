#!/bin/bash
# Install OpenFold into a micromamba environment, on the node that runs this.
# Reached from a site script; ../install.sh picks one. Idempotent per step.
set -euo pipefail

REPO=${VIZFOLD_OPENFOLD_HOME:-$(cd "$(dirname "${BASH_SOURCE[0]}")" && until [ -f setup.py ] || [ "$PWD" = / ]; do cd ..; done; pwd)}
PREFIX=${VIZFOLD_PREFIX:-$HOME/vizfold}
AF2=${VIZFOLD_AF2_ROOT:-/sw/external/alphafold2/data_hyun_official}
ENV_NAME=${VIZFOLD_ENV_NAME:-vizfold}
MAX_CUDA=${VIZFOLD_MAX_CUDA:-12.8}   # a runtime above the driver breaks Amber relaxation

DATA=$PREFIX/data
MM=$PREFIX/bin/micromamba
CUTLASS=$PREFIX/cutlass
UNICLUST=$DATA/uniclust30/uniclust30_2018_08
STEREO=$REPO/openfold/resources/stereo_chemical_props.txt

export CONDA_PKGS_DIRS=${VIZFOLD_PKGS_DIR:-$PREFIX/../.vizfold-pkgs}
export MAMBA_ROOT_PREFIX=$PREFIX/mamba TMPDIR=$PREFIX/tmp
export TORCH_CUDA_ARCH_LIST="${TORCH_CUDA_ARCH_LIST:-8.0;8.6;9.0}"   # no device to probe
export MAX_JOBS="${MAX_JOBS:-${SLURM_CPUS_PER_TASK:-4}}"

REQUIRED=(
    "$DATA/pdb_mmcif/mmcif_files"
    "$DATA/uniref90/uniref90.fasta"
    "$DATA/mgnify/mgy_clusters_2022_05.fa"
    "$DATA/pdb70/pdb70"
    "$UNICLUST/uniclust30_2018_08"
    "$DATA/bfd/bfd_metaclust_clu_complete_id30_c90_final_seq.sorted_opt"
    "$REPO/openfold/resources/params/params_model_1_ptm.npz"
    "$STEREO"
)
BINARIES=(jackhmmer hhblits hhsearch)

die() { echo "FATAL: $*" >&2; exit 1; }
step() { echo "== $* (+$((SECONDS))s)"; }
have() { test -e "$1" || compgen -G "${1}_*.ffindex" >/dev/null; }   # ffindex sets are prefixes

mkdir -p "$PREFIX/bin" "$TMPDIR" "$UNICLUST" "$REPO/openfold/resources" "$REPO/outputs"
hostname
nvidia-smi --query-gpu=name,compute_cap --format=csv,noheader 2>/dev/null || echo "no GPU on this node"
echo "prefix=$PREFIX repo=$REPO af2=$AF2 env=$ENV_NAME max_cuda=$MAX_CUDA"
test -f "$REPO/setup.py" || die "$REPO is not an OpenFold checkout"
test -d "$AF2" || die "no AlphaFold2 databases at $AF2; set VIZFOLD_AF2_ROOT"

step micromamba
[ -x "$MM" ] || curl -Ls https://micro.mamba.pm/api/micromamba/linux-64/latest |
    tar -xj -C "$PREFIX" bin/micromamba

step "conda env $ENV_NAME"
"$MM" env list | grep -qE "^\s*$ENV_NAME\s" ||
    "$MM" create -y -n "$ENV_NAME" -f "$REPO/environment.yml" "cuda-version<=$MAX_CUDA"

set +u   # the conda gcc hook reads SYS_SYSROOT unset
eval "$("$MM" shell hook --shell bash)"
micromamba activate "$ENV_NAME"
set -u

step "third-party dependencies"
# scripts/install_third_party_dependencies.sh, whose `conda env config vars set`
# micromamba does not implement.
[ -d "$CUTLASS/.git" ] ||
    git clone -q https://github.com/NVIDIA/cutlass --branch v3.6.0 --depth 1 "$CUTLASS"
mkdir -p "$CONDA_PREFIX/etc/conda/activate.d"
cat > "$CONDA_PREFIX/etc/conda/activate.d/openfold.sh" <<ACTIVATE
export CUTLASS_PATH=$CUTLASS
export KMP_AFFINITY=none
export LIBRARY_PATH=\$CONDA_PREFIX/lib:\${LIBRARY_PATH:-}
export LD_LIBRARY_PATH=\$CONDA_PREFIX/lib:\${LD_LIBRARY_PATH:-}
ACTIVATE
. "$CONDA_PREFIX/etc/conda/activate.d/openfold.sh"

step openfold
# No build isolation: the extension must link against this env's torch.
python3 -c 'import torch, openfold, attn_core_inplace_cuda' 2>/dev/null ||
    (cd "$REPO" && CUDA_HOME=$CONDA_PREFIX pip install --no-build-isolation -e .)

step datasets
ln -sfn "$AF2"/* "$DATA/"
ln -sfn "$AF2/params" "$REPO/openfold/resources/params"
[ -f "$STEREO" ] || { curl -Lsf -o "$STEREO.part" \
    https://git.scicore.unibas.ch/schwede/openstructure/-/raw/7102c63615b64735c4941278d92b554ec94415f8/modules/mol/alg/src/stereo_chemical_props.txt &&
    mv "$STEREO.part" "$STEREO"; }
mkdir -p "$REPO/tests/test_data/alphafold/common"
ln -sfn "$STEREO" "$REPO/tests/test_data/alphafold/common/stereo_chemical_props.txt"
# The schema asks for uniclust30; Delta ships its successor under uniref30.
for f in "$AF2"/uniref30/UniRef30_2021_03*; do
    ln -sfn "$f" "$UNICLUST/uniclust30_2018_08${f##*UniRef30_2021_03}"
done

step verify
python3 - <<'PY'
import importlib.util as util, os, torch, attn_core_inplace_cuda, openfold
from openfold.model.primitives import Linear
print("torch", torch.__version__, "cuda_devices", torch.cuda.device_count())
print("openfold", openfold.__file__)
assert util.find_spec("flash_attn"), "flash_attn is not importable"
assert os.path.isdir(os.environ.get("CUTLASS_PATH", "")), "CUTLASS_PATH is unset"
print("flash_attn ok, CUTLASS_PATH", os.environ["CUTLASS_PATH"])
PY
for b in "${BINARIES[@]}"; do command -v "$b" >/dev/null || die "missing binary: $b"; done
for p in "${REQUIRED[@]}"; do have "$p" || die "missing: $p"; done

# The site script names its GPU queue; without one, assume we are already on the box.
# --mem matters: the scheduler default is per-CPU and Amber relaxation is OOM-killed under it.
LAUNCH="${VIZFOLD_GPU_PARTITION:+srun ${VIZFOLD_GPU_ACCOUNT:+-A $VIZFOLD_GPU_ACCOUNT }-p $VIZFOLD_GPU_PARTITION --gres=gpu:1 --cpus-per-task=8 --mem=32G -t 00:30:00 }"
cat <<EOF
== ready (+$((SECONDS))s)

Check it works -- fold the bundled example and count the atoms:

  ${LAUNCH}env VIZFOLD_PREFIX=$PREFIX $REPO/run/fold.sh 6KWC_1
  grep -c '^ATOM' $PREFIX/outputs/6KWC_1/predictions/6KWC_1_model_1_ptm_relaxed.pdb

That should print 2839. To use the environment directly:

  export MAMBA_ROOT_PREFIX=$PREFIX/mamba
  eval "\$($MM shell hook --shell bash)" && micromamba activate $ENV_NAME
  export VIZFOLD_OPENFOLD_HOME=$REPO VIZFOLD_OPENFOLD_DATA_DIR=$DATA
EOF
