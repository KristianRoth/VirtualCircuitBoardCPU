; `Engine::propagate_split` (engine/src/sim2/split.rs) as it was at commit 6e8c4f4 (attempt 32), BEFORE attempt 33, arm64,
; Apple M5: the function that tells readers in a step with a record, 43 to 45% of a run on the user's CPU
; (specs/vcb-engine-profile-2026-11-20.md). Built with
;   RUSTFLAGS="--cfg separate -C target-cpu=native" cargo build --release --target-dir target/separate
; (a plain release build that keeps the function as its own symbol; in the everyday build it is inlined into the run loop)
; and disassembled with objdump; the comments are written by hand. Addresses are relative to the function's start.
; Compare `Engine::propagate` in vcb-engine-propagate-asm.s, which does the same work in a step simulated whole.
;
; A cell is 8 bytes per entity: +0 count (u16), +2 threshold (u16), +4 kind, +5 on, +6 queued, +7 toggle.
; Kinds: 0 OR, 1 Breakpoint, 2 NOR, 3 AND, 4 NAND, 5 XOR, 6 XNOR, 7 latch, 8 pin, 9 scheduled, 10 Random, 11 trace, 12 dead.
; In the engine (x19): +0xf8 cells, +0x158 writes.start, +0x170 writes.items, +0x188 readers.start, +0x1a0 readers.items,
; +0x228 direct.start, +0x240 direct.items, +0x270 packed, +0x288 slot, +0x2a0 the queue's buffer, +0x2a8 its length,
; +0x3a0/+0x3a8/+0x3b0 split.traces (capacity, buffer, length), +0x3e0 split.loaded's length, +0x5f8 split.start,
; +0x608 split.record, +0x610 split.cursor, +0x1150 tick.
; In the record (x23): +0x80 the `variable` bitmap's address, +0xb0 and +0xb8 `timed` (address, length).
; "RELOADED" marks a value that is loaded from the engine or the record inside a loop although nothing in the loop changes it.

