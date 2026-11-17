# Assembler checklist: asm-probe2

- Board: `asm-probe2.vcb`
- Board SHA-256: `573e00b6d4be06d261122d01b7919da16666e87e51899595277e31b19f7326fd`
- Game version: 1.0.1
- Covers: 23
- Observed by: Kristian Roth, 2026-11-17

## Procedure

1. Open the board and start the simulation.
2. If the Assembly Editor shows an error: find the `# case <id>` line above the flagged line, write the
   message next to that id below, put `#` in front of every line of that case (do not delete lines), and
   go to 1. If the flagged line belongs to no case, or the simulation starts although an error is shown,
   write that under Other and stop.
3. If the game hangs or crashes: note the case under Other, comment it out, and start again.
   Save As every few passes.
4. When the simulation starts: return to edit mode and Save As `asm-probe2.assembled.vcb`.

## Cases

Leave a case empty if the game accepted it. Otherwise write the error message.

- v0-control: 
- u-macro-no-placeholder: 
- u-macro-one-placeholder: 
- u-macro-three-placeholders: 
- u-macro-braces: 
- u-remac: 
- u-unmac: 
- z-unmac-then-macro: 
- z-unsymb-then-symbol: 
- z-macro-other-operators: 
- z-macro-comment-after: 
- z-macro-skips-a-letter: The argument "b?" was skipped
- z-macro-empty-body: 
- z-macro-use-in-expression: 
- d-macro-twice: Macro is already defined. Use (remac) to redefine it
- k-macro-in-macro: Identifier "pair" must be defined before usage
- k-macro-in-macro-with-arguments: Identifier "pair" must be defined before usage
- z-unsymb-of-new-name: Identifier "mn1" is already defined
- z-unpoint-of-new-name: Identifier "mn2" is already defined
- z-unmac-of-new-name: Macro is already undefined
- z-keyword-in-instruction: Unexpected token BUILD-IN-CONSTANT
- p-symbol-valued-by-pointer: 
- p-symbol-valued-by-label: 
- p-origin-by-symbol: Expected a NUMERIC primitive for the origin's address, got IDENTIFIER
- p-pointer-address-by-symbol: Expected NUMERIC primitive for the origin's address, got IDENTIFIER
- p-pointer-value-by-symbol: 
- s-label-then-inline-pointer: 
- v-repoint-inline: 
- v-repoint-of-symbol: Identifier "vs1" is not a pointer
- u-negative-operand-add: 
- u-negative-operand-or: 
- u-negative-operand-multiply: 
- u-negative-intermediate: 
- u-negative-shift-right: 
- u-negative-divide: 
- u-negative-modulo: 
- u-not-of-negative: 
- u-not-then-add: 
- u-add-then-not: 
- u-not-parenthesis: 
- u-not-not: 
- u-minus-name: 
- u-minus-parenthesis: 
- b-product-fits: 
- b-product-too-wide: Value exceeds 32 bits
- b-below-minus-2-to-31: Value exceeds 32 bits
- b-literal-below-minus-2-to-31: Value exceeds 32 bits
- b-literal-minus-2-to-32: Value exceeds 32 bits
- b-shift-out-the-top: Value exceeds 32 bits
- c-shift-right-by-31: 
- c-shift-right-by-32: 
- c-shift-right-by-33: 
- c-shift-left-zero-by-32: 
- c-shift-by-negative: Invalid expression "8 >> -1"
- w-divide-chain: 
- w-modulo-chain: 
- w-shift-left-chain: 
- w-add-chain: 
- a-all-levels: 
- a-parentheses-nested: 
- n-beyond-12-address-bits: 
- n-last-word: 
- n-past-the-last-word: 
- n-origin-beyond-vmem: Origin address is outside the Virtual Memory range (1 to 1,048,575)
- n-pointer-beyond-vmem: Origin address is outside the Virtual Memory range (1 to 1,048,575)
- n-origin-0: rejected (commented out in the save); the message was not written down
- n-pointer-at-0: Pointer address is outside the Virtaul Memory range (1 to 1,048,575)
- a-mul-div: 
- a-div-mul: 
- a-mul-mod: 
- a-mod-mul: 
- a-mul-add: 
- a-add-mul: 
- a-mul-sub: 
- a-sub-mul: 
- a-shl-mul: 
- a-mul-shr: 
- a-shr-mul: 
- a-mul-and: 
- a-and-mul: 
- a-mul-xor: 
- a-xor-mul: 
- a-mul-or: 
- a-or-mul: 
- a-div-mod: 
- a-mod-div: 
- a-div-add: 
- a-add-div: 
- a-div-sub: 
- a-sub-div: 
- a-div-shl: 
- a-shl-div: 
- a-div-shr: 
- a-shr-div: 
- a-div-and: 
- a-and-div: 
- a-div-xor: 
- a-xor-div: 
- a-div-or: 
- a-or-div: 
- a-mod-add: 
- a-add-mod: 
- a-mod-sub: 
- a-sub-mod: 
- a-mod-shl: 
- a-shl-mod: 
- a-mod-shr: 
- a-shr-mod: 
- a-mod-and: 
- a-and-mod: 
- a-mod-xor: 
- a-xor-mod: 
- a-mod-or: 
- a-or-mod: 
- a-sub-add: 
- a-add-shl: 
- a-shl-add: 
- a-add-shr: 
- a-shr-add: 
- a-add-and: 
- a-and-add: 
- a-add-xor: 
- a-xor-add: 
- a-add-or: 
- a-or-add: 
- a-sub-shl: 
- a-shl-sub: 
- a-sub-shr: 
- a-shr-sub: 
- a-sub-and: 
- a-and-sub: 
- a-sub-xor: 
- a-xor-sub: 
- a-sub-or: 
- a-or-sub: 
- a-shl-shr: 
- a-shr-shl: 
- a-shl-and: 
- a-and-shl: 
- a-shl-xor: 
- a-xor-shl: 
- a-shl-or: 
- a-or-shl: 
- a-shr-and: 
- a-and-shr: 
- a-shr-xor: 
- a-xor-shr: 
- a-shr-or: 
- a-or-shr: 
- a-and-xor: 
- a-xor-and: 
- a-and-or: 
- a-or-and: 
- a-xor-or: 
- a-or-xor: 

## Other

- Did the simulation start in the end (yes/no): 
- Anything unexpected: 

Notes added at import (2026-11-17):
- The message "Macro is already undefined" was written on the line of `z-unmac-then-macro`, but in the save that case is live and assembled, and the `unmac nn3` line of `z-unmac-of-new-name` is the one commented out. The message was moved to that case.
- In `d-macro-twice` only the second definition is commented out, and in `z-unmac-of-new-name` only the `unmac` line; the rest of each case assembled.
- `n-origin-0` is commented out in the save with no message written.
