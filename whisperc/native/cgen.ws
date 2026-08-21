// Whisper C Code Generator
// Opcode [byte, operand...] -> C code string
// Each group: [opcode] -> [opcode, c_code] or [opcode, #f]

: g1 {
    dup 0 @nth 48 = ??
        dup 1 @nth i64tostr "w_push((wval)(int64_t)" swap "L);\n" strcat strcat
    | dup 0 @nth 4 = ??
        dup 1 @nth i64tostr "w_pick_n(" swap ");\n" strcat strcat
    | dup 0 @nth 51 = ??
        dup 1 @nth i64tostr "w_push((wval)" swap "L);\n" strcat strcat
    | dup 0 @nth 80 = ??
        dup 1 @nth i64tostr "{wval _c=w_pop();if(!(int64_t)_c)goto L_" swap ";}\n" strcat strcat
    | dup 0 @nth 81 = ??
        dup 1 @nth i64tostr "goto L_" swap ";\n" strcat strcat
    | dup 0 @nth 82 = ??
        dup 1 @nth i64tostr "{wval _c=w_pop();if((int64_t)_c)goto L_" swap ";}\n" strcat strcat
    | dup 0 @nth 96 = ??
        "w_" over 1 @nth "();\n" strcat strcat
    | 0
    ]]]]]]]
} ;

: g2 {
    dup 0 @nth 0 = ?? "w_dup();\n"
    | dup 0 @nth 1 = ?? "w_swap();\n"
    | dup 0 @nth 2 = ?? "w_drop();\n"
    | dup 0 @nth 3 = ?? "w_rot();\n"
    | dup 0 @nth 97 = ?? "return;\n"
    | 0
    ]]]]]
} ;

: g3 {
    dup 0 @nth 16 = ?? "{wval _b=w_pop(),_a=w_pop();w_push((wval)((int64_t)_a+(int64_t)_b));}\n"
    | dup 0 @nth 17 = ?? "{wval _b=w_pop(),_a=w_pop();w_push((wval)((int64_t)_a-(int64_t)_b));}\n"
    | dup 0 @nth 18 = ?? "{wval _b=w_pop(),_a=w_pop();w_push((wval)((int64_t)_a*(int64_t)_b));}\n"
    | dup 0 @nth 19 = ?? "{wval _b=w_pop(),_a=w_pop();if(!(int64_t)_b)exit(1);w_push((wval)((int64_t)_a/(int64_t)_b));}\n"
    | dup 0 @nth 20 = ?? "{wval _b=w_pop(),_a=w_pop();if(!(int64_t)_b)exit(1);w_push((wval)((int64_t)_a%(int64_t)_b));}\n"
    | 0
    ]]]]]
} ;

: g4 {
    dup 0 @nth 24 = ?? "{wval _b=w_pop(),_a=w_pop();w_push((wval)((int64_t)_a==(int64_t)_b?1L:0L));}\n"
    | dup 0 @nth 25 = ?? "{wval _b=w_pop(),_a=w_pop();w_push((wval)((int64_t)_a<(int64_t)_b?1L:0L));}\n"
    | dup 0 @nth 26 = ?? "{wval _b=w_pop(),_a=w_pop();w_push((wval)((int64_t)_a>(int64_t)_b?1L:0L));}\n"
    | dup 0 @nth 27 = ?? "{wval _b=w_pop(),_a=w_pop();w_push((wval)((int64_t)_a!=(int64_t)_b?1L:0L));}\n"
    | dup 0 @nth 28 = ?? "{wval _b=w_pop(),_a=w_pop();w_push((wval)((int64_t)_a<=(int64_t)_b?1L:0L));}\n"
    | dup 0 @nth 29 = ?? "{wval _b=w_pop(),_a=w_pop();w_push((wval)((int64_t)_a>=(int64_t)_b?1L:0L));}\n"
    | dup 0 @nth 32 = ?? "{wval _b=w_pop(),_a=w_pop();w_push((wval)((int64_t)_a&&(int64_t)_b?1L:0L));}\n"
    | dup 0 @nth 33 = ?? "{wval _b=w_pop(),_a=w_pop();w_push((wval)((int64_t)_a||(int64_t)_b?1L:0L));}\n"
    | dup 0 @nth 34 = ?? "{wval _a=w_pop();w_push((wval)((int64_t)_a==0?1L:0L));}\n"
    | 0
    ]]]]]]]]]
} ;

