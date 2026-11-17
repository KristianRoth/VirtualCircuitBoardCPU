"""Writes the assembler probe boards (specs/vcb-assembler.md, milestone A2) into boards/:
asm-probe-32, asm-probe-16, asm-reject, asm-external, asm-novmem, each a .txt board and its .vcbasm program(s).
Run from engine/:  python3 tools/board-generators/asm_probes.py

Every case is the lines from its `# case <id>` line to the next one. An id starts with the letter of facts
question 23 it answers, `u` for an unverified feature, `v0` for a form the game is already known to accept
(a control), or `z` for a case without a letter. Each case stores into its own 16-word block, marker word
first, so one surprising answer cannot move another case's words:
    {B} = the block's first address, {B8} = {B}+8, {B9} = {B}+9, {O} = "origin {B}; <marker>;"
"""

# Statements every case may use. Only forms the game has already assembled.
PRELUDE = """symbol one 1
symbol two 2
symbol four 4
symbol six 6
macro pair { a? | b? }
"""

# Cases the game is expected to assemble.
PROBE = [
    # Must stay first: nothing before it sets an address.
    ("u-first-address", "0xF1F1; 0xF1F2"),
    ("v0-control", "{O} one two"),
    ("v0-pointer-value", "pointer pv1 {B8} 0x1234; {O} pv1"),
    ("u-comment-after", "{O} 3 # a comment after a statement"),
    ("u-underscore-decimal", "{O} 1_000"),
    ("u-underscore-hex", "{O} 0x1_0"),
    ("u-negative-literal", "{O} -1"),
    ("u-negative-symbol", "symbol ns1 -2; {O} ns1"),
    ("u-or-overlap", "{O} 3 6"),
    ("u-or-overlap-operator", "{O} 3 | 6"),
    ("u-three-primitives", "{O} one two four"),
    ("u-resymb", "symbol rs1 1; {O} rs1; resymb rs1 2; rs1"),
    ("u-unsymb", "symbol us1 1; unsymb us1; {O} 5"),
    ("u-inline", "{O} pointer in1 inline 5; in1; 9"),
    ("u-unpoint", "pointer up1 {B8} 7; unpoint up1; {O} 5"),
    ("u-orgprev", "origin {B}; origin {B8}; origin orgprev; {M}; 5"),
    ("z-orgprev-after-store", "{O} origin {B8}; origin orgprev; @ OP2; OP2"),
    ("u-add", "{O} 6 + 3"),
    ("u-subtract", "{O} 6 - 3"),
    ("u-multiply", "{O} 6 * 3"),
    ("u-and", "{O} 6 & 3"),
    ("u-xor", "{O} 6 ^ 3"),
    ("u-shift-left", "{O} 1 << 4"),
    ("u-divide", "{O} 6 / 3"),
    ("u-modulo", "{O} 7 % 4"),
    ("u-or-in-instruction", "{O} 6 | 1"),
    ("u-shift-right-in-instruction", "{O} (8 >> 1)"),
    ("u-macro-no-placeholder", "macro m0a { one | two }; {O} m0a"),
    ("u-macro-one-placeholder", "macro m1a { a? | one }; {O} m1a two"),
    ("u-macro-three-placeholders", "macro m3a { a? | b? | c? }; {O} m3a one two four"),
    ("u-macro-literal-arguments", "{O} pair 1 2"),
    ("u-macro-braces", "macro mb1 {a? | b?}; {O} mb1 one two"),
    ("u-remac", "macro rm1 { a? | b? }; remac rm1 { a? | b? | four }; {O} rm1 one two"),
    ("u-unmac", "macro um1 { a? | b? }; unmac um1; {O} 5"),
    ("u-bookmarks", "{O} 1; bookmark Bk1; sub_bookmark Bk2; 2"),
    ("a-or-shift", "{O} 1 | 2 << 3"),
    ("a-shift-or", "{O} 2 << 3 | 1"),
    ("a-subtract-multiply", "{O} 8 - 2 * 3"),
    ("a-add-multiply", "{O} 6 + 3 * 2"),
    ("b-wider-than-16-bits", "{O} 0x12345"),
    ("b-negative-result", "{O} 1 - 2"),
    ("b-overflow", "{O} 0xFFFFFFFF + 1"),
    ("l-address-after-pointer", "{O} pointer la1 {B8} 7; @ LA1; LA1"),
    ("m-two-writes", "{O} 1; origin {B}; 0x77"),
    ("q-lower-case-hex", "{O} 0xff"),
    ("s-label-then-origin", "{O} @ SL1; origin {B8}; 1; SL1"),
    ("s-label-then-pointer", "{O} @ SL2; pointer sl2p {B8} 7; origin {B9}; 1; SL2"),
    # If the game splits at ; before it removes comments, the marker and 0x99 are stored.
    ("t-semicolon-in-comment", "# not a statement; {O} 0x99"),
    ("u-not", "{O} ~ 1"),
    ("u-inexact-division", "{O} 7 / 2"),
    ("w-shift-chain", "{O} 64 >> 2 >> 1"),
    ("w-subtract-chain", "{O} 10 - 4 - 3"),
    # Must stay last in the probe: orgbase goes back to where the program started.
    ("u-orgbase", "origin {B}; origin {B8}; origin orgbase; @ OB1; OB1"),
]

