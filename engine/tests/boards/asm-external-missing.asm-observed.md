# Assembler checklist: asm-external-missing

- Board: `asm-external-missing.vcb`
- Board SHA-256: `f0a024c33a69b7606b0313e44a47c341b2d0c939f6f5d9cb2e589e63a46ab35f`
- Game version: 1.0.1
- Covers: 23r (the .vcbasm file is missing on purpose)
- Observed by: Kristian Roth, 2026-11-17

## Procedure

1. Open the board and start the simulation.
2. If the Assembly Editor shows an error: find the `# case <id>` line above the flagged line, write the
   message next to that id below, put `#` in front of every line of that case (do not delete lines), and
   go to 1. If the flagged line belongs to no case, or the simulation starts although an error is shown,
   write that under Other and stop.
3. If the game hangs or crashes: note the case under Other, comment it out, and start again.
   Save As every few passes.
4. When the simulation starts: return to edit mode and Save As `asm-external-missing.assembled.vcb`.

## Cases

Leave a case empty if the game accepted it. Otherwise write the error message.

- r-external-file-missing:

## Other

- Did the simulation start in the end (yes/no): 
- Is the External Editing switch shown as on after opening the board (yes/no): External assembly file not found 
- Anything unexpected: 

Note added at import (2026-11-17): the game refused to start the simulation, so there is no assembled save for this board.
