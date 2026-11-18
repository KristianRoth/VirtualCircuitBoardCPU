// Colours the names a .vcbasm program defines, wherever they are used: symbols, pointers, macros and
// labels each get their own token type. The plain syntax colouring is in syntaxes/vcbasm.tmLanguage.json.

const TOKEN_TYPES = ['vcbSymbol', 'vcbPointer', 'vcbMacro', 'vcbLabel'];
const DEFINES = { symbol: 'vcbSymbol', resymb: 'vcbSymbol', pointer: 'vcbPointer', repoint: 'vcbPointer', macro: 'vcbMacro', remac: 'vcbMacro' };
const KEYWORDS = new Set(['symbol', 'resymb', 'unsymb', 'pointer', 'repoint', 'unpoint', 'inline', 'origin', 'orgprev', 'orgbase', 'macro', 'remac', 'unmac', 'bookmark', 'sub_bookmark']);
const NAME = /[A-Za-z_][A-Za-z0-9_]*/g;

// The code part of a line: everything before a comment.
const code = (line) => line.split('#')[0];

/** Every use and definition of a defined name: { line, start, length, type, declaration }. */
function classify(text) {
  const lines = text.split(/\r?\n/);
  // Pass 1: what each name is. A name keeps the kind of its last definition.
  const kinds = new Map();
  for (const line of lines) {
    for (const statement of code(line).split(';')) {
      const words = statement.trim().split(/\s+/);
      if (words[0] === '@' && words[1]) kinds.set(words[1], 'vcbLabel');
      else if (words[0].startsWith('@') && words[0].length > 1) kinds.set(words[0].slice(1), 'vcbLabel');
      else if (DEFINES[words[0]] && words[1]) kinds.set(words[1], DEFINES[words[0]]);
    }
  }
  // Pass 2: every occurrence of those names.
  const tokens = [];
  lines.forEach((line, lineNumber) => {
    const text = code(line);
    const bookmark = /^\s*(bookmark|sub_bookmark)\b/.test(text);
    let previous = '';
    for (const match of text.matchAll(NAME)) {
      const name = match[0];
      const type = kinds.get(name);
      const placeholder = text[match.index + name.length] === '?';
      if (type && !KEYWORDS.has(name) && !placeholder && !bookmark) {
        const before = text.slice(0, match.index).trimEnd();
        const declaration = DEFINES[previous] !== undefined || before.endsWith('@');
        tokens.push({ line: lineNumber, start: match.index, length: name.length, type, declaration });
      }
      previous = name;
    }
  });
  return tokens;
}

function activate(context) {
  const vscode = require('vscode');
  const legend = new vscode.SemanticTokensLegend(TOKEN_TYPES, ['declaration']);
  const provider = {
    provideDocumentSemanticTokens(document) {
      const builder = new vscode.SemanticTokensBuilder(legend);
      for (const t of classify(document.getText())) {
        builder.push(new vscode.Range(t.line, t.start, t.line, t.start + t.length), t.type, t.declaration ? ['declaration'] : []);
      }
      return builder.build();
    },
  };
  context.subscriptions.push(vscode.languages.registerDocumentSemanticTokensProvider({ language: 'vcbasm' }, provider, legend));
}

module.exports = { activate, deactivate() {}, classify };
