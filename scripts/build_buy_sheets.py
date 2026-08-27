#!/usr/bin/env python3
"""Generates one-page buy-sheets + schematics for prototyping and field kits."""
import os
from fpdf import FPDF

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUT = os.path.join(ROOT, "docs", "pdf")
A = os.path.join(ROOT, "docs", "assets")
os.makedirs(OUT, exist_ok=True)

FD = "/usr/share/fonts/truetype/dejavu/"
BG=(7,12,27); PANEL=(15,30,50); GRID=(30,58,90); INK=(24,30,42)
TEXT=(18,28,42); DIM=(100,116,139); CIVIL=(34,197,94); WHITE=(245,248,252)
ALERT=(229,72,77); AMBER=(217,164,65); CYAN=(59,130,246)

class BuySheet(FPDF):
    def __init__(self, title, subtitle):
        super().__init__(unit="mm", format="A4")
        self.add_font("dv","",FD+"DejaVuSans.ttf")
        self.add_font("dv","B",FD+"DejaVuSans-Bold.ttf")
        self.add_font("dvm","",FD+"DejaVuSansMono.ttf")
        self.set_auto_page_break(False)
        self.title_txt = title; self.sub = subtitle
    def header(self):
        self.set_fill_color(*BG); self.rect(0,0,210,22,style="F")
        if os.path.exists(os.path.join(A,"logo.png")):
            self.image(os.path.join(A,"logo.png"), 10, 4, 14)
        self.set_xy(28,5); self.set_font("dv","B",13); self.set_text_color(*WHITE)
        self.cell(0,6,self.title_txt)
        self.set_xy(28,11); self.set_font("dvm","",7.5); self.set_text_color(120,140,165)
        self.cell(0,4,self.sub)
        self.set_xy(155,5); self.set_font("dv","",7); self.set_text_color(120,140,165)
        self.cell(50,6,"AeroPulse-NG v2.4", align="R")
        self.set_xy(155,10); self.cell(50,4,"Prototype Buy Sheet", align="R")
    def footer(self):
        self.set_xy(10,287); self.set_font("dvm","",6.5); self.set_text_color(*DIM)
        self.cell(0,4,"Prices indicative — Computer Village cash, Mouser/DigiKey online. Verify at purchase. Generated for prototype planning only.", align="C")

