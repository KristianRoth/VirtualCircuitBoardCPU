# VCB assembler

## Problem & goal
The game assembles a program only when a simulation starts, and never saves the result. A program for the CPU must be assembled in the game and saved with Persistent Memory before the engine can run it.

The goal is an assembler in `engine/` that turns the game's assembly language (User Guide chapter 8, `manual/08-assembly.md`) into VMem words **exactly as the game does**, so `vcb run` goes from source text to result without the game.

Related documents:
- `vcb-engine.md` (the framework spec) and `vcb-engine-v1.md` (the v1 engine). This spec adds to both.
- `vcb-game-facts.md` (the facts file). Every open question about the game's assembler is question 23 there, one letter per case (23a, 23b, …). **That list is the normative one**; this spec refers to the letters and does not repeat them.

Done, in two steps:
- **After A1** the day 1 programs run without the game: part 1 and part 2 assemble to the memory the game produced, and part 2 run by the engine ends with `count` (`0x0100`) = `0x16C7`.
- **After A3** every feature of chapter 8 is verified, or is known to be rejected by the game, or is listed here as still unverified or still undefined.

### Terms
- **VMem:** the game's memory component, 2^20 words of 32 bits, stored in a board's `vmem_data`.
- **Content bits, address bits:** the VMem pin counts in the board's `vmem_settings` (16 and 16 on the CPU). A word slot is always 32 bits, and the assembler fills all 32 whatever the content bits are (observed on a 16-bit board); the circuit sees the low content bits.
- **Persistent Memory:** the VMem range From..To that the game copies into the project when a simulation ends. A save made afterwards holds those words (never word 0).
- **Source:** the assembly text being assembled.
- **Primitive:** the manual's word for a number in an instruction: a literal, or a symbol, pointer or label name.
- **The CPU boards:** `engine/tests/boards/cpu-32bit-manual-start.vcb` (run switch off) and `cpu-aoc1p1.vcb` (run switch on, part 1 in memory, empty `assembly`).
- **Assembled save:** a save made in the game after entering simulation and returning to edit mode at once, with Persistent Memory over the program. It holds the game's assembled words before anything ran.
- **Verified / unverified / undefined:** see the gate.

## Users & roles
- **The user** writes programs, runs them with `vcb`, and makes the saves in the game.
- **The engine author (Claude)** writes the assembler, the probe programs and checklists, and turns the saves into fixtures.