; ENTRY. x0 = the engine, x1 = the list of entities that flipped in this tick, x2 = how many, w3 = `inject`.
; A 0x90-byte frame and twelve registers saved: paid once per call, that is once per tick (twice in a tick in which VMem loads).
+0x000:  sub sp, sp, #0x90
+0x004:  stp x28, x27, [sp, #0x30]
+0x008:  stp x26, x25, [sp, #0x40]
+0x00c:  stp x24, x23, [sp, #0x50]
+0x010:  stp x22, x21, [sp, #0x60]
+0x014:  stp x20, x19, [sp, #0x70]
+0x018:  stp x29, x30, [sp, #0x80]
+0x01c:  add x29, sp, #0x80

; THE RECORD'S BOX IS TAKEN OUT: the pointer is loaded and the engine's field is set to None (one load, one store).
; x23 = the record for the whole function, x19 = the engine.
+0x020:  ldr x23, [x0, #0x608]                            ; x23 = self.split.record
+0x024:  str xzr, [x0, #0x608]                            ; self.split.record = None
+0x028:  cbz x23, +0x938                                  ; None already: panic (never)
+0x02c:  mov x19, x0
+0x030:  ldr x0, [x0, #0x610]                             ; x0 = self.split.cursor (`from`); loaded once
+0x034:  mov x17, x0                                      ; x17 = `to`

; `inject` (the tick's own call): empty `traces` and `loaded`, and move the cursor over the record's `timed` entries of this
; tick. tick, split.start and the cursor are each loaded once and the cursor is stored once: nothing to gain here.
+0x038:  tbz w3, #0x0, +0x098
+0x03c:  ldr w9, [x19, #0x1150]                           ; self.tick (low half)
+0x040:  ldr w10, [x19, #0x5f8]                           ; self.split.start (low half)
+0x044:  str xzr, [x19, #0x3b0]                           ; split.traces.len = 0
+0x048:  str xzr, [x19, #0x3e0]                           ; split.loaded.len = 0
+0x04c:  ldr x8, [x23, #0xb8]                             ; record.timed.len
+0x050:  mov x17, x0
+0x054:  cmp x0, x8
+0x058:  b.hs +0x094
+0x05c:  sub w9, w9, w10                                  ; offset = tick - start
+0x060:  ldr x10, [x23, #0xb0]                            ; record.timed.ptr (12 bytes an entry: tick, entity, value)
+0x064:  mov w11, #0xc ; =12
+0x068:  madd x10, x0, x11, x10
+0x06c:  mov x17, x0
+0x070:  ldr w11, [x10], #0xc                             ; the entry's tick
+0x074:  cmp w11, w9
+0x078:  b.hi +0x094                                      ; later than this tick: stop
+0x07c:  add x17, x17, #0x1
+0x080:  cmp x8, x17
+0x084:  b.ne +0x070
+0x088:  mov x17, x8
+0x08c:  str x8, [x19, #0x610]                            ; self.split.cursor = to
+0x090:  b +0x098
+0x094:  str x17, [x19, #0x610]                           ; self.split.cursor = to

; `&record.timed[from..to]`: one bounds check per call (it cannot fail). x3 = the first entry, x4 (below) = past the last.
+0x098:  ldr x8, [x23, #0xb8]
+0x09c:  cmp x17, x0
+0x0a0:  ccmp x17, x8, #0x2, hs
+0x0a4:  b.hi +0x924                                      ; out of range: panic (never)
+0x0a8:  ldr x8, [x23, #0xb0]
+0x0ac:  mov w9, #0xc ; =12
+0x0b0:  madd x3, x0, x9, x8
+0x0b4:  add x5, x1, x2, lsl #2

; PASS 1, per flipped entity e: the traces it writes get their writer count adjusted. As in `propagate`, except that here
; the cells' address and writes.start are LOADED FROM THE ENGINE AGAIN per entity, and writes.items and the cells per trace.
+0x0b8:  cbz x2, +0x128
+0x0bc:  mov w9, #0x1 ; =1
+0x0c0:  mov x10, x1
+0x0c4:  b +0x0d0
+0x0c8:  cmp x10, x5
+0x0cc:  b.eq +0x128
+0x0d0:  ldr w12, [x10], #0x4                             ; e = next flipped entity
+0x0d4:  ldr x11, [x19, #0xf8]                            ; RELOADED: cells
+0x0d8:  add x11, x11, x12, lsl #3
+0x0dc:  ldrb w11, [x11, #0x5]                            ; e's 'on' byte
+0x0e0:  cmp w11, #0x0
+0x0e4:  cneg w11, w9, eq                                 ; w11 = on ? +1 : -1
+0x0e8:  ldr x13, [x19, #0x158]                           ; RELOADED: writes.start
+0x0ec:  add x12, x13, x12, lsl #2
+0x0f0:  ldp w13, w12, [x12]                              ; start and end of e's written traces
+0x0f4:  cmp w13, w12
+0x0f8:  b.hs +0x0c8
+0x0fc:  ldr x14, [x19, #0x170]                           ; RELOADED per trace: writes.items
+0x100:  ldr w14, [x14, x13, lsl #2]
+0x104:  add x13, x13, #0x1
+0x108:  ldr x15, [x19, #0xf8]                            ; RELOADED per trace: cells
+0x10c:  lsl x14, x14, #3
+0x110:  ldrh w16, [x15, x14]                             ; the trace's writer count
+0x114:  add w16, w16, w11
+0x118:  strh w16, [x15, x14]                             ; store
+0x11c:  cmp x12, x13
+0x120:  b.ne +0x0fc
+0x124:  b +0x0c8

; x4 = past the last timed entry. Any timed entries? Then their pass 1 first (at +0x4f4), which comes back to +0x13c.
+0x128:  mov w9, #0xc ; =12
+0x12c:  mul x9, x17, x9
+0x130:  add x4, x8, x9
+0x134:  cmp x0, x17
+0x138:  b.ne +0x4f4                                      ; timed entries: +0x4f4

; PASS 2 over the flipped entities. Three values are put on the stack because the call to the growth routine (below) would
; lose them. Bit tables for a reader's kind: w27 = XOR, XNOR, Random (always woken); w7 = scheduled, trace, dead (never);
; w30 = latch or pin; later w17 = the five threshold gates, w0 = kinds a falling input never wakes.
+0x13c:  stp x17, x0, [sp]
+0x140:  str x4, [sp, #0x18]
+0x144:  cbz x2, +0x4b8                                   ; nothing flipped (VMem's call with no pin changed)
+0x148:  mov w6, #0x1 ; =1
+0x14c:  mov w27, #0x460 ; =1120
+0x150:  mov w7, #0x1a00 ; =6656
+0x154:  mov w30, #0x180 ; =384
+0x158:  str x3, [sp, #0x20]
+0x15c:  str x5, [sp, #0x10]
+0x160:  b +0x16c
+0x164:  cmp x1, x5
+0x168:  b.eq +0x4b8

; Per flipped entity e: its DIRECT readers first.
+0x16c:  ldr w8, [x1], #0x4                               ; e = next flipped entity
+0x170:  ldr x11, [x19, #0x228]                           ; RELOADED per entity: direct.start
+0x174:  ldr w10, [x11, x8, lsl #2]
+0x178:  add x9, x8, #0x1
+0x17c:  ldr w11, [x11, x9, lsl #2]
+0x180:  cmp w10, w11                                     ; no direct readers
+0x184:  b.hs +0x260
+0x188:  ldr x12, [x19, #0xf8]                            ; RELOADED: cells
+0x18c:  add x12, x12, x8, lsl #3
+0x190:  ldrb w12, [x12, #0x5]                            ; e's 'on' byte (pass 1 read it too)
+0x194:  cmp w12, #0x0
+0x198:  cneg w13, w6, eq                                 ; w13 = +1 or -1
+0x19c:  b +0x1c0

; ...a FIXED reader (not in the record's variable part): its count is adjusted and nothing else. 5 instructions.
+0x1a0:  ldr x15, [x19, #0xf8]                            ; RELOADED per reader: cells
+0x1a4:  lsl x14, x14, #3
+0x1a8:  ldrh w16, [x15, x14]
+0x1ac:  add w16, w16, w13
+0x1b0:  strh w16, [x15, x14]

; Next direct reader.
+0x1b4:  add x10, x10, #0x1
+0x1b8:  cmp x11, x10
+0x1bc:  b.eq +0x260

; DIRECT READER LOOP, one reader per round. The test that `propagate` does not have: is the reader variable?
; Two loads from structures (the bitmap's address from the record, the reader list's address from the engine), then the
; bitmap's word, a shift and a branch: 4 instructions and one load of data more than `propagate`, plus the two reloads.
+0x1c0:  ldr x15, [x23, #0x80]                            ; RELOADED per reader: record.variable's address (the source held `&Vec`, not a slice)
+0x1c4:  ldr x14, [x19, #0x240]                           ; RELOADED per reader: direct.items
+0x1c8:  ldr w14, [x14, x10, lsl #2]                      ; r = next reader
+0x1cc:  lsr x16, x14, #6                                 ; r / 64
+0x1d0:  ldr x15, [x15, x16, lsl #3]                      ; that word of the bitmap
+0x1d4:  lsr x15, x15, x14                                ; bit r
+0x1d8:  tbz w15, #0x0, +0x1a0                            ; fixed: +0x1a0

; A VARIABLE reader: `feed`, inlined. ONE copy serves both directions here (`propagate` has a copy per direction), so the
; kind is sorted out by up to three table tests before the threshold test, where `propagate` has one compare.
+0x1dc:  ldr x15, [x19, #0xf8]                            ; RELOADED per reader: cells
+0x1e0:  add x15, x15, x14, lsl #3
+0x1e4:  ldrh w16, [x15]                                  ; count
+0x1e8:  add w17, w16, w13
+0x1ec:  strh w17, [x15]                                  ; store count + step
+0x1f0:  ldrb w0, [x15, #0x4]                             ; kind
+0x1f4:  cmp w0, #0xc                                     ; kind > 12 cannot be; the compiler does not know
+0x1f8:  b.hi +0x228
+0x1fc:  lsr w2, w27, w0
+0x200:  tbnz w2, #0x0, +0x23c                            ; XOR-like: wake
+0x204:  lsr w2, w7, w0
+0x208:  tbnz w2, #0x0, +0x1b4                            ; never woken: next reader
+0x20c:  lsr w0, w30, w0
+0x210:  tbz w0, #0x0, +0x228                             ; not a latch or pin: a threshold gate, +0x228
+0x214:  tbz w12, #0x0, +0x1b4                            ; latch or pin, input fell: next reader
+0x218:  ldrb w16, [x15, #0x7]                            ; input rose: flip its toggle mark, then wake
+0x21c:  eor w16, w16, #0x1
+0x220:  strb w16, [x15, #0x7]
+0x224:  b +0x23c
+0x228:  and w17, w17, #0xffff                            ; threshold gate: max(old, new) == threshold?
+0x22c:  umax w16, w16, w17
+0x230:  ldrh w17, [x15, #0x2]
+0x234:  cmp w16, w17
+0x238:  b.ne +0x1b4                                      ; no: next reader (the common case)

; Wake the reader unless it is queued already. The queue's address and length are loaded from the engine at every push.
+0x23c:  ldrb w16, [x15, #0x6]
+0x240:  tbnz w16, #0x0, +0x1b4
+0x244:  strb w6, [x15, #0x6]
+0x248:  ldr x15, [x19, #0x2a0]                           ; RELOADED per push: the queue's buffer
+0x24c:  ldr x16, [x19, #0x2a8]                           ; RELOADED per push: its length
+0x250:  str w14, [x15, x16, lsl #2]                      ; queue[len] = r (no capacity check: attempt 30)
+0x254:  add x14, x16, #0x1
+0x258:  str x14, [x19, #0x2a8]                           ; len + 1 stored
+0x25c:  b +0x1b4

; The TRACES e writes. x20..x24 = their range in writes.items.
+0x260:  ldr x10, [x19, #0x158]                           ; RELOADED per entity: writes.start
+0x264:  ldr w20, [x10, x8, lsl #2]
+0x268:  ldr w24, [x10, x9, lsl #2]
+0x26c:  cmp w20, w24
+0x270:  mov w17, #0x1f ; =31                             ; w17, w0: two more kind tables, set again for every flipped entity
+0x274:  mov w0, #0x1b80 ; =7040
+0x278:  b.lo +0x28c
+0x27c:  b +0x164
+0x280:  add x20, x20, #0x1
+0x284:  cmp x20, x24
+0x288:  b.eq +0x164

; TRACE LOOP, one written trace t per round. First the test that `propagate` does not have: is the trace variable?
; (A fixed trace has had its count adjusted in pass 1 and is otherwise left to the record.)
+0x28c:  ldr x21, [x23, #0x80]                            ; RELOADED per trace: record.variable's address (kept in x21 for this trace's readers)
+0x290:  ldr x8, [x19, #0x170]                            ; RELOADED per trace: writes.items
+0x294:  ldr w28, [x8, x20, lsl #2]                       ; t
+0x298:  lsr x8, x28, #6
+0x29c:  ldr x8, [x21, x8, lsl #3]
+0x2a0:  lsr x8, x8, x28
+0x2a4:  tbz w8, #0x0, +0x280                             ; fixed trace: next
+0x2a8:  ldr x8, [x19, #0xf8]                             ; RELOADED per trace: cells
+0x2ac:  add x8, x8, x28, lsl #3
+0x2b0:  ldrh w26, [x8]                                   ; the trace's writer count
+0x2b4:  umin w22, w26, #0x1                              ; 1 if any writer is on
+0x2b8:  ldrb w9, [x8, #0x5]                              ; the trace's state
+0x2bc:  cmp w22, w9
+0x2c0:  b.eq +0x280                                      ; unchanged: next trace

; The trace changed: store its state, flip its bit in the packed copy.
+0x2c4:  strb w22, [x8, #0x5]
+0x2c8:  ldr x8, [x19, #0x288]                            ; RELOADED: slot
+0x2cc:  ldr w8, [x8, x28, lsl #2]
+0x2d0:  lsl x9, x6, x8
+0x2d4:  ldr x10, [x19, #0x270]                           ; RELOADED: packed
+0x2d8:  lsr x8, x8, #3
+0x2dc:  and x8, x8, #0x1ffffff8
+0x2e0:  ldr x11, [x10, x8]
+0x2e4:  eor x9, x9, x11
+0x2e8:  str x9, [x10, x8]                                ; flipped in the packed copy

; `self.split.traces.push((t, now))`, for `leave_split`. The push KEEPS ITS CAPACITY CHECK AND A CALL to the growth routine
; (at +0x48c): the fault attempt 30 item 1 removed from `feed`'s queue push. 4 loads, 3 stores, a compare and a branch per
; trace that changes.
+0x2ec:  ldr x25, [x19, #0x3b0]                           ; traces.len
+0x2f0:  ldr x8, [x19, #0x3a0]                            ; traces.capacity
+0x2f4:  cmp x25, x8
+0x2f8:  b.eq +0x48c                                      ; full: grow (+0x48c)
+0x2fc:  ldr x8, [x19, #0x3a8]                            ; traces.ptr
+0x300:  add x8, x8, x25, lsl #3
+0x304:  str w28, [x8]                                    ; the trace
+0x308:  strb w22, [x8, #0x4]                             ; its new state
+0x30c:  add x8, x25, #0x1
+0x310:  str x8, [x19, #0x3b0]                            ; len + 1 stored

; The trace's readers: their range; none: next trace. Then a loop per direction (two copies, as in `propagate`).
+0x314:  ldr x8, [x19, #0x188]                            ; RELOADED: readers.start
+0x318:  add x9, x8, x28, lsl #2
+0x31c:  ldp w8, w9, [x9]
+0x320:  cmp w8, w9
+0x324:  b.hs +0x280
+0x328:  cmp w26, #0x0
+0x32c:  cneg w10, w6, eq
+0x330:  cbnz w26, +0x358                                 ; turned on: +0x358; off: +0x410
+0x334:  b +0x410

; ...a fixed reader of a trace that turned on: count only.
+0x338:  ldr x12, [x19, #0xf8]                            ; RELOADED per reader: cells
+0x33c:  lsl x11, x11, #3
+0x340:  ldrh w13, [x12, x11]
+0x344:  add w13, w13, w10
+0x348:  strh w13, [x12, x11]
+0x34c:  add x8, x8, #0x1
+0x350:  cmp x9, x8
+0x354:  b.eq +0x280

; TRACE READER LOOP, trace turned ON. Same as the direct loop: bit test, then `feed` with the kind sorted out by three tables.
+0x358:  ldr x11, [x19, #0x1a0]                           ; RELOADED per reader: readers.items
+0x35c:  ldr w11, [x11, x8, lsl #2]                       ; r
+0x360:  lsr x12, x11, #6
+0x364:  ldr x12, [x21, x12, lsl #3]                      ; the bitmap's word (its address is in x21 here)
+0x368:  lsr x12, x12, x11
+0x36c:  tbz w12, #0x0, +0x338                            ; fixed: +0x338
+0x370:  ldr x12, [x19, #0xf8]                            ; RELOADED per reader: cells
+0x374:  add x12, x12, x11, lsl #3
+0x378:  ldrh w13, [x12]
+0x37c:  add w14, w13, w10
+0x380:  strh w14, [x12]
+0x384:  ldrb w15, [x12, #0x4]
+0x388:  cmp w15, #0xc
+0x38c:  b.hi +0x3b8
+0x390:  lsr w16, w27, w15
+0x394:  tbnz w16, #0x0, +0x3cc
+0x398:  lsr w16, w7, w15
+0x39c:  tbnz w16, #0x0, +0x34c
+0x3a0:  lsr w15, w30, w15
+0x3a4:  tbz w15, #0x0, +0x3b8
+0x3a8:  ldrb w13, [x12, #0x7]                            ; latch or pin: flip its toggle mark, wake
+0x3ac:  eor w13, w13, #0x1
+0x3b0:  strb w13, [x12, #0x7]
+0x3b4:  b +0x3cc
+0x3b8:  and w14, w14, #0xffff
+0x3bc:  umax w13, w13, w14
+0x3c0:  ldrh w14, [x12, #0x2]
+0x3c4:  cmp w13, w14
+0x3c8:  b.ne +0x34c                                      ; threshold not passed: next reader
+0x3cc:  ldrb w13, [x12, #0x6]
+0x3d0:  tbnz w13, #0x0, +0x34c
+0x3d4:  strb w6, [x12, #0x6]
+0x3d8:  ldr x12, [x19, #0x2a0]                           ; RELOADED per push: the queue's buffer
+0x3dc:  ldr x13, [x19, #0x2a8]                           ; RELOADED per push: its length
+0x3e0:  str w11, [x12, x13, lsl #2]
+0x3e4:  add x11, x13, #0x1
+0x3e8:  str x11, [x19, #0x2a8]
+0x3ec:  b +0x34c

; ...a fixed reader of a trace that turned off: count only.
+0x3f0:  ldr x12, [x19, #0xf8]
+0x3f4:  lsl x11, x11, #3
+0x3f8:  ldrh w13, [x12, x11]
+0x3fc:  add w13, w13, w10
+0x400:  strh w13, [x12, x11]
+0x404:  add x8, x8, #0x1
+0x408:  cmp x9, x8
+0x40c:  b.eq +0x280

; TRACE READER LOOP, trace turned OFF. Here the compiler tests for a threshold gate first (table w17), so the common
; reader takes one table test, not three.
+0x410:  ldr x11, [x19, #0x1a0]
+0x414:  ldr w11, [x11, x8, lsl #2]
+0x418:  lsr x12, x11, #6
+0x41c:  ldr x12, [x21, x12, lsl #3]
+0x420:  lsr x12, x12, x11
+0x424:  tbz w12, #0x0, +0x3f0
+0x428:  ldr x12, [x19, #0xf8]
+0x42c:  add x12, x12, x11, lsl #3
+0x430:  ldrh w13, [x12]
+0x434:  add w14, w13, w10
+0x438:  strh w14, [x12]
+0x43c:  ldrb w15, [x12, #0x4]
+0x440:  lsr w16, w17, w15
+0x444:  tbnz w16, #0x0, +0x454                           ; a threshold gate: +0x454
+0x448:  lsr w13, w0, w15
+0x44c:  tbnz w13, #0x0, +0x404                           ; never woken by a falling input: next reader
+0x450:  b +0x468
+0x454:  and w14, w14, #0xffff
+0x458:  umax w13, w13, w14
+0x45c:  ldrh w14, [x12, #0x2]
+0x460:  cmp w13, w14
+0x464:  b.ne +0x404
+0x468:  ldrb w13, [x12, #0x6]
+0x46c:  tbnz w13, #0x0, +0x404
+0x470:  strb w6, [x12, #0x6]
+0x474:  ldr x12, [x19, #0x2a0]
+0x478:  ldr x13, [x19, #0x2a8]
+0x47c:  str w11, [x12, x13, lsl #2]
+0x480:  add x11, x13, #0x1
+0x484:  str x11, [x19, #0x2a8]
+0x488:  b +0x404

; The growth call of the `traces` push (never taken once the list has grown), and the reloading of what the call lost.
+0x48c:  add x0, x19, #0x3a0
+0x490:  str x1, [sp, #0x28]
+0x494:  bl RawVec::grow_one
+0x498:  mov w0, #0x1b80 ; =7040
+0x49c:  mov w17, #0x1f ; =31
+0x4a0:  mov w30, #0x180 ; =384
+0x4a4:  mov w7, #0x1a00 ; =6656
+0x4a8:  mov w6, #0x1 ; =1
+0x4ac:  ldp x5, x4, [sp, #0x10]
+0x4b0:  ldp x3, x1, [sp, #0x20]
+0x4b4:  b +0x2fc

; After the flipped entities: timed entries? Then their pass 2 (at +0x588).
+0x4b8:  ldp x9, x8, [sp]
+0x4bc:  cmp x8, x9
+0x4c0:  b.ne +0x588

; EXIT. THE RECORD'S BOX IS PUT BACK: the field is loaded and tested (it is None; the compiler does not know, and keeps a
; call that would free an old record), then the pointer is stored. With the entry that is 2 loads, 2 stores and 2 branches
; per call for taking the record out and putting it back.
+0x4c4:  ldr x0, [x19, #0x608]                            ; self.split.record (None)
+0x4c8:  cbz x0, +0x4d0
+0x4cc:  bl drop(Box<Record>)                             ; drop the old one: never runs
+0x4d0:  str x23, [x19, #0x608]                           ; self.split.record = Some(record)
+0x4d4:  ldp x29, x30, [sp, #0x80]
+0x4d8:  ldp x20, x19, [sp, #0x70]
+0x4dc:  ldp x22, x21, [sp, #0x60]
+0x4e0:  ldp x24, x23, [sp, #0x50]
+0x4e4:  ldp x26, x25, [sp, #0x40]
+0x4e8:  ldp x28, x27, [sp, #0x30]
+0x4ec:  add sp, sp, #0x90
+0x4f0:  ret

; PASS 1 OF THE TIMED ENTRIES (fixed signals the record says flip in this tick, read by something variable): a fixed
; component's variable written traces get their writer count adjusted. The component's kind is looked up, its list of
; written traces walked, and each trace tested against the bitmap.
+0x4f4:  mov w8, #0x1 ; =1
+0x4f8:  mov x9, x3
+0x4fc:  b +0x50c
+0x500:  add x9, x9, #0xc
+0x504:  cmp x9, x4
+0x508:  b.eq +0x13c
+0x50c:  ldr w11, [x9, #0x4]                              ; the entry's entity e
+0x510:  ldr x10, [x19, #0xf8]                            ; RELOADED: cells
+0x514:  add x10, x10, x11, lsl #3
+0x518:  ldrb w10, [x10, #0x4]                            ; e's kind
+0x51c:  cmp w10, #0xb
+0x520:  b.eq +0x500                                      ; a trace writes nothing: next entry
+0x524:  ldrb w10, [x9, #0x8]                             ; the entry's value
+0x528:  cmp w10, #0x0
+0x52c:  cneg w10, w8, eq
+0x530:  ldr x12, [x19, #0x158]                           ; RELOADED: writes.start
+0x534:  add x12, x12, x11, lsl #2
+0x538:  ldp w11, w12, [x12]
+0x53c:  cmp w11, w12
+0x540:  b.lo +0x554
+0x544:  b +0x500
+0x548:  add x11, x11, #0x1
+0x54c:  cmp x12, x11
+0x550:  b.eq +0x500
+0x554:  ldr x13, [x19, #0x170]                           ; RELOADED per trace: writes.items
+0x558:  ldr w13, [x13, x11, lsl #2]
+0x55c:  ldr x14, [x23, #0x80]                            ; RELOADED per trace: record.variable's address
+0x560:  lsr x15, x13, #6
+0x564:  ldr x14, [x14, x15, lsl #3]
+0x568:  lsr x14, x14, x13
+0x56c:  tbz w14, #0x0, +0x548                            ; fixed trace: skip
+0x570:  ldr x14, [x19, #0xf8]                            ; RELOADED per trace: cells
+0x574:  lsl x13, x13, #3
+0x578:  ldrh w15, [x14, x13]
+0x57c:  add w15, w15, w10
+0x580:  strh w15, [x14, x13]
+0x584:  b +0x548

; PASS 2 OF THE TIMED ENTRIES. w21 and w5 are the offsets in the engine of `readers` and `direct`: which of the two lists
; an entry's readers are in depends on whether the entity is a trace.
+0x588:  mov w5, #0x220 ; =544
+0x58c:  mov w21, #0x180 ; =384
+0x590:  mov w6, #0x1 ; =1
+0x594:  mov w7, #0x460 ; =1120
+0x598:  mov w30, #0x1a00 ; =6656
+0x59c:  mov w17, #0x1f ; =31
+0x5a0:  mov w0, #0x1b80 ; =7040
+0x5a4:  b +0x5b4
+0x5a8:  add x3, x3, #0xc
+0x5ac:  cmp x3, x4
+0x5b0:  b.eq +0x4c4

; Per timed entry: choose the list by the entity's kind, and its range of readers.
+0x5b4:  ldr w8, [x3, #0x4]                               ; the entry's entity e
+0x5b8:  ldr x9, [x19, #0xf8]                             ; RELOADED: cells
+0x5bc:  add x9, x9, x8, lsl #3
+0x5c0:  ldrb w12, [x9, #0x4]                             ; e's kind
+0x5c4:  cmp w12, #0xb
+0x5c8:  csel x9, x21, x5, eq                             ; a trace: `readers`, else `direct`
+0x5cc:  add x9, x19, x9
+0x5d0:  ldr x11, [x9, #0x8]                              ; that list's start array
+0x5d4:  ldr w10, [x11, x8, lsl #2]
+0x5d8:  add x9, x8, #0x1
+0x5dc:  ldr w11, [x11, x9, lsl #2]
+0x5e0:  cmp w10, w11
+0x5e4:  b.hs +0x6cc                                      ; no readers
+0x5e8:  ldrb w12, [x3, #0x8]                             ; the entry's value
+0x5ec:  cmp w12, #0x0
+0x5f0:  cneg w13, w6, eq
+0x5f4:  ldr x14, [x19, #0x2a0]                           ; the queue's buffer: in a register in this loop (the one place)
+0x5f8:  b +0x608
+0x5fc:  add x10, x10, #0x1
+0x600:  cmp x11, x10
+0x604:  b.eq +0x6b8

; TIMED READER LOOP, one reader per round. THE LIST IS CHOSEN AGAIN FOR EVERY READER: e's kind is loaded and compared and
; the list's address loaded (7 instructions, 3 loads) before the reader itself is loaded. The source says so twice (once
; for the range, once inside the loop, to satisfy the borrow checker), and the compiler did not hoist it. A fixed reader
; is skipped: the record has its count.
+0x608:  ldr x16, [x19, #0xf8]                            ; RELOADED per reader: cells
+0x60c:  add x15, x16, x8, lsl #3
+0x610:  ldrb w15, [x15, #0x4]                            ; e's kind, per reader
+0x614:  cmp w15, #0xb
+0x618:  csel x15, x21, x5, eq                            ; the list, per reader
+0x61c:  add x15, x19, x15
+0x620:  ldr x15, [x15, #0x20]                            ; its items, per reader
+0x624:  ldr w15, [x15, x10, lsl #2]                      ; r
+0x628:  ldr x17, [x23, #0x80]                            ; RELOADED per reader: record.variable's address
+0x62c:  lsr x0, x15, #6
+0x630:  ldr x17, [x17, x0, lsl #3]
+0x634:  lsr x17, x17, x15
+0x638:  tbz w17, #0x0, +0x5fc                            ; fixed: next reader
+0x63c:  add x16, x16, x15, lsl #3
+0x640:  ldrh w17, [x16]
+0x644:  add w0, w17, w13
+0x648:  strh w0, [x16]
+0x64c:  ldrb w1, [x16, #0x4]
+0x650:  cmp w1, #0xc
+0x654:  b.hi +0x684
+0x658:  lsr w2, w7, w1
+0x65c:  tbnz w2, #0x0, +0x698
+0x660:  lsr w2, w30, w1
+0x664:  tbnz w2, #0x0, +0x5fc
+0x668:  lsr w1, w21, w1
+0x66c:  tbz w1, #0x0, +0x684
+0x670:  tbz w12, #0x0, +0x5fc
+0x674:  ldrb w17, [x16, #0x7]
+0x678:  eor w17, w17, #0x1
+0x67c:  strb w17, [x16, #0x7]
+0x680:  b +0x698
+0x684:  and w0, w0, #0xffff
+0x688:  umax w17, w17, w0
+0x68c:  ldrh w0, [x16, #0x2]
+0x690:  cmp w17, w0
+0x694:  b.ne +0x5fc
+0x698:  ldrb w17, [x16, #0x6]
+0x69c:  tbnz w17, #0x0, +0x5fc
+0x6a0:  strb w6, [x16, #0x6]
+0x6a4:  ldr x16, [x19, #0x2a8]                           ; RELOADED per push: the queue's length
+0x6a8:  str w15, [x14, x16, lsl #2]
+0x6ac:  add x15, x16, #0x1
+0x6b0:  str x15, [x19, #0x2a8]                           ; len + 1 stored
+0x6b4:  b +0x5fc

; After the entry's readers: e's kind once more; a fixed component (not a trace) also has its written traces looked at.
+0x6b8:  ldr x10, [x19, #0xf8]
+0x6bc:  add x10, x10, x8, lsl #3
+0x6c0:  ldrb w12, [x10, #0x4]
+0x6c4:  mov w17, #0x1f ; =31
+0x6c8:  mov w0, #0x1b80 ; =7040
+0x6cc:  cmp w12, #0xb
+0x6d0:  b.eq +0x5a8                                      ; a trace: next entry
+0x6d4:  ldr x10, [x19, #0x158]
+0x6d8:  ldr w25, [x10, x8, lsl #2]
+0x6dc:  ldr w24, [x10, x9, lsl #2]
+0x6e0:  cmp w25, w24
+0x6e4:  b.lo +0x6f8
+0x6e8:  b +0x5a8
+0x6ec:  add x25, x25, #0x1
+0x6f0:  cmp x25, x24
+0x6f4:  b.eq +0x5a8

; TIMED TRACE LOOP: `trace_split` inlined a second time, with its own push to `traces` and its own two reader loops.
+0x6f8:  ldr x28, [x23, #0x80]                            ; RELOADED per trace: record.variable's address
+0x6fc:  ldr x8, [x19, #0x170]
+0x700:  ldr w22, [x8, x25, lsl #2]
+0x704:  lsr x8, x22, #6
+0x708:  ldr x8, [x28, x8, lsl #3]
+0x70c:  lsr x8, x8, x22
+0x710:  tbz w8, #0x0, +0x6ec                             ; fixed trace: next
+0x714:  ldr x8, [x19, #0xf8]
+0x718:  add x8, x8, x22, lsl #3
+0x71c:  ldrh w27, [x8]
+0x720:  umin w26, w27, #0x1
+0x724:  ldrb w9, [x8, #0x5]
+0x728:  cmp w26, w9
+0x72c:  b.eq +0x6ec
+0x730:  strb w26, [x8, #0x5]
+0x734:  ldr x8, [x19, #0x288]
+0x738:  ldr w8, [x8, x22, lsl #2]
+0x73c:  lsl x9, x6, x8
+0x740:  ldr x10, [x19, #0x270]
+0x744:  lsr x8, x8, #3
+0x748:  and x8, x8, #0x1ffffff8
+0x74c:  ldr x11, [x10, x8]
+0x750:  eor x9, x9, x11
+0x754:  str x9, [x10, x8]
+0x758:  ldr x20, [x19, #0x3b0]
+0x75c:  ldr x8, [x19, #0x3a0]
+0x760:  cmp x20, x8
+0x764:  b.eq +0x8f8                                      ; `traces` full: grow (+0x8f8)
+0x768:  ldr x8, [x19, #0x3a8]
+0x76c:  add x8, x8, x20, lsl #3
+0x770:  str w22, [x8]
+0x774:  strb w26, [x8, #0x4]
+0x778:  add x8, x20, #0x1
+0x77c:  str x8, [x19, #0x3b0]
+0x780:  ldr x8, [x19, #0x188]
+0x784:  add x9, x8, x22, lsl #2
+0x788:  ldp w8, w9, [x9]
+0x78c:  cmp w8, w9
+0x790:  b.hs +0x6ec
+0x794:  cmp w27, #0x0
+0x798:  cneg w10, w6, eq
+0x79c:  cbnz w27, +0x7c4
+0x7a0:  b +0x87c
+0x7a4:  ldr x12, [x19, #0xf8]
+0x7a8:  lsl x11, x11, #3
+0x7ac:  ldrh w13, [x12, x11]
+0x7b0:  add w13, w13, w10
+0x7b4:  strh w13, [x12, x11]
+0x7b8:  add x8, x8, #0x1
+0x7bc:  cmp x9, x8
+0x7c0:  b.eq +0x6ec

; Reader loop of a timed trace that turned on (as +0x358).
+0x7c4:  ldr x11, [x19, #0x1a0]
+0x7c8:  ldr w11, [x11, x8, lsl #2]
+0x7cc:  lsr x12, x11, #6
+0x7d0:  ldr x12, [x28, x12, lsl #3]
+0x7d4:  lsr x12, x12, x11
+0x7d8:  tbz w12, #0x0, +0x7a4
+0x7dc:  ldr x12, [x19, #0xf8]
+0x7e0:  add x12, x12, x11, lsl #3
+0x7e4:  ldrh w13, [x12]
+0x7e8:  add w14, w13, w10
+0x7ec:  strh w14, [x12]
+0x7f0:  ldrb w15, [x12, #0x4]
+0x7f4:  cmp w15, #0xc
+0x7f8:  b.hi +0x824
+0x7fc:  lsr w16, w7, w15
+0x800:  tbnz w16, #0x0, +0x838
+0x804:  lsr w16, w30, w15
+0x808:  tbnz w16, #0x0, +0x7b8
+0x80c:  lsr w15, w21, w15
+0x810:  tbz w15, #0x0, +0x824
+0x814:  ldrb w13, [x12, #0x7]
+0x818:  eor w13, w13, #0x1
+0x81c:  strb w13, [x12, #0x7]
+0x820:  b +0x838
+0x824:  and w14, w14, #0xffff
+0x828:  umax w13, w13, w14
+0x82c:  ldrh w14, [x12, #0x2]
+0x830:  cmp w13, w14
+0x834:  b.ne +0x7b8
+0x838:  ldrb w13, [x12, #0x6]
+0x83c:  tbnz w13, #0x0, +0x7b8
+0x840:  strb w6, [x12, #0x6]
+0x844:  ldr x12, [x19, #0x2a0]
+0x848:  ldr x13, [x19, #0x2a8]
+0x84c:  str w11, [x12, x13, lsl #2]
+0x850:  add x11, x13, #0x1
+0x854:  str x11, [x19, #0x2a8]
+0x858:  b +0x7b8
+0x85c:  ldr x12, [x19, #0xf8]
+0x860:  lsl x11, x11, #3
+0x864:  ldrh w13, [x12, x11]
+0x868:  add w13, w13, w10
+0x86c:  strh w13, [x12, x11]
+0x870:  add x8, x8, #0x1
+0x874:  cmp x9, x8
+0x878:  b.eq +0x6ec

; Reader loop of a timed trace that turned off (as +0x410).
+0x87c:  ldr x11, [x19, #0x1a0]
+0x880:  ldr w11, [x11, x8, lsl #2]
+0x884:  lsr x12, x11, #6
+0x888:  ldr x12, [x28, x12, lsl #3]
+0x88c:  lsr x12, x12, x11
+0x890:  tbz w12, #0x0, +0x85c
+0x894:  ldr x12, [x19, #0xf8]
+0x898:  add x12, x12, x11, lsl #3
+0x89c:  ldrh w13, [x12]
+0x8a0:  add w14, w13, w10
+0x8a4:  strh w14, [x12]
+0x8a8:  ldrb w15, [x12, #0x4]
+0x8ac:  lsr w16, w17, w15
+0x8b0:  tbnz w16, #0x0, +0x8c0
+0x8b4:  lsr w13, w0, w15
+0x8b8:  tbnz w13, #0x0, +0x870
+0x8bc:  b +0x8d4
+0x8c0:  and w14, w14, #0xffff
+0x8c4:  umax w13, w13, w14
+0x8c8:  ldrh w14, [x12, #0x2]
+0x8cc:  cmp w13, w14
+0x8d0:  b.ne +0x870
+0x8d4:  ldrb w13, [x12, #0x6]
+0x8d8:  tbnz w13, #0x0, +0x870
+0x8dc:  strb w6, [x12, #0x6]
+0x8e0:  ldr x12, [x19, #0x2a0]
+0x8e4:  ldr x13, [x19, #0x2a8]
+0x8e8:  str w11, [x12, x13, lsl #2]
+0x8ec:  add x11, x13, #0x1
+0x8f0:  str x11, [x19, #0x2a8]
+0x8f4:  b +0x870

; The second growth call of the `traces` push.
+0x8f8:  add x0, x19, #0x3a0
+0x8fc:  str x3, [sp, #0x20]
+0x900:  bl RawVec::grow_one
+0x904:  mov w0, #0x1b80 ; =7040
+0x908:  mov w17, #0x1f ; =31
+0x90c:  mov w30, #0x1a00 ; =6656
+0x910:  mov w7, #0x460 ; =1120
+0x914:  mov w6, #0x1 ; =1
+0x918:  mov w5, #0x220 ; =544
+0x91c:  ldp x4, x3, [sp, #0x18]
+0x920:  b +0x768

; The two panics: the slice bounds of `timed`, and a record that is not there.
+0x924:  adrp x3, 0x1001ad000
+0x928:  add x3, x3, #0x988
+0x92c:  mov x1, x17
+0x930:  mov x2, x8
+0x934:  bl slice_index_fail
+0x938:  adrp x0, 0x1001ad000
+0x93c:  add x0, x0, #0x970
+0x940:  bl unwrap_failed
; ======================================================================================================================
; WHAT IS NEEDED AND WHAT IS NOT (read by hand and then measured, attempt 33, 2026-11-20)
;
; What a reader costs here that it does not cost in `propagate`:
;   - the membership test: the reader's bit in the record's `variable` bitmap. 4 instructions (shift, load of the bitmap's
;     word, shift, branch) and one load of data. This is the algorithm: a fixed reader has its count adjusted and nothing else.
;   - reloads. NOTHING is kept in a register across readers: the cells' address is loaded from the engine 20 times in the
;     function, in every loop body (once per reader, twice for a variable one), the reader list's address once per reader,
;     the bitmap's address once per direct reader and once per trace, the queue's buffer and length at every push.
;     `propagate` loads each of them once per call. Every one of them is a load from the first cache that waits for nothing.
;   - the kind of a variable reader goes through up to three table tests before the threshold compare in four of the six
;     copies of `feed` (+0x1f4, +0x388, +0x650, +0x7f4); `propagate` has `cmp kind, #5`.
; And per timed entry (a fixed signal's flip handed to the variable part): the list is chosen from the entity's kind again
; for every reader (+0x608, 7 instructions and 3 loads), every reader is tested against the bitmap, and its written traces
; are walked twice (pass 1 and pass 2). COUNTED on day 4 part 2 over 300 million ticks: 70 million flips, 201 million
; readers of them told (121 variable, 80 fixed), and 62 million readers of timed entries looked at, of which 33 million
; are variable. So a quarter of all reader rounds were in the timed loop, and half of those found a fixed reader and did nothing.
; (The sample computer: 8 million flips, 29 million variable readers, 159 million fixed ones, 8.6 million timed readers.)
; That, not waste per reader, is most of the 10.0 ns a flip costs here against 7.5 ns in `propagate`: a flip here has
; 3.75 reader rounds with the timed ones, and a bit test on each.
;
; The questions that were asked of this listing:
; 1. Is the record's Box taken out and put back with stores per call? YES: +0x020/+0x024 and +0x4c4 to +0x4d0, 2 loads,
;    2 stores, 2 branches and a never-taken call per call. Per tick, not per reader.
; 2. Is `record.variable`'s address reloaded per reader? YES (+0x1c0, +0x28c, +0x55c, +0x628, +0x6f8): the source held
;    `&Vec<u64>` and turned it into a slice at each use.
; 3. The bit test per reader: lsr, ldr, lsr, tbz (+0x1cc to +0x1d8). One load of a word of the bitmap per reader.
; 4. Does `split.traces.push` keep a capacity check and a call to the growth routine in the trace loop? YES, twice
;    (+0x2f4/+0x48c and +0x760/+0x8f8).
; 5. Are `split.cursor`, `tick`, `split.start` reloaded? NO: once each per call; the cursor is stored once.
; 6. Are the two list selections by `op == Trace` in the timed loop hoisted? NO: the one inside the loop is done per reader.
; 7. Is `feed` inlined into `feed_split` in each place? YES, six copies (direct readers; a trace's readers, on and off,
;    twice: once for the flipped entities' traces and once for the timed entries'; the timed readers). No call in any loop
;    but the growth routine's.
;
; TRIED (attempt 33). Each built alone from this code and timed alone against it. "Long": five programs for 0.5 to 2 s
; each (day 4, 1 and 3 part 2 on the user's CPU, the two sample computers), best of five runs, geometric mean; "quick":
; the benchmark's 20 million ticks, three runs, where it was run. The quick benchmark's runs are 0.05 to 0.2 s, timed to a
; millisecond: one program moves by 2% from that alone.
;   A. The `traces` push without its capacity check (room for two entries per entity reserved when a record is entered:
;      a call flips a trace at most once, and a tick calls at most twice).       long 0.994, quick 1.000. Neutral: dropped.
;   B. `variable` held as a slice, so that the bitmap's address stays in a register.         long 1.007 (the sample 1.04).
;   D. The record borrowed through a pointer, the Box left where it is (needs `unsafe`).     long 1.003. Neutral: dropped.
;   A+B+D.                                                                                    long 1.013, quick 1.005.
;   T. The record's `timed` list holds the VARIABLE READERS of each fixed flip, one entry each, in the order they were
;      told before (a gate with the direction of its input, or a trace with the direction of its writer), made once in
;      `make_record`. The timed loops then look nothing up and test nothing: no kind, no list, no bitmap.
;                                                                                             long 1.011, quick 1.016.
;   L. A fixed reader's count not adjusted at each flip but once in `leave_split`, for the variable signals that are not
;      where they were at the step's start (found by an XOR of the packed signals with a copy taken at the start).
;      long 0.928: the user's CPU loses 9 to 15% (most variable signals that flip in a step do end up changed, and finding
;      them at the end costs more than the adjustments in the loop), the sample gains 5%. Rejected.
;   M. One code path for fixed and variable readers (the count always, the wake only if variable, no branch on the bit).
;      long 0.942, the sample 0.81: the branch on the bit is well predicted, and a fixed reader pays for the kind and the
;      threshold. Rejected.
;   H. On top of A+B+D+T: every part of the engine the loops use borrowed once into locals (`feed` as a function on the
;      parts), so that NOTHING is reloaded: the cells' address is then loaded once in the function. long 1.012 against
;      1.028 without it: the user's CPU 0.985 to 0.994, the sample 1.08. The reloads cost nothing on this machine and
;      keeping twenty values in registers does. Rejected (as attempt 31 found for `propagate`).
;   Combinations: B+T long 1.024; B+D+T 1.027; A+B+D+T 1.028 and 1.030 (twice). A and D add nothing that can be told
;   from noise, so they are left out.
;
; KEPT: B and T. Quick 1.024, to the Breakpoints (two runs) 1.025; every program gains, the two samples most (1.04 to 1.05).
;
; NOT TRIED:
;   - A byte or bit per entity set when a record is entered, for a cheaper membership test: entering a record would touch
;     its 500 to 800 variable signals twice (set, clear) at each of 3 million steps, about as many stores as there are
;     reader rounds, to save two shifts per reader. And a byte per entity kept in each record makes a record eight times
;     the bitmap's size (thousands of records are held). Not worth building.
;   - The order of the kind tests in `feed` (three tables before the threshold compare in four copies): the profile-guided
;     build, which is the one that is run, orders them by what it measured, so a gain in a plain build would not carry over.
;   - Why the compiler reloads here and not in `propagate`: not established. Removing the growth call (A) and the Box's
;     move (D) did not stop it; borrowing the parts once (H) did.