# Cases the game is expected to reject. The first two need an empty origin stack, so they come first.
REJECT = [
    ("o-orgprev-nothing-to-revert", "origin orgprev; @ OE1; OE1"),
    ("o-orgbase-nothing-to-revert", "origin orgbase; @ OE2; OE2"),
    ("b-wider-than-32-bits", "{O} 0x1_0000_0000"),
    ("d-symbol-twice", "symbol d1 1; symbol d1 2; {O} d1"),
    ("d-pointer-twice", "pointer d2 {B8} 1; pointer d2 {B9} 2; {O} d2"),
    ("d-macro-twice", "macro d3 { a? | b? }; macro d3 { a? | b? | four }; {O} d3 one two"),
    ("d-label-twice", "{O} @ D4; 1; @ D4; 2; D4"),
    ("e-symbol-and-pointer", "symbol e1 1; pointer e1 {B8} 2; {O} e1"),
    ("e-symbol-and-label", "symbol e2 1; {O} @ e2; 5; e2"),
    ("e-keyword-as-name", "symbol sub_bookmark 1; {O} 5"),
    ("f-undefined-name", "{O} nope_f1"),
    ("f-after-unsymb", "symbol f2 1; unsymb f2; {O} f2"),
    ("f-after-unpoint", "pointer f3 {B8} 1; unpoint f3; {O} f3"),
    ("g-symbol-before-definition", "{O} g1; symbol g1 5"),
    ("g-pointer-before-definition", "{O} g2; pointer g2 {B8} 5"),
    ("h-fewer-arguments", "{O} pair one"),
    ("h-more-arguments", "{O} pair one two four"),
    ("i-expression-argument", "{O} pair (one | two) four"),
    ("j-macro-among-primitives", "{O} one pair"),
    ("k-multi-line-macro", "macro k1 {\na? | b?\n}\n{O} k1 one two"),
    ("n-beyond-address-bits", "origin 0x10000; {M}"),
    ("p-symbol-value-is-a-name", "symbol p1 one; {O} p1"),
    ("p-symbol-value-is-an-expression", "symbol p2 1 + 1; {O} p2"),
    ("p-operator-without-spaces", "{O} 1|2"),
    ("q-label-and-instruction", "{O} @ Q1 5"),
    ("q-keyword-upper-case", "Symbol q2 1; {O} 5"),
    ("q-prefix-upper-case", "{O} 0X10"),
    ("q-plus-sign", "{O} +5"),
    ("v-repoint", "pointer v1 {B8} 7; repoint v1 {B9} 8; {O} v1"),
    ("x-dot-in-name", "symbol x.1 1; {O} 5"),
    ("x-name-starts-with-digit", "symbol 1x 1; {O} 5"),
    ("z-primitives-beside-expression", "{O} 1 2 | 4"),
    ("z-unclosed-parenthesis", "{O} (1"),
    ("z-resymb-of-new-name", "resymb zr1 1; {O} zr1"),
    # From here on: cases that might hang the game. Save As before going on.
    ("c-divide-by-zero", "{O} 6 / 0"),
    ("c-modulo-by-zero", "{O} 6 % 0"),
    ("c-shift-by-32", "{O} 1 << 32"),
    ("c-shift-by-1000", "{O} 1 << 1000"),
    ("k-macro-in-macro", "macro k2 { pair | a? | b? }; {O} k2 one two"),
    # Must stay last: a label with nothing after it.
    ("s-label-at-end", "@ S_END"),
]


