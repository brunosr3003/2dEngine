#!/usr/bin/env python3
"""Original marine voxel rigs. +Y faces forward, Z is up; tails are separate."""
from pathlib import Path
import math
import molde_corpo as vox

vox.W, vox.D, vox.H = 72, 128, 64
vox.TRANSL = "0 0 0"
PALETTE = {1:(87,118,139),2:(57,83,106),3:(210,221,218),4:(16,24,31),
           5:(237,220,136),6:(101,176,165),7:(58,113,108),8:(115,65,143),
           9:(179,123,192),10:(185,203,205),11:(48,78,91),12:(225,240,239)}

def box(out,x0,x1,y0,y1,z0,z1,color):
    for x in range(x0,x1+1):
        for y in range(y0,y1+1):
            for z in range(z0,z1+1):out[x,y,z]=color

def solid(out,c,r,color):
    for x in range(math.floor(c[0]-r[0]),math.ceil(c[0]+r[0])+1):
        for y in range(math.floor(c[1]-r[1]),math.ceil(c[1]+r[1])+1):
            for z in range(math.floor(c[2]-r[2]),math.ceil(c[2]+r[2])+1):
                if sum(((v-c[i])/r[i])**2 for i,v in enumerate((x,y,z)))<=1:
                    out[x,y,z]=color(x,y,z) if callable(color) else color

def fin(out,points,axis,fixed,color):
    # A filled triangular prism made exclusively of cubes.
    def cross(a,b,p):return (b[0]-a[0])*(p[1]-a[1])-(b[1]-a[1])*(p[0]-a[0])
    for a in range(min(p[0] for p in points),max(p[0] for p in points)+1):
        for b in range(min(p[1] for p in points),max(p[1] for p in points)+1):
            signs=[cross(points[i],points[(i+1)%3],(a,b)) for i in range(3)]
            if not (all(s>=0 for s in signs) or all(s<=0 for s in signs)):continue
            for f in range(fixed,fixed+2):
                p=[a,b];p.insert(axis,f);out[tuple(p)]=color

def fish(kind):
    body,tail={},{}
    whale=kind==90;eel=kind==72
    if whale:
        solid(body,(36,75,17),(12,34,10),lambda x,y,z:3 if z<13 else 2 if z>24 else 1)
        # Broad square jaw, blowhole, white chin and ventral throat pleats.
        solid(body,(36,98,17),(10,14,8),lambda x,y,z:3 if z<14 else 1)
        box(body,34,37,89,92,26,26,4)
        for x in range(30,43,3):box(body,x,x,82,104,8,9,10)
        for side in [-1,1]:
            box(body,36+side*9,36+side*9+1,99,101,18,19,4)
            fin(body,[(36+side*8,82),(36+side*27,59),(36+side*12,64)],2,13,2)
        solid(tail,(36,39,16),(4,12,4),1)
        for side in [-1,1]:fin(tail,[(36,32),(36+side*22,19),(36+side*9,36)],2,16,2)
    elif eel:
        for y in range(41,99):
            x=36+round(math.sin((y-41)*.09)*3)
            solid(body,(x,y,16),(3,2,3),lambda x,y,z:5 if y%9<3 else 7)
        solid(body,(37,98,16),(4,7,4),7)
        box(body,33,34,100,101,18,18,4);box(body,40,41,100,101,18,18,4)
        for y in range(25,47):solid(tail,(36,y,16),(2,2,2),5 if y%9<3 else 7)
        fin(tail,[(36,28),(36,19),(36,33)],2,16,7)
    else:
        solid(body,(36,70,16),(6,25,6),lambda x,y,z:3 if z<15 else 1 if z<20 else 2)
        # Long pointed snout, dark mouth, gill slits and contrasting eyes.
        solid(body,(36,92,16),(4,10,3),1)
        box(body,33,39,93,99,14,14,4)
        for side in [-1,1]:
            box(body,36+side*4,36+side*4,91,92,18,19,4)
            for y in [76,79,82]:box(body,36+side*5,36+side*5,y,y,15,18,2)
            fin(body,[(36+side*4,78),(36+side*19,57),(36+side*5,62)],2,15,2)
        fin(body,[(69,20),(60,30),(52,20)],0,36,2)
        solid(tail,(36,42,16),(2,9,3),1)
        fin(tail,[(39,16),(24,31),(29,16)],0,36,2)
        fin(tail,[(39,16),(26,7),(29,16)],0,36,2)
    return body,tail

