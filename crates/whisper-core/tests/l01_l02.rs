use std::rc::Rc;
use whisper_core::opcode::Opcode;
use whisper_core::value::Value;
use whisper_core::vm::Vm;

fn vm() -> Vm {
    Vm::new()
}

#[test]
fn op_effect_known() {
    let mut v = vm();
    let r = v
        .execute(&[
            Opcode::PushStr("kv-set".to_string().into()),
            Opcode::OpEffect,
            Opcode::Swap,
        ])
        .unwrap();
    // op-effect 留 [pops pushes]，swap 后栈底余 pushes... 直接看返回顶：
    // execute 返回 pop() 一次 —— [pops, pushes] 顶是 pushes
    let _ = r;
    // 用带 drop 的形态断言 pops：kv-set (2,0) → 顶 0（pushes）
    let mut v2 = vm();
    let r2 = v2
        .execute(&[
            Opcode::PushStr("kv-set".to_string().into()),
            Opcode::OpEffect,
        ])
        .unwrap();
    assert_eq!(r2, Some(Value::I64(0)));
}

#[test]
fn op_effect_missing_is_minus_one() {
    let mut v = vm();
    let r = v
        .execute(&[
            Opcode::PushStr("zzz-nope".to_string().into()),
            Opcode::OpEffect,
        ])
        .unwrap();
    assert_eq!(r, Some(Value::I64(-1)));
}

#[test]
fn quote_size_counts_opcodes() {
    let mut v = vm();
    let q: Rc<[Opcode]> = Rc::from(vec![Opcode::Dup, Opcode::Drop, Opcode::Dup].into_boxed_slice());
    let r = v
        .execute(&[Opcode::PushRef(q.to_vec()), Opcode::QuoteSize])
        .unwrap();
    assert_eq!(r, Some(Value::I64(3)));
}

#[test]
fn word_size_known_and_missing() {
    let mut v = vm();
    v.define_word("t2".into(), vec![Opcode::Dup, Opcode::Drop]);
    let r = v
        .execute(&[Opcode::PushStr("t2".to_string().into()), Opcode::WordSize])
        .unwrap();
    assert_eq!(r, Some(Value::I64(2)));
    let r2 = v
        .execute(&[
            Opcode::PushStr("absent".to_string().into()),
            Opcode::WordSize,
        ])
        .unwrap();
    assert_eq!(r2, Some(Value::I64(-1)));
}
