# AeroPulse-NG: Tactical Radar & Civil-Military Airspace Surveillance Engine

**AeroPulse-NG** is an air-gapped, offline-first tactical radar and airspace surveillance platform engineered for civil air traffic management (NAMA) and defense command centers (Nigerian Air Force).

Built on a **Tauri v2 + Rust + WebGL** architecture, it ingests multi-source radio telemetry, executes high-speed 3D kinematic dead-reckoning, and fuses offline meteorological streams without external internet dependencies.

---

## Technical Specifications

### Engine & System Architecture
* **Language Stack:** Rust (High-throughput ingestion & EKF math), Python (Sidecar analytics & AWOS serial reader), TypeScript (WebGL 60 FPS radar UI).
* **Application Framework:** Tauri v2 with native multi-window rendering.
* **Network Isolation:** 100% offline-first execution designed for air-gapped workstations.
* **Target Platforms:** Cross-platform native binary packaging (`.AppImage` on Linux / `.msi` on Windows).

### Ingestion & Signal Processing
* **Triple-SDR Concurrent Ingestion Engine:** Dedicated USB hardware routing using `libusb` and device serial locking.
* **Mode S ADS-B Decoder (1090 MHz):** Asynchronous parsing of Downlink Format 17 (`DF=17`) packets, extracting Compact Position Reporting (CPR) lat/long coordinates, barometric altitude, and ground speed vectors.
* **ACARS VHF Decoder (131.550 MHz):** Continuous over-the-air capture and parsing of D-ATIS and regional METAR weather frames.

### Kinematics & Safety Algorithms
* **Extended Kalman Filter (EKF):** 6-state continuous smoothing ($x, y, z, v_x, v_y, v_z$) to eliminate sensor noise and multipath jitter.
* **Dead-Reckoning Extrapolation:** Kinematic path projection during transponder masking, terrain shadowing, or RF dropouts.
* **3D Short-Term Conflict Alerts (STCA):** Sub-millisecond $R^*$-Tree spatial indexing executing pairwise separation checks ($5\text{ NM}$ horizontal / $1000\text{ ft}$ vertical threshold) within a 2-minute lookahead window.

### Triple-Fusion Offline Weather Pipeline
* **Surface Layer:** Direct RS-232/RS-485 serial polling of airfield Automated Weather Observing Systems (AWOS) for local QNH pressure, surface wind, and temperature.
* **Terminal Layer:** Over-the-air VHF ACARS D-ATIS decoding covering regional airfield approach corridors.
* **High-Altitude En-Route Layer:** Airborne weather Extraction from Mode S BDS 4,4 (wind speed/dir & temperature) and BDS 4,5 (turbulence/icing) transponder registers.
* **3D Spatial Interpolation:** Weighted meteorological fusion matrix refining trajectory dead-reckoning during Harmattan dust layers and severe weather fronts.

### Defense & Military Module
* **Dark-Target & Anomaly Detection:** Heuristic detection for non-cooperative aircraft operating with disabled or unflagged transponders.
* **Geofence Incursion Warning:** Dynamic spatial boundary monitoring for restricted military sectors, operational forward bases, and critical infrastructure.
* **Tactical Intercept Calculator:** Minimum-time speed, bearing, and altitude intercept vector generation for quick-reaction defense aircraft.
* **Civil-Military Access Control:** Built-in Role-Based Access Control (RBAC) separating standard ATC views from classified military operational overlays.

---

## Hardware Architecture

