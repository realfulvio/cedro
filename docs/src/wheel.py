import math
def wheel(cx,cy,R,r,labels,active,n=None,fill="#EEF2F6",act="#0066CC",txt="#1A2A3A",size=22,gap=4):
    n=len(labels); out=[]
    for i,l in enumerate(labels):
        a0=-90+360*i/n+gap/2*0+1.5; a1=-90+360*(i+1)/n-1.5
        def p(rad,a): return (cx+rad*math.cos(math.radians(a)),cy+rad*math.sin(math.radians(a)))
        x0,y0=p(R,a0);x1,y1=p(R,a1);x2,y2=p(r,a1);x3,y3=p(r,a0)
        d=f"M{x0:.1f},{y0:.1f} A{R},{R} 0 0 1 {x1:.1f},{y1:.1f} L{x2:.1f},{y2:.1f} A{r},{r} 0 0 0 {x3:.1f},{y3:.1f} Z"
        f=act if i==active else fill; t="#fff" if i==active else txt
        mx,my=p((R+r)/2,(a0+a1)/2)
        out.append(f'<path d="{d}" fill="{f}" stroke="#fff" stroke-width="5" stroke-linejoin="round" filter="url(#sh)"/>')
        out.append(f'<text x="{mx:.1f}" y="{my+size*0.35:.1f}" text-anchor="middle" font-size="{size}" font-weight="700" fill="{t}">{l}</text>')
    return "\n".join(out)
def svg(w,h,body,extra=""):
    return f'<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}"><defs><filter id="sh" x="-20%" y="-20%" width="140%" height="140%"><feDropShadow dx="0" dy="3" stdDeviation="4" flood-color="#00243a" flood-opacity=".28"/></filter>{extra}</defs>{body}</svg>'
