# Assembler checklist: asm-probe-16

- Board: `asm-probe-16.vcb`
- Board SHA-256: `e283049522f4e3bcdf9e228e77a18bf9a50f36da950747dd62cb572561f33f14`
- Game version: 1.0.1
- Covers: 23, 19
- Observed by: Kristian Roth, 2026-11-17

## Procedure

1. Open the board and start the simulation.
2. If the Assembly Editor shows an error: find the `# case <id>` line above the flagged line, write the
   message next to that id below, put `#` in front of every line of that case (do not delete lines), and
   go to 1. If the flagged line belongs to no case, or the simulation starts although an error is shown,
   write that under Other and stop.
3. If the game hangs or crashes: note the case under Other, comment it out, and start again.
   Save As every few passes.
4. When the simulation starts: return to edit mode and Save As `asm-probe-16.assembled.vcb`.

## Cases

Leave a case empty if the game accepted it. Otherwise write the error message.

- u-first-address: 
- v0-control: 
- v0-pointer-value: 
- u-comment-after: 
- u-underscore-decimal: 
- u-underscore-hex: 
- u-negative-literal: 
- u-negative-symbol: 
- u-or-overlap: 
- u-or-overlap-operator: 
- u-three-primitives: 
- u-resymb: 
- u-unsymb: 
- u-inline: 
- u-unpoint: 
- u-orgprev: 
- z-orgprev-after-store: 
- u-add: 
- u-subtract: 
- u-multiply: 
- u-and: 
- u-xor: 
- u-shift-left: 
- u-divide: 
- u-modulo: 
- u-or-in-instruction: 
- u-shift-right-in-instruction: 
- u-macro-no-placeholder: Unexpected statement terminator (;) in a line with a macro definition
- u-macro-one-placeholder: Unexpected statement terminator (;) in a line with a macro definition
- u-macro-three-placeholders: Unexpected statement terminator (;) in a line with a macro definition
- u-macro-literal-arguments: 
- u-macro-braces: Unexpected statement terminator (;) in a line with a macro
- u-remac: Unexpected statement terminator (;) in a line with a macro
- u-unmac: Unexpected statement terminator (;) in a line with a macro
- u-bookmarks: 
- a-or-shift: 
- a-shift-or: 
- a-subtract-multiply: 
- a-add-multiply: 
- b-wider-than-16-bits: 
- b-negative-result: 
- b-overflow: Value exceeds 32 bits
- l-address-after-pointer: 
- m-two-writes: 
- q-lower-case-hex: 
- s-label-then-origin: 
- s-label-then-pointer: 
- t-semicolon-in-comment: 
- u-not: 
- u-inexact-division: 
- w-shift-chain: 
- w-subtract-chain: 
- u-orgbase: 

## Other

- Did the simulation start in the end (yes/no): 
- Anything unexpected: 