: g5 {
    dup 0 @nth 64 = ?? "{int64_t _i=(int64_t)w_pop();wval*_l=(wval*)w_pop();w_push(w_list_nth(_l,_i));}\n"
    | dup 0 @nth 65 = ?? "{wval _e=w_pop();wval*_l=(wval*)w_pop();w_push((wval)w_list_append(_l,_e));}\n"
    | dup 0 @nth 66 = ?? "{wval*_l=(wval*)w_pop();w_push((wval)w_list_len(_l));}\n"
    | dup 0 @nth 70 = ?? "{const char*_s=(const char*)w_pop();w_push((wval)(int64_t)w_str_len(_s));}\n"
    | dup 0 @nth 71 = ?? "{const char*_b=(const char*)w_pop();const char*_a=(const char*)w_pop();w_push((wval)w_str_concat(_a,_b));}\n"
    | dup 0 @nth 73 = ?? "{const char*_b=(const char*)w_pop();const char*_a=(const char*)w_pop();w_push((wval)w_str_eq(_a,_b));}\n"
    | dup 0 @nth 75 = ?? "{const char*_n=(const char*)w_pop();const char*_h=(const char*)w_pop();w_push((wval)w_str_find(_h,_n));}\n"
    | dup 0 @nth 77 = ?? "{const char*_s=(const char*)w_pop();w_push((wval)w_str_to_i64(_s));}\n"
    | dup 0 @nth 78 = ?? "{int64_t _n=(int64_t)w_pop();w_push((wval)w_i64_to_str(_n));}\n"
    | 0
    ]]]]]]]]]
} ;

: g6 {
    dup 0 @nth 144 = ?? "w_print_val(w_pop());\n"
    | dup 0 @nth 146 = ?? "w_push((wval)w_read_line());\n"
    | dup 0 @nth 176 = ?? "{int64_t _n=(int64_t)w_pop();double _f=(double)_n;w_push((wval)*(uint64_t*)&_f);}\n"
    | dup 0 @nth 177 = ?? "{uint64_t _b=(uint64_t)w_pop();double _f=*(double*)&_b;w_push((wval)(int64_t)_f);}\n"
    | dup 0 @nth 178 = ?? "{uint64_t _b=(uint64_t)w_pop();double _f=*(double*)&_b;_f=w_fsqrt(_f);w_push((wval)*(uint64_t*)&_f);}\n"
    | dup 0 @nth 179 = ?? "{uint64_t _b=(uint64_t)w_pop();double _f=*(double*)&_b;_f=w_fsin(_f);w_push((wval)*(uint64_t*)&_f);}\n"
    | dup 0 @nth 180 = ?? "{uint64_t _b=(uint64_t)w_pop();double _f=*(double*)&_b;_f=w_fcos(_f);w_push((wval)*(uint64_t*)&_f);}\n"
    | dup 0 @nth 181 = ?? "{uint64_t _b=(uint64_t)w_pop();double _f=*(double*)&_b;_f=w_ftan(_f);w_push((wval)*(uint64_t*)&_f);}\n"
    | 0
    ]]]]]]]]
} ;

: emit-one {
    g1 dup 0 = ?? drop
        g2 dup 0 = ?? drop
            g3 dup 0 = ?? drop
                g4 dup 0 = ?? drop
                    g5 dup 0 = ?? drop
                        g6 dup 0 = ?? drop
                            "// unknown\n"
                        | swap drop ]
                    | swap drop ]
                | swap drop ]
            | swap drop ]
        | swap drop ]
    | swap drop ]
} ;

: scn-impl {
    over over dup len >= ?? drop drop ]
    over over @nth
    dup dup 48 >= swap 57 <= & ??
        drop drop ctos swap swap append swap 1 + swap scn-impl
    | dup dup 65 >= swap 90 <= & ??
        drop drop ctos swap swap append swap 1 + swap scn-impl
    | dup dup 97 >= swap 122 <= & ??
        drop drop ctos swap swap append swap 1 + swap scn-impl
    | dup 95 = ??
        drop drop ctos swap swap append swap 1 + swap scn-impl
    |
        drop drop "_" swap swap append swap 1 + swap scn-impl
    ]]]]
} ;

: safe-c-name { dup 0 "" scn-impl } ;

: compile-to-c {
    { emit-one } @map
    "" { strcat } @fold
} ;

export emit-one
export compile-to-c
export safe-c-name