## Scope
### In scope
- A module `engine/src/asm.rs` covering all of chapter 8: `symbol`/`resymb`/`unsymb`, `pointer`/`repoint`/`unpoint` with an address or `inline`, labels (`@`), `origin` with `orgprev` and `orgbase`, `macro`/`remac`/`unmac`, the operators, numeric literals, comments, `bookmark` and `sub_bookmark`.
- The two sources the game has (the board's `assembly` field, and `<board>.vcbasm` with External Editing), plus an explicit file.
- `vcb run` assembles before tick 0, as the game does.
- A new command `vcb asm` with three outputs: a new `.vcb`, a memory dump, a listing.
- The gate.
- Probe boards for question 23, and their import as fixtures.
- `vcb gen` header keys that put assembly text into a generated board.

### Out of scope
- Anything the game's language does not have: includes, several source files, symbol names in `--watch` or `--break-addr`, conditional assembly.
- Matching the game's error texts.
- An editor, syntax highlighting, a disassembler.

## The gate: verified, unverified, undefined
| Class | Meaning | What the assembler does |
|---|---|---|
| Verified | A game save shows it, and a test compares the assembler with that save. | Accepts it. |
| Unverified | The manual states a rule that gives one definite result, but no save shows it. | Gate hit naming the feature. With `--allow-unverified` it follows the manual's rule and warns once per feature. |
| Undefined | The manual does not say, or its rule does not give one definite result. | Gate hit naming the question letter. No flag turns this off. |
| Rejected | The game was seen to refuse it with an error. | Refused: `…: the game rejects this`. Listed like a gate hit. |

The engine may reject a program the game accepts. It must never accept a construct whose result in the game is unknown.

Relation to the ink gate in the v1 spec: there, "unverified" means "no observed fixture" and `--allow-unverified` lets the board run. The same flag opens both gates (the user's choice). Every warning says which side it is about (`ink …` or `asm …`), and unverified assembler features are added to the existing `(unverified: …)` note on the `stopped at tick` line and to the `assembled` line.

Like `VERIFIED` in `src/ink.rs`, the classes live in one list in `asm.rs` (a feature enum and its `VERIFIED` set), so moving a feature after an observation is a one-line change, made in the same commit as the fixture, this spec and the facts file.

### Verified: the shapes the game has assembled
"Verified" is defined by shape, not by feature name: the parser accepts exactly the forms that occur in the two game-assembled programs and rejects everything else by default. The programs are the `assembly` text of `engine/tests/boards/cpu-aoc1p1.after.vcb` (part 1) and, after milestone A0, of `cpu-aoc1p2.assembled.vcb` (part 2).

```
source      = statement { (newline | ";") statement }      the last line may lack a newline
statement   = empty | comment | directive | instruction
comment     = "#" … end of line                            only as a whole line
directive   = "symbol" name literal
            | "macro" name "{" body "}"                    on one line
            | "pointer" name literal literal               address, value
            | "origin" literal
            | "@" name                                     alone in its statement
instruction = primitive [ primitive ]                      OR of the values, bits not overlapping
            | macro-name name name                         the body with a? and b? replaced
body        = term { "|" term }
term        = name | "a?" | "b?" | "(" ( name | "a?" | "b?" ) ">>" literal ")"
primitive   = literal | symbol | pointer | label
literal     = decimal | "0x" hex | "0b" binary, underscores allowed in binary
name        = letters, digits, underscore; not starting with a digit
```
Also verified:
- Spaces and tabs before, between and after tokens, in any number. (Leading tabs, runs of spaces and trailing spaces all occur; spaces and tabs are treated as one class, which is an inference.)
- A symbol, pointer or macro is used after its definition. A label is used before and after its definition; two labels may name one address.
- A label takes the address of the next plain instruction, with only comments, blank lines and other labels in between.
- Pointers defined before the first `origin`; several `origin` directives in any address order.
- Words the program does not define keep their `vmem_data` value: words 1–5 of the CPU board survive in the game's after-save.
- From part 2 (verified 2026-11-16 by `cpu-aoc1p2.assembled.vcb`): `@ name` followed by `;`, and an indented label, behave like a plain label.

Two limits of this evidence, stated so they are not forgotten:
- **OR.** In both programs the ORed values never share a bit, so OR cannot be told from addition or XOR. Overlapping bits are unverified.
- **Pointer values.** Three words of `cpu-aoc1p1.vcb` were set back by hand after the run (`0x100`, `0x101`, `0xE000`), and two of them are the words the `pointer` statements store. For part 1 they are inferred from the tick-exact run. The part 2 save shows them directly: `0x100` = 0 and `0x101` = `0x1000`.

### Shown by the probe boards (2026-11-17)
Fixtures `asm-probe-32`, `asm-probe-16`, `asm-reject`, `asm-external`, `asm-novmem` in `engine/tests/boards/`. The two probe boards gave identical memory.

Accepted, with these results:
- The first instruction goes to address 1.
- A comment after a statement; a `;` inside a comment is part of the comment.
- Underscores in decimal and hex literals; lower-case hex digits; a leading `+`.
- **Values are 32 bits wide on every board.** `0x12345` and `-1` are stored whole on the 16-bit board. A negative literal or result is stored in two's complement (`-1` and `1 - 2` give `0xFFFFFFFF`).
- OR of values that share bits is a plain OR (`3 6` and `3 | 6` give 7); three primitives in one instruction.
- `resymb`, `unsymb`, `unpoint`, `pointer … inline`, `bookmark`, `sub_bookmark`.
- A symbol's value may be the name of another symbol.
- `repoint <name> <address> <value>`: stores the value at the new address and moves the name there; the old word stays.
- A pointer with an address does not move the current address.
- `origin orgprev` goes back to where the assembler was when it met the last `origin` (also when words were stored since); `origin orgbase` to where it was before the first one.
- All operators directly in an instruction: `+ - * / % & | ^ << >> ~` and parentheses. `/` truncates (`7 / 2` = 3). `~ 1` = `0xFFFFFFFE`.
- Precedence without parentheses for three pairs: `<<` before `|`, and `*` before `+` and before `-`. Chains of `>>` and of `-` run left to right.
- Two statements storing to one address: the later one wins.
- A label names the next instruction, also across an `origin` and a pointer with an address.
- A macro argument may be a literal.
- External Editing: the game assembles `<board>.vcbasm`, not the board's own text, and reads CRLF line ends.
- A board with VMem disabled and assembly text starts normally.
- Preloaded words the program does not write are kept (question 19, now fully answered).

Rejected by the game (the engine refuses them too):
- **A `;` on the line of a macro definition** ("Unexpected statement terminator (;) in a line with a macro definition"). A macro use may be followed by `;`.
- `orgprev` or `orgbase` with no `origin` to revert.
- A value wider than 32 bits: a literal, the result of `+`, or of `<<` (`1 << 32`, `1 << 1000`).
- Division or modulo by zero.
- A name defined twice or used for two kinds (symbol, pointer, label); a keyword as a name; a name with other characters or starting with a digit.
- A name that is not defined at the point of use: never defined, defined later, or removed with `unsymb`/`unpoint`.
- A macro used with the wrong number of arguments, with an expression as an argument, or not as the whole statement.
- A macro body over several lines.
- A symbol value that is an expression; an operator without spaces; primitives beside an expression (`1 2 | 4`); an unclosed parenthesis.
- A label and an instruction in one statement; a label with no instruction after it.
- `0X` as a prefix. A keyword in another letter case is not a keyword (`Symbol q 1` is an instruction of undefined names).
- `resymb` of a name that is not a symbol.

### Still unverified
The probe cases for these were lost to the `;` rule above, so only the manual backs them. Each is a gate hit without `--allow-unverified`.

| Feature | Trigger |
|---|---|
| Macro without exactly two placeholders (none, one, or three and more) | such a macro |
| Macro braces without spaces (`{a? \| b?}`, as in the manual's example) | a brace touching the body |
| `remac` | the keyword |
| `unmac` | the keyword |

### Still undefined
Question 23 in the facts file, the letters that remain: a (operator pairs other than the three shown), b (a product wider than 32 bits; a negative value below -2^31), c (`>>` by 32 or more), d (a macro defined twice), k (a macro inside a macro), n (address 0 or beyond the address bits: the game accepted `origin 0x10000`, but where the word went was outside the saved range), p (a symbol valued by a pointer or label; a non-literal `origin` or pointer argument), r (a carriage return in the board's own text; External Editing with the file missing), s (a label followed by an inline pointer), u (a negative value as an operand; `~` inside a larger expression), v (`repoint` with `inline`), w (chains of `/`, `%` or `<<`). Anything the parser has no rule for is undefined too (`no observed rule`).

A second, small probe board with each macro definition on its own line would settle the four unverified features and d and k. It is not planned until a program needs them.

## Behavior
### Which source
In this order:
1. `--no-asm`: nothing is assembled.
2. `--asm <file>`: that file.
3. The board's `assembly_is_external` is true: the board's path with the extension `.vcb` replaced by `.vcbasm` (`dir/foo.vcb` → `dir/foo.vcbasm`). The `assembly` field is not used (observed on `asm-external`).
4. Otherwise the board's `assembly` field.

- A source that is empty or only whitespace assembles nothing, prints nothing and is not an error.
- `--asm` together with `--no-asm` on the command line is an error. From the TOML config the keys are `asm` and `no-asm`; a command-line flag overrides the config, so `--no-asm` cancels a config `asm` and the reverse. Paths in the config are relative to the working directory, as `dump-vmem` is.
- **Line ends in files.** A file source (`--asm` or `<board>.vcbasm`) stands for the text as the Assembly Editor would hold it, so `\r\n` is read as `\n`. This is the engine's rule for its own input (the user's `.vcbasm` files are CRLF; the editor field the game assembled from is LF). The game reads a CRLF external file the same way (observed on `asm-external`). A `\r` in a board's `assembly` field, or a lone `\r` in a file, is undefined (23r).

### Assembling
1. Check the source: valid UTF-8, at most 16 MiB, no control characters other than tab and newline.
2. Remove each comment (from `#` to the end of its line). Then split into statements at newlines and `;`. Keep each statement's line and column in the original text. A line with a macro definition may not contain a `;`.
3. Preprocess: handle `macro`/`remac`/`unmac` in source order, and replace each macro use with its body, placeholders filled in. A macro name inside a macro body is undefined (23k).
4. First pass, in source order: apply directives; give every instruction and pointer its address; give each pending label the address of the next instruction, also across `origin` and pointers with an address. A label followed by an inline pointer is undefined (23s); a label with no instruction after it is rejected.
5. Second pass, in source order: evaluate every instruction with the symbols and pointers as defined at that point, and the labels of the whole program. Precedence is C's, accepted only for the operator pairs the game showed. All arithmetic is checked: overflow, a bad shift or a zero divisor is a gate hit or a rejection, never a panic. Parentheses nest at most 64 deep.
6. Write each value as a 32-bit word (a negative result in two's complement). When two statements store to one address the later one wins. Other words are left as they are. Word 0 is never written.

### Errors and gate hits
- A **gate hit** is an unverified feature used without the flag, an undefined construct, or a form the game rejects. Assembling continues after a gate hit, so one run lists them all.
- A **real error** (a statement that cannot be parsed: a missing operand or argument, an unclosed parenthesis, a number that is not a number, a control character) stops the assembler at once.
- Output: every gate hit so far, then the real error if there was one, each as `<source>:<line>:<column>: <message>`. For the board's own field, `<source>` is `<board.vcb>:assembly`. A problem inside a macro expansion reports the position of the use and names the macro.
- Messages: `'<feature>' is not verified against the game; pass --allow-unverified`; `<what>: the game's result is not known (facts question 23<letter>)` or `<what>: no observed rule`; `<what>: the game rejects this`.
- Source text printed in messages and listings has control characters escaped.
- With any gate hit or error, nothing is written and nothing is run, and the exit code is non-zero.

### `vcb run`
`vcb run <board.vcb>` picks the source as above and assembles into the loaded board before tick 0. The input file is never changed. New flags: `--asm <file>` and `--no-asm`.

When something was assembled it prints one line before the run:
`assembled <n> words from <source>`, where `<n>` counts stored words (instructions and pointers). Two notes are added when they apply: `, <k> replaced a different non-zero word` (the board already held another program there; the game never starts from such memory) and `(unverified: asm <feature>, …)`.

### `vcb asm`
```
vcb asm <board.vcb>
  --asm <file.vcbasm>      # default: the board's own source, as for vcb run
  -o, --output <new.vcb>   # the board with the assembled memory
  --dump-vmem <file>       # all of VMem after assembling, same format as vcb run --dump-vmem
  --listing <file>         # "-" for standard output
  --allow-unverified
  --force                  # allow overwriting existing output files
```
- The board gives the content bits, the address bits and the starting memory. Assemble onto a clean board: words of an earlier program that the new one does not overwrite stay (see the `replaced` note).
- With no output flag it only checks the program and prints the `assembled` line.
- `--output` writes a copy of the board with the new `vmem_data`, the source (with LF line ends) in the `assembly` field and `assembly_is_external` false. The text and the memory in the file then agree, and the game shows and re-assembles the same program. Such a board is for running; it is not evidence about the game.
- `--listing` writes one line per stored word in address order: `0xAAAAA 0xWWWWWWWW  <line>  <statement as written>` (5 and 8 hex digits; a macro use is shown as written). Then the labels and pointers with their addresses, sorted by address.
- Write safety, reusing `check_outputs`, `check_overwrite` and `write_file` in `main.rs`: every output is checked against every file the command reads (the board, its `.vcbmem`, the `--asm` file, `<board>.vcbasm`) and against the other outputs; existing files need `--force`; all checks and the whole assembly finish before the first write; files are written to a temporary name and renamed. The same input list applies to `vcb run`'s outputs.

### Library
`asm::assemble(source, source_name, &mut Board, allow_unverified) -> Result<Report>`. `Report` holds the stored words with their source positions, the label and pointer table, the unverified features used and the `replaced` count. Loading a board does not assemble; the command line calls `assemble` between `Board::read` and `Components::build`. Tests that use the library directly are not affected.

## Edge cases
- **VMem disabled** (`is_vmem_enabled` false) with a non-empty board source: there is no memory to assemble into, so nothing is assembled and one line says so (`assembly text not assembled: VMem is disabled`). The game starts such a board normally (observed on `asm-novmem`). Two evidence boards are like this (`engine/tests/evidence/AOC1P2_asm.vcb`, `timer-and-random.vcb`). An explicit `--asm` on such a board is an error.
- **External Editing with no `.vcbasm` file:** undefined (23r), the message names the path.
- **`cpu-32bit-manual-start.vcb`** carries an older program in its `assembly` field. `vcb run` on it now assembles that text, as the game would. No existing test runs it through the command line.
- **A program that stores nothing** (directives only): succeeds, `assembled 0 words`.
- **Address 0, or an address beyond the address bits:** undefined (23n). Checked before any write. The game accepts `origin 0x10000` on a 16-bit-address board; where it stores the word is not observed.
- **The Persistent Memory region** has no effect on assembling.

## Data
- `Board` settings get two more typed fields, read and written unchanged: `assembly` (text) and `assembly_is_external` (bool). A board without `assembly_is_external` reads as false. `src/template.json` already has both keys.
- **Test sources are the `assembly` fields of game saves**, not copies of the user's files: part 1 from `engine/tests/boards/cpu-aoc1p1.after.vcb`, part 2 from `cpu-aoc1p2.assembled.vcb`. `AoC2025/days/day1/part1.vcbasm` and `part2.vcbasm` equal those texts except for their CRLF line ends (checked 2026-11-16; part 2 against `engine/tests/evidence/AOC1P2_asm.vcb`). These fields contain the user's puzzle input. Advent of Code asks that inputs are not shared; the user allows it in this repository (framework spec, decision log), and the input is already in the existing fixtures.
- Probe sources live in `engine/boards/` next to their text-art boards: `asm-probe-32.vcbasm`, `asm-probe-16.vcbasm`, `asm-reject.vcbasm`, `asm-external.vcbasm` with `asm-external-field.vcbasm`, `asm-novmem.vcbasm`. All are written by `engine/tools/board-generators/asm_probes.py`, which holds the list of cases.
- `vcb gen` header keys: `assembly = <file>` puts that file's text (LF) into the board's `assembly` field; `assembly_external = <file>` sets `assembly_is_external` and writes that file next to the generated board as `<name>.vcbasm`, with CRLF line ends when `assembly_crlf = on`. A board with either key gets the assembler checklist (procedure and one line per `# case`) in place of the LED checklist. The file must be in the `.txt` file's directory. `textboard::parse` only records the keys; `gen()` in `main.rs` reads the file, as it already reads the board text. Without the keys the output is byte-identical to before.

## Security & privacy
The source is untrusted text. The assembler reads only the files named above and executes nothing. Its work is bounded (source size, nesting depth, no recursive macros, checked arithmetic, addresses checked before any write), and a bad source gives an error, never a crash. Outputs cannot overwrite inputs.

## Performance
No target. The day 1 programs are about 4,300 lines; assembling them takes a negligible part of a run.

## Observation
The game shows only the first error it finds, in the editor, before the simulation starts (reported by the user, 2026-11-16; step 2 below also covers the case that it starts anyway).

### A0: the part 2 save
`saves/pending/cpu-aoc1p2.vcb`: the manual-start CPU board with part 2 as its assembly text and Persistent Memory 0–65535. The user opens it, enters simulation, returns to edit mode at once, and saves as `cpu-aoc1p2.assembled.vcb`. The CPU does not start by itself on this board, so the save holds the assembled program untouched.

### A2: the probe boards
All are written by `vcb gen` into `saves/pending/` with their program in place and Persistent Memory over the probed range.

| Board | Content bits | What it holds |
|---|---|---|
| `asm-probe-32` | 32 | Cases expected to assemble: every unverified feature, and the undefined cases about values, names and addresses. Some words are preloaded, to see which ones assembling keeps (question 19). One `pointer` case. |
| `asm-probe-16` | 16 | The same cases, to see what happens to values wider than the content bits. |
| `asm-reject` | 32 | About 30 cases the game is expected to reject. Cases that might hang the game (division by zero, very large shifts, a macro inside a macro) come last. |
| `asm-external` | 32 | External Editing on, a CRLF `asm-external.vcbasm` beside it, and different text in the `assembly` field: shows which source the game uses and how it reads CRLF. |
| `asm-novmem` | — | VMem disabled, one line of assembly. The only question: does the simulation start? |

Not planned to be observed, so they stay errors: whether address 0 can be written by the assembler (Persistent Memory does not save word 0), and a missing external file.

**Layout of a probe program**, so that one surprising answer cannot spoil the others:
- A prelude of verified statements only (the symbols the cases share).
- Each case is one line (several statements joined with `;` where needed), with names used by no other case.
- Above each case, on its own line: `# case <letter>-<what>` for a question 23 case, `# case u-<feature>` for an unverified feature, `# case v0-…` for a control the game is known to accept, `# case z-…` for a case without a letter. A case is every line up to the next `# case` line (one case, the multi-line macro, has several).
- Each case starts with its own `origin <literal>` into a private block of 16 words (case k at `0x1000 + 16k`) and stores a marker word first (`0xA500 + k` on the probe boards, `0xB500 + k` on `asm-reject`). The few cases that are about having no `origin` yet come first and have no block. A case about where the current address goes stores a label's value (`@ Ck; Ck`), so the address is read from the save directly, and ends with a second `origin` and a closing marker.
- This holds for `asm-reject` too, so a case the game accepts after all can still be read.

**Procedure for each board:**
1. Open the board and start the simulation.
2. If the editor shows an error: write the message next to the case's id in the checklist, put `#` in front of every line of that case (do not delete lines, so line numbers stay), and go to 1. If the error is on a line that is not a case line, or the simulation starts despite an error message, write that down and stop; that board goes back to the engine author.
3. If the game hangs or crashes: note the case, comment it out and start again. Save As every few passes; `last_simulated.vcb` in the game's autosave folder is the fallback.
4. When the simulation starts: return to edit mode and Save As `<name>.assembled.vcb`.

Expected effort: one pass each for A0, the two probe boards, `asm-external` and `asm-novmem` if nothing is rejected, and one pass per rejected case on `asm-reject`.

**Import.** The files go to `engine/tests/boards/` as `<name>.vcb`, `<name>.assembled.vcb` and the checklist `<name>.asm-observed.md`. These suffixes keep them out of the existing fixture tests, which pick up `*.after.vcb` and `*.observed.md` by name and do not assemble. The checklist must have an "Observed by" line and the game version even when no case was rejected; without it the fixture test fails. That, and never naming a `vcb asm --output` board `.assembled.vcb`, is what keeps the assembler from being compared with itself.

## Acceptance criteria
A1, the day 1 programs:
- [ ] Assembling the `assembly` text of `cpu-aoc1p1.after.vcb` over the memory of `cpu-32bit-manual-start.vcb` gives memory equal to `cpu-aoc1p1.vcb` in every word, with no gate hit and without `--allow-unverified`.
- [ ] Assembling the `assembly` text of `cpu-aoc1p2.assembled.vcb` over the memory of `cpu-32bit-manual-start.vcb` equals that save's memory in words 1–65535, with no gate hit and without the flag.
- [ ] An ignored release test, next to `cpu_runs_advent_of_code_day_1_like_the_game`: the pixels of `cpu-aoc1p1.vcb`, the memory of `cpu-32bit-manual-start.vcb`, part 2 assembled into it; run with at most 500,000,000 ticks; the stop reason is `breakpoint` and word `0x100` = `0x16C7`.
- [ ] By hand: `vcb run AoC2025/cpu-32bit-autorun-aoc1p1.vcb --asm AoC2025/days/day1/part2.vcbasm --ticks 500000000 --watch 0x100` (the CRLF file) prints `0x16C7` for word `0x100`.
- [ ] `cargo test` and `cargo test --release -- --ignored` pass with no edit to an existing test or to `engine/tests/expected/*`. `vcb run` on `cpu-aoc1p1.vcb` (empty `assembly`) prints no `assembled` line.

A1, the gate:
- [ ] For each row of "Still unverified": a snippet that is a gate hit without `--allow-unverified` with the feature's name in the message, and with the flag stores the manual's word and reports the feature once. (Before A3 this covered every manual-only feature; the probe fixtures now test those against the game.)
- [ ] Every form listed under "Rejected by the game" has a snippet that is refused with `the game rejects this`, with and without the flag.
- [ ] Every letter still open in question 23 has a snippet that is a gate hit with its letter in the message, with and without the flag.
- [ ] `1 ? 2`, `symbol`, `macro m {`, `@`, `pointer p`, `(1`, `1 |`, `0b102`, `1 2 | 3`: an error or a gate hit; none assembles.
- [ ] A source with three gated statements and no real error reports all three in one run. A source with a gated statement followed by a statement that cannot be parsed reports both, the gate hit first.
- [ ] Positions: `origin 0x10\n1; foo` reports `<file>:2:4:`. A use of `macro m { a? | b? | nope }` reports the line of the use and `(in macro m)`. The board's field reports `<board.vcb>:assembly:<line>:<column>:`.

A1, sources, outputs and safety:
- [ ] One command-line test per source rule: `--asm` wins over the field; the field is used otherwise; `--no-asm` assembles nothing; `--asm` with `--no-asm` fails; config `asm` is cancelled by `--no-asm`; config `no-asm` is overridden by `--asm`.
- [ ] A file source with CRLF assembles to the same memory as its LF form. A `\r` in a board's `assembly` field is a 23r gate hit. A stored word is not cut to the content bits.
- [ ] Source `""` and `" \n\t\n"`: exit 0, no `assembled` line, memory unchanged. Source `symbol a 1`: exit 0, `assembled 0 words from <source>`.
- [ ] A source with the byte `0xFF`: non-zero exit, the message names the file and UTF-8. A source over 16 MiB, and one with 65 nested parentheses: an error, no crash.
- [ ] A board with VMem disabled and assembly text runs, prints the `not assembled` line, and gives the same result as with `--no-asm`. With `--asm` it fails.
- [ ] Assembling part 2 over `cpu-aoc1p1.vcb` (which holds part 1) prints a `replaced` count above zero; over the manual-start memory it prints none.
- [ ] After `vcb run` and after `vcb asm` with every output flag, the input `.vcb` and the `--asm` file have the same bytes as before.
- [ ] `vcb asm` with `--output`, `--dump-vmem` or `--listing` pointing at the board, the `--asm` file or `<board>.vcbasm` fails, also with `--force`. Onto an existing other file it fails without `--force`. After a failed assembly no output or temporary file exists.
- [ ] `vcb asm --output` writes a board that the reader loads, whose memory equals the `--dump-vmem` output, whose `assembly` field is the source with LF line ends, and which assembles to the same memory again.
- [ ] `vcb asm --listing -` on the part 1 text: the first line is for address `0x00100` (`pointer count 0x0100 0`); the line for `0x0FF00` has the word `0x00003032` and the statement `lia 50`; the table has `count`, `input`, `LOOP` = `0x0FF02` and `END` = `0x0FF3B`.
- [ ] Reading and writing a board keeps `assembly` and `assembly_is_external` byte for byte; a board without the key reads as false. Word 0 is the same before and after assembling in every test.

A2 and A3:
- [ ] `vcb gen` with `assembly = <file>` writes the text into the board; with `assembly_external = <file>` it writes `<name>.vcbasm` beside the board; a path outside the `.txt` directory fails; a board generated without the keys is byte-identical to before.
- [ ] A test asserts that every probe program has unique case ids, that every clause of question 23 planned for observation and every unverified row has a case, and that no two case lines are equal.
- [ ] For each imported probe: the test assembles the `assembly` text of `<name>.assembled.vcb` over the memory of `<name>.vcb`, without `--allow-unverified`, and compares the words of the save's Persistent Memory range except word 0. It asserts that the range covers every marker, and fails without the checklist's "Observed by" line.
- [ ] For each case that is commented out in the assembled save but not in the board: the checklist has its message, and the case line assembled alone after the prelude is an error or a gate hit in the engine.
- [ ] After A3 every feature is in `VERIFIED`, or is refused with "the game rejects this", or is listed above as still unverified or still undefined; this spec and question 23 say the same.

## Rollout
Each milestone ends with a local commit once all tests pass.

| # | Milestone | Needs from the user |
|---|---|---|
| A0 | `saves/pending/cpu-aoc1p2.vcb` (two fields of the manual-start board changed); the save imported as `cpu-aoc1p2.assembled.vcb` with its checklist | one save in the game, a minute. Done 2026-11-16: the game's memory equals the engine's in words 1–65535 |
| A1a | The typed fields; `asm.rs` with the gate; the part 1 and part 2 tests; `vcb asm`. `vcb run` unchanged | nothing (part 2's criteria wait for A0). Done 2026-11-16: part 1 and part 2 match the game in every word; part 2 runs to the Breakpoint at tick 70,339,472 with `0x100` = `0x16C7` |
| A1b | `vcb run` assembles by default, `--asm`, `--no-asm`; documents (below) | nothing. Done 2026-11-16: `vcb run AoC2025/cpu-32bit-autorun-aoc1p1.vcb --asm AoC2025/days/day1/part2.vcbasm --ticks 500000000 --watch 0x100` prints `0x16C7` |
| A2 | `vcb gen` keys; the five probe boards and checklists in `saves/pending/` | run five boards in the game. Boards written 2026-11-17: 53 cases on the probe boards, 40 on `asm-reject`; waiting for the user |
| A3 | Import: fixtures, classes moved, facts file and this spec updated | nothing. Done 2026-11-17: the engine matches the game's memory on both probe boards and on `asm-reject`, and refuses each of the 50 cases the user had to comment out |

- A1a and A1b are separate commits because A1b is the only step that changes what an existing command does. `--no-asm` is the way back for any board.
- A2 comes straight after A1, while the cases are fresh. When its boards are in `saves/pending/`, `CLAUDE.md`'s State says so.
- Documents updated in A1b: `engine/README.md` (the command and option tables, and "Running a new program", which says the engine ignores the assembly text), `CLAUDE.md` (State, "What could come next"). `AoC2025/README.md` is the user's repository; the user is told what changed.
- A game update means re-running A0 and the probe boards, as for every fixture; each checklist records the game version.

Changes made to the other documents with this spec (2026-11-16): `vcb-engine.md` (the assembler has its own spec; `--asm`, `--no-asm`), `vcb-engine-v1.md` (pointers to this spec; v1 stays done), `vcb-game-facts.md` (the `assembly_is_external` field, first-error-only, CRLF in the user's files, question 23).

## Open questions
All are about the game and are question 23 in the facts file; owner the user (the saves) and the engine author (the boards). None blocks A1a. A0 blocks only part 2's criteria.

## Decision log
- Fidelity → the same as the game; anything unobserved is rejected.
- Coverage → the whole of manual chapter 8, all of it in A1 (confirmed after the review, which proposed building the rest from the probe saves).
- Sources → an `--asm` file, the board's `assembly` field, and `<board>.vcbasm`.
- Outputs → run directly, a new `.vcb`, a memory dump, a listing.
- `vcb run` without flags → assembles as the game does; `--no-asm` turns it off.
- Ground truth → probe programs run and saved, plus error cases checked by hand.
- Unsure whether the game accepts something → reject.
- Several sources → `--asm` wins, otherwise what the game would use.
- Test source → first a copy of the part 1 file in `engine/tests/programs/`. Changed in the review: the game-assembled text is already in the fixtures' `assembly` fields, so those are used and no copy is made.
- The game's errors → the first error only, in the editor, before the simulation starts.
- `assembly` field of a written board → the source that was assembled.
- Part 2 → `0x16C7` is the right count and is an acceptance check.
- Manual-stated features without a save → gate hit unless `--allow-unverified`. Features whose result the manual does not pin down are undefined instead (review).
- Extras beyond the game → none.
- Review, CRLF → a file source is read with CRLF as LF; the user's files are CRLF.
- Review, part 2's label forms → one assembled save of part 2 in the game (A0).
- Review, reject board → kept in full; cases numbered, commented out instead of deleted.
- Review, flag → one shared `--allow-unverified` for inks and assembler; warnings say which.
- Review, engine error reporting → stop at the first real error, but list every gate hit in one run.
- A3 (2026-11-17) → the assembler's rules were rewritten to what the probe saves showed: 32-bit values on every board, the observed precedence pairs, later write wins, labels across `origin` and pointers, `repoint`, the game's rejections. Four macro features stay unverified because a macro definition may not share a line with `;`.
- Review, clear fixes → applied: verified defined by shape; the keep-memory rule verified; own suffixes for assembler fixtures; External Editing unverified with its own board; probe layout; write safety over all inputs; parser bounds; VMem-disabled boards run; concrete criteria.