def merfolk(kind):
    body,tail={},{};siren=kind in [75,91]
    # Block faces, shaped shoulders, separate hands, hair and armour.
    for z in range(26,40):
        w=4 if z<31 else 5
        box(body,36-w,36+w,61,66,z,z,6 if siren else 7)
    box(body,34,38,62,66,40,42,6)
    box(body,32,40,61,68,43,51,6)
    box(body,33,34,69,69,47,48,4);box(body,38,39,69,69,47,48,4)
    box(body,35,37,69,69,44,44,7)
    for side in [-1,1]:
        x=36+side*8
        box(body,x-1,x+1,62,65,30,38,6)
        box(body,x-1,x+1,63,67,27,30,6)
        box(body,x-2,x+2,61,66,37,40,9 if siren else 10)
    if siren:
        box(body,31,41,60,68,51,53,8)
        box(body,31,32,60,65,40,51,8);box(body,40,41,60,65,40,51,8)
        box(body,33,39,59,60,36,50,8)
        box(body,31,41,65,67,34,37,9)
        if kind==75:box(body,32,40,61,68,53,54,5)
    else:
        box(body,31,41,61,67,32,36,10)
        box(body,32,40,62,67,51,53,10)
        box(body,36,36,63,66,54,58,5)
        # Three-pronged harpoon, with a dark shaft and golden blades.
        box(body,47,47,67,67,22,52,11)
        box(body,44,50,67,67,50,51,5)
        for x in [44,47,50]:box(body,x,x,67,67,51,57,5)
        if kind==92:box(body,25,28,65,66,28,40,10)
    for z in range(10,27):
        w=max(2,(z-7)//4);y=60-(26-z)//2
        box(tail,36-w,36+w,y-2,y+2,z,z,8 if siren else 7)
        if z%4==0:box(tail,36-w,36+w,y+3,y+3,z,z,9 if siren else 6)
    for side in [-1,1]:fin(tail,[(36,50),(36+side*10,40),(36+side*4,53)],2,10,9 if siren else 6)
    return body,tail

def octopus():
    body,tail={},{}
    solid(body,(36,64,24),(9,10,12),lambda x,y,z:9 if z>28 else 8)
    for x in [30,41]:
        box(body,x,x+2,73,74,22,24,12);box(body,x+1,x+1,75,75,23,24,4)
    for i in range(8):
        a=i*math.tau/8
        for j in range(23):
            x=36+round(math.cos(a)*(7+j));y=64+round(math.sin(a)*(7+j))
            z=12+round(math.sin(j*.16+i*.5)*3)
            r=3 if j<10 else 2 if j<19 else 1
            solid(tail,(x,y,z),(r,r,r),8)
            if j%4==0:tail[x,y,z-r]=12
    return body,tail

def hydra():
    _,tail=fish(70)
    body={}
    solid(body,(36,65,16),(20,24,8),lambda x,y,z:3 if z<12 else 6 if (x+y)%7==0 else 7)
    for i in range(5):
        x=20+i*8;top=43+(i%2)*6
        for z in range(18,top):solid(body,(x,76+(z-18)//4,z),(3,3,2),7)
        solid(body,(x,91,top),(4,10,4),6)
        box(body,x-3,x+3,95,101,top-2,top-2,4)
        for side in [-1,1]:body[x+side*3,96,top+2]=5
        for z in range(top+3,top+8):box(body,x-1,x+1,87,89,z,z,10)
    return body,tail

if __name__=='__main__':
    out=Path(__file__).resolve().parents[2]/'assets/vox/marinhos';out.mkdir(parents=True,exist_ok=True)
    for kind in [70,71,72,73,75,76,90,91,92,93]:
        parts=merfolk(kind) if kind in [71,73,75,91,92] else octopus() if kind==76 else hydra() if kind==93 else fish(kind)
        for part in parts:
            assert all(0<=x<72 and 0<=y<128 and 0<=z<64 for x,y,z in part)
        (out/f'{kind}.vox').write_bytes(vox.arquivo_cena(list(zip(['body','tail'],parts)),PALETTE,camada=f'marine_{kind}'))
        print(kind,'voxels',sum(map(len,parts)))
