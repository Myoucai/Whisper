// expect: 97|10|1|1|1|99|20
// S-4：std/str 与 std/list 曾经有 first/last/empty?/contains? 四对同名导出，
// 同时 import 时 C 链报 redefinition。字符串侧改名后两者可共存。
import "std/str"
import "std/list"
"abc" first-char 0 strnth .
[10 20] first .
"" str-empty? ?? 1 | 0 ] .
[] empty? ?? 1 | 0 ] .
"abc" "b" str-contains? ?? 1 | 0 ] .
"abc" last-char 0 strnth .
[10 20] last .