# ---- The second round (asm-probe2): everything the first round left open. ----
# A macro definition must be alone on its line, so these cases have several lines.
PROBE2 = [
    ("v0-control", "{O} one two"),
    ("u-macro-no-placeholder", "macro m0a { one | two }\n{O} m0a"),
    ("u-macro-one-placeholder", "macro m1a { a? | one }\n{O} m1a two"),
    ("u-macro-three-placeholders", "macro m3a { a? | b? | c? }\n{O} m3a one two four"),
    ("u-macro-braces", "macro mb1 {a? | b?}\n{O} mb1 one two"),
    ("u-remac", "macro rm1 { a? | b? }\nremac rm1 { a? | b? | four }\n{O} rm1 one two"),
    ("u-unmac", "macro um1 { a? | b? }\nunmac um1\n{O} 5"),
    ("z-unmac-then-macro", "macro um2 { a? | b? }\nunmac um2\nmacro um2 { a? | b? | four }\n{O} um2 one two"),
    ("z-unsymb-then-symbol", "symbol rr1 1; unsymb rr1; symbol rr1 2; {O} rr1"),
    ("z-macro-other-operators", "macro mo1 { a? + b? * 2 }\n{O} mo1 one two"),
    ("z-macro-comment-after", "macro mc1 { a? | b? } # a comment after a macro definition\n{O} mc1 one two"),
    ("z-macro-skips-a-letter", "macro ms1 { a? | c? }\n{O} ms1 one two"),
    ("z-macro-empty-body", "macro me1 { }\n{O} 5"),
    ("z-macro-use-in-expression", "{O} pair one two | four"),
    ("d-macro-twice", "macro d3 { a? | b? }\nmacro d3 { a? | b? | four }\n{O} d3 one two"),
    ("k-macro-in-macro", "macro k2 { pair | a? | b? }\n{O} k2 one two"),
    ("k-macro-in-macro-with-arguments", "macro k3 { pair a? b? }\n{O} k3 one two"),
    ("z-unsymb-of-new-name", "unsymb nn1; {O} 5"),
    ("z-unpoint-of-new-name", "unpoint nn2; {O} 5"),
    ("z-unmac-of-new-name", "unmac nn3\n{O} 5"),
    ("z-keyword-in-instruction", "{O} one inline"),
    ("p-symbol-valued-by-pointer", "pointer pp1 {B8} 7; symbol ps1 pp1; {O} ps1"),
    ("p-symbol-valued-by-label", "{O} @ PL1; 1; symbol ps2 PL1; ps2"),
    ("p-origin-by-symbol", "symbol po1 {B}; origin po1; {M}; 5"),
    ("p-pointer-address-by-symbol", "symbol pa1 {B8}; pointer pp2 pa1 7; {O} pp2"),
    ("p-pointer-value-by-symbol", "pointer pp3 {B8} six; {O} pp3"),
    ("s-label-then-inline-pointer", "{O} @ SI1; pointer si1 inline 5; 1; SI1"),
    ("v-repoint-inline", "pointer vi1 {B8} 7; {O} repoint vi1 inline 9; vi1"),
    ("v-repoint-of-symbol", "symbol vs1 1; repoint vs1 {B8} 2; {O} vs1"),
    ("u-negative-operand-add", "{O} -1 + 2"),
    ("u-negative-operand-or", "{O} -1 1"),
    ("u-negative-operand-multiply", "{O} -2 * 3"),
    ("u-negative-intermediate", "{O} (1 - 2) + 5"),
    ("u-negative-shift-right", "{O} -8 >> 1"),
    ("u-negative-divide", "{O} -7 / 2"),
    ("u-negative-modulo", "{O} -7 % 3"),
    ("u-not-of-negative", "{O} ~ -1"),
    ("u-not-then-add", "{O} ~ 1 + 1"),
    ("u-add-then-not", "{O} 1 + ~ 1"),
    ("u-not-parenthesis", "{O} ~ (1 + 1)"),
    ("u-not-not", "{O} ~ ~ 5"),
    ("u-minus-name", "{O} - six"),
    ("u-minus-parenthesis", "{O} - (1 + 1)"),
    ("b-product-fits", "{O} 0xFFFF * 0x10000"),
    ("b-product-too-wide", "{O} 0x10000 * 0x10000"),
    ("b-below-minus-2-to-31", "{O} 0 - 0x80000001"),
    ("b-literal-below-minus-2-to-31", "{O} -0x80000001"),
    ("b-literal-minus-2-to-32", "{O} -0xFFFFFFFF"),
    ("b-shift-out-the-top", "{O} 0x80000000 << 1"),
    ("c-shift-right-by-31", "{O} 0x80000000 >> 31"),
    ("c-shift-right-by-32", "{O} 0x80000000 >> 32"),
    ("c-shift-right-by-33", "{O} 0x80000000 >> 33"),
    ("c-shift-left-zero-by-32", "{O} 0 << 32"),
    ("c-shift-by-negative", "{O} 8 >> -1"),
    ("w-divide-chain", "{O} 64 / 4 / 2"),
    ("w-modulo-chain", "{O} 100 % 7 % 3"),
    ("w-shift-left-chain", "{O} 1 << 2 << 1"),
    ("w-add-chain", "{O} 1 + 2 + 3"),
    ("a-all-levels", "{O} 1 | 6 ^ 7 & 12 << 1 + 2 * 3"),
    ("a-parentheses-nested", "{O} ((1 + 2) * (3 + 4)) - 1"),
    ("n-beyond-12-address-bits", "origin 0x1800; {M}; 5"),
    ("n-last-word", "origin 0xFFFFF; {M}"),
    ("n-past-the-last-word", "origin 0xFFFFF; 0xEEE1; 0xEEE2"),
    ("n-origin-beyond-vmem", "origin 0x100000; 0xEEE3"),
    ("n-pointer-beyond-vmem", "pointer nb1 0x100000 7; {O} 5"),
    ("n-origin-0", "origin 0; 0xAAA0; 0xAAA1"),
    ("n-pointer-at-0", "pointer nz1 0 0xAAA2; {O} nz1"),
]

