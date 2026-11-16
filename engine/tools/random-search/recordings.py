import json,base64,subprocess,struct,sys
def words(f):
    d=json.load(open(f)); raw=base64.b64decode(d['vmem_data'])
    out=subprocess.run(['zstd','-dc'],input=raw,capture_output=True).stdout
    return struct.unpack('>%dI'%(len(out)//4),out[:len(out)//4*4])
W={s:words(f'engine/tests/boards/random-vmem-seed-{s}.after.vcb')[1:128] for s in (1,2)}
if __name__=='__main__':
    for s,w in W.items():
        bits=[[ (x>>k)&1 for k in range(32)] for x in w]
        n=len(w)*32; ones=sum(map(sum,bits))
        same11=sum(bits[i][k]&bits[i+1][k] for i in range(len(w)-1) for k in range(32))
        pairs=(len(w)-1)*32
        print('seed',s,'words',len(w),'ones %.3f'%(ones/n),'P(1 then 1 in same bit) %.3f'%(same11/pairs))
        print('  per-bit ones:',[sum(b[k] for b in bits) for k in range(32)])
        print('  first words:',' '.join('%08x'%x for x in w[:6]))
