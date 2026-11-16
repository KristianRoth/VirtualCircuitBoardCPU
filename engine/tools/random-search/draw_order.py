import sys; sys.path.insert(0, __import__('os').path.dirname(__import__('os').path.abspath(__file__)))
import io,contextlib
with contextlib.redirect_stdout(io.StringIO()):
    import candidates as gen
from candidates import GENS,W
for s,w in W.items():
    out=GENS['mt19937'](s,64*130)
    bits=[o&1 for o in out]
    mapping=[]
    for k in range(32):
        col=[(x>>k)&1 for x in w]
        js=[j for j in range(64) if all(bits[64*(c+1)+j]==col[c] for c in range(len(w)))]
        mapping.append(js)
    print('seed',s,'bit k -> draw index within the 64-draw cycle:',mapping)
    # what are the other 32 draws? compare second half with the first half / nothing
    print('  first cycle (address 0) draws 0..31 low bits:',''.join(str(b) for b in bits[0:32]))
