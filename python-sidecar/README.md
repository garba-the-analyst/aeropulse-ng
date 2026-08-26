# Python Sidecar — Persistence & AWOS Bridge

Path: `python-sidecar/`
Tests: 14 pytest cases · live-process smoke verified

A single-responsibility companion process serving two jobs the Rust core
deliberately does not own: **DuckDB flight recording** and **RS-485 AWOS serial
ingestion**. Communicates with the engine over NDJSON stdio; dies quietly rather
than ever taking surveillance down.

---

## Wire Protocol (one JSON object per line)

```text
Rust → Python                          Python → Rust
{"cmd":"init","db_path":"..."}   ──►   {"ev":"ready"}
{"cmd":"log_tracks","tracks":[…]} ──►  {"ev":"db_ack","rows":N}
{"cmd":"shutdown"}                ──►  {"ev":"awos", qnh_hpa… observed_ms}
```

Malformed inbound lines log to stderr and are skipped. Every outbound frame is
flushed immediately — the Rust reader has no line-buffering patience.

## Files

| Path | Role |
|---|---|
| `main.py` | stdio command loop, AWOS thread lifecycle |
| `db/schema.sql` | `track_positions` · `stca_alerts` · `weather_observations` |
| `db/duckdb_engine.py` | transactional batch writer + readers |
| `weather/awos_serial.py` | sentence codec + serial/simulated mast |
| `weather/metar_parser.py` | dependency-free METAR/SPECI decoder |
| `tests/test_awos_parser.py` | the suite (codecs, parsers, engine durability) |

## AWOS Sentence Format

```
$AWOS,QNH,1013.2,WIND,182.0,WSPD,11.4,TMP,33.5,DEW,24.2,VIS,8000*7A
       └ hPa          └ dir°      └ kt        └ °C     └ °C      └ m    └ XOR checksum
```

* Checksum is XOR of all bytes before `*`; corrupted frames return `None` —
  bad mast data must never reach the fusion matrix.
* Frames missing QNH are invalid by contract (partial groups otherwise parse).
* `format_awos_sentence()` is the inverse — used by fixtures and hardware bring-up.

**Serial → simulated degradation:** the reader probes `/dev/ttyUSB* /ACM*/S*`
at 9600 baud when pyserial exists; any failure flips to the synthetic mast
(diurnal temperature curve, bounded random-walk wind/QNH, Kano harmattan
afternoon baseline) so the HUD always has surface truth labelled `AWOS-SIM`.

## METAR Parser

Regex-light extraction of the groups the fusion chain consumes: station/type
header, `dddffGggKT` wind (incl. `VRB`), 4-digit visibility (CAVOK ⇒ 10 km),
`TT/DT` temperatures with `M` = negative, `Qnnnn` QNH. Never raises — degraded RF
text yields whatever decoded, plus `is_harmattan_profile()` haze heuristic
(1.5–9 km visibility with warm temps).

## DuckDB Engine

* Schema applied idempotently from `schema.sql`.
* Track batches insert transactionally (`BEGIN … COMMIT`, rollback on error);
  numeric coercion failures become SQL NULL rather than exceptions.
* Durability proven across close/reopen in tests; alert/weather tables exist and
  `record_weather()` is wired — per-row alert writing is roadmap work.

## Running Standalone

```bash
cd python-sidecar
venv/bin/python main.py --db test.duckdb          # then paste protocol lines
venv/bin/python -m pytest tests -q                # suite
printf '{"cmd":"init","db_path":"t.duckdb"}\n' | venv/bin/python main.py --db t.duckdb
```

Exit codes: clean shutdown on `shutdown` command or stdin EOF; KeyboardInterrupt
handled. The engine treats sidecar absence as non-fatal by design — see
`src-tauri/src/sidecar.rs` for the Rust half of this contract.
