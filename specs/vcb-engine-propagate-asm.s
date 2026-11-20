; The engine's hottest function, `Engine::propagate` (engine/src/sim2.rs), as it is in the profile-guided build
; of commit 2f9f893 (attempt 30), arm64, Apple M5. Disassembled from engine/target/release/vcb with objdump; the
; comments are written by hand. In this build the function is not inlined: it is called from three places, once
; per simulated tick from step_real. Addresses change with every build.
;
; A cell is 8 bytes per entity: +0 count (u16), +2 threshold (u16), +4 kind, +5 on, +6 queued, +7 toggle.
; Kinds: 0 OR, 1 Breakpoint, 2 NOR, 3 AND, 4 NAND, 5 XOR, 6 XNOR, 7 latch, 8 pin, 9 scheduled, 10 Random, 11 trace, 12 dead.


; ENTRY. x0 = the engine, x1 = the list of entities that flipped in this tick, x2 = how many.
; Nothing flipped: return at once.
+0x000:  cbz x2, +0x338
; Save ten registers the function will use (five pairs on the stack). Paid once per call, that is once per tick.
+0x004:  stp x28, x27, [sp, #-0x50]!
+0x008:  stp x26, x25, [sp, #0x10]
+0x00c:  stp x24, x23, [sp, #0x20]
+0x010:  stp x22, x21, [sp, #0x30]
+0x014:  stp x20, x19, [sp, #0x40]
; Set-up for PASS 1. x8 = end of the flipped list, x9 = the cells (8 bytes per entity), x10 = writes.items,
; x11 = writes.start, w12 = the constant 1.
+0x018:  add x8, x1, x2, lsl #2
+0x01c:  ldr x9, [x0, #0xf8]                              ; x9 = cells
+0x020:  ldr x10, [x0, #0x170]                            ; x10 = writes.items
+0x024:  mov w12, #0x1
+0x028:  mov x13, x1
+0x02c:  ldr x11, [x0, #0x158]                            ; x11 = writes.start

; PASS 1, per flipped entity e: the traces it writes get their writer count adjusted (+1 if e is now on, -1 if off).
; Every count is done before any trace is looked at, so a trace that loses one writer and gains another does not flicker.
+0x030:  ldr w15, [x13], #0x4                             ; e = next flipped entity
+0x034:  add x14, x9, x15, lsl #3
+0x038:  ldrb w14, [x14, #0x5]                            ; e's 'on' byte
+0x03c:  cmp w14, #0x0
+0x040:  cneg w14, w12, eq                                ; w14 = on ? +1 : -1
+0x044:  add x15, x11, x15, lsl #2
+0x048:  ldp w16, w17, [x15]                              ; w16, w17 = start and end of e's written traces
+0x04c:  cmp w16, w17                                     ; none written?
+0x050:  b.ne +0x60
+0x054:  cmp x13, x8                                      ; more flipped entities?
+0x058:  b.ne +0x30
+0x05c:  b +0x8c
; PASS 1 inner loop: one written trace per round. 6 instructions per trace.
+0x060:  add x15, x10, x16, lsl #2
+0x064:  lsl x17, x17, #2
+0x068:  sub x16, x17, x16, lsl #2
+0x06c:  ldr w17, [x15], #0x4                             ; t = next written trace
+0x070:  lsl x17, x17, #3
+0x074:  ldrh w2, [x9, x17]                               ; its writer count
+0x078:  add w2, w2, w14                                  ; adjust
+0x07c:  strh w2, [x9, x17]                               ; store
+0x080:  subs x16, x16, #0x4
+0x084:  b.ne +0x6c
+0x088:  b +0x54
; Set-up for PASS 2. Eleven values are loaded from the engine ONCE and stay in registers (this is what attempt 30's
; item 1 achieved): x12 direct.start, x13 direct.items, x14 the queue's buffer, x15 slot, x16 packed, x17 readers.start,
; x3 readers.items, x7 the queue's length. Three bit tables for the reader's kind: w4 = latch or pin,
; w5 = XOR, XNOR or Random (always woken), w6 = kinds a falling input never wakes.
+0x08c:  ldr x12, [x0, #0x228]
+0x090:  ldr x13, [x0, #0x240]
+0x094:  ldr x14, [x0, #0x2a0]
+0x098:  ldr x15, [x0, #0x288]
+0x09c:  ldr x16, [x0, #0x270]
+0x0a0:  ldr x17, [x0, #0x188]
+0x0a4:  mov w2, #0x1
+0x0a8:  ldr x3, [x0, #0x1a0]
+0x0ac:  mov w4, #0x180
+0x0b0:  mov w5, #0x460
+0x0b4:  mov w6, #0x1b80
+0x0b8:  ldr x7, [x0, #0x2a8]
; PASS 2, per flipped entity e. First its DIRECT readers: gates that read a trace only e writes.
+0x0bc:  ldr w19, [x1], #0x4                              ; e = next flipped entity
+0x0c0:  ldr w21, [x12, x19, lsl #2]                      ; start of e's direct readers
+0x0c4:  add x20, x19, #0x1
+0x0c8:  ldr w22, [x12, x20, lsl #2]                      ; end of them
+0x0cc:  cmp w21, w22                                     ; NOT NEEDED: this compare is repeated by the next line (source: 'if from != to' then 'for i in from..to')
+0x0d0:  ccmp w21, w22, #0x2, ne                          ; start >= end ...
+0x0d4:  b.hs +0x150                     ; ... then no direct readers

; e has direct readers. w23 = +1 or -1 again; then the loop is chosen by whether e is now on or off.
; The compiler made two copies of the reader loop, one per direction (this one: e turned OFF).
+0x0d8:  add x23, x9, x19, lsl #3
+0x0dc:  ldrb w24, [x23, #0x5]                            ; e's 'on' byte (second time; pass 1 read it too)
+0x0e0:  cmp w24, #0x0
+0x0e4:  cneg w23, w2, eq                                 ; w23 = on ? +1 : -1
+0x0e8:  tbnz w24, #0x0, +0x2a4          ; on: use copy D

; READER LOOP, copy A (direct readers, input turned off). One reader per round.
+0x0ec:  ldr w24, [x13, x21, lsl #2]                      ; r = next reader
+0x0f0:  add x25, x9, x24, lsl #3                         ; x25 = r's cell
+0x0f4:  ldrh w26, [x25]                                  ; LOAD 1 of the cell: count
+0x0f8:  add w28, w26, w23                                ; count + step
+0x0fc:  strh w28, [x25]                                  ; store the count
+0x100:  ldrb w27, [x25, #0x4]                            ; LOAD 2 of the cell: kind
+0x104:  cmp w27, #0x5                                    ; kinds 0-4 are the threshold gates (OR, Breakpoint, NOR, AND, NAND)
+0x108:  b.hs +0x124                     ; other kinds: below
+0x10c:  and w27, w28, #0xffff                            ; new count as 16 bits
+0x110:  umax w26, w26, w27                               ; max(old, new)
+0x114:  ldrh w27, [x25, #0x2]                            ; LOAD 3 of the cell: threshold
+0x118:  cmp w26, w27                                     ; the count is at or just left the threshold?
+0x11c:  b.eq +0x12c                     ; yes: its output can change, wake it
+0x120:  b +0x144                        ; no. EXTRA JUMP: the common case (not woken) takes two branches here; copies B and C fall through

; ...the reader is not a threshold gate: woken only if it is an XOR-like kind (a falling input does not wake a latch).
+0x124:  lsr w26, w5, w27
+0x128:  tbz w26, #0x0, +0x144
; Wake the reader unless it is queued already: set its flag, append it to the queue.
+0x12c:  ldrb w26, [x25, #0x6]                            ; LOAD 4 of the cell: queued flag
+0x130:  tbnz w26, #0x0, +0x144
+0x134:  strb w2, [x25, #0x6]                             ; queued = true
+0x138:  str w24, [x14, x7, lsl #2]                       ; queue[len] = r   (no capacity check, no call: attempt 30 item 1)
+0x13c:  add x7, x7, #0x1                                 ; len + 1
+0x140:  str x7, [x0, #0x2a8]                             ; COULD WAIT: the queue's length is written back to the engine at every push; x7 already holds it, once at the end would do

; Next direct reader.
+0x144:  add x21, x21, #0x1
+0x148:  cmp x22, x21
+0x14c:  b.ne +0xec
; Now the TRACES e writes (those with several writers). x19..x20 = their range in writes.items.
+0x150:  ldr w19, [x11, x19, lsl #2]
+0x154:  ldr w20, [x11, x20, lsl #2]
+0x158:  cmp w19, w20
+0x15c:  b.lo +0x16c
+0x160:  cmp x1, x8
+0x164:  b.eq +0x324
+0x168:  b +0xbc
; TRACE LOOP: one written trace t per round. Its state is 'writer count > 0'.
+0x16c:  ldr w21, [x10, x19, lsl #2]
+0x170:  add x22, x9, x21, lsl #3
+0x174:  ldrh w24, [x22]                                  ; the trace's writer count
+0x178:  umin w23, w24, #0x1                              ; 1 if any writer is on
+0x17c:  ldrb w25, [x22, #0x5]                            ; the trace's state
+0x180:  eor w25, w23, w25
+0x184:  tbz w25, #0x0, +0x294           ; unchanged: next trace

; The trace changed: store its new state, flip its bit in the packed copy of the state (slot -> word and bit).
+0x188:  strb w23, [x22, #0x5]
+0x18c:  ldr w22, [x15, x21, lsl #2]                      ; its place in the packed state
+0x190:  lsl x23, x2, x22                                 ; the bit
+0x194:  lsr x22, x22, #3
+0x198:  and x22, x22, #0x1ffffff8                        ; the word's offset
+0x19c:  ldr x25, [x16, x22]
+0x1a0:  eor x23, x23, x25
+0x1a4:  str x23, [x16, x22]                              ; flipped in the packed copy

; The trace's readers: their range. None: next trace.
+0x1a8:  add x22, x17, x21, lsl #2
+0x1ac:  ldp w21, w22, [x22]                              ; start and end of the trace's readers
+0x1b0:  cmp w21, w22
+0x1b4:  b.hs +0x294
; w23 = +1 or -1 for the trace's direction; choose the reader loop copy by it.
+0x1b8:  cmp w24, #0x0
+0x1bc:  cneg w23, w2, eq
+0x1c0:  cbz w24, +0x254                 ; turned off: copy C

; READER LOOP, copy B (a trace's readers, trace turned ON). Same work as copy A.
+0x1c4:  ldr w24, [x3, x21, lsl #2]
+0x1c8:  add x25, x9, x24, lsl #3
+0x1cc:  ldrh w27, [x25]
+0x1d0:  add w28, w27, w23
+0x1d4:  strh w28, [x25]
+0x1d8:  ldrb w26, [x25, #0x4]
+0x1dc:  cmp w26, #0x5
+0x1e0:  b.hs +0x224
+0x1e4:  and w26, w28, #0xffff
+0x1e8:  umax w26, w27, w26
+0x1ec:  ldrh w27, [x25, #0x2]
+0x1f0:  cmp w26, w27
+0x1f4:  b.eq +0x208                     ; wake: out of line
+0x1f8:  add x21, x21, #0x1                               ; next reader (the common case falls straight through here)
+0x1fc:  cmp x22, x21
+0x200:  b.ne +0x1c4
+0x204:  b +0x294
; Wake (copy B).
+0x208:  ldrb w26, [x25, #0x6]
+0x20c:  tbnz w26, #0x0, +0x1f8
+0x210:  strb w2, [x25, #0x6]
+0x214:  str w24, [x14, x7, lsl #2]
+0x218:  add x7, x7, #0x1
+0x21c:  str x7, [x0, #0x2a8]                             ; COULD WAIT (as above)
+0x220:  b +0x1f8
; Copy B, reader is not a threshold gate: a latch or pin has its 'toggle' mark flipped (far below, at +0x33c), an
; XOR-like gate is woken, anything else is left.
+0x224:  lsr w27, w4, w26
+0x228:  tbnz w27, #0x0, +0x33c
+0x22c:  b +0x34c
; Copy C, reader is not a threshold gate (input turned off): woken unless its kind is in the never-woken table.
+0x230:  lsr w26, w6, w27
+0x234:  tbnz w26, #0x0, +0x288
+0x238:  ldrb w26, [x25, #0x6]
+0x23c:  tbnz w26, #0x0, +0x288
+0x240:  strb w2, [x25, #0x6]
+0x244:  str w24, [x14, x7, lsl #2]
+0x248:  add x7, x7, #0x1
+0x24c:  str x7, [x0, #0x2a8]                             ; COULD WAIT (as above)
+0x250:  b +0x288
; READER LOOP, copy C (a trace's readers, trace turned OFF).
+0x254:  ldr w24, [x3, x21, lsl #2]
+0x258:  add x25, x9, x24, lsl #3
+0x25c:  ldrh w26, [x25]
+0x260:  add w28, w26, w23
+0x264:  strh w28, [x25]
+0x268:  ldrb w27, [x25, #0x4]
+0x26c:  cmp w27, #0x5
+0x270:  b.hs +0x230
+0x274:  and w27, w28, #0xffff
+0x278:  umax w26, w26, w27
+0x27c:  ldrh w27, [x25, #0x2]
+0x280:  cmp w26, w27
+0x284:  b.eq +0x238
+0x288:  add x21, x21, #0x1
+0x28c:  cmp x22, x21
+0x290:  b.ne +0x254
; Next written trace; after the last one, the next flipped entity.
+0x294:  add x19, x19, #0x1
+0x298:  cmp x19, x20
+0x29c:  b.eq +0x160
+0x2a0:  b +0x16c
; READER LOOP, copy D (direct readers, input turned ON).
+0x2a4:  ldr w24, [x13, x21, lsl #2]
+0x2a8:  add x25, x9, x24, lsl #3
+0x2ac:  ldrh w27, [x25]
+0x2b0:  add w28, w27, w23
+0x2b4:  strh w28, [x25]
+0x2b8:  ldrb w26, [x25, #0x4]
+0x2bc:  cmp w26, #0x5
+0x2c0:  b.hs +0x2dc
+0x2c4:  and w26, w28, #0xffff
+0x2c8:  umax w26, w27, w26
+0x2cc:  ldrh w27, [x25, #0x2]
+0x2d0:  cmp w26, w27
+0x2d4:  b.eq +0x2e4
+0x2d8:  b +0x2fc
+0x2dc:  lsr w27, w5, w26
+0x2e0:  tbz w27, #0x0, +0x30c
+0x2e4:  ldrb w26, [x25, #0x6]
+0x2e8:  tbnz w26, #0x0, +0x2fc
+0x2ec:  strb w2, [x25, #0x6]
+0x2f0:  str w24, [x14, x7, lsl #2]
+0x2f4:  add x7, x7, #0x1
+0x2f8:  str x7, [x0, #0x2a8]                             ; COULD WAIT (as above)
+0x2fc:  add x21, x21, #0x1
+0x300:  cmp x22, x21
+0x304:  b.eq +0x150
+0x308:  b +0x2a4
; Copy D, latch or pin: flip its toggle mark, then wake it.
+0x30c:  lsr w26, w4, w26
+0x310:  tbz w26, #0x0, +0x2fc
+0x314:  ldrb w26, [x25, #0x7]
+0x318:  eor w26, w26, #0x1
+0x31c:  strb w26, [x25, #0x7]
+0x320:  b +0x2e4
; EXIT: restore the ten registers and return.
+0x324:  ldp x20, x19, [sp, #0x40]
+0x328:  ldp x22, x21, [sp, #0x30]
+0x32c:  ldp x24, x23, [sp, #0x20]
+0x330:  ldp x26, x25, [sp, #0x10]
+0x334:  ldp x28, x27, [sp], #0x50
+0x338:  ret

; Copy B's latch-or-pin case: flip the toggle mark, then wake.
+0x33c:  ldrb w26, [x25, #0x7]
+0x340:  eor w26, w26, #0x1
+0x344:  strb w26, [x25, #0x7]
+0x348:  b +0x208
+0x34c:  lsr w26, w5, w26
+0x350:  tbz w26, #0x0, +0x1f8
+0x354:  b +0x208
; ======================================================================================================================
; WHAT IS NEEDED AND WHAT IS NOT (read by hand, 2026-11-20; nothing below was measured)
;
; The work per reader told, in the common case (a threshold gate that is not woken), copy B: 16 instructions:
;   1 load of the reader's number, 1 address, 4 touching the count (load, add, store, mask), 3 for the kind (load,
;   compare, branch), 4 for the threshold test (max, load, compare, branch), 3 for the loop (add, compare, branch).
; A reader that is woken adds 6: load and test of the queued flag, store of the flag, store into the queue, add, and
; the store of the queue's length. Per flipped entity outside the reader loops: about 25. All of it reads and writes
; the first cache. The compiler made four copies of the reader loop (direct readers or a trace's readers, input
; turned on or off), so the direction is not tested per reader.
;
; NOT NEEDED, or could be less:
; 1. +0x0cc: a compare that the next instruction repeats. Once per flipped entity. From `if from != to` before
;    `for i in from..to` in the source; dropping the `if` removes it. One instruction per flip: nothing measurable.
; 2. +0x140, +0x21c, +0x24c, +0x2f8: the queue's length is stored back into the engine at every push, although it is
;    kept in a register (x7) for the whole function. Writing it once at the end would save one store per gate woken
;    (6 to 8 a tick). Needs the push written against a local length in `propagate`.
; 3. The threshold test is `max(old, new) == threshold` (+0x10c to +0x11c: mask, max, load, compare). In a copy where
;    the direction is known it could be one compare less: turned on, `new == threshold`; turned off,
;    `old == threshold`. The compiler did not see that, because the step is computed (+1 or -1) and not a constant in
;    each copy. Writing the test per direction in `feed` would save the max, and in the turned-off copies the mask too.
; 4. +0x120: in copy A (and D) the common case, not woken, takes a conditional branch and then an unconditional one;
;    copies B and C fall through. Layout chosen by the compiler from the profile; not controllable from the source.
; 5. The four loads from the reader's cell (count, kind, threshold, queued flag) are four instructions on the same
;    8 bytes. One 64-bit load and three field extractions would be one load and three register operations. Not clearly
;    better: loads that hit the first cache are cheap and run side by side, and the count is stored back as 16 bits,
;    so a 64-bit load of a cell fed twice in a row would have to wait for that store. Listed, not recommended.
; 6. The function is entered by a call and saves and restores ten registers (+0x004 to +0x014, +0x324 to +0x334):
;    a call, a return and twenty stack accesses per tick, against about 250 instructions of work in a busy tick.
;    Inlining it into `step_real` (`#[inline(always)]`) would remove them, at the price of `step_real` having to keep
;    more in registers itself. The compiler chose not to inline with the profile in hand. Cheap to try.
; 7. e's `on` byte and its range of written traces are loaded in pass 1 and again in pass 2. That follows from doing
;    all counts before all traces, which is needed (see pass 1).
;
; NEEDED: everything else. In particular nothing is reloaded from the engine inside the loops any more, there is no
; call and no bounds check, and the kind is tested with one compare and one table bit.
;
; TRIED (attempt 31, 2026-11-20): items 1 to 6 each built and timed alone against attempt 30: 1.005, 0.996, 1.000,
; 0.985, 0.964, 0.997. None helps; 4 and 5 are worse. Nothing was kept.
;
; What it adds up to: items 2 and 3 are one or two instructions out of 16 to 22 per reader; item 6 is perhaps twenty
; instructions out of 250 per tick. Each is at most a few percent of this function, which is about half the run.
