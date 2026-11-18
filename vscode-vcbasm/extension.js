// Colours the names a .vcbasm program defines, wherever they are used: symbols, pointers, macros and
// labels each get their own token type. It also provides go-to-definition and find-references for those names.
// The plain syntax colouring is in syntaxes/vcbasm.tmLanguage.json.

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

/** Where the name at (line, column) is defined: every definition of it, as { line, start, length }. */
function definitionsAt(text, line, column) {
  const tokens = classify(text);
  const lines = text.split(/\r?\n/);
  const here = tokens.find((t) => t.line === line && t.start <= column && column <= t.start + t.length);
  if (!here) return [];
  const name = lines[line].substr(here.start, here.length);
  return tokens.filter((t) => t.declaration && lines[t.line].substr(t.start, t.length) === name);
}

/** Every place the name at (line, column) occurs, definitions included when asked for. */
function referencesAt(text, line, column, includeDeclaration) {
  const tokens = classify(text);
  const lines = text.split(/\r?\n/);
  const here = tokens.find((t) => t.line === line && t.start <= column && column <= t.start + t.length);
  if (!here) return [];
  const name = lines[line].substr(here.start, here.length);
  return tokens.filter((t) => (includeDeclaration || !t.declaration) && lines[t.line].substr(t.start, t.length) === name);
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
  // Cmd+click (or F12) on a name goes to the symbol, pointer, macro or label line that defines it.
  const definitions = {
    provideDefinition(document, position) {
      return definitionsAt(document.getText(), position.line, position.character).map(
        (t) => new vscode.Location(document.uri, new vscode.Range(t.line, t.start, t.line, t.start + t.length))
      );
    },
  };
  context.subscriptions.push(vscode.languages.registerDefinitionProvider({ language: 'vcbasm' }, definitions));
  // Shift+F12 lists every use of a name. Cmd+click on a definition does the same: VS Code falls back to
  // the references when "go to definition" lands where the cursor already is.
  const references = {
    provideReferences(document, position, options) {
      return referencesAt(document.getText(), position.line, position.character, options.includeDeclaration).map(
        (t) => new vscode.Location(document.uri, new vscode.Range(t.line, t.start, t.line, t.start + t.length))
      );
    },
  };
  context.subscriptions.push(vscode.languages.registerReferenceProvider({ language: 'vcbasm' }, references));
}

module.exports = { activate, deactivate() {}, classify, definitionsAt, referencesAt };
