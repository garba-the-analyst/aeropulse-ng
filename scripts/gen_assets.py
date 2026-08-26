#!/usr/bin/env python3
"""AeroPulse-NG documentation asset generator (Pillow, fully reproducible)."""
import math, os
from PIL import Image, ImageDraw, ImageFont

OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "docs", "assets")
os.makedirs(OUT, exist_ok=True)
BG=(11,14,20); PANEL=(16,20,29); GRID=(42,50,65); TEXT=(200,210,224); DIM=(107,118,137)
CIVIL=(0,255,102); MILV=(155,120,255); AMBER=(255,179,0); ALERT=(255,23,68); WHITE=(240,244,250)
FD="/usr/share/fonts/truetype/dejavu/"
def fnt(sz,bold=False,mono=False):
    n="DejaVuSansMono.ttf" if mono else ("DejaVuSans-Bold.ttf" if bold else "DejaVuSans.ttf")
    return ImageFont.truetype(FD+n,sz)
def save(img,name):
    img.save(os.path.join(OUT,name),optimize=True); print("wrote",name,img.size)

def box(d,xy,title,lines,accent,tz=15,bz=12):
    x0,y0,x1,y1=xy
    d.rounded_rectangle(xy,radius=8,fill=PANEL,outline=accent,width=2)
    d.text((x0+10,y0+9),title,font=fnt(tz,True),fill=accent)
    yy=y0+9+tz+8
    for ln in lines:
        d.text((x0+13,yy),ln,font=fnt(bz,mono=True),fill=TEXT); yy+=bz+4

def arrow(d,p0,p1,color=DIM,w=3):
    d.line([p0,p1],fill=color,width=w)
    a=math.atan2(p1[1]-p0[1],p1[0]-p0[0]); L=11
    for da in (2.6,-2.6):
        d.line([p1,(p1[0]-L*math.cos(a+da),p1[1]-L*math.sin(a+da))],fill=color,width=w)

