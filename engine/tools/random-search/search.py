import sys; sys.path.insert(0, __import__('os').path.dirname(__import__('os').path.abspath(__file__)))
import io,contextlib
with contextlib.redirect_stdout(io.StringIO()):
    import candidates as gen
from candidates import GENS,W,BITS
found=False
for s,w in W.items():
    pops=[bin(x).count('1') for x in w]
    for sv in sorted(set([s,s-1,s+1,0,s*2,s+12345,(s*1103515245+12345)&0xffffffff, s^0x5DEECE66D])):
        for gname in ['mt19937','msvc_rand','pcg_godot3','pcg_godot4','minstd48271','minstd16807']:
            out=GENS[gname](sv,40000)
            for bname in ['lsb','msb32','bit14','bit15','bit30']:
                bits=[BITS[bname](o) for o in out]
                for stride in (1,2):
                    for phase in range(stride):
                        st=bits[phase::stride]; pre=[0]
                        for x in st: pre.append(pre[-1]+x)
                        for C in (32,33,34,40,48,64,65,66,96,128):
                            for off in range(0,400):
                                if off+C*127+32>=len(pre): break
                                ok=inv=True
                                for n,p in enumerate(pops[:10]):
                                    c=pre[off+C*n+32]-pre[off+C*n]
                                    ok&=(c==p); inv&=(32-c==p)
                                    if not(ok or inv): break
                                if ok or inv:
                                    print('MATCH data',s,'seed',sv,gname,bname,'stride',stride,phase,'chunk',C,'off',off,'inv' if inv and not ok else ''); found=True
print('any match:',found)
