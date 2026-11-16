# Random -> VMem recorder.
#   Top:    an 8-bit synchronous counter built on the VMem address pins. It pauses on a Breakpoint at address 128.
#   Bottom: 32 Random pixels, each wired straight to one VMem content pin.
#   Right:  a clock and a rising-edge detector that give one 1-tick pulse every 24 ticks.
# Each pulse triggers every Random (the bits are written to the current address two ticks later)
# and steps the address eight ticks after the pulse.
N, M = 8, 32             # address bits, content bits
SW, DW = 6, 2            # width of an address slice and of a content slice
YP = 3 * (N - 1) + 1     # row of the address pins
A = YP + 3               # row of the pulse line above the Randoms
G = {}
def put(x, y, c):
    assert (x, y) not in G, (x, y)
    G[(x, y)] = c
def column(x, y, chars):
    for i, c in enumerate(chars): put(x, y + i, c)
def row(x, y, chars):
    for i, c in enumerate(chars): put(x + i, y, c)

WIDTH = M * DW                                   # both sections end at this column; bit 0 is rightmost
# Address counter. Slice k: carry-in comes from the right on row 0, AND(carry-in, pin k) is the carry-out to the left.
# The pin's toggle input is the carry-in delayed by N-1-k buffers, so that all pins toggle in the same tick.
for k in range(N):
    ox = WIDTH - SW * (k + 1)
    row(ox, 0, '5WAR55')
    column(ox + 2, 1, 'R' + '5' * (YP - 1))      # the pin's output, up to the AND
    row(ox + 3, YP, 'WlR')                       # Write, pin, Read
    delay = 'RBW' * (N - 1 - k)
    column(ox + 5, 1, delay + '5' * (YP - 1 - len(delay)))
row(WIDTH - SW * N, 3, '!R')                     # Breakpoint on the top address bit
# Randoms: pulse line, Read, Random, Write, Read, content pin.
for j in range(M):
    column(WIDTH - DW * (j + 1) + 1, A, '5R?WR' + 'l')
    put(WIDTH - DW * (j + 1), A, '5')
# Pulse generator: clock -> AND(clock, NOT clock) gives a 1-tick pulse on each rising clock edge.
BX = WIDTH + 1
row(WIDTH, 0, '55' + '5WAR55WC')
column(BX, 1, '5' * A)
put(WIDTH, A, '5')
GX = WIDTH + 2
column(GX + 5, 1, 'RN'); row(GX + 2, 2, '55W'); put(GX + 2, 1, 'R')

W = max(x for x, _ in G) + 1; H = max(y for _, y in G) + 1
for seed in (1, 2):
    rows = [''.join(G.get((x, y), '.') for x in range(W)).rstrip('.') or '.' for y in range(H)]
    header = ['clock_interval = 12', f'random_seed = {seed}', 'covers = Random, 14',
              f'vmem.address = {N},{WIDTH - SW + 4},{YP},{SW},0', f'vmem.content = {M},{WIDTH - 1},{A + 5},{DW},0', 'persistent = 0,255']
    open(f'boards/random-vmem-seed-{seed}.txt', 'w').write('\n'.join(header + [''] + rows) + '\n')
print(W, H)
