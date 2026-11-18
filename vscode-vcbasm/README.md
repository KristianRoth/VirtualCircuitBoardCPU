# VCB Assembly for VS Code

Syntax colouring for `.vcbasm` files, the assembly language of the game Virtual Circuit Board.

It colours comments (`#`), the directives (`symbol`, `pointer`, `origin`, `macro` and the rest), the name a directive defines, labels (`@ NAME`), numbers (decimal, `0x`, `0b`, with underscores), macro placeholders (`a?`), operators and `;`. Instruction names are not coloured specially, because every program defines its own with `symbol`.

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

The rules are in `syntaxes/vcbasm.tmLanguage.json`. The colours themselves come from your VS Code theme. After a change, run "Developer: Reload Window".
