# VMem timing board.
#   Top bus:    a chain of buffers from a Latch On; tap e[k] turns on at tick k.
#   XOR(e[k], e[k+1]) is a 1-tick pulse on one of four toggle traces on the bottom bus.
#   Each toggle trace feeds one VMem pin, which then flips at tick k+2. Every pin has an LED on its output.
B = 24
G = {}
def put(x, y, c):
    assert (x, y) not in G, (x, y, c, G[(x, y)]); G[(x, y)] = c
COL = '0123456789abcdef'; sig = {1: {}, 2: {}}
def colour(bus, name):
    s = sig[bus]
    if name not in s:
        assert len(s) < 16; s[name] = COL[len(s)]
    return s[name]
x = 0
def gate(ink, ins, out):
    """Ports named '^x' go to the top bus, others to the bottom bus."""
    global x
    first = x
    for p, kind in [(i, 'R') for i in ins] + [(out, 'W')]:
        if p.startswith('^'): put(x, B - 4, kind); put(x, B - 5, colour(2, p[1:]))
        else:                 put(x, B - 2, kind); put(x, B - 1, colour(1, p))
        x += 2
    for xx in range(first, x - 1): put(xx, B - 3, ink)
probes = {}
def pin(label, toggle):
    """A VMem pin: toggle trace below, LED above."""
    global x
    x += 1
    for dy, c in zip(range(1, 7), [colour(1, toggle), 'R', 'l', 'W', '5', 'R']): put(x, B - dy, c)
    for dy in (7, 8, 9):
        for dx in (-1, 0, 1): put(x + dx, B - dy, 'E')
    probes[label] = (x, B - 8); at = x
    x += 5
    return at
# Event k flips its pin at tick k+2. Two events on one trace need a tick between them, or their pulses merge.
EVENTS = [(0, 'tD0'), (1, 'tA0'), (2, 'tD0'), (4, 'tD0'), (5, 'tA0'), (6, 'tA1'), (8, 'tA1'), (10, 'tA0'), (12, 'tD0'), (13, 'tD1'), (13, 'tA1')]
def idle_pin():
    """A pin nothing is wired to. The game uses at least 4 address and 4 content bits."""
    global x
    x += 1; put(x, B - 3, 'l'); x += 5
idle_pin(); idle_pin()
a1 = pin('address_1', 'tA1'); a0 = pin('address_0', 'tA0')
idle_pin(); idle_pin()
d1 = pin('content_1', 'tD1'); d0 = pin('content_0', 'tD0')
x += 1; gates_from = x
gate('L', [], '^e0')
for k in range(1, 15): gate('B', [f'^e{k-1}'], f'^e{k}')
for k, target in EVENTS: gate('X', [f'^e{k}', f'^e{k+1}'], target)
for xx in range(0, x - 1): put(xx, B, 'u')
for xx in range(gates_from, x - 1): put(xx, B - 6, 'v')
W = max(a for a, _ in G) + 1; H = max(b for _, b in G) + 1
rows = [''.join(G.get((a, b), '.') for a in range(W)).rstrip('.') or '.' for b in range(H)]
header = ['labels = on', 'ticks = 0..18', 'covers = vmem, 8, 10, 15, 16, 21',
          f'vmem.address = 4,{a0},{B-3},{a0-a1},0', f'vmem.content = 4,{d0},{B-3},{d0-d1},0', 'persistent = 0,3',
          'word.1 = 0xFFFFFFF2', 'word.2 = 2', 'word.3 = 0xABCD0003']
header += [f'probe.{l} = {px},{py}' for l, (px, py) in probes.items()]
open('boards/vmem-timing.txt', 'w').write('\n'.join(header + [''] + rows) + '\n')
print(W, H)
