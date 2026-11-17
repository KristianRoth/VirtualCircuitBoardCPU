# Assembler checklist: asm-crfield

- Board: `asm-crfield.vcb`
- Board SHA-256: `cb7a08e40e8268009eb0bd67ef0497680228179f8ab6a096e319c13ae0262160`
- Game version: 1.0.1
- Covers: 23r
- Observed by: Kristian Roth, 2026-11-17

## Procedure

1. Open the board and start the simulation.
2. If the Assembly Editor shows an error: find the `# case <id>` line above the flagged line, write the
   message next to that id below, put `#` in front of every line of that case (do not delete lines), and
   go to 1. If the flagged line belongs to no case, or the simulation starts although an error is shown,
   write that under Other and stop.
3. If the game hangs or crashes: note the case under Other, comment it out, and start again.
   Save As every few passes.
4. When the simulation starts: return to edit mode and Save As `asm-crfield.assembled.vcb`.

## Cases

Leave a case empty if the game accepted it. Otherwise write the error message.

- r-carriage-return-in-the-editor-text: Expected a numberic primitive for the origin's address, got INVALID_TOKEN 

## Other

- Did the simulation start in the end (yes/no):
- Anything unexpected: 

Note added at import (2026-11-17): the game refused to start the simulation, so there is no assembled save for this board.
