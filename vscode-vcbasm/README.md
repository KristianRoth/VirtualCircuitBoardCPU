# VCB Assembly for VS Code

Syntax colouring for `.vcbasm` files, the assembly language of the game Virtual Circuit Board.

It colours comments (`#`), the directives (`symbol`, `pointer`, `origin`, `macro` and the rest), labels (`@ NAME`), numbers (decimal, `0x`, `0b`, with underscores), macro placeholders (`a?`), operators and `;`.

Every name the program defines is coloured by what it is, where it is defined and wherever it is used:

| Defined with | Default colour |
|---|---|
| `symbol` | purple `#C792EA` |
| `pointer` | yellow `#FFCB6B` |
| `macro` | orange `#F78C6C` |
| `@` (a label) | blue `#82AAFF` |

Cmd+click on a name (Ctrl+click on Windows and Linux, or F12) jumps to the line that defines it: the `symbol`, `pointer` or `macro` line, or the `@` label.

Cmd+click on the definition itself (or Shift+F12 anywhere on a name) lists every place the name is used, in a small window you can step through.

A name that is not defined in the file stays uncoloured, which makes a misspelt name easy to see. The names are read from the file that is open; a name keeps the kind of its last definition.

## Install

VS Code loads any folder in its extensions directory. Link this folder there (so later changes here apply at once), then restart VS Code:

```
ln -s "$HOME/Documents/VirtualCircuitBoardCPU/vscode-vcbasm" "$HOME/.vscode/extensions/vcbasm"
```

To copy instead of link: `cp -R vscode-vcbasm ~/.vscode/extensions/vcbasm`.

Open a `.vcbasm` file; the language shown at the bottom right of the window should be "VCB Assembly". If it is not, run "Developer: Reload Window" from the command palette.

## Uninstall

```
rm "$HOME/.vscode/extensions/vcbasm"
```

## Changing the colours

The four colours above can be changed in your VS Code `settings.json`:

```json
"editor.semanticTokenColorCustomizations": {
  "rules": {
    "vcbSymbol": "#C792EA",
    "vcbPointer": "#FFCB6B",
    "vcbMacro": "#F78C6C",
    "vcbLabel": "#82AAFF"
  }
}
```

A rule can also set a style, for example `"vcbLabel": { "foreground": "#82AAFF", "bold": true }`, and `"vcbSymbol.declaration"` styles only the place where a symbol is defined.

The other colours (comments, numbers, directives) come from your theme. Their rules are in `syntaxes/vcbasm.tmLanguage.json`; the name colouring is in `extension.js`. After a change to either, run "Developer: Reload Window".
