#!/usr/bin/env python3
"""AeroPulse-NG PDF documentation suite builder (fpdf2, no LaTeX needed).

Outputs into docs/pdf/:
  AeroPulse-NG_Pitch_Deck.pdf          10-slide NDAIE pitch (16:9, dark)
  AeroPulse-NG_Executive_Summary.pdf   2-page A4
  AeroPulse-NG_Architecture.pdf        deep-dive A4
  AeroPulse-NG_User_Manual.pdf         operations A4
  AeroPulse-NG_Technical_Reference.pdf compiled module documentation
"""
import os
from fpdf import FPDF

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
A = os.path.join(ROOT, "docs", "assets")
OUT = os.path.join(ROOT, "docs", "pdf")
os.makedirs(OUT, exist_ok=True)

FD = "/usr/share/fonts/truetype/dejavu/"
BG=(11,14,20); PANEL=(16,20,29); GRID=(42,50,65); TEXT=(200,210,224); DIM=(107,118,137)
CIVIL=(0,200,80); MILV=(124,77,255); AMBER=(255,179,0); ALERT=(255,23,68)
WHITE=(245,248,252); INK=(24,30,42); PAPER=(255,255,255)

class Theme:
    def __init__(self, pdf):
        pdf.add_font("dv", "", FD+"DejaVuSans.ttf")
        pdf.add_font("dv", "B", FD+"DejaVuSans-Bold.ttf")
        pdf.add_font("dvm", "", FD+"DejaVuSansMono.ttf")
        pdf.add_font("dvm", "B", FD+"DejaVuSansMono-Bold.ttf")

def ktxt(pdf, x, y, w, h, txt, sz=11, color=TEXT, bold=False, mono=False, align="L"):
    pdf.set_xy(x, y); pdf.set_font("dvm" if mono else "dv", "B" if bold else "", sz)
    pdf.set_text_color(*color)
    pdf.multi_cell(w, h, txt, align=align)

# ----------------------------------------------------------------- PITCH ---
class Slide(FPDF):
    def __init__(self):
        # fpdf2 interprets custom tuples as (height, width) before applying
        # orientation. Passing (167, 297) produces the intended 16:9 page.
        super().__init__(orientation="L", unit="mm", format=(167, 297))
        Theme(self)
        self.set_auto_page_break(False)
    def bg(self):
        self.set_fill_color(*BG); self.rect(0,0,297,167, style="F")
        self.set_draw_color(*GRID); self.set_line_width(0.2)
        for i in range(1, 7):
            self.line(0, i*167/7, 297, i*167/7)
        self.set_text_color(*TEXT)
    def chrome(self, n, total, section):
        self.set_font("dvm", "", 8); self.set_text_color(*DIM)
        self.set_xy(12, 159); self.cell(120, 4, f"AeroPulse-NG v2.4  ·  NDAIE 2026 Innovation Pitch")
        self.set_xy(250, 159); self.cell(35, 4, f"{section}   {n:02d} / {total:02d}", align="R")
        self.set_draw_color(*CIVIL); self.set_line_width(0.8)
        self.line(12, 157, 285, 157)
    def kicker(self, txt, color=CIVIL):
        self.set_xy(16, 16); self.set_font("dv","B",11); self.set_text_color(*color)
        self.cell(0, 6, txt.upper())
    def h1(self, txt, y=26, sz=30, color=WHITE):
        self.set_xy(16, y); self.set_font("dv","B",sz); self.set_text_color(*color)
        self.multi_cell(265, sz*0.55, txt)
    def bullets(self, items, x=16, y=None, w=265, sz=13, gap=8.5, color=TEXT, mark="■", mcolor=CIVIL):
        yy = y if y else self.get_y()+6
        for head, body in items:
            self.set_xy(x, yy); self.set_font("dv","B",sz); self.set_text_color(*mcolor)
            self.cell(7, sz*0.5, mark)
            self.set_text_color(*WHITE if not body else color)
            self.cell(w-7, sz*0.5, head)
            if body:
                self.set_xy(x+7, yy+sz*0.5); self.set_font("dv","",sz-2)
                self.set_text_color(*DIM); self.multi_cell(w-7, (sz-2)*0.5, body)
                yy += sz*0.5 + len(body)//(w//2)*0 + (sz-2)*0.5*max(1, (len(body)//90)+1) + 3
            else:
                yy += sz*0.5 + 4
        return yy

