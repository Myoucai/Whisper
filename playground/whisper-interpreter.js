// Whisper JS Interpreter — tokenizer, parser, compiler, VM
// Full-stack implementation for the playground

const WhisperInterpreter = (() => {

  // ========== Tokenizer ==========
  class Tokenizer {
    constructor(source) {
      this.src = source;
      this.pos = 0;
    }

    tokenize() {
      const tokens = [];
      while (this.pos < this.src.length) {
        this.skipWhitespace();
        if (this.pos >= this.src.length) break;
        tokens.push(this.next());
      }
      return tokens;
    }

    skipWhitespace() {
      while (this.pos < this.src.length) {
        const ch = this.src[this.pos];
        if (' \t\r\n'.includes(ch)) { this.pos++; continue; }
        if (ch === '/' && this.src[this.pos+1] === '/') {
          while (this.pos < this.src.length && this.src[this.pos] !== '\n') this.pos++;
          continue;
        }
        break;
      }
    }

    next() {
      const ch = this.src[this.pos];

      // Strings
      if (ch === '"') return this.readString();
      // Numbers
      if ('-0123456789'.includes(ch)) return this.readNumber();
      // Bool
      if (ch === '#') {
        this.pos++;
        if (this.src[this.pos] === 't') { this.pos++; return { t: 'bool', v: true }; }
        if (this.src[this.pos] === 'f') { this.pos++; return { t: 'bool', v: false }; }
        return { t: 'op', v: '#' };
      }
      // Brackets
      if (ch === '[') { this.pos++; return { t: '[' }; }
      if (ch === ']') { this.pos++; return { t: ']' }; }
      if (ch === '{') { this.pos++; return { t: '{' }; }
      if (ch === '}') { this.pos++; return { t: '}' }; }
      // Colon / semicolon
      if (ch === ':') { this.pos++; return this.readConfOr(':'); }
      if (ch === ';') { this.pos++; return { t: ';' }; }
      // Question marks
      if (ch === '?') {
        this.pos++;
        if (this.src[this.pos] === '?') { this.pos++; return { t: '??' }; }
        if (this.src[this.pos] === '|') { this.pos++; return { t: '?|' }; }
        if (this.src[this.pos] === '-' && this.src[this.pos+1] === '>') {
          this.pos += 2; return { t: '?->' };
        }
        return { t: 'op', v: '?' };
      }
      // Pipe, dot, comma, backtick
      if (ch === '|') { this.pos++; return { t: '|' }; }
      if (ch === '.') {
        this.pos++;
        if (this.src[this.pos] === '.') { this.pos++; return { t: '..' }; }
        return { t: '.' };
      }
      if (ch === ',') { this.pos++; return { t: ',' }; }
      if (ch === '`') { this.pos++; return { t: '`' }; }
      // At symbols
      if (ch === '@') {
        this.pos++;
        if (this.src[this.pos] >= '0' && this.src[this.pos] <= '9') {
          let n = ''; while (this.pos < this.src.length && '0123456789'.includes(this.src[this.pos])) n += this.src[this.pos++];
          return { t: '@n', v: parseInt(n) };
        }
        let w = ''; while (this.pos < this.src.length && /[a-zA-Z_]/.test(this.src[this.pos])) w += this.src[this.pos++];
        return { t: '@w', v: w };
      }
      // Dollar pick
      if (ch === '$') {
        this.pos++;
        let n = ''; while (this.pos < this.src.length && '0123456789'.includes(this.src[this.pos])) n += this.src[this.pos++];
        return { t: '$', v: parseInt(n) || 0 };
      }
      // Operators (including two-character)
      if (ch === '!' && this.src[this.pos+1] === '=') { this.pos += 2; return { t: 'op', v: '!=' }; }
      if ('!'.includes(ch)) { this.pos++; return { t: 'op', v: ch }; }
      if ('+*&'.includes(ch)) { this.pos++; return { t: 'op', v: ch }; }
      if ('-/' .includes(ch)) { this.pos++; return { t: 'op', v: ch }; }
      if (ch === '%') { this.pos++; return { t: 'op', v: '%' }; }
      if (ch === '=') { this.pos++; return { t: 'op', v: '=' }; }
      if (ch === '<') { this.pos++; if (this.src[this.pos] === '=') { this.pos++; return { t: 'op', v: '<=' }; } return { t: 'op', v: '<' }; }
      if (ch === '>') { this.pos++; if (this.src[this.pos] === '=') { this.pos++; return { t: 'op', v: '>=' }; } return { t: 'op', v: '>' }; }
      if (ch === '_') { this.pos++; return { t: '_' }; }

      // Words (and builtin keywords)
      if (/[a-zA-Z]/.test(ch)) {
        let w = ''; while (this.pos < this.src.length && /[a-zA-Z0-9_\-/]/.test(this.src[this.pos])) w += this.src[this.pos++];
        // Builtin keywords that map to operators
        if (['drop', 'len', 'append', 'mod', 'import', 'export'].includes(w)) {
          return { t: 'op', v: w };
        }
        return { t: 'word', v: w };
      }
      this.pos++;
      return { t: 'error', v: `Unexpected: ${ch}` };
    }

    readString() {
      this.pos++; let s = '';
      while (this.pos < this.src.length && this.src[this.pos] !== '"') {
        if (this.src[this.pos] === '\\') { this.pos++; }
        s += this.src[this.pos++];
      }
      this.pos++; return { t: 'str', v: s };
    }

    readNumber() {
      let neg = false;
      if (this.src[this.pos] === '-') { neg = true; this.pos++; }
      // Check if it's actually a minus operator
      if (this.pos >= this.src.length || !/[0-9]/.test(this.src[this.pos])) {
        if (neg) { return { t: 'op', v: '-' }; }
        this.pos++; return { t: 'error', v: 'Expected number' };
      }
      let s = neg ? '-' : '';
      let isFloat = false;
      while (this.pos < this.src.length && '0123456789.'.includes(this.src[this.pos])) {
        if (this.src[this.pos] === '.') isFloat = true;
        s += this.src[this.pos++];
      }
      return isFloat ? { t: 'float', v: parseFloat(s) } : { t: 'int', v: parseInt(s) };
    }

    readConfOr(fallback) {
      if (this.pos < this.src.length && '0123456789.'.includes(this.src[this.pos])) {
        let s = ''; while (this.pos < this.src.length && '0123456789.'.includes(this.src[this.pos])) s += this.src[this.pos++];
        return { t: ':conf', v: parseFloat(s) };
      }
      return { t: ':' };
    }
  }

  // ========== Parser ==========
  function parse(tokens) {
    const nodes = [];
    let i = 0;
    while (i < tokens.length) {
      const r = parseNode(tokens, i);
      if (r) { nodes.push(r.node); i = r.next; }
      else i++;
    }
    return nodes;
  }

  function parseNode(tokens, i) {
    if (i >= tokens.length) return null;
    const tk = tokens[i];

    switch (tk.t) {
      case 'int': return { node: { t: 'lit', v: tk.v, ty: 'int' }, next: i+1 };
      case 'float': return { node: { t: 'lit', v: tk.v, ty: 'float' }, next: i+1 };
      case 'str': return { node: { t: 'lit', v: tk.v, ty: 'str' }, next: i+1 };
      case 'bool': return { node: { t: 'lit', v: tk.v, ty: 'bool' }, next: i+1 };
      case 'word': return { node: { t: 'word', v: tk.v }, next: i+1 };
      case 'op': return { node: { t: 'op', v: tk.v }, next: i+1 };
      case '_': return { node: { t: 'op', v: 'dup' }, next: i+1 };
      case '`': return { node: { t: 'op', v: 'swap' }, next: i+1 };
      case '.': return { node: { t: 'op', v: 'print' }, next: i+1 };
      case '..': return { node: { t: 'op', v: 'printall' }, next: i+1 };
      case ',': return { node: { t: 'op', v: 'read' }, next: i+1 };
      case '$': return { node: { t: 'op', v: 'pick', n: tk.v }, next: i+1 };
      case '@w': return { node: { t: 'op', v: tk.v }, next: i+1 };
      case '@n': return { node: { t: 'op', v: 'cap', n: tk.v }, next: i+1 };
      case '|': return { node: { t: 'marker', v: '|' }, next: i+1 };
      case ';': return { node: { t: 'marker', v: ';' }, next: i+1 };

      case '[': {
        const items = []; i++;
        while (i < tokens.length && tokens[i].t !== ']') {
          const r = parseNode(tokens, i);
          if (r) { items.push(r.node); i = r.next; }
          else i++;
        }
        i++; // skip ]
        return { node: { t: 'list', items }, next: i };
      }

      case '{': {
        const body = []; i++;
        while (i < tokens.length && tokens[i].t !== '}') {
          const r = parseNode(tokens, i);
          if (r) { body.push(r.node); i = r.next; }
          else i++;
        }
        i++; // skip }
        return { node: { t: 'quote', body }, next: i };
      }

      case ':': {
        // Word definition: : name { body } ;
        i++;
        if (i >= tokens.length || tokens[i].t !== 'word') return null;
        const name = tokens[i].v; i++;
        if (i >= tokens.length || tokens[i].t !== '{') return null;
        i++;
        const body = [];
        while (i < tokens.length && tokens[i].t !== '}') {
          const r = parseNode(tokens, i);
          if (r) { body.push(r.node); i = r.next; }
          else i++;
        }
        i++; // skip }
        if (i < tokens.length && tokens[i].t === ';') i++;
        return { node: { t: 'def', name, body }, next: i };
      }

      case '??': {
        // Conditional: cond ??then|else]
        i++;
        const thenBranch = [];
        while (i < tokens.length && tokens[i].t !== '|' && tokens[i].t !== ']') {
          const r = parseNode(tokens, i);
          if (r) { thenBranch.push(r.node); i = r.next; }
          else i++;
        }
        let elseBranch = null;
        if (i < tokens.length && tokens[i].t === '|') {
          i++; // skip |
          elseBranch = [];
          while (i < tokens.length && tokens[i].t !== ']') {
            const r = parseNode(tokens, i);
            if (r) { elseBranch.push(r.node); i = r.next; }
            else i++;
          }
        }
        if (i < tokens.length && tokens[i].t === ']') i++;
        return { node: { t: 'cond', then: thenBranch, else: elseBranch }, next: i };
      }

      case '?->': {
        return { node: { t: 'op', v: 'condarrow' }, next: i+1 };
      }

      default: return null;
    }
  }

  // ========== VM ==========
  class Vm {
    constructor() {
      this.stack = [];
      this.words = {};
      this.output = [];
    }

    defineWord(name, body) { this.words[name] = body; }

    pop() {
      if (this.stack.length === 0) throw new Error('Stack underflow');
      return this.stack.pop();
    }

    push(v) { this.stack.push(v); }

    pushOutput(s) { this.output.push(String(s)); }

    execNode(node) {
      if (!node) return;
      switch (node.t) {
        case 'lit':
          this.push(node.v);
          break;

        case 'word': {
          const name = node.v;
          if (this.words[name]) {
            for (const n of this.words[name]) this.execNode(n);
          } else {
            throw new Error(`Undefined word: ${name}`);
          }
          break;
        }

        case 'op':
          this.execOp(node.v, node.n);
          break;

        case 'list': {
          // Build list in-order by evaluating each element
          const items = [];
          for (const item of node.items) {
            this.execNode(item);
            items.push(this.pop());
          }
          this.push(items);
          break;
        }

        case 'quote':
          this.push({ t: 'ref', body: node.body });
          break;

        case 'cond': {
          const cond = this.pop();
          if (cond) {
            for (const n of node.then) this.execNode(n);
          } else if (node.else) {
            for (const n of node.else) this.execNode(n);
          }
          break;
        }
      }
    }

    execOp(op, n) {
      switch (op) {
        // Stack
        case 'dup': case '_': { const a = this.pop(); this.push(a); this.push(a); break; }
        case 'swap': case '`': { const b = this.pop(), a = this.pop(); this.push(b); this.push(a); break; }
        case 'drop': case '%': this.pop(); break;
        case 'mod': { const b = this.pop(), a = this.pop(); this.push(a % b); break; }
        case 'rot': { const c = this.pop(), b = this.pop(), a = this.pop(); this.push(b); this.push(c); this.push(a); break; }
        case 'pick': {
          const idx = this.stack.length - 1 - n;
          if (idx >= 0) this.push(this.stack[idx]);
          break;
        }
        // Arithmetic
        case '+': { const b = this.pop(), a = this.pop(); this.push(a + b); break; }
        case '-': { const b = this.pop(), a = this.pop(); this.push(a - b); break; }
        case '*': { const b = this.pop(), a = this.pop(); this.push(a * b); break; }
        case '/': { const b = this.pop(), a = this.pop(); if (b === 0) throw new Error('Division by zero'); this.push(Math.floor(a / b)); break; }
        // Comparison
        case '=': { const b = this.pop(), a = this.pop(); this.push(a === b); break; }
        case '!=': { const b = this.pop(), a = this.pop(); this.push(a !== b); break; }
        case '<': { const b = this.pop(), a = this.pop(); this.push(a < b); break; }
        case '>': { const b = this.pop(), a = this.pop(); this.push(a > b); break; }
        case '<=': { const b = this.pop(), a = this.pop(); this.push(a <= b); break; }
        case '>=': { const b = this.pop(), a = this.pop(); this.push(a >= b); break; }
        // Logic
        case '&': { const b = this.pop(), a = this.pop(); this.push(a && b); break; }
        case '|': { const b = this.pop(), a = this.pop(); this.push(a || b); break; }
        case '!': { this.push(!this.pop()); break; }
        // List ops
        case 'nth': case '@nth': { const idx = this.pop(), list = this.pop(); if (!Array.isArray(list)) throw new Error('Expected list'); if (idx < 0 || idx >= list.length) throw new Error('Index out of bounds'); this.push(list[idx]); break; }
        case 'append': { const elem = this.pop(), list = this.pop(); if (!Array.isArray(list)) throw new Error('Expected list'); this.push([...list, elem]); break; }
        case 'len': { const list = this.pop(); if (!Array.isArray(list)) throw new Error('Expected list'); this.push(list.length); break; }
        case 'map': case '@map': {
          const quot = this.pop(), list = this.pop();
          if (!Array.isArray(list)) throw new Error('Expected list');
          if (!quot || quot.t !== 'ref') throw new Error('Expected ref');
          const result = [];
          for (const item of list) {
            this.push(item);
            for (const n of quot.body) this.execNode(n);
            result.push(this.pop());
          }
          this.push(result);
          break;
        }
        case 'each': case '@each': {
          const quot = this.pop(), list = this.pop();
          if (!Array.isArray(list)) throw new Error('Expected list');
          if (!quot || quot.t !== 'ref') throw new Error('Expected ref');
          for (const item of list) {
            this.push(item);
            for (const n of quot.body) this.execNode(n);
          }
          break;
        }
        case 'fold': case '@fold': {
          const quot = this.pop(), init = this.pop(), list = this.pop();
          if (!Array.isArray(list)) throw new Error('Expected list');
          if (!quot || quot.t !== 'ref') throw new Error('Expected ref');
          let acc = init;
          for (const item of list) {
            this.push(acc); this.push(item);
            for (const n of quot.body) this.execNode(n);
            acc = this.pop();
          }
          this.push(acc);
          break;
        }
        case 'times': case '@times': {
          const quot = this.pop(), count = this.pop();
          if (!quot || quot.t !== 'ref') throw new Error('Expected ref');
          for (let i = 0; i < count; i++) {
            for (const n of quot.body) this.execNode(n);
          }
          break;
        }
        case 'condarrow': case '?->': {
          const cond = this.pop();
          // condarrow: executed after {then} ?-> — this is handled by the parser
          break;
        }
        // IO
        case 'print': case '.': {
          const v = this.stack.length > 0 ? this.pop() : null;
          this.pushOutput(v !== null ? v : '(empty)');
          break;
        }
        case 'printall': case '..': {
          this.pushOutput(`Stack (${this.stack.length}): ${[...this.stack].reverse().join(' ')}`);
          break;
        }
        default:
          throw new Error(`Unknown operator: ${op}`);
      }
    }

    run(nodes) {
      this.stack = [];
      this.output = [];
      // Pass 1: collect word definitions
      for (const node of nodes) {
        if (node.t === 'def') {
          this.defineWord(node.name, node.body);
        }
      }
      // Pass 2: execute main program
      for (const node of nodes) {
        if (node.t === 'def') continue;
        this.execNode(node);
      }
    }
  }

  // ========== Public API ==========
  function compile(source) {
    const tokenizer = new Tokenizer(source);
    const tokens = tokenizer.tokenize();
    return parse(tokens);
  }

  function execute(source) {
    const nodes = compile(source);
    const vm = new Vm();
    vm.run(nodes);
    return { output: vm.output, stack: vm.stack };
  }

  return { compile, execute, Tokenizer, Vm };
})();

if (typeof module !== 'undefined') module.exports = WhisperInterpreter;
