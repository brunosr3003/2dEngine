#!/usr/bin/env python3
"""Import Zone14's original Hydra, preserving its palette and all 21 joints."""
from pathlib import Path
import argparse
from voxrender import parse_vox
from voxsimplify import reduzir
import molde_corpo as vox

if __name__=='__main__':
    parser=argparse.ArgumentParser()
    parser.add_argument('--source',type=Path,default=Path.home()/'zone14/models')
    args=parser.parse_args()
    names=['body','tail']+[f'paw_{p}' for p in ['front_right','front_left','back_right','back_left']]
    names += [f'{p}_{i}' for i in range(1,6) for p in ['neck','head','jaw']]
    parts=[];palette=None
    for name in names:
        m=max(parse_vox(args.source/f'hydra_{name}.vox'),key=lambda m:len(m.voxels))
        # One shared grid/factor keeps jaws and necks attached. No reshaping.
        parts.append((name,reduzir(m,2)))
        palette=palette or m.palette
    vox.W=vox.D=vox.H=64;vox.TRANSL='0 0 0'
    output=Path(__file__).resolve().parents[2]/'assets/vox/marinhos/hydra_zone14.vox'
    output.write_bytes(vox.arquivo_cena(parts,{i:tuple(palette[i][:3])for i in range(1,256)},camada='zone14_hydra'))
    print(output,len(parts),'parts',sum(len(v)for _,v in parts),'voxels')