# Every pair of operators from different precedence levels of C, in both orders, with operands for which the
# two readings differ; and the pairs within one level, where the question is the direction.
NAMES = {"*": "mul", "/": "div", "%": "mod", "+": "add", "-": "sub", "<<": "shl", ">>": "shr", "&": "and", "^": "xor", "|": "or"}
LEVELS = [["*", "/", "%"], ["+", "-"], ["<<", ">>"], ["&"], ["^"], ["|"]]


def value(op, a, b):
    if a is None or b is None or (op in "/%" and b == 0) or (op in ("<<", ">>") and b > 31):
        return None
    v = eval(f"a {op.replace('/', '//')} b")
    return v if 0 <= v < 2**32 else None


def operands(a, b):
    """Small x, y, z for which (x a y) b z and x a (y b z) are both valid and differ; None when the two
    readings can never differ (x * y << z)."""
    for x, y, z in ((x, y, z) for x in (1, 2, 3, 5, 7, 12, 20, 100) for y in (1, 2, 3, 5, 6, 9) for z in (1, 2, 3, 4, 7)):
        left, right = value(b, value(a, x, y), z), value(a, x, value(b, y, z))
        if left is not None and right is not None and left != right:
            return x, y, z
    return None


for i, level in enumerate(LEVELS):
    for a in level:
        for b in (op for other in LEVELS[i:] for op in other if op != a):
            if b in level and level.index(b) < level.index(a):
                continue
            for first, second in ((a, b), (b, a)):
                if operands(first, second) is None:
                    continue
                x, y, z = operands(first, second)
                PROBE2.append((f"a-{NAMES[first]}-{NAMES[second]}", f"{{O}} {x} {first} {y} {second} {z}"))

