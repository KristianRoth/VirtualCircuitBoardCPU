import sys; sys.path.insert(0, __import__('os').path.dirname(__import__('os').path.abspath(__file__)))
from recordings import W
M64=(1<<64)-1; M32=(1<<32)-1
def pcg_step(st,inc):
    old=st; st=(old*6364136223846793005+inc)&M64
    xs=(((old>>18)^old)>>27)&M32; rot=old>>59
    return st,((xs>>rot)|(xs<<((-rot)&31)))&M32
def pcg_godot3(seed,n,inc=1442695040888963407):
    st=seed&M64; st,_=pcg_step(st,inc); out=[]
    for _ in range(n): st,o=pcg_step(st,inc); out.append(o)
    return out
def pcg_godot4(seed,n,seq=1442695040888963407):
    inc=((seq<<1)|1)&M64; st=0; st,_=pcg_step(st,inc); st=(st+seed)&M64; st,_=pcg_step(st,inc); out=[]
    for _ in range(n): st,o=pcg_step(st,inc); out.append(o)
    return out
def pcg_ref(seed,n): return pcg_godot4(seed,n,seq=721347520444481703)  # pcg32 global default-ish
def mt19937(seed,n):
    mt=[0]*624; mt[0]=seed&M32
    for i in range(1,624): mt[i]=(1812433253*(mt[i-1]^(mt[i-1]>>30))+i)&M32
    out=[]; idx=624
    for _ in range(n):
        if idx>=624:
            for k in range(624):
                y=(mt[k]&0x80000000)|(mt[(k+1)%624]&0x7fffffff)
                mt[k]=mt[(k+397)%624]^(y>>1)^(0x9908b0df if y&1 else 0)
            idx=0
        y=mt[idx]; idx+=1
        y^=y>>11; y^=(y<<7)&0x9d2c5680; y^=(y<<15)&0xefc60000; y^=y>>18
        out.append(y&M32)
    return out
def msvc_rand(seed,n):
    st=seed&M32; out=[]
    for _ in range(n): st=(st*214013+2531011)&M32; out.append((st>>16)&0x7fff)
    return out
def minstd(seed,n,a=48271):
    st=seed%2147483647 or 1; out=[]
    for _ in range(n): st=st*a%2147483647; out.append(st)
    return out
def xorshift32(seed,n):
    x=seed&M32 or 1; out=[]
    for _ in range(n):
        x^=(x<<13)&M32; x^=x>>17; x^=(x<<5)&M32; out.append(x)
    return out
def xorshift64(seed,n):
    x=seed&M64 or 1; out=[]
    for _ in range(n):
        x^=(x<<13)&M64; x^=x>>7; x^=(x<<17)&M64; out.append(x&M32)
    return out
def splitmix(seed,n):
    s=seed; out=[]
    for _ in range(n):
        s=(s+0x9e3779b97f4a7c15)&M64; z=s
        z=((z^(z>>30))*0xbf58476d1ce4e5b9)&M64; z=((z^(z>>27))*0x94d049bb133111eb)&M64
        out.append((z^(z>>31))&M32)
    return out
def godot_lcg(seed,n):   # Math::rand_from_seed (Godot 3)
    s=seed&M64; out=[]
    for _ in range(n):
        if s==0: s=49616
        s=(s^(s<<21))&M64; s^=s>>35; s=(s^(s<<4))&M64  # xorshift used in older rand_from_seed variants
        out.append(s&M32)
    return out
