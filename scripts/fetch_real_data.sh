#!/usr/bin/env bash
# AeroPulse-NG — real-data fetch helper (air-gapped friendly: downloads are explicit, no auto-pip)
# Wraps exact URL patterns verified 2026-09-22:
#   samples.adsbexchange.com  — free only 1st of each month, decoded JSON, gzip without .gz
#   globe_history_2024        — archived 1455 GiB / 980 releases, decoded readsb traces, ODbL
#   ATCO2 1h (Jzuluaga/atco2_corpus_1h) — 871 parquet rows, Czech airspace
# Usage:
#   ./scripts/fetch_real_data.sh all                          # everything (large!)
#   ./scripts/fetch_real_data.sh adsbex 2026-09-01 d6        # ADS-B Exchange traces for 2026-09-01 suffix d6
#   ./scripts/fetch_real_data.sh adsbex-hist 2026-09-01      # ADS-B Exchange global snapshots
#   ./scripts/fetch_real_data.sh adsblol 2024-06-15          # one day from globe_history_2024
#   ./scripts/fetch_real_data.sh atco2                       # ATCO2 1h via datasets
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT="$ROOT/data/real"
mkdir -p "$OUT"

need_cmd() { command -v "$1" >/dev/null 2>&1 || { echo "missing $1" >&2; exit 1; }; }

# ADS-B Exchange — decoded, free only 1st of month
# https://samples.adsbexchange.com/readsb-hist/YYYY/MM/DD/
# https://samples.adsbexchange.com/traces/YYYY/MM/DD/xx/  (xx = last 2 hex of ICAO)
fetch_adsbex_hist() {
  local date="${1:-2026-09-01}" # must be YYYY-MM-01
  local day="${date##*-}"; if [ "$day" != "01" ]; then echo "ADS-B Exchange free is only 01 of month (got $date)" >&2; exit 2; fi
  need_cmd wget; need_cmd zcat
  local IFS=-; read -r y m d <<<"$date"
  local url="https://samples.adsbexchange.com/readsb-hist/$y/$m/$d/"
  echo "→ $url (browse first: $url)"
  wget -r -np -nH --cut-dirs=1 -R "index.html*" -P "$OUT/adsbex-hist/$date" "$url" || true
  echo "Note: files are gzip without .gz — use: zcat \"$OUT/adsbex-hist/$date\"/* | jq . | head"
  echo "Preview: zcat $OUT/adsbex-hist/$date/* 2>/dev/null | head -c 500 | jq . 2>&1 | head -20 || zcat $OUT/adsbex-hist/$date/* 2>/dev/null | head -c 500"
}

fetch_adsbex_traces() {
  local date="${1:-2026-09-01}" suffix="${2:-d6}"
  local day="${date##*-}"; if [ "$day" != "01" ]; then echo "free is only 01" >&2; exit 2; fi
  need_cmd wget
  local IFS=-; read -r y m d <<<"$date"
  local url="https://samples.adsbexchange.com/traces/$y/$m/$d/$suffix/"
  echo "→ $url"
  wget -r -np -nH --cut-dirs=1 -R "index.html*" -P "$OUT/adsbex-traces/$date/$suffix" "$url" || true
  echo "zcat \"$OUT/adsbex-traces/$date/$suffix\"/* | jq . | head"
}

# adsb.lol globe_history_2024 — decoded readsb traces, 980 releases
# https://github.com/adsblol/globe_history_2024/releases + RELEASES.md
fetch_adsblol() {
  local date="${1:-2024-06-15}" # YYYY-MM-DD
  need_cmd wget; need_cmd tar
  local rel_dir="$OUT/adsblol/$date"
  mkdir -p "$rel_dir"
  # Try to find asset via GitHub API (no auth, rate-limited) or RELEASES.md
  local api="https://api.github.com/repos/adsblol/globe_history_2024/releases/tags/v${date}-planes-readsb-prod-0"
  echo "→ $api (or browse https://github.com/adsblol/globe_history_2024/releases)"
  if command -v jq >/dev/null 2>&1; then
    local url
    url=$(curl -s "$api" | jq -r '.assets[0].browser_download_url // empty' 2>/dev/null || true)
    if [ -n "$url" ] && [ "$url" != "null" ]; then
      echo "Downloading $url"
      wget -O "$rel_dir/${date}.tar" "$url"
      tar -tf "$rel_dir/${date}.tar" | head -20
      echo "Extract: tar -xf $rel_dir/${date}.tar -C $rel_dir"
      exit 0
    fi
  fi
  echo "Could not auto-resolve asset for $date. Steps:"
  echo "  1) Open https://github.com/adsblol/globe_history_2024/releases"
  echo "  2) Find tag v${date}-planes-readsb-prod-0, copy .tar link"
  echo "  3) wget https://github.com/adsblol/globe_history_2024/releases/download/v${date}-planes-readsb-prod-0/<file>.tar -O $rel_dir/${date}.tar"
  echo "  4) tar -xf $rel_dir/${date}.tar -C $rel_dir  # gzip JSON per aircraft"
  echo "  5) Also check RELEASES.md in repo root for grep-able links"
}

# ATCO2 1h — 871 clips, parquet on Hugging Face Jzuluaga/atco2_corpus_1h
fetch_atco2() {
  echo "→ Jzuluaga/atco2_corpus_1h (free 1h subset, 871 rows, Czech airspace)"
  echo "  Full 5281h via ELDA (paid). Python:"
  cat <<'PY'
pip install datasets soundfile --break-system-packages  # or pipx
python3 - <<'PYEOF'
from datasets import load_dataset
ds = load_dataset("Jzuluaga/atco2_corpus_1h", split="test")
print(f"rows: {len(ds)}", ds[0].keys())
print(ds[0]["text"][:200])
# Save wavs:
# import soundfile as sf
# for i, row in enumerate(ds):
#     sf.write(f"atco2_clip_{i:04d}.wav", row["audio"]["array"], row["audio"]["sampling_rate"])
PYEOF
PY
  if command -v python3 >/dev/null 2>&1; then
    read -rp "Download now via datasets? [y/N] " ans
    if [[ "$ans" == y* ]]; then
      python3 - <<'PYEOF'
from datasets import load_dataset
ds = load_dataset("Jzuluaga/atco2_corpus_1h", split="test")
print(f"loaded {len(ds)} rows, first text: {ds[0]['text'][:120]!r}")
PYEOF
    fi
  fi
}

case "${1:-help}" in
  all) fetch_adsbex_hist 2026-09-01; fetch_adsbex_traces 2026-09-01 d6; fetch_adsblol 2024-06-15; fetch_atco2 ;;
  adsbex) fetch_adsbex_traces "${2:-2026-09-01}" "${3:-d6}" ;;
  adsbex-hist) fetch_adsbex_hist "${2:-2026-09-01}" ;;
  adsblol) fetch_adsblol "${2:-2024-06-15}" ;;
  atco2) fetch_atco2 ;;
  *) echo "Usage: $0 {all|adsbex [YYYY-MM-DD d6]|adsbex-hist [YYYY-MM-DD]|adsblol [YYYY-MM-DD]|atco2}"; exit 1 ;;
esac
