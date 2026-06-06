// WASM end-to-end verification
// Usage: node tests/wasm_verify.js

import fs from 'fs';
import path from 'path';
import { execSync } from 'child_process';
import { fileURLToPath } from 'url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));

const TEMP = process.env.TEMP || '/tmp';
const WHISPER = 'target/release/whisper.exe';

const tests = [
    { name: 'add', source: '42 13 + .', expected: 55n },
    { name: 'sub', source: '10 3 - .', expected: 7n },
    { name: 'mul', source: '6 7 * .', expected: 42n },
    { name: 'div', source: '100 10 / .', expected: 10n },
];

let passed = 0;
let failed = 0;

for (const test of tests) {
    const wsFile = path.join(TEMP, `test_${test.name}.ws`);
    const wasmFile = path.join(TEMP, `test_${test.name}.wasm`);

    // Write source
    fs.writeFileSync(wsFile, test.source);

    // Build WASM
    try {
        execSync(`${WHISPER} build "${wsFile}" --target wasm -o "${wasmFile}"`, {
            cwd: path.join(__dirname, '..'),
            stdio: 'pipe'
        });
    } catch (e) {
        console.log(`[${test.name}] BUILD FAILED: ${e.stderr?.toString() || e.message}`);
        failed++;
        continue;
    }

    // Execute WASM
    try {
        const wasm = fs.readFileSync(wasmFile);
        const module = await WebAssembly.instantiate(wasm);
        const result = module.instance.exports.whisper_run();
        const ok = result === test.expected;
        console.log(`[${test.name}] ${test.source} => ${result} ${ok ? 'PASS' : `FAIL (expected ${test.expected})`}`);
        if (ok) passed++; else failed++;
    } catch (e) {
        console.log(`[${test.name}] RUNTIME ERROR: ${e.message}`);
        failed++;
    }
}

console.log(`\n${passed} passed, ${failed} failed`);
