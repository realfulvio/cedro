import re,sys,os,math
A=sys.argv[1]; OUT=sys.argv[2]
def attrs(s): return dict(re.findall(r'([a-z-]+)="([^"]*)"',s))
def f(v): return float(v)
icons=[]
for fn in sorted(os.listdir(A+'/icons')):
    if not fn.endswith('.svg'): continue
    s=open(A+'/icons/'+fn).read()
    inner=re.search(r'<svg[^>]*>(.*)</svg>',s,re.S).group(1)
    ds=[]
    for tag,body in re.findall(r'<(\w+)([^>]*)/?>',inner):
        a=attrs(body)
        if tag=='path': d=a['d'].strip(); ds.append(re.sub(r'^m\s*(-?[\d.]+)[ ,]+(-?[\d.]+)\s*',lambda m:f'M{m.group(1)} {m.group(2)}l',d) if d.startswith('m') else d)
        elif tag in('circle','ellipse'):
            cx,cy=f(a['cx']),f(a['cy']); rx=f(a.get('r',a.get('rx'))); ry=f(a.get('r',a.get('ry')))
            ds.append(f"M{cx-rx} {cy}a{rx} {ry} 0 1 0 {2*rx} 0a{rx} {ry} 0 1 0 {-2*rx} 0z")
        elif tag=='rect':
            x,y,w,h=f(a['x']),f(a['y']),f(a['width']),f(a['height']); r=f(a.get('rx','0'))
            if r>0: ds.append(f"M{x+r} {y}h{w-2*r}a{r} {r} 0 0 1 {r} {r}v{h-2*r}a{r} {r} 0 0 1 {-r} {r}h{-(w-2*r)}a{r} {r} 0 0 1 {-r} {-r}v{-(h-2*r)}a{r} {r} 0 0 1 {r} {-r}z")
            else: ds.append(f"M{x} {y}h{w}v{h}h{-w}z")
        elif tag=='line': ds.append(f"M{a['x1']} {a['y1']}L{a['x2']} {a['y2']}")
        elif tag in('polyline','polygon'):
            p=a['points'].split(); ds.append('M'+' L'.join(p)+('z' if tag=='polygon' else ''))
    icons.append((fn[:-4].replace('-','_').upper(),' '.join(ds)))
def dp(pts,eps):
    if len(pts)<3: return pts
    a,b=pts[0],pts[-1]; dx,dy=b[0]-a[0],b[1]-a[1]; L=math.hypot(dx,dy) or 1e-9
    i,dm=0,0
    for k in range(1,len(pts)-1):
        p=pts[k]; d=abs(dy*(p[0]-a[0])-dx*(p[1]-a[1]))/L
        if d>dm: i,dm=k,d
    if dm>eps: return dp(pts[:i+1],eps)[:-1]+dp(pts[i:],eps)
    return [a,b]
s=open(A+'/brand/cedro-lockup.svg').read()
logo=[]
tot=0
for body in re.findall(r'<path([^>]*)/?>',s):
    a=attrs(body); col=a['fill'].lstrip('#')
    contours=[]
    for sub in re.split(r'Z',a['d']):
        nums=re.findall(r'-?[\d.]+',sub)
        pts=[(float(nums[i]),float(nums[i+1])) for i in range(0,len(nums)-1,2)]
        if len(pts)<3: continue
        far=max(range(len(pts)),key=lambda k:(pts[k][0]-pts[0][0])**2+(pts[k][1]-pts[0][1])**2)
        pts=dp(pts[:far+1],0.2)[:-1]+dp(pts[far:]+[pts[0]],0.2)[:-1]; tot+=len(pts)
        contours.append(pts)
    logo.append((col,contours))
o=["// Generato da gen.py dal kit Cedro (assets/icons e assets/brand/cedro-lockup.svg): non modificare a mano.",
"","/// Icone 24×24 a tratto (spessore 1,8, estremi arrotondati), in sintassi `d` di SVG.",""]
for n,d in icons: o.append(f'pub const {n}: &str = "{d}";')
o.append("")
o.append("/// Il logo Spicchio con la scritta, viewBox 1130×512: (colore, contorni pieni pari-dispari).")
o.append("/// Il simbolo da solo sta nei primi due colori, dentro il quadrato 0..512.")
o.append("pub const LOCKUP: &[(u32, &[&[f32]])] = &[")
for col,cs in logo:
    o.append(f"    (0x{col}, &[")
    for c in cs: o.append("        &["+",".join(f"{float(x):.2f},{float(y):.2f}".replace(".00",".0") for x,y in c)+"],")
    o.append("    ]),")
o.append("];")
open(OUT,'w').write("\n".join(o)+"\n")
print(len(icons),'icons',tot,'points')
