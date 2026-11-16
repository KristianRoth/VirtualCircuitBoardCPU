# Rung B: small combinations, laid out on buses.
# Every signal is one trace colour. A bus joins all stubs of the same colour that touch it, so a circuit is
# just a row of gate modules standing on a bus: no crossings to route.
#   bus 1 runs below the gates (row B); a second bus (row B-6) runs above them where a circuit needs more signals.
import sys
CLOCK = int(sys.argv[1]) if len(sys.argv) > 1 else 3
B = 8
G = {}
def put(x, y, c):
    assert (x, y) not in G, (x, y, c, G[(x, y)])
    G[(x, y)] = c
COL = '0123456789abcdef'
class Circuit:
    def __init__(self, x):
        self.x0 = self.x = x; self.sig = {1: {}, 2: {}}; self.probes = []
    def colour(self, bus, name):
        s = self.sig[bus]
        if name not in s:
            assert len(s) < 16, 'more than 16 signals on one bus'
            s[name] = COL[len(s)]
        return s[name]
    def gate(self, ink, ins, out=None):
        """ins/out: 'name' on bus 1, or '^name' on bus 2."""
        ports = [(p, 'R') for p in ins] + ([(out, 'W')] if out else [])
        first = self.x
        for p, kind in ports:
            up = p.startswith('^'); name = p.lstrip('^')
            if up: put(self.x, B - 4, kind); put(self.x, B - 5, self.colour(2, name))
            else:  put(self.x, B - 2, kind); put(self.x, B - 1, self.colour(1, name))
            self.x += 2
        for x in range(first, self.x - 1): put(x, B - 3, ink)
    def probe(self, label, name):
        self.probes.append((label, name))
    def finish(self):
        width = max(self.x - self.x0 - 1, 5 * len(self.probes) - 2)
        for x in range(self.x0, self.x0 + width): put(x, B, 'u')
        if self.sig[2]:
            for x in range(self.x0, self.x0 + width): put(x, B - 6, 'v')
        out = []
        for i, (label, name) in enumerate(self.probes):
            x = self.x0 + 1 + 5 * i
            put(x, B + 1, self.colour(1, name)); put(x, B + 2, 'R')
            for dy in range(3):
                for dx in (-1, 0, 1): put(x + dx, B + 3 + dy, 'E')
            out.append((label, x, B + 4))
        return self.x0 + width + 3, out
probes = []; x = 0

# 1. Ring oscillator: NOT -> buffer -> buffer -> back to the NOT.
c = Circuit(x)
c.gate('N', ['c'], 'a'); c.gate('B', ['a'], 'b'); c.gate('B', ['b'], 'c')
c.probe('ring', 'c')
x, p = c.finish(); probes += p

# 2. SR latch from two NORs. A 2-tick set pulse comes at the start, a reset pulse four buffers later.
c = Circuit(x)
c.gate('L', [], 'src'); c.gate('B', ['src'], 'late'); c.gate('N', ['late'], 'nlate'); c.gate('A', ['src', 'nlate'], 'set')   # 2-tick pulse
c.gate('B', ['set'], 'd1'); c.gate('B', ['d1'], 'd2'); c.gate('B', ['d2'], 'd3'); c.gate('B', ['d3'], 'reset')
c.gate('Q', ['reset', 'qn'], 'q'); c.gate('Q', ['set', 'q'], 'qn')
c.probe('sr_q', 'q'); c.probe('sr_qn', 'qn')
x, p = c.finish(); probes += p

# 3. D flip-flop from two gated NAND latches: the master follows D while the clock is low, the slave copies the
#    master while the clock is high. D comes from a latch that toggles on every rising clock edge.
c = Circuit(x)
c.gate('C', [], 'clk'); c.gate('N', ['clk'], 'nclk'); c.gate('l', ['clk'], 'd'); c.gate('N', ['d'], 'nd')
c.gate('D', ['d', 'nclk'], 'ms'); c.gate('D', ['nd', 'nclk'], 'mr')
c.gate('D', ['ms', 'mqn'], 'mq'); c.gate('D', ['mr', 'mq'], 'mqn')
c.gate('D', ['mq', 'clk'], 'ss'); c.gate('D', ['mqn', 'clk'], 'sr')
c.gate('D', ['ss', 'qn'], 'q'); c.gate('D', ['sr', 'q'], 'qn')
c.probe('dff_clock', 'clk'); c.probe('dff_d', 'd'); c.probe('dff_q', 'q'); c.probe('dff_qn', 'qn')
x, p = c.finish(); probes += p

# 4. 4-bit ripple counter (latches) feeding a 4-bit adder that adds 3.
#    Sum bit = XOR of its inputs; carry = majority. Where the constant's bit is 0 the adder stage reduces to XOR/AND.
c = Circuit(x)
c.gate('C', [], '^clk'); c.gate('l', ['^clk'], 'a0')
for i in (1, 2, 3):
    c.gate('N', [f'a{i-1}'], f'n{i-1}'); c.gate('l', [f'n{i-1}'], f'a{i}')
c.gate('L', [], 'one')
c.gate('X', ['a0', 'one'], 's0'); c.gate('A', ['a0', 'one'], 'c1')
c.gate('X', ['a1', 'one', 'c1'], 's1')
c.gate('A', ['a1', 'one'], '^t1'); c.gate('A', ['a1', 'c1'], '^t2'); c.gate('A', ['one', 'c1'], '^t3')
c.gate('O', ['^t1', '^t2', '^t3'], 'c2')
c.gate('X', ['a2', 'c2'], 's2'); c.gate('A', ['a2', 'c2'], 'c3')
c.gate('X', ['a3', 'c3'], 's3'); c.gate('A', ['a3', 'c3'], 'c4')
for i in range(4): c.probe(f'count_{i}', f'a{i}')
for i in range(4): c.probe(f'sum_{i}', f's{i}')
c.probe('carry_out', 'c4')
x, p = c.finish(); probes += p

W = max(a for a, _ in G) + 1; H = max(b for _, b in G) + 1
rows = [''.join(G.get((a, b), '.') for a in range(W)).rstrip('.') or '.' for b in range(H)]
header = ['labels = on', f'clock_interval = {CLOCK}', 'ticks = 0..15', 'covers = rungB'] + [f'probe.{l} = {a},{b}' for l, a, b in probes]
open('boards/combinations.txt', 'w').write('\n'.join(header + [''] + rows) + '\n')
print(W, H, len(probes), 'probes')