def prototyping():
    pdf = BuySheet("AeroPulse-NG — Prototyping Kit", "Bench testing · Air-gapped · No field equipment required")
    pdf.add_page()
    # Summary bar
    pdf.set_fill_color(*PANEL); pdf.set_draw_color(*GRID); pdf.set_line_width(0.3)
    pdf.rect(10,26,190,10,style="DF")
    pdf.set_xy(12,27); pdf.set_font("dv","B",9); pdf.set_text_color(*CIVIL); pdf.cell(60,7,"BENCH PROTOTYPE KIT")
    pdf.set_font("dv","",7.5); pdf.set_text_color(*WHITE); pdf.set_xy(78,27)
    pdf.cell(55,7,"6-aircraft synthetic feed")
    pdf.set_xy(135,27); pdf.set_font("dvm","",6.5); pdf.set_text_color(*DIM); pdf.cell(0,7,"4 SDRs on powered hub · Laptop host", align="R")
    # Table
    y=40
    headers=["Item","Qty","Unit","Total","Notes / Source"]
    widths=[78,12,22,22,66]
    pdf.set_font("dv","B",7.5); pdf.set_fill_color(*BG); pdf.set_text_color(*WHITE)
    for h,w in zip(headers,widths): pdf.set_xy(10+sum(widths[:headers.index(h)]),y); pdf.cell(w,7,f" {h}",fill=True, border=1)
    y+=7
    rows=[
        ["RTL-SDR Blog V4 (1090 MHz ADS-B)", "1", "$35", "$35", "H.S. Electronics, Ikeja"],
        ["RTL-SDR Blog V4 (131.55 MHz ACARS)", "1", "$35", "$35", "H.S. Electronics"],
        ["RTL-SDR Blog V4 (118–137 MHz VHF)", "1", "$35", "$35", "H.S. Electronics"],
        ["HackRF One (TX/RX 1 MHz–6 GHz)", "1", "$180", "$180", "Mouser / AliExpress Great Scott"],
        ["Powered USB 3.0 Hub 7-port 12V/3A", "1", "$22", "$22", "Orico / Ugreen — Katanga"],
        ["USB Headset (Jabra Evolve2 / H390)", "1", "$80", "$80", "Jumia / Slot"],
        ["SMA male–SMA male cables 30 cm", "3", "$3", "$9", "Katanga cable stalls"],
        ["SMA–N pigtails (future outdoor use)", "3", "$5", "$15", "Katanga"],
    ]
    pdf.set_font("dv","",7.5)
    fill=False
    for r in rows:
        pdf.set_fill_color(*(246,248,251) if fill else (255,255,255))
        x=10
        maxh=7
        for cell,w in zip(r,widths):
            if pdf.get_string_width(cell) > w-3: maxh=10
        for cell,w in zip(r,widths):
            pdf.set_xy(x, y); pdf.set_text_color(*TEXT)
            pdf.cell(w, maxh, f" {cell}", fill=True, border=1)
            x+=w
        y+=maxh; fill=not fill
    # Total
    pdf.set_fill_color(*BG); pdf.set_text_color(*WHITE); pdf.set_font("dv","B",8.5)
    pdf.set_xy(10,y); pdf.cell(90,8," TOTAL BENCH PROTOTYPE", fill=True, border=1)
    pdf.cell(22,8,"$409", fill=True, border=1, align="C")
    pdf.cell(88,8," ≈ ₦630k @ ₦1,550/$  ·  Cash ~₦500k", fill=True, border=1)
    y+=12
    pdf.set_font("dvm","",6.5); pdf.set_text_color(*DIM)
    pdf.set_xy(10,y); pdf.multi_cell(190,3.2,"Notes: 2 dongles will run but 3 is recommended for full 3-band demo. Outdoor antennas, LMR-400, mast, industrial PC, UPS and enclosure are NOT required for bench testing — simulator provides synthetic traffic. Headset shows as standard USB audio device; no driver needed.")
    # Schematic page 2
    pdf.add_page()
    pdf.set_xy(10,26); pdf.set_font("dv","B",11); pdf.set_text_color(*INK); pdf.cell(0,6,"Bench Setup — Connection Schematic")
    pdf.set_xy(10,32); pdf.set_font("dvm","",7); pdf.set_text_color(*DIM); pdf.cell(0,4,"All devices on a single powered hub · Laptop host · Indoor whip antennas supplied with dongles")
    # Diagram
    # Laptop
    pdf.set_fill_color(*PANEL); pdf.rect(70,45,70,28,style="DF")
    pdf.set_xy(70,48); pdf.set_font("dv","B",9); pdf.set_text_color(*WHITE); pdf.cell(70,6,"LAPTOP (Host PC)", align="C")
    pdf.set_xy(70,55); pdf.set_font("dvm","",6.5); pdf.set_text_color(*DIM); pdf.cell(70,4,"Ubuntu 22.04 · npm run dev", align="C")
    pdf.set_xy(70,60); pdf.cell(70,4,"or scripts/desktop.sh", align="C")
    # USB hub
    pdf.set_fill_color(*PANEL); pdf.rect(70,90,70,22,style="DF")
    pdf.set_xy(70,93); pdf.set_font("dv","B",8); pdf.set_text_color(*AMBER); pdf.cell(70,5,"POWERED USB 3.0 HUB", align="C")
    pdf.set_xy(70,99); pdf.set_font("dvm","",6); pdf.cell(70,4,"7-port · 12V 3A · per-port switch", align="C")
    # Cable laptop->hub
    pdf.set_draw_color(*GRID); pdf.set_line_width(0.6); pdf.line(105,73,105,90)
    pdf.set_font("dvm","",6); pdf.set_text_color(*DIM); pdf.set_xy(110,80); pdf.cell(20,4,"USB 3.0")
    # 4 dongles
    dongles=[
        ("RTL-SDR #1","1090 MHz","ADS-B / Mode S",30),
        ("RTL-SDR #2","131.55 MHz","ACARS / D-ATIS",75),
        ("RTL-SDR #3","118–137 MHz","VHF COMMS RX",120),
        ("HackRF One","118–137 MHz","COMMS TX/RX",165),
    ]
    for label,freq,role,x in dongles:
        pdf.set_fill_color(*PANEL); pdf.rect(x,125,38,24,style="DF")
        pdf.set_draw_color(AMBER if "Hack" in label else CIVIL); pdf.set_line_width(0.5)
        pdf.rect(x,125,38,24,style="D")
        pdf.set_xy(x,127); pdf.set_font("dv","B",6.5); pdf.set_text_color(AMBER if "Hack" in label else CIVIL); pdf.cell(38,4,label, align="C")
        pdf.set_xy(x,131); pdf.set_font("dvm","",5); pdf.set_text_color(*DIM); pdf.cell(38,3,freq, align="C")
        pdf.set_xy(x,135); pdf.set_font("dvm","",5); pdf.cell(38,3,role, align="C")
        pdf.set_font("dvm","",5); pdf.set_xy(x+8,141); pdf.set_text_color(*DIM); pdf.cell(22,3,"SMA whip", align="C")
        # line to hub
        pdf.set_draw_color(*GRID); pdf.line(x+19,122,x+19,112); pdf.line(105,112, x+19,112)
    # Headset
    pdf.set_fill_color(*PANEL); pdf.rect(150,45,45,28,style="DF")
    pdf.set_xy(150,48); pdf.set_font("dv","B",8); pdf.set_text_color(*WHITE); pdf.cell(45,5,"USB HEADSET", align="C")
    pdf.set_xy(150,54); pdf.set_font("dvm","",6); pdf.set_text_color(*DIM); pdf.cell(45,3,"Jabra / Logitech", align="C")
    pdf.set_xy(150,58); pdf.set_font("dvm","",6); pdf.cell(45,3,"Mic + Speakers", align="C")
    pdf.set_draw_color(*GRID); pdf.line(150,59,135,59); pdf.line(135,59,135,73); pdf.line(135,73,122,73)
    pdf.set_font("dvm","",5.5); pdf.set_xy(112,54); pdf.cell(20,3,"USB Audio")
    # Power note
    pdf.set_xy(10,162); pdf.set_font("dv","B",7); pdf.set_text_color(*ALERT); pdf.cell(0,4,"POWER: Hub must be 12V powered — passive hubs brown out with 3 dongles.")
    pdf.set_xy(10,167); pdf.set_font("dv","",7); pdf.set_text_color(*DIM); pdf.cell(0,4,"Software auto-detects via sdr_registry.rs — chips flip Standby → Active. No code change.")
    pdf.set_font("dv","B",7); pdf.set_text_color(*INK); pdf.set_xy(10,178); pdf.cell(0,4,"What you can test with zero hardware (today):  npm run dev  →  http://localhost:1420/  (synthetic 6-aircraft feed)")
    pdf.output(os.path.join(OUT,"AeroPulse-NG_Prototyping_Kit_Buy_Sheet.pdf"))
    print("prototyping buy-sheet done")