def pitch():
    total = 10
    pdf = Slide()
    # S1 title
    pdf.add_page(); pdf.bg()
    pdf.image(os.path.join(A,"logo.png"), 20, 30, 60)
    pdf.set_xy(95, 48); pdf.set_font("dv","B",40); pdf.set_text_color(*WHITE)
    pdf.cell(0, 16, "AeroPulse-NG")
    pdf.set_xy(96, 70); pdf.set_font("dv","B",15); pdf.set_text_color(*CIVIL)
    pdf.cell(0, 8, "TACTICAL RADAR & AIRSPACE SURVEILLANCE ENGINE")
    ktxt(pdf, 96, 84, 180, 10, "Air-gapped · offline-first · NCAA / ICAO Annex 5 SI-metric", 12, DIM, mono=True)
    ktxt(pdf, 96, 112, 180, 8.5,
         "National Drone & Aerospace Innovation Expo 2026\nNational Innovation Pitch Challenge — Flight Planning & Air Traffic Management",
         13, TEXT)
    pdf.set_draw_color(*GRID); pdf.line(96, 108, 270, 108)
    pdf.chrome(1, total, "TITLE")

    # S2 problem
    pdf.add_page(); pdf.bg(); pdf.kicker("The problem", ALERT); pdf.h1("Nigeria's low-level airspace is a surveillance gap")
    pdf.bullets([
        ("Radar is unaffordable at scale", "Primary/secondary installations cost millions of dollars each and demand specialist maintenance."),
        ("Coverage is uneven", "Low-level corridors, secondary aerodromes and terrain-shadowed sectors operate with minimal or no radar picture."),
        ("Weather degrades exactly when it matters", "Harmattan haze and convective storms hit visibility during diversion surges."),
        ("Non-cooperative traffic is invisible", "Transponder-off aircraft — smuggling, insecurity, rogue drones — defeat dependent surveillance."),
    ], y=48, sz=14)
    ktxt(pdf, 16, 128, 265, 7.5,
         "When connectivity or a radar head fails, controllers lose the traffic picture precisely when risk peaks.",
         13, AMBER, bold=True)
    pdf.chrome(2, total, "PROBLEM")

    # S3 solution
    pdf.add_page(); pdf.bg(); pdf.kicker("The solution"); pdf.h1("A radar workstation from USD 150 of radio hardware")
    pdf.image(os.path.join(A,"radar-scope.png"), 14, 42, w=180)
    pdf.bullets([
        ("Software-defined radios in, tactical picture out", None),
        ("1090 MHz Mode S ADS-B decode with CRC repair + CPR", None),
        ("6-state Kalman filtering and dead-reckoning through dropouts", None),
        ("Automatic conflict alerts (STCA) to ICAO Doc 4444 minima", None),
        ("Triple-fusion weather: AWOS + ACARS + satellite-free", None),
        ("100% offline — sovereign by construction", None),
    ], x=202, y=46, w=84, sz=11, gap=7)
    pdf.chrome(3, total, "SOLUTION")

    # S4 how it works
    pdf.add_page(); pdf.bg(); pdf.kicker("How it works"); pdf.h1("One decode path — synthetic or real RF", sz=26)
    pdf.image(os.path.join(A,"architecture.png"), 22, 44, w=253)
    pdf.chrome(4, total, "TECHNOLOGY")

    # S5 depth
    pdf.add_page(); pdf.bg(); pdf.kicker("Engineering depth"); pdf.h1("Safety-critical math, proven by tests", sz=26)
    cols = [
        ("DECODE", CIVIL, ["DF17 CRC-24 single-bit repair","Global CPR even/odd solve","Gillham altitude codec","ACARS CRC-16 + METAR/D-ATIS"]),
        ("KINEMATICS", MILV, ["6-state EKF, Joseph-form update","Mahalanobis gate chi2(3)=16.27","R*-tree spatial index (full)","STCA 9.26 km / 305 m / 120 s"]),
        ("WEATHER", AMBER, ["BDS 4,4/4,5 register codecs","ISA atmosphere + IDW blend","Vector-averaged wind","Harmattan dust-layer model"]),
        ("DEFENCE", ALERT, ["7700/7600/7500 alerting","Dark-target & silence detection","Polygon geofences + bands","Lead-pursuit intercept solver"]),
    ]
    x = 16
    for title, col, items in cols:
        pdf.set_fill_color(*PANEL); pdf.set_draw_color(*col); pdf.set_line_width(0.4)
        pdf.rect(x, 46, 64, 96, style="DF")
        pdf.set_xy(x+4, 50); pdf.set_font("dv","B",12); pdf.set_text_color(*col)
        pdf.cell(56, 6, title)
        yy = 62
        for it in items:
            pdf.set_xy(x+4, yy); pdf.set_font("dv","",9.5); pdf.set_text_color(*TEXT)
            pdf.multi_cell(56, 5, it); yy += pdf.get_y()-yy
        x += 70
    ktxt(pdf, 16, 148, 265, 8, "104 automated checks green: Rust 90 · Python 14 · TypeScript strict · production bundle", 11, CIVIL, mono=True)
    pdf.chrome(5, total, "TECHNOLOGY")

    # S6 demo timeline
    pdf.add_page(); pdf.bg(); pdf.kicker("Live demonstration"); pdf.h1("A scripted airspace you can watch", sz=26)
    pdf.image(os.path.join(A,"timeline.png"), 16, 46, w=265)
    pdf.bullets([
        ("Judges watch a conflict develop in real time", "Emergency squawk, intercept geometry, RF blackout and dark-target flags — every alert path exercised in 3 minutes."),
    ], y=126, sz=13)
    pdf.chrome(6, total, "DEMO")

    # S7 weather
    pdf.add_page(); pdf.bg(); pdf.kicker("Offline weather"); pdf.h1("Three radio sources, zero internet", sz=26)
    pdf.image(os.path.join(A,"fusion.png"), 30, 44, w=237)
    pdf.bullets([
        ("Harmattan-aware", "Visibility-driven dust-layer estimate tuned to West African dry-season operations."),
        ("Honest degradation", "Every sample carries source provenance and a confidence score."),
    ], y=128, sz=13)
    pdf.chrome(7, total, "WEATHER")

    # S8 standards
    pdf.add_page(); pdf.bg(); pdf.kicker("Standards alignment"); pdf.h1("NCAA / ICAO compliant by construction", sz=26)
    rows = [
        ("Units", "ICAO Annex 5 / NCAA — SI metric presentation", "km · m · km/h · hPa"),
        ("Surveillance", "DO-260B Mode S / ADS-B MOPS", "DF17, CPR, Gillham"),
        ("Separation", "ICAO Doc 4444 radar minima", "9.26 km / 305 m / 120 s"),
        ("Symbology", "MIL-STD-2525D + HF-STD-010A", "diamond/caret/quad frames"),
        ("Identity", "ICAO Annex 10 squawk semantics", "7500 / 7600 / 7700"),
        ("Integrity", "NDAIE rulebook — AI use declared", "original work, MIT/Apache deps"),
    ]
    yy = 50
    for a, b, c in rows:
        pdf.set_fill_color(*PANEL); pdf.rect(16, yy, 265, 13, style="F")
        pdf.set_xy(20, yy+2); pdf.set_font("dv","B",11); pdf.set_text_color(*CIVIL); pdf.cell(40, 8, a)
        pdf.set_font("dv","",11); pdf.set_text_color(*TEXT); pdf.cell(150, 8, b)
        pdf.set_font("dvm","",10); pdf.set_text_color(*DIM); pdf.cell(0, 8, c)
        yy += 15.5
    pdf.chrome(8, total, "STANDARDS")

    # S9 impact
    pdf.add_page(); pdf.bg(); pdf.kicker("Impact"); pdf.h1("What changes if this ships", sz=28)
    cards = [
        (">99%", "capital cost reduction per surveillance station\n(USD 150 receiver vs multi-million radar head)", CIVIL),
        ("24/7", "conflict alerting at aerodromes that can\nnever justify conventional radar", MILV),
        ("100%", "sovereign: no cloud, no internet, no\nforeign dependency in the loop", AMBER),
        ("Open", "engineering reference platform for Nigerian\navionics talent — civil + military dual-use", ALERT),
    ]
    x = 16
    for big, small, col in cards:
        pdf.set_fill_color(*PANEL); pdf.set_draw_color(*col); pdf.set_line_width(0.5)
        pdf.rect(x, 52, 62, 74, style="DF")
        pdf.set_xy(x+4, 60); pdf.set_font("dv","B",26); pdf.set_text_color(*col)
        pdf.cell(54, 12, big)
        pdf.set_xy(x+4, 78); pdf.set_font("dv","",10); pdf.set_text_color(*TEXT)
        pdf.multi_cell(54, 5, small)
        x += 68
    ktxt(pdf, 16, 133, 265, 6.5,
         "Operational: NAMA civil ATC + NAF tactical command share one picture.\n"
         "Economic: sector-wide low-level coverage becomes a line item, not a capital programme.", 12, TEXT)
    pdf.chrome(9, total, "IMPACT")

    # S10 roadmap/team
    pdf.add_page(); pdf.bg(); pdf.kicker("Readiness & next steps"); pdf.h1("Working prototype today, RF tap next", sz=26)
    pdf.bullets([
        ("Now — verified prototype", "Full engine + dual-display application; 104 automated checks; live demo scenario."),
        ("Next — real RF front-end", "RTL-SDR I/Q tap feeding the existing decoder (interfaces already shaped)."),
        ("Then — field trial", "Physical AWOS mast, Comm-B weather interrogation, station hardening."),
        ("Always — open engineering reference", "Documentation suite shipped with the codebase."),
    ], y=46, sz=13)
    ktxt(pdf, 16, 118, 265, 6.5,
         "Integrity declaration: developed with declared generative-AI assistance per NDAIE rules;\n"
         "all third-party components MIT/Apache licensed. Team roster submitted via official form.",
         11, DIM, mono=True)
    ktxt(pdf, 16, 140, 265, 10, "garba-the-analyst · github.com/garba-the-analyst/aeropulse-ng", 12, CIVIL, mono=True)
    pdf.chrome(10, total, "CLOSE")

    pdf.output(os.path.join(OUT, "AeroPulse-NG_Pitch_Deck.pdf"))
    print("pitch deck:", total, "slides")

