fn main() {
    let src = "0 0 { _ . 1 + } { _ 2 < } # drop { . } { _ 3 < } #";
    let (flat, _) = whisper_codegen::block_gen::compile_block(src).unwrap();
    for (i, op) in flat.iter().enumerate() { println!("[{:2}] {:?}", i, op); }
    let mut vm = whisper_core::vm::Vm::new();
    let r = vm.execute(&flat);
    println!("result={:?}", r);
}