def field():
    pdf = BuySheet("AeroPulse-NG — Field Deployment Kit", "Station hardening · Outdoor mast · Industrial compute · Power continuity")
    pdf.add_page()
    # Banner: field = prototyping + field subtotal
    pdf.set_fill_color(*PANEL); pdf.rect(10,26,190,10,style="DF")
    pdf.set_xy(12,27); pdf.set_font("dv","B",9); pdf.set_text_color(*AMBER); pdf.cell(60,7,"FIELD DEPLOYMENT KIT")
    pdf.set_font("dv","",7.5); pdf.set_text_color(*WHITE); pdf.set_xy(78,27); pdf.cell(55,7,"Adds to bench prototype")
    pdf.set_xy(135,27); pdf.set_font("dvm","",6.5); pdf.set_text_color(*DIM); pdf.cell(0,7,"Outdoor mast · LMR-400 · Filters · Mini-PC · UPS", align="R")
    # Table
    y=40
    headers=["Item","Qty","Unit","Total","Notes"]
    widths=[78,12,22,22,66]
    pdf.set_font("dv","B",7.5); pdf.set_fill_color(*BG); pdf.set_text_color(*WHITE)
    for h,w in zip(headers,widths): pdf.set_xy(10+sum(widths[:headers.index(h)]),y); pdf.cell(w,7,f" {h}",fill=True, border=1)
    y+=7
    rows=[
        ["Prototyping kit (above) — reused", "1", "$409", "$409", "Core is the field core"],
        ["Industrial Mini-PC i5/16GB/512GB NVMe", "1", "$450", "$450", "Fanless · on-site edge compute"],
        ["1090 MHz Collinear 5 dBi fibreglass", "1", "$180", "$180", "N-female · 1090 MHz ADS-B"],
        ["VHF Airband Dipole 118–137 MHz", "1", "$120", "$120", "N-female · VHF comms"],
        ["LMR-400 Coax 15 m N-male", "2", "$85", "$170", "Low-loss feedlines"],
        ["1090 MHz Band-pass Filter + LNA", "1", "$95", "$95", "Pre-filter + gain"],
        ["VHF Band-pass Filter + LNA", "1", "$85", "$85", "Clean VHF RX/TX"],
        ["Lightning Arrestor N-female", "2", "$45", "$90", "Surge protection"],
        ["Mast 6 m telescopic, guyed", "1", "$350", "$350", "Rooftop / tower mount"],
        ["Guy wires / clamps / grounding kit", "1", "$120", "$120", "Installation hardware"],
        ["UPS 1000 VA (2 h runtime)", "1", "$220", "$220", "Power continuity"],
        ["Weatherproof Enclosure IP66", "1", "$180", "$180", "400×300×150 mm"],
    ]
    pdf.set_font("dv","",7)
    fill=False
    for r in rows:
        pdf.set_fill_color(*(246,248,251) if fill else (255,255,255))
        x=10; maxh=7
        for c,w in zip(r,widths):
            if pdf.get_string_width(c) > w-3: maxh=9
        for c,w in zip(r,widths):
            pdf.set_xy(x,y); pdf.set_text_color(*TEXT if r[1]!="1" or True else (0,0,0))
            pdf.cell(w, maxh, f" {c}", fill=True, border=1); x+=w
        y+=maxh; fill=not fill
    pdf.set_fill_color(*BG); pdf.set_text_color(*WHITE); pdf.set_font("dv","B",8.5)
    pdf.set_xy(10,y); pdf.cell(90,8," FIELD SUBTOTAL", fill=True, border=1)
    pdf.cell(22,8,"$1,140", fill=True, border=1, align="C")
    pdf.cell(88,8,"  Total field kit (prototype + field) $1,549 ≈ ₦2.4M", fill=True, border=1)
    y+=12
    pdf.set_font("dvm","",6.5); pdf.set_text_color(*DIM)
    pdf.set_xy(10,y); pdf.multi_cell(190,3.2,"Field kit is deferred until after NDAIE quarter-final. All bench hardware moves to the mast — purchase is not wasted. Outdoor antennas replace indoor whips; industrial PC replaces laptop host.")
    # Schematic page 2
    pdf.add_page()
    pdf.set_xy(10,26); pdf.set_font("dv","B",11); pdf.set_text_color(*INK); pdf.cell(0,6,"Field Station — Connection Schematic")
    pdf.set_xy(10,32); pdf.set_font("dvm","",7); pdf.set_text_color(*DIM); pdf.cell(0,4,"Outdoor mast → LMR-400 → filters/LNAs → enclosure → powered hub → industrial PC · Lightning arrestors + UPS + grounding")
    # Enclosure
    pdf.set_fill_color(*PANEL); pdf.rect(60,45,90,90,style="DF")
    pdf.set_draw_color(*AMBER); pdf.rect(60,45,90,90,style="D")
    pdf.set_xy(60,48); pdf.set_font("dv","B",8); pdf.set_text_color(*AMBER); pdf.cell(90,5,"WEATHERPROOF ENCLOSURE IP66", align="C")
    pdf.set_font("dvm","",6); pdf.set_xy(65,58)
    pdf.multi_cell(80,3,"• Powered USB 3.0 Hub (12V)\n• 3× RTL-SDR + HackRF One\n• Band-pass Filters + LNAs\n• Lightning Arrestors\n• UPS-backed 12V rail", align="L")
    # Mini-PC
    pdf.set_fill_color(*PANEL); pdf.rect(65,108,80,18,style="DF")
    pdf.set_xy(65,112); pdf.set_font("dv","B",7); pdf.set_text_color(*WHITE); pdf.cell(80,5,"INDUSTRIAL MINI-PC", align="C")
    # Mast - elevated to avoid header overlap
    pdf.set_draw_color(*GRID); pdf.set_line_width(0.6)
    pdf.line(105,45,105,42); pdf.line(105,42,105,38)
    for yy in [40,42,44]:
        pdf.line(100,yy,110,yy)
    pdf.set_font("dv","B",6); pdf.set_text_color(*TEXT); pdf.set_xy(112,39); pdf.cell(30,3,"6 m MAST")
    pdf.set_fill_color(20,40,70); pdf.rect(90,36,30,5,style="F")
    pdf.set_font("dvm","",4.5); pdf.set_xy(90,37); pdf.set_text_color(*CIVIL); pdf.cell(30,3,"1090 MHz COLL.", align="C")
    pdf.set_fill_color(20,40,70); pdf.rect(90,30,30,5,style="F")
    pdf.set_xy(90,31); pdf.set_text_color(*AMBER); pdf.cell(30,3,"VHF DIPOLE", align="C")
    pdf.set_draw_color(*CIVIL); pdf.line(105,38,105,45)
    # Power
    pdf.set_fill_color(*PANEL); pdf.rect(155,120,35,15,style="DF")
    pdf.set_xy(155,123); pdf.set_font("dv","B",6); pdf.set_text_color(*WHITE); pdf.cell(35,4,"UPS 1000VA", align="C")
    pdf.set_xy(155,128); pdf.set_font("dvm","",5); pdf.set_text_color(*DIM); pdf.cell(35,3,"2 h runtime", align="C")
    pdf.set_draw_color(*GRID); pdf.line(155,127,150,127)
    # Notes
    pdf.set_xy(10,150); pdf.set_font("dv","B",7); pdf.set_text_color(*ALERT); pdf.cell(0,4,"LIGHTNING: Arrestors before enclosure entry · Ground rod <10 Ω · Guy wires bonded.")
    pdf.set_xy(10,156); pdf.set_font("dvm","",6.5); pdf.set_text_color(*DIM); pdf.cell(0,4,"All bench SDRs move to enclosure; laptop replaced by industrial PC as host; whips replaced by outdoor collinear/dipole.")
    pdf.output(os.path.join(OUT,"AeroPulse-NG_Field_Deployment_Kit_Buy_Sheet.pdf"))
    print("field buy-sheet done")

if __name__ == "__main__":
    prototyping(); field()
    print("ALL BUY-SHEETS DONE ->", OUT)
