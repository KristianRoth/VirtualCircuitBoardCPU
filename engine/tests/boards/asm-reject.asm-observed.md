# Assembler checklist: asm-reject

- Board: `asm-reject.vcb`
- Board SHA-256: `282f03fb24b948b56bfc6c6c73d11b6c6fddbde4c2983c9de56b068f0238e41d`
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
4. When the simulation starts: return to edit mode and Save As `asm-reject.assembled.vcb`.

## Cases

Leave a case empty if the game accepted it. Otherwise write the error message.

- o-orgprev-nothing-to-revert: Origin is already set to the base address
- o-orgbase-nothing-to-revert: Origin is already set to the base address
- b-wider-than-32-bits: Value exceeds 32 bits
- d-symbol-twice: Identifier "d1" is already defined at line 17 as SYMBOL
- d-pointer-twice: Identifier "d2" is already defined at line 20 as POINTER
- d-macro-twice: Unexpected statement terminator (;) in a line with a macro definition
- d-label-twice: Identifier "D4" is already defined at line 26 as LABEL
- e-symbol-and-pointer: Identifier "e1" is already defined at line 29 as SYMBOL
- e-symbol-and-label: Identifier "e2" is already defined at line 32 as LABEL
- e-keyword-as-name: Expected an IDENTIFIER token, got BOOKMARK_SUB-KEYWORD
- f-undefined-name: Identifier "nope_f1" must be defined before usage
- f-after-unsymb: Identifier "f2 must be defined before usage
- f-after-unpoint: Identifier "f3" must be difined before usage
- g-symbol-before-definition: Identifier "g1" must be defined before usage
- g-pointer-before-definition: Identifier "g2" must be defined before usage
- h-fewer-arguments: Macro "pair" expansion failed. Expected 2 arguments, got 1
- h-more-arguments: Invalid expression "1 | 2 4"
- i-expression-argument: Invalid expression "(1 || 2) 4"
- j-macro-among-primitives: Macro "pair" expansion failed. Expeded 2 arguments, got 0
- k-multi-line-macro: Expected a non-empty macro body "(...)"
- n-beyond-address-bits: 
- p-symbol-value-is-a-name: 
- p-symbol-value-is-an-expression:  Expected 3 tokens, got 5
- p-operator-without-spaces: Invalid token: 1|2. Could be caused by missing spaces between operators
- q-label-and-instruction: Expected 2 tokens, got 3
- q-keyword-upper-case: Identifier "q2" must be defined before usage
- q-prefix-upper-case: Invalid token: 0X10
- q-plus-sign: 
- v-repoint: 
- x-dot-in-name: Expected an IDENTIFIER token, got INVALID_TOKEN
- x-name-starts-with-digit: Expected an IDENTIFIER, got INVALID_TOKEN
- z-primitives-beside-expression: Invalid expression "1 2 | 4"
- z-unclosed-parenthesis: Invalid expression "( 1"
- z-resymb-of-new-name: Identifier "zr1" is already defined
- c-divide-by-zero: Invalid expression "6 / 0"
- c-modulo-by-zero: Invalid expression "6 % 0"
- c-shift-by-32: Value exceeds 32 bits
- c-shift-by-1000: Invalid expression "1 << 1000"
- k-macro-in-macro: Unexpected statement terminator (;) in a line with a macro definition
- s-label-at-end: No instruction statement found on any below lines for the label to point to

## Other

- Did the simulation start in the end (yes/no): yes
- Anything unexpected: 

Note added at import (2026-11-17): the save has one Write pixel more than the board, drawn by accident. It does not touch the VMem pins and has no effect on what the game assembled.