# ------------------------------------------------------- A4 DOC ENGINE ----
class Doc(FPDF):
    def __init__(self, title, subtitle):
        super().__init__(unit="mm", format="A4")
        Theme(self)
        self.set_auto_page_break(True, margin=20)
        self.title = title; self.subtitle = subtitle
        self.set_margins(18, 24, 18)
    def header(self):
        self.set_fill_color(*INK); self.rect(0, 0, 210, 16, style="F")
        self.set_xy(10, 4); self.set_font("dv","B",11); self.set_text_color(*WHITE)
        self.cell(0, 5, self.title)
        self.set_xy(10, 9.5); self.set_font("dvm","",8); self.set_text_color(*DIM)
        self.cell(0, 4, self.subtitle)
        self.set_draw_color(*CIVIL); self.set_line_width(0.8)
        self.line(10, 16, 200, 16)
        self.set_y(22)
    def footer(self):
        self.set_y(-14); self.set_font("dvm","",8); self.set_text_color(*DIM)
        self.cell(0, 6, f"AeroPulse-NG v2.4 — proprietary · generated {self._stamp()}", align="L")
        self.set_xy(-25, -14); self.cell(15, 6, str(self.page_no()), align="R")
    @staticmethod
    def _stamp():
        import datetime
        return datetime.date.today().isoformat()
    def h1(self, txt):
        if self.get_y() > 250: self.add_page()
        self.ln(2); self.set_font("dv","B",17); self.set_text_color(*INK)
        self.multi_cell(0, 8, txt)
        self.set_draw_color(*CIVIL); self.set_line_width(0.6)
        self.line(18, self.get_y()+1, 90, self.get_y()+1); self.ln(4)
    def h2(self, txt):
        if self.get_y() > 260: self.add_page()
        self.ln(1.5); self.set_font("dv","B",12.5); self.set_text_color(*MILV)
        self.multi_cell(0, 6.5, txt); self.ln(1)
    def p(self, txt, sz=10.5):
        self.set_font("dv","",sz); self.set_text_color(*INK)
        self.multi_cell(0, 5.2, txt); self.ln(1.2)
    def bullets(self, items, sz=10.5):
        self.set_x(self.l_margin); self.set_font("dv","",sz)
        for it in items:
            self.set_text_color(*CIVIL); self.cell(5, 5.2, "•")
            self.set_text_color(*INK); self.multi_cell(0, 5.2, it)
            self.set_x(self.l_margin)  # fpdf2 multi_cell defaults to new_x=RIGHT
        self.ln(1.2)
    def code(self, txt, sz=8.5):
        self.set_font("dvm","",sz); self.set_text_color(*INK)
        self.set_fill_color(242,244,248)
        for ln in txt.split("\n"):
            self.multi_cell(0, 4.4, ln, fill=True)
            self.set_x(self.l_margin)
        self.ln(1.5)
    def table(self, headers, rows, widths):
        self.set_font("dv","B",9.5); self.set_fill_color(*INK); self.set_text_color(*WHITE)
        for h, w in zip(headers, widths): self.cell(w, 7, f" {h}", fill=True)
        self.ln()
        self.set_font("dv","",9.3); self.set_text_color(*INK)
        fill = False
        for r in rows:
            maxh = 7
            for cell, w in zip(r, widths):
                if self.get_string_width(cell) > w-3: maxh = max(maxh, 6* (self.get_string_width(cell)//(w-3) + 1))
            for i, (cell, w) in enumerate(zip(r, widths)):
                self.set_fill_color(*(246,248,251) if fill else (255,255,255))
                self.cell(w, maxh, f" {cell}", fill=True)
            self.ln(); fill = not fill
        self.ln(2)
    def image_fit(self, name, w=174):
        path = os.path.join(A, name)
        if os.path.exists(path):
            if self.get_y() > 200: self.add_page()
            self.image(path, x=self.l_margin, w=w)
            self.set_x(self.l_margin); self.ln(3)

def exec_summary():
    d = Doc("AeroPulse-NG — Executive Project Summary", "NDAIE 2026 · Flight Planning & Air Traffic Management")
    d.add_page()
    d.h1("AeroPulse-NG: Offline Tactical Radar & Airspace Surveillance")
    d.p("Entry category: Flight Planning & Air Traffic Management · Team lead submission via official NDAIE form · Word count 285/300 (rulebook §3).")
    d.h2("Problem")
    d.p("Nigeria's airspace is monitored largely through expensive imported primary and secondary surveillance radar — installations costing millions of dollars, demanding specialist maintenance, yet still leaving low-altitude corridors, secondary aerodromes and terrain-shadowed sectors under-covered. When connectivity or a radar head fails, controllers lose the traffic picture precisely when harmattan haze, diversion surges or military activity raise risk. Non-cooperative and transponder-silent aircraft widen the gap further.")
    d.h2("Proposed solution")
    d.p("AeroPulse-NG is an air-gapped surveillance engine that turns commodity software-defined radios (about USD 150 of hardware) plus a standard PC into a working tactical radar display. It ingests over-the-air aviation telemetry with zero internet or cloud dependency, giving civil air traffic management (NAMA) and tactical command (NAF) a sovereign, locally maintainable traffic picture at a fraction of conventional cost.")
    d.h2("Technical approach")
    d.p("A Rust/Tauri backend demodulates 1090 MHz Mode S ADS-B, resolving positions through Compact Position Reporting; a six-state Extended Kalman Filter smooths jitter and dead-reckons tracks through 30-second RF dropouts. An R*-tree spatial index drives continuous Short-Term Conflict Alerting against ICAO Doc 4444 separation minima (rendered 9.26 km / 305 m) within a 120-second lookahead. A triple-fusion weather matrix merges AWOS serial telemetry, ACARS D-ATIS broadcasts and Mode S BDS 4,4/4,5 downlinks into an offline 3D wind/temperature model tuned for harmattan operations. Defence overlays flag dark targets, squawk emergencies and geofence incursions, and compute intercept geometry. A hardware-accelerated display layer renders HF-STD-010A-compliant screens across dual operator monitors, in SI metric units per NCAA / ICAO Annex 5 policy.")
    d.h2("Likely impact")
    d.p("Operational: conflict alerting and fused weather at aerodromes that cannot justify radar. Economic: over 99% capital reduction per station. Sovereignty: fully offline national capability. Educational: an open engineering reference for Nigerian avionics talent. Development stage: prototype core (decoder, EKF, STCA engines) verified by 104 automated checks; dual-display operator interface complete; RF front-end and field-trial hardware integration next.")
    d.add_page()
    d.h1("Verification Snapshot")
    d.table(["Suite","Checks","Covers"],[
        ["Rust engine (cargo)","90","Decoders, CPR, EKF, R*-tree, STCA, fusion pipeline end-to-end"],
        ["Python sidecar (pytest)","14","AWOS codec, METAR parser, DuckDB durability"],
        ["TypeScript strict (tsc)","clean","Wire contracts, SI unit policy, components"],
        ["Production bundle (vite)","build","Dual-window workspace, 65 KB gzipped"],
    ],[45,20,129])
    d.image_fit("radar-scope.png", 170)
    d.h2("Demonstration scenario")
    d.bullets([
        "T+45 s — VL604 squawks 7700 (general emergency) with TC-28 status frame.",
        "T+70 s — NAF911 intercept crosses VL604's path: predicted miss under 1 km triggers STCA.",
        "T+112 s — VL604 enters RF dropout: EKF dead-reckoning keeps the picture alive.",
        "Rolling — dark target flagged, ACARS METAR and AWOS frames feeding the fusion matrix.",
    ])
    d.output(os.path.join(OUT, "AeroPulse-NG_Executive_Summary.pdf"))
    print("executive summary: 2 pages")

def architecture_pdf():
    d = Doc("AeroPulse-NG — System Architecture", "design rationale · data flow · decisions · roadmap")
    d.add_page()
    d.h1("1. System Overview")
    d.p("AeroPulse-NG is an air-gapped surveillance workstation: software-defined radios feed a Rust fusion engine that publishes a 60 Hz tactical snapshot to two operator displays, while a Python sidecar persists flight history and reads the AWOS weather mast. There is no internet dependency anywhere in the pipeline.")
    d.image_fit("architecture.png", 178)
    d.h2("1.1 Guiding constraints")
    d.bullets([
        "Air-gapped by contract — no build-time or runtime network access; vector-drawn PPI instead of map tiles.",
        "One decode path — synthetic traffic is encoded into genuine DF17 frames (CRC parity included) and pushed through the production decoder.",
        "Deterministic core, async shell — the engine is a pure state machine driven by explicit timestamps; the Tauri runtime owns clocks and I/O.",
        "Presentation converts once — engine internals stay native (metres, m/s; wire ft/kt per DO-260B); SI formatting happens only at the display boundary.",
    ])
    d.add_page()
    d.h1("2. Ingestion & Decoding")
    d.p("Three receiver channels bind to factory serial numbers (SDR-1 ADS-B 1090 MHz, SDR-2 ACARS 131.55 MHz, SDR-3 VHF guard). The Mode S decoder validates CRC-24 parity (remainder equals ICAO address, or zero for even parity), repairs single-bit errors across all 112 positions, and extracts identity, position, velocity and aircraft status. Position resolves through Compact Position Reporting: an even/odd frame pair inside a 10-second epoch yields absolute latitude/longitude via the standard zone arithmetic (dlat 6° / 360/59°, NL-table guard, longitude index m). Altitude decodes the 12-bit field with the Q-bit arithmetic path or legacy Gillham gray code. ACARS frames parse SOH..ETX structure with CCITT-CRC integrity; weather-bearing labels classify into METAR/SPECI and D-ATIS products.")
    d.h2("2.1 The synthetic feed")
    d.p("The bench simulator advances a six-aircraft truth fleet and encodes every observation into genuine DF17 frames — CRC address parity included — before handing them to the same decoder. Deterministic seeding (xorshift64*) makes the frame timeline bit-reproducible. Scripted events: emergency squawk at T+45 s, an analytically solved intercept crossing VL604's path at T+70 s with a sub-kilometre predicted miss, scheduled RF dropouts, and a dark target that never broadcasts identity.")
    d.h1("3. Kinematics")
    d.p("Each track carries a 6-state Extended Kalman Filter [x, y, z, vx, vy, vz] on the local East-North-Up tangent plane. The process model is piecewise-constant white-noise acceleration; covariance updates use the Joseph form for numerical stability; a Mahalanobis gate (chi-squared, 3 dof, 16.27) engages after a 12-fix acquisition warm-up — before the velocity state converges, constant-velocity prediction errors legitimately exceed steady-state bounds. During signal loss the filter simply keeps predicting: dead reckoning falls out of the same equations, and the display flags coasting tracks with dashed halos.")
    d.p("Conflict detection bulk-loads every track's 120-second predicted corridor into an R*-tree (Beckmann split criteria with forced reinsertion), retrieves candidate pairs by rectangle query, then refines with 5-second time-stepping that requires simultaneous infringement of the Doc 4444 minima — 9.26 km horizontal and 305 m vertical in the SI presentation, evaluated internally as the regulatory 5 NM / 1,000 ft constants.")
    d.add_page()
    d.h1("4. Triple-Fusion Weather")
    d.image_fit("fusion.png", 150)
    d.p("Surface truth arrives from the AWOS mast through the Python sidecar; terminal-sector text arrives over VHF as ACARS D-ATIS/METAR; en-route profiles come from Mode S BDS 4,4 (wind) and 4,5 (temperature deviation) register downlinks. Airborne samples accumulate in a bounded deque (15-minute TTL) and blend by inverse-distance weighting with an altitude-mismatch penalty — wind averages as vectors, never by averaging degrees. Pressure blends toward the ICAO standard atmosphere in proportion to altitude mismatch; every query returns a confidence score from source diversity and range. A Harmattan dust-layer estimate rises from visibility degradation plus a convective thermal bump above 28 °C surface temperature.")
    d.h1("5. Defence Overlays")
    d.bullets([
        "Squawk semantics: 7700 general emergency, 7600 radio failure, 7500 unlawful interference — mapped to alert states with display priority.",
        "Dark targets: identity-less emitters flag after a 30 s arming window; previously cooperative tracks that go silent flag after 45 s.",
        "Geofencing: ray-cast polygon inclusion with floor/ceiling bands; entry events only, 30 s hold-down, exit re-arms.",
        "Intercept solving: damped fixed-point pursuit iteration with forward-simulation verification; minimum feasible speed by bisection.",
    ])
    d.h1("6. Host Integration")
    d.p("The Tauri runtime owns the clock: a 60 Hz tokio task ticks the simulator, drains queued events through the engine, and emits telemetry://snapshot to both windows. Twelve IPC commands cover snapshot pulls, status, weather sampling, geofence administration, emergency override and intercept computation. The sidecar speaks NDJSON over stdio (init / log_tracks / shutdown; ready / awos / db_ack) and degrades gracefully — losing persistence never takes surveillance down.")
    d.add_page()
    d.h1("7. Key Decisions & Trade-offs")
    d.table(["Decision","Rationale","Rejected alternative"],[
        ["Hand-rolled 6x6 linear algebra","Zero transitive native deps; bit-exact reproducibility on air-gapped hosts","nalgebra (audit surface)"],
        ["Bulk R*-tree rebuild per pass","Tens of microseconds at operational scale; simpler than incremental rebalancing","k-d tree / grid"],
        ["Joseph-form covariance","Stability across the wide dynamic range of acquisition","(I-KH)P form"],
        ["Queue-then-drain ingestion","Fixes double-prediction: measurements fuse against state propagated exactly once","fuse-inside-ingest"],
        ["JSON IPC at 60 Hz","Webview simplicity; ~40 KB/frame measured; zero-copy path documented","SharedArrayBuffer ring"],
        ["Canvas2D renderer","60 fps sustained at operational track counts; no GPU dependency","Pixi/WebGL now"],
    ],[52,80,42])
    d.h1("8. Roadmap")
    d.bullets([
        "RTL-SDR I/Q front-end: libusb transfers, DC/PPM correction, Manchester sync feeding the existing decoder.",
        "Live FFT spectrum from the RF tap (plot currently synthesised from message-rate telemetry).",
        "Physical AWOS bring-up and Comm-B interrogation scheduling for live BDS weather.",
        "Alert/weather persistence writers; zero-copy IPC migration; multi-site handover and RBAC views.",
    ])
    d.output(os.path.join(OUT, "AeroPulse-NG_Architecture.pdf"))
    print("architecture: pages", d.page_no())

def user_manual_pdf():
    d = Doc("AeroPulse-NG — User Manual", "setup · operations · troubleshooting")
    d.add_page()
    d.h1("1. Requirements & Install")
    d.table(["Component","Minimum","Notes"],[
        ["OS","Ubuntu 22.04/24.04 (X11)","Windows/macOS untested here"],
        ["CPU / RAM","4 cores / 8 GB","60 fps canvas + EKF inside budget"],
        ["Rust / Node / Python",">= 1.77 / 20 / 3.10","rustup + npm + sidecar venv"],
        ["Hardware","optional","3x RTL-SDR + AWOS; synthetic feed without"],
    ],[38,50,86])
    d.code("npm install\ncd python-sidecar && python3 -m venv venv && venv/bin/pip install duckdb pyserial pytest && cd ..\n./scripts/verify.sh")
    d.h2("1.1 Webkit headers without sudo")
    d.p("The desktop shell compiles against webkit2gtk-4.1 development headers; the runtime library ships with Ubuntu. scripts/webkit-env.sh prepares a local prefix by downloading the -dev packages with apt-get download and extracting them via dpkg -x into ~/ap-deps/merged. With sudo, a plain apt install of libwebkit2gtk-4.1-dev and libsoup-3.0-dev replaces the whole dance.")
    d.h1("2. Launch Modes")
    d.code("scripts/desktop.sh        # full application (both windows + runtime + sidecar)\ncargo run --manifest-path src-tauri/Cargo.toml --no-default-features \\\n        --bin aeropulse-headless   # terminal tactical console\nnpm run dev               # browser UI (synthetic feed) -> http://localhost:1420\ncd python-sidecar && venv/bin/python main.py --db test.duckdb")
    d.p("First desktop launch compiles the dependency tree once; later starts take roughly fifteen seconds. The verification battery ./scripts/verify.sh runs every suite in one command.")
    d.add_page()
    d.h1("3. Screen Reference")
    d.image_fit("radar-scope.png", 172)
    d.h2("3.1 Master command bar (Screen 0)")
    d.bullets([
        "Channel health chips for SDR-1/2/3, AWOS and the EKF engine — green nominal, flashing red fault.",
        "Range-scale selector in kilometres (25-450 km, SI policy) with rings at 100/200/300 km.",
        "ZULU and WAT (UTC+1, no DST) clocks; EMERG 7700 override arms fleet-wide emergency for 30 s.",
    ])
    d.h2("3.2 Display 1 — tactical radar")
    d.p("Rotating sweep with phosphor fade; MIL-STD-2525D symbols (diamond civil, caret military, quadrangle unknown/anomaly); three-line Flight Data Blocks in SI (altitude metres, speed km/h, source and squawk); dead-reckoning halos; flashing red STCA connectors with minima labels; geofence polygons; R&B ruler tool (click anchor, move, double-click clears); intercept calculator panel.")
    d.h2("3.3 Display 2 — operations HUD")
    d.p("Flight-strip bay (arrivals/departures with green/amber/red edge states), triple-fusion weather panel (AWOS surface block, upper-air matrix with provenance, dust-layer top, raw D-ATIS/METAR feed), threat matrix, and diagnostics with the spectrum plot and EKF latency gauge against the 2 ms ceiling.")
    d.add_page()
    d.h1("4. Operational Walkthrough")
    d.bullets([
        "Boot: six tracks already filtered (EKF converges in ~12 fixes); weather fills within the first ACARS/AWOS cadence.",
        "T+45 s: VL604 squawks 7700 — symbol, FDB annotation and strip edge go red; threat matrix logs the emergency.",
        "T+60-80 s: NAF911 crosses VL604's path — red dashed STCA connector flashes at 2 Hz with minima label.",
        "T+112 s: VL604 drops off RF — dashed coasting halo, FDB source flips to DR, leader line keeps projecting.",
        "Rolling: dark target flagged after its arming window; ACARS METAR every 45 s; AWOS every 30 s.",
    ])
    d.h1("5. Data & Persistence")
    d.p("The sidecar writes aeropulse.duckdb (track_positions, stca_alerts, weather_observations) with transactional batches every 2 s. Query example:")
    d.code("python-sidecar/venv/bin/python - <<'PY'\nimport duckdb\nc = duckdb.connect('python-sidecar/aeropulse.duckdb', read_only=True)\nprint(c.execute('SELECT callsign, count(*) FROM track_positions GROUP BY 1').fetchall())\nPY")
    d.h1("6. Troubleshooting")
    d.table(["Symptom","Cause","Fix"],[
        ["webkit pkg-config not found","dev headers absent","scripts/webkit-env.sh or apt install"],
        ["cargo run: which binary","older checkout","git pull; default-run is set in Cargo.toml"],
        ["Vite config-loader warning","pre-type:module layout","fixed; update checkout"],
        ["No weather data","sidecar missing","check [sidecar] log lines; persistence degrades gracefully"],
        ["STCA never fires in browser","demo window timing","conflict window t=20-140 s after load; reload"],
    ],[52,48,74])
    d.output(os.path.join(OUT, "AeroPulse-NG_User_Manual.pdf"))
    print("user manual: pages", d.page_no())

def tech_ref_pdf():
    d = Doc("AeroPulse-NG — Technical Reference", "module documentation · wire contracts · test inventory")
    d.add_page()
    d.h1("1. Hardware Layer (src-tauri/src/hardware/)")
    d.p("Mode S decoder: frames left-align into a u128 register so accessors use spec MSB-first numbering. CRC-24 with generator x^24+...+x^3+1; validity = remainder equals ICAO (address parity) or zero (even parity); single-bit repair scans 112 flips. Global CPR pairs even/odd fractions inside a 10 s epoch with the NL-table guard. Altitude: Q-bit arithmetic (x25 ft) or Gillham gray (x500 ft). Velocity ME TC19 subtypes 1/2 give vector ground speed and track; 3/4 give heading and airspeed; vertical rate is a shared 9-bit x64 ft/min field.")
    d.p("ACARS: SOH MODE TAIL ACK LABEL BID [STX FLIGHT CR TEXT] ETX BCS16 grammar with CRC-16/CCITT-FALSE (reference vector 123456789 -> 0x29B1). Weather classification routes METAR/SPECI prefixes and D-ATIS label shapes. synthesize_frame() is public so the simulator and fixtures build wire-exact frames.")
    d.p("Registry: three channels bind to factory serials via ChannelPlan; UsbProbe trait offers a sysfs libusb-free enumeration and a simulated inventory; mismatched hardware renders channels offline instead of refusing boot.")
    d.h2("1.1 Simulator scenarios")
    d.bullets([
        "NAF911/912 intercept geometry solved analytically: crosses VL604 path at T+70 s, ~1 km miss (guaranteed STCA).",
        "VL604 squawk 7700 at T+45 s (event + TC-28 status frame); dropout windows exercise coasting.",
        "0BADC0 dark target transmits positions only; parity alternates per emitted frame (not per tick).",
        "xorshift64* seeded: same seed reproduces the byte-identical frame timeline.",
    ])
    d.h1("2. Kinematics Layer (src-tauri/src/kinematics/)")
    d.p("linalg.rs: row-major fixed-size kernels, Cholesky factorisation returning None on non-positive pivots, multi-RHS solving used for both Kalman gains and Mahalanobis distances. ekf.rs: 6-state EKF, sigma_a = 3.5 m/s^2 default, Joseph-form updates, gate chi2(3)=16.27 engaged after 12 accepted fixes. dead_reckoning.rs: SiteOrigin series-expanded metres-per-degree, leader-line projection, analytic closest approach. rtree.rs: full R* (overlap-enlargement ChooseSubtree, margin-minimising split, 30% forced reinsertion), arena-backed. stca_math.rs: corridor bulk-load, candidate retrieval, 5 s time-stepped refinement requiring simultaneous infringement.")
    d.h1("3. Weather Fusion (src-tauri/src/weather_fusion/)")
    d.p("BDS 4,4: WS[0..12] kt, WD[12..23] x360/2048 deg, validity flag, SAT[24..40] x0.25 C. BDS 4,5: turbulence/shear/microburst pairs, icing state with all-invalid sentinel, ISA deviation x0.1 C. Encoders and decoders round-trip. spatial_interp: ISA atmosphere (lapse 6.5 C/km, isothermal above tropopause), IDW with altitude penalty, vector-averaged wind, ISA-blended pressure, confidence from diversity x range; dust layer from visibility plus thermal bump above 28 C. Fusion engine: 512-node deque, 15 min TTL, priority chain AWOS > D-ATIS > METAR (first writer wins a gap, AWOS always overwrites).")
    d.h1("4. Defence (src-tauri/src/defense/)")
    d.p("AnomalyDetector: emergency squawk mapping (7700/7600/7500), identity-less emitter flagging after 30 s arming, absentee silence after 45 s; transition-only verdicts; squawk churn counted but never alerting alone. GeofenceMonitor: ray-cast polygons with vertical bands, entry-only events with occupancy sets, runtime activation toggles. Intercept: damped fixed-point pursuit iteration (tolerance 15 m, <=48 iterations) with forward-simulation feasibility check; headings measured from north matching engine convention (a from-east/from-north mix once flew the interceptor north when ordered east - cardinal cases now pinned by tests).")
    d.add_page()
    d.h1("5. Engine & Host (engine.rs · sidecar.rs · commands/)")
    d.p("SurveillanceEngine: queued ingestion drained after the single per-tick prediction pass (the queue-then-drain structure exists because fusing inside ingest double-propagated filters and locked the acquisition gate out of reality); periodic scans STCA 15 Hz, anomaly 1 Hz, geofence 2 Hz; alert composition folds squawk family > dark verdicts > geofence hold-down > manual override; snapshots publish on a tokio broadcast bus as Arc<EngineSnapshot>.")
    d.p("Sidecar bridge: NDJSON stdio (init/log_tracks/shutdown; ready/awos/db_ack), writer task serialising child stdin, graceful None on spawn failure. Commands: twelve synchronous Tauri handlers over Arc<Mutex<Engine>> with sub-millisecond lock discipline; compute_intercept converts the frontend km/h to solver knots at the boundary.")
    d.h1("6. Frontend (src/)")
    d.p("Telemetry store: 60 Hz snapshots land in a module singleton; the radar canvas reads the ref inside its own rAF loop (zero React churn), panels sample at 2-8 Hz. SI policy enforced through lib/units.ts (km rings 100/200/300, metres altitudes, km/h speeds, Doc-4444 minima rendered 9.26 km / 305 m). Browser demo feed mirrors the Rust roster at 20 Hz for offline operation and screen capture.")
    d.h2("6.1 Wire contracts (models.rs mirror)")
    d.code('Track { icao24, callsign, class, alert, latitude, longitude,\n        altitude_ft, ground_speed_kt, course_deg, vertical_rate_fpm,\n        vertical_trend, squawk, on_ground, last_update_ms, age_s,\n        coasting, position_sigma_m, leader_line[[lat,lon];13] }\nEngineSnapshot { tracks, flight_data_blocks, stca_alerts,\n                 geofence_breaches, anomalies, weather, status }')
    d.p("Wire values stay in DO-260B native units (feet/knots); conversion to SI happens only in presentation code. Field names are serde snake_case exactly.")
    d.h1("7. Test Inventory")
    d.table(["Suite","Count","Highlights"],[
        ["mode_s_decoder","12","CRC reference frames, repair, CPR round-trips, Gillham"],
        ["acars_decoder","5","framing, checksum corruption, classification"],
        ["sdr_registry","3","serial binding, offline degradation"],
        ["simulator","10","100% decode rate, truth coordinates, determinism, STCA geometry"],
        ["kinematics","17","EKF convergence/gating, R*-tree integrity, STCA cases"],
        ["weather_fusion","14","ISA values, IDW, priority chain, BDS round-trips"],
        ["defense","16","squawk mapping, geofence cycles, intercept envelope"],
        ["engine","10","110 s end-to-end: identity, STCA, coasting, breaches"],
        ["sidecar bridge","6","codec round-trips, spawn degradation"],
        ["python sidecar","14","AWOS codec, METAR, DuckDB durability"],
    ],[40,16,118])
    d.output(os.path.join(OUT, "AeroPulse-NG_Technical_Reference.pdf"))
    print("tech reference: pages", d.page_no())

if __name__ == "__main__":
    pitch(); exec_summary(); architecture_pdf(); user_manual_pdf(); tech_ref_pdf()
    print("ALL PDFS DONE ->", OUT)
