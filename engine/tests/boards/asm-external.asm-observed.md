# Assembler checklist: asm-external

- Board: `asm-external.vcb`
- Board SHA-256: `898b5f1c6ec41d98133f0f13083c6808527a939119a224bf2f02f1f0f7a68b3d`
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
4. When the simulation starts: return to edit mode and Save As `asm-external.assembled.vcb`.

## Cases

Leave a case empty if the game accepted it. Otherwise write the error message.

- r-field-text-used: 
- r-external-file-used: 

## Other

- Did the simulation start in the end (yes/no): 
- Is the External Editing switch shown as on after opening the board (yes/no): 
- Anything unexpected: 