GENS={'pcg_godot3':pcg_godot3,'pcg_godot4':pcg_godot4,'pcg_ref':pcg_ref,'mt19937':mt19937,'msvc_rand':msvc_rand,'minstd48271':minstd,'minstd16807':lambda s,n:minstd(s,n,16807),'xorshift32':xorshift32,'xorshift64':xorshift64,'splitmix64':splitmix,'godot_lcg':godot_lcg}
BITS={'lsb':lambda o:o&1,'msb32':lambda o:(o>>31)&1,'bit14':lambda o:(o>>14)&1,'bit15':lambda o:(o>>15)&1,'bit30':lambda o:(o>>30)&1,'mod100<50':lambda o:int(o%100<50),'mod2':lambda o:o%2}
def rev32(x): return int('{:032b}'.format(x)[::-1],2)
found=False
for s,w in W.items():
    obsA=''.join(str((x>>k)&1) for x in w for k in range(32))          # bit 0 (rightmost Random) first
    obsD=''.join(str((x>>k)&1) for x in w for k in range(31,-1,-1))    # leftmost Random first
    cols={k:''.join(str((x>>k)&1) for x in w) for k in range(32)}
    for seed_variant,sv in (('seed',s),('seed-1',s-1),('seed+1',s+1)):
        for gname,g in GENS.items():
            out=g(sv,12000)
            ws=set(out); wr=set(rev32(o) for o in out)
            hits=sum(x in ws for x in w); hitr=sum(x in wr for x in w)
            if hits>3 or hitr>3: print('WORD MATCH',s,seed_variant,gname,hits,hitr); found=True
            for bname,b in BITS.items():
                stream=''.join(str(b(o)) for o in out)
                inv=stream.translate(str.maketrans('01','10'))
                for oname,obs in (('asc',obsA),('desc',obsD)):
                    for iname,st in (('',stream),('inverted',inv)):
                        i=st.find(obs[:200])
                        if i>=0: print('BIT MATCH',s,seed_variant,gname,bname,oname,iname,'offset',i,'full',st[i:i+len(obs)]==obs); found=True
                for k in (0,31):
                    for iname,st in (('',stream),('inverted',inv)):
                        i=st.find(cols[k][:100])
                        if i>=0: print('COLUMN MATCH',s,seed_variant,gname,bname,'bit',k,iname,'offset',i); found=True
print('any match:',found)

# Order-independent test: the number of ones in each word must equal the number of ones among 32 consecutive draws.
def mt19937_64(seed,n):
    NN=312; mt=[0]*NN; mt[0]=seed&M64
    for i in range(1,NN): mt[i]=(6364136223846793005*(mt[i-1]^(mt[i-1]>>62))+i)&M64
    out=[]; idx=NN
    for _ in range(n):
        if idx>=NN:
            for k in range(NN):
                x=(mt[k]&0xFFFFFFFF80000000)|(mt[(k+1)%NN]&0x7FFFFFFF)
                mt[k]=mt[(k+156)%NN]^(x>>1)^(0xB5026F5AA96619E9 if x&1 else 0)
            idx=0
        x=mt[idx]; idx+=1
        x^=(x>>29)&0x5555555555555555; x^=(x<<17)&0x71D67FFFEDA60000; x^=(x<<37)&0xFFF7EEE000000000; x^=x>>43
        out.append(x)
    return out
GENS['mt19937_64']=mt19937_64
BITS['msb64']=lambda o:(o>>63)&1
BITS['bit32']=lambda o:(o>>32)&1
import itertools
found2=False
for s,w in W.items():
    pops=[bin(x).count('1') for x in w]
    for sv in range(max(0,s-2),s+3):
        for gname,g in GENS.items():
            out=g(sv,20000)
            for bname,b in BITS.items():
                bits=[b(o) for o in out]
                for stride in (1,2,3):
                    for phase in range(stride):
                        st=bits[phase::stride]
                        pre=[0]
                        for x in st: pre.append(pre[-1]+x)
                        for off in range(0,min(600,len(st)-32*127)):
                            ok=True; inv=True
                            for n,p in enumerate(pops[:12]):
                                c=pre[off+32*n+32]-pre[off+32*n]
                                if c!=p: ok=False
                                if 32-c!=p: inv=False
                                if not ok and not inv: break
                            if ok or inv:
                                full=all((pre[off+32*n+32]-pre[off+32*n])==(p if ok else 32-p) for n,p in enumerate(pops))
                                print('POPCOUNT MATCH seed',s,'used',sv,gname,bname,'stride',stride,'phase',phase,'offset',off,'inverted' if inv and not ok else '','all words:',full); found2=True
print('any popcount match:',found2)