def logo(size=512):
    img=Image.new("RGBA",(size,size),(0,0,0,0)); d=ImageDraw.Draw(img)
    c=s=size/2; r=size*0.40
    d.ellipse([c-r*1.35,s-r*1.35,c+r*1.35,s+r*1.35],outline=GRID,width=max(2,size//170))
    for a0 in (0,120,240):
        d.arc([c-r*1.18,s-r*1.18,c+r*1.18,s+r*1.18],a0,a0+70,fill=(0,255,102,90),width=max(2,size//110))
    pts=[(c,s-r),(c+r*0.82,s),(c,s+r),(c-r*0.82,s)]
    d.polygon(pts,fill=(0,200,80,255),outline=CIVIL+(255,),width=max(2,size//85))
    d.line([(c,s-r*0.55),(c,s+r*0.55)],fill=(11,14,20,220),width=max(2,size//60))
    save(img,"logo.png")

def architecture():
    W,H=1560,900; img=Image.new("RGB",(W,H),BG); d=ImageDraw.Draw(img)
    d.text((30,20),"AeroPulse-NG — System Pipeline",font=fnt(26,True),fill=WHITE)
    d.text((30,58),"air-gapped · offline-first · SI-metric presentation (NCAA / ICAO Annex 5)",font=fnt(13,mono=True),fill=DIM)
    box(d,(30,100,430,195),"RF SOURCES",["SDR-1 1090 MHz ADS-B / Mode S","SDR-2 131.55 MHz ACARS","AWOS mast RS-485 (sidecar)"],CIVIL)
    box(d,(30,215,430,300),"SIMULATOR (bench mode)",["6-aircraft truth fleet","encodes genuine DF17 frames","same decoders as real RF"],AMBER)
    box(d,(500,100,950,300),"HARDWARE DECODE LAYER",["mode_s_decoder  DF17 · CRC-24 repair","                 CPR even/odd -> lat/lon","                 Gillham alt · velocity","acars_decoder   framing · CRC-16","                 METAR / D-ATIS classify","sdr_registry    serial-lock channels"],MILV)
    box(d,(500,340,950,565),"SURVEILLANCE ENGINE  (60 Hz)",["queue > EKF predict > fuse pos/vel","STCA scan 15 Hz (R*-tree)","   Doc-4444: 9.26 km / 305 m / 120 s","anomaly scan 1 Hz · geofence 2 Hz","","EngineSnapshot {tracks·FDB·STCA·wx}","tokio bus -> telemetry://snapshot"],CIVIL)
    box(d,(1020,100,1530,262),"KINEMATICS",["6-state EKF [x y z vx vy vz]","Joseph-form covariance update","Mahalanobis gate chi2(3)=16.27","dead-reckoning coasting","leader-line projection 120 s"],MILV)
    box(d,(1020,282,1530,425),"WEATHER FUSION",["AWOS > D-ATIS > BDS 4,4/4,5","IDW spatial blend (vector wind)","ISA fallback + confidence","Harmattan dust-layer estimate"],AMBER)
    box(d,(1020,445,1530,585),"DEFENCE OVERLAYS",["7700/7600/7500 alerting","dark-target & silence detect","polygon geofences + bands","lead-pursuit intercept solver"],ALERT)
    box(d,(30,380,430,565),"DISPLAY 1 — RADAR CANVAS",["60 fps PPI · rings 100/200/300 km","MIL-STD-2525D symbols","3-line FDB tags (SI units)","flashing STCA connectors 2 Hz","R&B ruler · geofence overlay"],CIVIL)
    box(d,(30,605,430,790),"DISPLAY 2 — OPERATIONS HUD",["flight-strip bay (arr / dep)","triple-fusion weather matrix","FFT diagnostics · latency gauge","threat matrix · intercept calc"],CIVIL)
    box(d,(500,620,950,800),"PYTHON SIDECAR (NDJSON stdio)",["DuckDB flight recorder (batched)","track_positions · alerts · weather","AWOS serial reader (+ simulated mast)"],AMBER)
    arrow(d,(430,145),(498,145)); arrow(d,(430,255),(498,230))
    arrow(d,(725,300),(725,338))
    arrow(d,(500,450),(432,455)); arrow(d,(1020,180),(952,180)); arrow(d,(1020,350),(952,350)); arrow(d,(1020,515),(952,515))
    arrow(d,(725,800),(725,835)); arrow(d,(950,700),(1018,700))
    d.text((30,850),"telemetry://snapshot @ 60 Hz",font=fnt(14,mono=True),fill=DIM)
    save(img,"architecture.png")

def radar_scope():
    W,H=1400,900; img=Image.new("RGB",(W,H),BG); d=ImageDraw.Draw(img)
    cx,cy=int(W*0.42),int(H*0.52); R=int(H*0.44)
    for i,k in enumerate((1.0,0.66,0.33)):
        rr=int(R*k); lbl=[300,200,100][i]
        d.ellipse([cx-rr,cy-rr,cx+rr,cy+rr],outline=GRID,width=2)
        t=f"{lbl} KM"; d.text((cx+6,cy-rr-16),t,font=fnt(12,mono=True),fill=DIM)
    d.line([cx-R,cy,cx+R,cy],fill=GRID,width=1); d.line([cx,cy-R,cx,cy+R],fill=GRID,width=1)
    for deg in range(0,360,30):
        a=math.radians(deg); d.line([cx,cy,(cx+math.sin(a)*R,cy-math.cos(a)*R)],fill=(30,36,48),width=1)
    swa=math.radians(-38)
    for i in range(60):
        a=swa-i*0.008; fade=int(70*(1-i/60)); col=(0,int(255*fade/70//1) if False else int(160*(1-i/60)+40),102) if False else (0,int(120*(1-i/60)+30),int(50*(1-i/60)))
        d.pieslice([cx-R,cy-R,cx+R,cy+R],math.degrees(a)-1.4,math.degrees(a),fill=col)
    d.ellipse([cx-4,cy-4,cx+4,cy+4],fill=CIVIL); d.text((cx+8,cy+8),"DNKN",font=fnt(12,mono=True),fill=CIVIL)
    def project(lat,lon):
        px=cx+(lon-8.5241)*(111320*math.cos(math.radians(12.0476))/1000)*(R/250)
        py=cy-(lat-12.0476)*(111132/1000)*(R/250)
        return px,py
    def sym(px,py,color,kind,label,altm,spd,crs,sq,alert=False,coast=False,ldx=13):
        st = 2 if alert else 1
        if kind=="dia":
            d.polygon([(px,py-8),(px+7,py),(px,py+8),(px-7,py)],outline=color,width=st)
        elif kind=="car":
            d.line([(px-8,py+6),(px,py-8),(px+8,py+6)],fill=color,width=st)
        else:
            d.rectangle([px-6,py-6,px+6,py+6],outline=color,width=st)
        if alert: d.ellipse([px-13,py-13,px+13,py+13],outline=color,width=1)
        if coast: d.ellipse([px-11,py-11,px+11,py+11],outline=color,width=1)
        c=color if not alert else ALERT
        d.text((px+ldx,py-22),f"{label:<8}{altm}M",font=fnt(12,mono=True),fill=c)
        d.text((px+ldx,py-8), f"1090 {spd}KM/H {sq}",font=fnt(12,mono=True),fill=c)
        d.text((px+ldx,py+6), ("EMRG" if alert else "S-04"),font=fnt(12,mono=True),fill=c)
    targets=[
        (12.60 ,8.44 ,"VL604 ","9750",734,205,"3421",CIVIL,"dia",True,False, 13),
        (12.42 ,8.18 ,"NAF911","9753",963, 27,"6101",MILV,"car",False,False,-128),
        (12.16 ,7.94 ,"NAF912","9462",925, 27,"6102",MILV,"car",False,False,-128),
        (11.60 ,9.30 ,"VL702 ","8695",796,335,"4203",CIVIL,"dia",False,False, 13),
        (13.45 ,7.50 ,"DLH501",11582,907,148,"5172",CIVIL,"dia",False,False, 13),
        (12.90 ,9.35 ,"BADC0 ",1981,389,232,"0000",AMBER,"sq",False,True,  13),
    ]
    for lat,lon,lbl,alt,spd,crs,sq,col,kind,alrt,cst,ldx in targets:
        px,py=project(lat,lon); sym(px,py,col,kind,lbl,alt,spd,crs,sq,alrt,cst,ldx)
    a=project(12.407,8.154); b=project(12.155,8.027)
    d.line([a,b],fill=ALERT,width=2)
    d.setLineDash if False else None
    mx,my=(a[0]+b[0])/2,(a[1]+b[1])/2
    d.text((mx-250,my+46),"STCA 1.1 KM / 0 M / T-12s",font=fnt(13,mono=True),fill=ALERT)
    # right info column
    X=W-380
    d.rounded_rectangle([X-16,60,W-20,H-60],radius=10,fill=PANEL,outline=GRID,width=2)
    d.text((X,76),"TACTICAL PICTURE — LIVE",font=fnt(15,True),fill=WHITE)
    rows=[("SDR-1 1090MHz",True,"412 msg/s"),("SDR-2 ACARS",True,"2.1 msg/s"),("SDR-3 GUARD",False,"offline"),("AWOS RS-485",True,"QNH 1013.2"),("EKF ENGINE",True,"248 us")]
    yy=108
    for name,ok,stat in rows:
        col=CIVIL if ok else ALERT
        d.ellipse([X,yy+3,X+10,yy+13],fill=col); d.text((X+18,yy),name,font=fnt(12,mono=True),fill=TEXT)
        d.text((X+210,yy),stat,font=fnt(12,mono=True),fill=DIM); yy+=24
    d.text((X,yy+12),"ZULU 12:41:07   WAT 13:41:07",font=fnt(13,mono=True),fill=CIVIL)
    yy+=34
    d.text((X,yy),"THREAT MATRIX",font=fnt(13,True),fill=WHITE)
    yy+=26
    for txt,col in [("VL604 SQK 7700 - EMERGENCY",ALERT),("NAF911 <-> VL604 - STCA PAIR",ALERT),("BADC0 - DARK TARGET",AMBER)]:
        d.text((X,yy),txt,font=fnt(11,mono=True),fill=col); yy+=22
    save(img,"radar-scope.png")

def timeline():
    W,H=1560,420; img=Image.new("RGB",(W,H),BG); d=ImageDraw.Draw(img)
    d.text((30,18),"Demonstration scenario timeline (synthetic feed)",font=fnt(20,True),fill=WHITE)
    y=250; x0,x1=80,W-80
    d.line([x0,y,x1,y],fill=GRID,width=4)
    marks=[(0,"BOOT","6 tracks airborne;\nSTCA inside lookahead",CIVIL),
           (45,"SQUAWK 7700","VL604 general emergency\n+ TC-28 status frame",ALERT),
           (70,"STCA FLASH","NAF911 intercept <1 km\npredicted miss",RED_ := (255,23,68)),
           (112,"RF DROPOUT","VL604 coasts on EKF\ndead reckoning 28 s",AMBER),
           (150,"DARK TARGET","0BADC0 flagged:\nno identity broadcast",AMBER)]
    span=x1-x0
    for t,name,desc,col in marks:
        x=x0+int(span*t/160)
        d.ellipse([x-9,y-9,x+9,y+9],fill=col)
        d.text((x-len(name)*4,y-56),name,font=fnt(14,True),fill=col)
        d.text((x-len(name)*4,y+18),f"T+{t}s",font=fnt(12,mono=True),fill=DIM)
        ty=y+44
        for ln in desc.split("\n"):
            d.text((x-70,ty),ln,font=fnt(11,mono=True),fill=TEXT); ty+=15
    d.text((x0,y+130),"rolling: ACARS METAR every 45 s · AWOS surface every 30 s · upper-air BDS sample every 5 s",
           font=fnt(12,mono=True),fill=DIM)
    save(img,"timeline.png")

def fusion():
    W,H=1200,560; img=Image.new("RGB",(W,H),BG); d=ImageDraw.Draw(img)
    d.text((30,18),"Triple-Fusion Weather Matrix",font=fnt(20,True),fill=WHITE)
    centers={"SURFACE\nAWOS RS-485":(330,190,CIVIL),"TERMINAL\nACARS D-ATIS":(600,190,MILV),"EN-ROUTE\nBDS 4,4 / 4,5":(870,190,AMBER)}
    for lbl,(x,y,col) in centers.items():
        r=95
        d.ellipse([x-r,y-r,x+r,y+r],outline=col,width=3)
        ty=y-24
        for ln in lbl.split("\n"):
            w=d.textlength(ln,font=fnt(14,True)); d.text((x-w/2,ty),ln,font=fnt(14,True),fill=col); ty+=19
    d.text((600-70,175),"FUSION",font=fnt(16,True),fill=WHITE)
    box(d,(280,360,920,520),"WEATHER MATRIX",["IDW spatial blend · vector-average wind (never degrees)","ISA fallback with confidence metric (source diversity x range)","priority: live AWOS overrides D-ATIS seeds; METAR fills gaps","dust-layer top estimated from visibility + thermal bump"],WHITE,tz=14,bz=12)
    for (x,y,col) in centers.values():
        arrow(d,(x,y+95),(x+(600-x)*0.35,y+150),color=col,w=2)
    save(img,"fusion.png")

logo(); architecture(); radar_scope(); timeline(); fusion()
print("ASSETS DONE")