The platform supports a concurrent 3-receiver Software Defined Radio (SDR) ingestion topology:

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                     TRIPLE-SDR HARDWARE INGESTION PIPELINE                  │
│                                                                             │
│  [ Antenna #1: 1090 MHz ] ────► [ SDR #1: ADS-B ]  ──┐                      │
│                                                      │                      │
│  [ Antenna #2: 131.55 MHz] ───► [ SDR #2: ACARS ]  ──┼──► [ Powered USB Hub ]│
│                                                      │           │          │
│  [ Antenna #3: VHF Guard ] ───► [ SDR #3: TAC SCAN] ─┘           │          │
│                                                                  ▼          │
│  [ Airfield AWOS Mast ]  ────── (RS-485 Serial) ────────► [ Host PC ]     │
└─────────────────────────────────────────────────────────────────────────────┘
```

### Bill of Materials (BOM)

#### Prototype & Bench Setup
* **Host Machine:** Linux PC / Laptop or Raspberry Pi 5.
* **Receivers:** $3\times$ RTL-SDR Blog v4 USB Dongles.
* **Antennas:** $1\times$ 1090 MHz tuned whip antenna + $2\times$ Telescopic VHF airband dipoles.
* **USB Infrastructure:** $1\times$ Active Powered USB 3.0 Hub.

#### Field & Operational Station Setup
* **Compute Host:** Industrial Mini-PC (Intel Core i5, 16GB DDR5 RAM, 512GB NVMe SSD).
* **RF Receivers:** $3\times$ Metal-shielded FlightAware Pro Stick Plus / RTL-SDR v4 units.
* **Antenna Mast:** $1\times$ 7dBi 1090 MHz Fiberglass Omnidirectional Antenna + $1\times$ Outdoor VHF Airband Dipole ($118\text{--}137\text{ MHz}$).
* **Cabling & Filters:** LMR-400 coaxial cables + 1090 MHz / VHF bandpass filters & inline LNAs.
* **Display Output:** Dual 2K Displays (Display 1: WebGL Tactical Radar Canvas; Display 2: Operations & Weather HUD).

---

## Repository Layout

```text
aeropulse-ng/
├── .github/workflows/               # Verification automation
├── build-scripts/                   # USB driver & sidecar packaging scripts
├── docs/                            # System architecture specs
├── python-sidecar/                  # DuckDB logger & AWOS serial reader
├── src-tauri/                       # Rust Core Engine & Tauri Backend
│   ├── src/
│   │   ├── hardware/                # Multi-SDR ingestion & bit-parsers
│   │   ├── kinematics/              # EKF, dead-reckoning & STCA math
│   │   ├── weather_fusion/          # 3D Weather matrix interpolation
│   │   ├── defense/                 # Anomaly detection & geofencing
│   │   └── commands/                # Tauri IPC bridge handlers
└── src/                             # TypeScript & WebGL Radar Frontend
    ├── components/                  # WebGL Canvas, Flight Data Blocks, HUDs
    ├── hooks/                       # Custom IPC state hooks
    └── types/                       # Shared data contracts
    
    
## Development Setup

### System Prerequisites (Ubuntu / Linux)

```
# Install core build tooling, C headers, and USB libraries
sudo apt update && sudo apt install -y \
  build-essential \
  cmake \
  pkg-config \
  libusb-1.0-0-dev \
  librtlsdr-dev

# Blacklist default kernel TV tuner driver
echo "blacklist dvb_usb_rtl28xxu" | sudo tee /etc/modprobe.d/blacklist-rtl.conf

# Install udev rules for non-root USB device access
```
sudo wget -O /etc/udev/rules.d/20-rtlsdr.rules [https://raw.githubusercontent.com/osmocom/rtl-sdr/master/rtl-sdr.rules](https://raw.githubusercontent.com/osmocom/rtl-sdr/master/rtl-sdr.rules)
sudo udevadm control --reload-rules && sudo udevadm trigger
```

### Dependency Installation
```
# 1. Setup Python Virtual Environment & Sidecar Dependencies
cd python-sidecar
python3 -m venv venv
source venv/bin/activate
pip install duckdb pyserial metar pytest
deactivate
cd ..

# 2. Install TypeScript & Frontend Dependencies
npm install

# 3. Verify Cargo Rust Dependencies
cd src-tauri
cargo check
cd ..
```

### Running in Development Mode

```
# Start the Tauri v2 multi-window application
npm run tauri dev
```

* **Display 1 (Primary Radar)**: http://localhost:1420/index.html

* **Display 2 (Operations HUD)**: http://localhost:1420/hud.html


## License & Security

* Proprietary Dual-Use Airspace Surveillance Platform. All rights reserved.
