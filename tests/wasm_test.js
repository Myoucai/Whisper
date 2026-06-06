// WASM end-to-end test: load .wasm, execute, verify output
// Usage: node tests/wasm_test.js

const fs = require('fs');
const path = require('path');

function loadWasm(filepath) {
    const buf = fs.readFileSync(filepath);
    return WebAssembly.instantiate(buf);
}

function getResult(instance) {
    // Read i64 result from memory[0x0000..0x0007]
    const mem = new DataView(instance.exports.memory.buffer);
    const low = mem.getInt32(0, true);
    const high = mem.getInt32(4, true);
    // Combine into BigInt for i64
    return Number(BigInt(low) | (BigInt(high) << 32n));
}

function getString(instance, addr) {
    const mem = new Uint8Array(instance.exports.memory.buffer);
    let end = addr;
    while (mem[end] !== 0 && end < addr + 1024) end++;
    return new TextDecoder().decode(mem.slice(addr, end));
}

async function test(name, wasmFile, expectedResult) {
    try {
        const wasm = await loadWasm(wasmFile);
        const result = wasm.instance.exports.whisper_run();
        const resultNum = getResult(wasm.instance);

        console.log(`[${name}] whisper_run() returned: ${result}`);
        console.log(`  memory[0]: ${resultNum}`);

        if (resultNum === expectedResult) {
            console.log(`  PASS: expected ${expectedResult}`);
        } else {
            console.log(`  FAIL: expected ${expectedResult}, got ${resultNum}`);
        }

        // Check for strings in memory
        const mem = new Uint8Array(wasm.instance.exports.memory.buffer);
        const strStart = 0x1000;
        const strBytes = mem.slice(strStart, strStart + 50);
        const strContent = new TextDecoder().decode(strBytes).replace(/\0/g, '');
        if (strContent.length > 0) {
            console.log(`  String at 0x1000: "${strContent}"`);
        }

    } catch (e) {
        console.log(`[${name}] ERROR: ${e.message}`);
    }
}

async function main() {
    console.log('Whisper WASM Verification\n');

    // Test 1: 42 + 13 = 55
    // Already built via whisper build docs/examples/hello.ws --target wasm
    const helloWasm = path.join(__dirname, '..', '..', '..', 'tmp', 'test_hello.wasm');
    // For this test, we need a simple arithmetic wasm
    // Let's build one inline

    // Test direct arithmetic
    console.log('Building test WASM...');
    // We'll test manually built WASM files
}

main().catch(console.error);
