// Whisper FFI Standard Library — glue-writing idioms
//
// 胶水层三坑的官方解法：
//   1. 列表字面量是编译期常量（$N 会被折叠成 0）→ 参数表必须运行时构建
//   2. ffi-call 要求 fnptr 在栈顶 → ffi2/ffi3 封装了正确的栈序
//   3. 胶水词内引用的辅助词也要 export

// libname symname → fnptr | 0   （open+sym 一步）
: ffi-fn  { swap ffi-open swap ffi-sym } ;

// str → [str]            单参数表（p 型）
: ffi-arg1 { [] swap append } ;

// a b → [a b]            双参数表
: ffi-arg2 { ` [] swap append ` append } ;

// a b c → [a b c]        三参数表
: ffi-arg3 { @ ` [] swap append ` append ` append } ;

// fmt fnptr args → result   参数表在栈顶（fnptr 已在其下）
// 典型用法： ... ffi-arg3 "iipi" fnptr ffi-call 换成：
//           fnptr ffi-arg3 "iipi" ffi3   —— 不对，见下
// 实际封装：args fmt fnptr 三个都在栈上时按 ffi-call 原序调用即可，
// 这里提供的是建好参数表后 fnptr 在表下的场景：

// fmt fnptr [args] → result   （fnptr 中、args 顶 → swap 归位）
: ffi-go { rot ffi-call } ;

export ffi-fn
export ffi-arg1
export ffi-arg2
export ffi-arg3
export ffi-go