def program(cases, first_block, marker_base):
    lines = [PRELUDE]
    for k, (case, text) in enumerate(cases):
        block, marker = first_block + 16 * k, f"0x{marker_base + k:04X}"
        fill = {"B": f"0x{block:04X}", "B8": f"0x{block + 8:04X}", "B9": f"0x{block + 9:04X}", "M": marker, "O": f"origin 0x{block:04X}; {marker};"}
        for key, value in fill.items():
            text = text.replace("{" + key + "}", value)
        lines.append(f"# case {case}\n{text}\n")
    return "\n".join(lines)


# One LED, so the board is not empty; VMem pins on two rows of bare board. Persistent Memory covers every block.
def board(content_bits, covers, extra=""):
    pins = f"vmem.address = 16,40,2,2,0\nvmem.content = {content_bits},70,6,2,0\npersistent = 0,8191\n" if content_bits else ""
    # Preloaded words the programs never write: does assembling keep them (facts question 19)?
    preload = "word.0x0800 = 0x0BAD\nword.0x100C = 0x0C0C\nword.0x1FFF = 0x0E0E\n" if content_bits else ""
    return f"covers = {covers}\n{pins}{preload}{extra}\nLW5RE\n"


def write(name, text):
    with open(f"boards/{name}", "w", newline="\n") as f:
        f.write(text)


probe = program(PROBE, 0x1000, 0xA500)
write("asm-probe-32.vcbasm", probe)
write("asm-probe-16.vcbasm", probe)
write("asm-reject.vcbasm", program(REJECT, 0x1000, 0xB500))
write("asm-probe-32.txt", board(32, "23, 19", "assembly = asm-probe-32.vcbasm\n"))
write("asm-probe-16.txt", board(16, "23, 19", "assembly = asm-probe-16.vcbasm\n"))
write("asm-reject.txt", board(32, "23", "assembly = asm-reject.vcbasm\n"))

# External Editing: the board's own text and the file store different words, and the file has CRLF line ends.
write("asm-external-field.vcbasm", "# case r-field-text-used\norigin 0x1000; 0xF1E1\n")
write("asm-external.vcbasm", "# case r-external-file-used\norigin 0x1010\n0xE7E1\n0xE7E2\n")
write("asm-external.txt", board(32, "23r", "assembly = asm-external-field.vcbasm\nassembly_external = asm-external.vcbasm\nassembly_crlf = on\n"))

# VMem disabled: does the simulation start at all?
write("asm-novmem.vcbasm", "# case r-vmem-disabled\norigin 0x1000; 1\n")
write("asm-novmem.txt", board(0, "23r", "assembly = asm-novmem.vcbasm\n"))

# The second round. 12 address bits, so most of VMem is "beyond the address bits"; blocks start at 0x100.
# Persistent Memory is set to all of VMem, to see words stored beyond the address bits.
write("asm-probe2.vcbasm", program(PROBE2, 0x100, 0xC000))
write("asm-probe2.txt", f"covers = 23\nvmem.address = 12,40,2,2,0\nvmem.content = 32,70,6,2,0\npersistent = 0,1048575\nword.0x0FFF = 0x0BAD\nassembly = asm-probe2.vcbasm\n\nLW5RE\n")

# A carriage return in the Assembly Editor text itself.
write("asm-crfield.vcbasm", "# case r-carriage-return-in-the-editor-text\norigin 0x1000\n0xC4C1\n0xC4C2\n")
write("asm-crfield.txt", board(32, "23r", "assembly = asm-crfield.vcbasm\nassembly_field_crlf = on\n"))

# External Editing on, and no .vcbasm file (it is deleted after `vcb gen`); the board's own text is there.
write("asm-external-missing-field.vcbasm", "# case r-external-file-missing\norigin 0x1000; 0xF1E1\n")
write("asm-external-missing-file.vcbasm", "origin 0x1010\n0xE7E1\n")
write("asm-external-missing.txt", board(32, "23r (the .vcbasm file is missing on purpose)", "assembly = asm-external-missing-field.vcbasm\nassembly_external = asm-external-missing-file.vcbasm\n"))
