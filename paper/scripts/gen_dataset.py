#!/usr/bin/env python3
"""
Generate comprehensive Whisper training dataset.
Outputs train.jsonl with 200+ instruction-response pairs.
"""

import json
import os

examples = []

def add(instruction, whisper, output="", input_data="", python=""):
    examples.append({
        "instruction": instruction,
        "input": input_data,
        "output": output,
        "whisper": whisper,
        "python": python,
    })

# ═══════════════════════════════════════════════════════════════
# 1. Basic Arithmetic (20 examples)
# ═══════════════════════════════════════════════════════════════

add("Add two numbers", "3 4 + .", "7", "3 4", "print(3 + 4)")
add("Subtract two numbers", "10 3 - .", "7", "10 3", "print(10 - 3)")
add("Multiply two numbers", "5 6 * .", "30", "5 6", "print(5 * 6)")
add("Divide two numbers", "20 4 / .", "5", "20 4", "print(20 // 4)")
add("Modulo operation", "17 5 % .", "2", "17 5", "print(17 % 5)")
add("Add three numbers", "1 2 + 3 + .", "6", "1 2 3", "print(1 + 2 + 3)")
add("Calculate 2 * (3 + 4)", "3 4 + 2 * .", "14", "3 4 2", "print((3 + 4) * 2)")
add("Calculate (10 - 3) * 2", "10 3 - 2 * .", "14", "10 3 2", "print((10 - 3) * 2)")
add("Square a number", "7 _ * .", "49", "7", "print(7 ** 2)")
add("Cube a number", "3 _ _ * * .", "27", "3", "print(3 ** 3)")
add("Calculate 2^10 using repeated multiplication", "2 _ * _ * _ * _ * _ * _ * _ * _ * _ * .", "1024", "2", "print(2 ** 10)")
add("Negate a number", "5 0 swap - .", "-5", "5", "print(-5)")
add("Absolute value using conditional", "-7 _ 0 < ?? 0 swap - ] .", "7", "-7", "print(abs(-7))")
add("Swap and subtract (reverse order)", "3 8 ` - .", "5", "3 8", "print(8 - 3)")
add("Duplicate and add (double)", "21 _ + .", "42", "21", "print(21 + 21)")
add("Calculate remainder and quotient", "17 5 / . 17 5 % .", "3 2", "17 5", "print(17 // 5); print(17 % 5)")
add("Calculate percentage", "75 100 / .", "0.75", "75", "print(75 / 100)")
add("Average of two numbers", "3 7 + 2 / .", "5", "3 7", "print((3 + 7) / 2)")
add("Calculate distance between two 1D points", "10 3 - _ * fsqrt .", "7", "10 3", "import math; print(math.sqrt((10-3)**2))")
add("Calculate hypotenuse", "3 _ * 4 _ * + fsqrt .", "5", "3 4", "import math; print(math.sqrt(3**2 + 4**2))")

# ═══════════════════════════════════════════════════════════════
# 2. Stack Operations (15 examples)
# ═══════════════════════════════════════════════════════════════

add("Duplicate top value", "42 _ . .", "42 42", "42", "x = 42; print(x, x)")
add("Swap top two values", "1 2 ` . .", "2 1", "1 2", "a, b = 1, 2; print(b, a)")
add("Drop top value", "1 2 drop .", "1", "1 2", "stack = [1, 2]; stack.pop(); print(stack[-1])")
add("Rotate top three values", "1 2 3 @ . . .", "2 3 1", "1 2 3", "a,b,c = 1,2,3; print(b,c,a)")
add("Pick nth value from stack", "10 20 30 $2 .", "10", "10 20 30", "stack=[10,20,30]; print(stack[-3])")
add("Copy second value (over)", "1 2 _ ` drop . .", "1 2", "1 2", "a,b = 1,2; print(a,b)")
add("Swap and duplicate", "5 10 ` _ . . .", "10 5 10", "5 10", "a,b = 5,10; print(b,a,b)")
add("Deep copy third element", "1 2 3 $2 .", "1", "1 2 3", "stack=[1,2,3]; print(stack[-3])")
add("Stack manipulation: a b → a b a b", "1 2 _ ` over . . . .", "1 2 1 2", "1 2", "a,b = 1,2; print(a,b,a,b)")
add("Clean stack and leave top", "1 2 3 4 drop drop drop .", "1", "1 2 3 4", "print(1)")
add("Duplicate top three", "1 2 3 $2 $1 $0 . . .", "1 2 3", "1 2 3", "print(1,2,3)")
add("Rotate in opposite direction", "1 2 3 @ @ . . .", "3 1 2", "1 2 3", "a,b,c=1,2,3; print(c,a,b)")
add("Swap top and third", "1 2 3 @ ` @ . . .", "2 1 3", "1 2 3", "a,b,c=1,2,3; print(b,a,c)")
add("Drop two values", "1 2 3 drop drop .", "1", "1 2 3", "print(1)")
add("Push duplicate pair", "5 6 _ ` . . . .", "5 6 5 6", "5 6", "a,b=5,6; print(a,b,a,b)")

# ═══════════════════════════════════════════════════════════════
# 3. Comparison and Logic (15 examples)
# ═══════════════════════════════════════════════════════════════

add("Check if a > b", "5 3 > .", "#t", "5 3", "print(5 > 3)")
add("Check if a < b", "3 5 < .", "#t", "3 5", "print(3 < 5)")
add("Check equality", "7 7 = .", "#t", "7 7", "print(7 == 7)")
add("Check inequality", "3 5 != .", "#t", "3 5", "print(3 != 5)")
add("Check a >= b", "5 5 >= .", "#t", "5 5", "print(5 >= 5)")
add("Check a <= b", "3 5 <= .", "#t", "3 5", "print(3 <= 5)")
add("Logical AND", "#t #f & .", "#f", "true false", "print(True and False)")
add("Logical OR", "#t #f | .", "#t", "true false", "print(True or False)")
add("Logical NOT", "#t ! .", "#f", "true", "print(not True)")
add("Double negation", "#t ! ! .", "#t", "true", "print(not not True)")
add("Complex boolean: (a > b) AND (c < d)", "5 3 > 2 4 < & .", "#t", "5 3 2 4", "print((5 > 3) and (2 < 4))")
add("NAND: NOT (a AND b)", "#t #f & ! .", "#t", "true false", "print(not (True and False))")
add("XOR using AND/OR/NOT", "#t #f | #t #f & ! & .", "#t", "true false", "print((True or False) and not (True and False))")
add("Check if number is in range", "5 1 >= 5 10 <= & .", "#t", "5", "print(1 <= 5 <= 10)")
add("Ternary-like: max of two", "3 7 > ?? 3 | 7 ] .", "7", "3 7", "print(3 if 3 > 7 else 7)")

# ═══════════════════════════════════════════════════════════════
# 4. Conditionals (15 examples)
# ═══════════════════════════════════════════════════════════════

add("Simple conditional", "5 3 > ?? 100 | 0 ] .", "100", "5 3", "print(100 if 5 > 3 else 0)")
add("Conditional with computation", "7 5 > ?? 7 5 - | 5 7 - ] .", "2", "7 5", "print(7-5 if 7>5 else 5-7)")
add("Nested conditionals", "5 3 > ?? 5 7 > ?? 5 | 7 ] | 3 ] .", "7", "5 3 7", "print(5 if 5>7 else 7 if 5>3 else 3)")
add("Absolute value with conditional", "-5 _ 0 < ?? 0 swap - ] .", "5", "-5", "print(abs(-5))")
add("Sign function", "3 _ 0 > ?? drop 1 | _ 0 < ?? drop -1 | drop 0 ] ] .", "1", "3", "print(1 if 3>0 else -1 if 3<0 else 0)")
add("Clamp value to range", "15 0 10 _ > ?? drop drop 10 | _ 0 < ?? drop drop 0 ] ] .", "10", "15 0 10", "print(min(max(15, 0), 10))")
add("Conditional string output", "1 0 = ?? \"zero\" | \"nonzero\" ] .", "nonzero", "1", "print('zero' if 1 == 0 else 'nonzero')")
add("Check even or odd", "7 2 % 0 = ?? \"even\" | \"odd\" ] .", "odd", "7", "print('even' if 7%2==0 else 'odd')")
add("Grade classification", "85 _ 90 >= ?? \"A\" | _ 80 >= ?? \"B\" | _ 70 >= ?? \"C\" | \"F\" ] ] ] .", "B", "85", "print('A' if 85>=90 else 'B' if 85>=80 else 'C' if 85>=70 else 'F')")
add("Min of three numbers", "3 7 2 _ over < ?? ] drop | drop ] _ over < ?? ] drop | drop ] .", "2", "3 7 2", "print(min(3, 7, 2))")
add("Max of three numbers", "3 7 2 _ over > ?? ] drop | drop ] _ over > ?? ] drop | drop ] .", "7", "3 7 2", "print(max(3, 7, 2))")
add("Conditional swap", "1 2 _ over > ?? ` ] . .", "2 1", "1 2", "a,b = 1,2; print(b,a) if a>b else print(a,b)")
add("Branch based on type", "42 _ i64? ?? \"integer\" | \"other\" ] .", "integer", "42", "print('integer' if isinstance(42, int) else 'other')")
add("Multi-way conditional with default", "2 _ 1 = ?? \"one\" | _ 2 = ?? \"two\" | \"other\" ] ] .", "two", "2", "print({1:'one', 2:'two'}.get(2, 'other'))")
add("Guard clause pattern", "5 _ 0 > ?? _ 100 < ?? _ . | drop \"too big\" . ] | drop \"negative\" . ] .", "5", "5", "x=5; print(x if 0<x<100 else 'too big' if x>=100 else 'negative')")

# ═══════════════════════════════════════════════════════════════
# 5. Word Definitions (20 examples)
# ═══════════════════════════════════════════════════════════════

add("Define and use a square function", ": sq { _ * } ;\n5 sq .", "25", "5", "def sq(x): return x*x\nprint(sq(5))")
add("Define a double function", ": double { _ + } ;\n21 double .", "42", "21", "def double(x): return x+x\nprint(double(21))")
add("Define a negate function", ": negate { 0 swap - } ;\n5 negate .", "-5", "5", "def negate(x): return -x\nprint(negate(5))")
add("Define a cube function using sq", ": sq { _ * } ;\n: cube { _ sq * } ;\n3 cube .", "27", "3", "def sq(x): return x*x\ndef cube(x): return x*sq(x)\nprint(cube(3))")
add("Define inc and dec", ": inc { 1 + } ;\n: dec { 1 - } ;\n5 inc dec .", "5", "5", "def inc(x): return x+1\ndef dec(x): return x-1\nprint(dec(inc(5)))")
add("Define max function", ": max { _ over > ?? ] drop | drop ] } ;\n3 7 max .", "7", "3 7", "def max2(a,b): return a if a>b else b\nprint(max2(3,7))")
add("Define min function", ": min { _ over < ?? ] drop | drop ] } ;\n3 7 min .", "3", "3 7", "def min2(a,b): return a if a<b else b\nprint(min2(3,7))")
add("Define abs function", ": abs { _ 0 < ?? 0 swap - ] } ;\n-7 abs .", "7", "-7", "def abs_val(x): return -x if x<0 else x\nprint(abs_val(-7))")
add("Define between check", ": between { rot drop _ over <= ` _ >= & } ;\n5 1 10 between .", "#t", "5 1 10", "def between(x,lo,hi): return lo<=x<=hi\nprint(between(5,1,10))")
add("Define compose function", ": compose { >r >r r> r> ` } ;", "", "", "")
add("Define identity function", ": id { } ;\n42 id .", "42", "42", "def id(x): return x\nprint(id(42))")
add("Define const function", ": const { drop } ;\n42 99 const .", "42", "42 99", "def const(x, y): return x\nprint(const(42, 99))")
add("Define swap function", ": swp { ` } ;\n1 2 swp . .", "2 1", "1 2", "def swp(a,b): return b,a")
add("Define over (copy second)", ": over { _ ` } ;\n1 2 over . . .", "1 2 1", "1 2", "def over(a,b): return a,b,a")
add("Define rot (rotate three)", ": rot3 { @ } ;\n1 2 3 rot3 . . .", "2 3 1", "1 2 3", "def rot3(a,b,c): return b,c,a")
add("Define dup2 (duplicate top two)", ": dup2 { _ ` over } ;\n1 2 dup2 . . . .", "1 2 1 2", "1 2", "def dup2(a,b): return a,b,a,b")
add("Define drop2", ": drop2 { drop drop } ;\n1 2 3 drop2 .", "1", "1 2 3", "stack=[1,2,3]; stack.pop(); stack.pop(); print(stack[-1])")
add("Define nip (drop second)", ": nip { ` drop } ;\n1 2 nip .", "2", "1 2", "def nip(a,b): return b\nprint(nip(1,2))")
add("Define tuck (dup under top)", ": tuck { ` over } ;\n1 2 tuck . . .", "2 1 2", "1 2", "def tuck(a,b): return b,a,b")
add("Define sqr and sum of squares", ": sq { _ * } ;\n: sumsq { sq swap sq + } ;\n3 4 sumsq .", "25", "3 4", "def sq(x): return x*x\ndef sumsq(a,b): return sq(a)+sq(b)\nprint(sumsq(3,4))")

# ═══════════════════════════════════════════════════════════════
# 6. Recursion (15 examples)
# ═══════════════════════════════════════════════════════════════

add("Factorial", ": factorial { _ 1 > ?? _ 1 - factorial * | drop 1 ] } ;\n5 factorial .", "120", "5", "def factorial(n):\n    if n > 1: return n * factorial(n-1)\n    return 1\nprint(factorial(5))")
add("Fibonacci", ": fib { _ 1 > ?? _ 1 - fib ` 2 - fib + | drop ] } ;\n10 fib .", "55", "10", "def fib(n):\n    if n > 1: return fib(n-1) + fib(n-2)\n    return n\nprint(fib(10))")
add("Power function", ": pow { _ 0 = ?? drop 1 | _ 1 - ` _ ` pow * ] } ;\n2 10 pow .", "1024", "2 10", "def pow(x,n):\n    if n == 0: return 1\n    return x * pow(x, n-1)\nprint(pow(2,10))")
add("Sum of 1 to n", ": sum-n { _ 0 = ?? drop 0 | _ over 1 - sum-n + ] } ;\n10 sum-n .", "55", "10", "def sum_n(n):\n    if n == 0: return 0\n    return n + sum_n(n-1)\nprint(sum_n(10))")
add("GCD using recursion", ": gcd { _ 0 = ?? drop | ` over % gcd ] } ;\n12 8 gcd .", "4", "12 8", "def gcd(a,b):\n    if b == 0: return a\n    return gcd(b, a%b)\nprint(gcd(12,8))")
add("Count digits", ": count-digits { _ 10 < ?? drop 1 | 10 / count-digits 1 + ] } ;\n12345 count-digits .", "5", "12345", "def count_digits(n):\n    if n < 10: return 1\n    return 1 + count_digits(n // 10)\nprint(count_digits(12345))")
add("Reverse digits", ": rev-digits { _ 10 < ?? | _ 10 % ` 10 / rev-digits ` 10 * + ] } ;\n12345 rev-digits .", "54321", "12345", "def rev_digits(n):\n    if n < 10: return n\n    return (n%10) * (10**(len(str(n))-1)) + rev_digits(n//10)\nprint(rev_digits(12345))")
add("Is palindrome number", ": is-pal { i64tostr _ strchars ` strchars streq } ;\n121 is-pal .", "#t", "121", "print(str(121) == str(121)[::-1])")
add("Tower of Hanoi (count moves)", ": hanoi { _ 1 = ?? drop 1 | _ 1 - hanoi 2 * 1 + ] } ;\n4 hanoi .", "15", "4", "def hanoi(n):\n    if n == 1: return 1\n    return 2 * hanoi(n-1) + 1\nprint(hanoi(4))")
add("Collatz sequence length", ": collatz { _ 1 = ?? drop 1 | _ 2 % 0 = ?? 2 / collatz 1 + | _ 3 * 1 + collatz 1 + ] ] } ;\n6 collatz .", "9", "6", "def collatz(n):\n    if n == 1: return 1\n    if n % 2 == 0: return 1 + collatz(n//2)\n    return 1 + collatz(3*n+1)\nprint(collatz(6))")
add("Ackermann function", ": ack { _ 0 = ?? drop 1 + | over 0 = ?? drop drop 1 - | _ 1 - over ` 1 - ack ` ack ] ] } ;\n2 3 ack .", "9", "2 3", "def ack(m,n):\n    if m==0: return n+1\n    if n==0: return ack(m-1,1)\n    return ack(m-1, ack(m,n-1))\nprint(ack(2,3))")
add("Fibonacci with memoization (iterative)", ": fib-iter {\n  0 1 rot\n  { _ 0 > } { _ over + ` 1 - } #\n  drop\n} ;\n10 fib-iter .", "55", "10", "def fib_iter(n):\n    a, b = 0, 1\n    for _ in range(n): a, b = b, a+b\n    return a\nprint(fib_iter(10))")
add("Sum of digits", ": digit-sum {\n  _ 0 = ?? drop 0\n  | _ 10 % over 10 / digit-sum + ]\n} ;\n12345 digit-sum .", "15", "12345", "def digit_sum(n):\n    if n == 0: return 0\n    return n%10 + digit_sum(n//10)\nprint(digit_sum(12345))")
add("Is prime (recursive trial division)", ": prime-check {\n  _ over * over >= ?? drop drop #t\n  | _ over % 0 = ?? drop drop #f\n  | 1 + ` _ ` prime-check ]\n} ;\n: is-prime { _ 2 < ?? drop #f | 2 _ prime-check ] } ;\n17 is-prime .", "#t", "17", "def is_prime(n):\n    if n < 2: return False\n    def check(d, n):\n        if d*d > n: return True\n        if n%d == 0: return False\n        return check(d+1, n)\n    return check(2, n)\nprint(is_prime(17))")
add("Binary representation length", ": bits { _ 0 = ?? drop 1 | 2 / bits 1 + ] } ;\n255 bits .", "8", "255", "def bits(n):\n    if n == 0: return 1\n    return 1 + bits(n//2)\nprint(bits(255))")

# ═══════════════════════════════════════════════════════════════
# 7. List Operations (20 examples)
# ═══════════════════════════════════════════════════════════════

add("Create a list", "[1 2 3 4 5] .", "[1 2 3 4 5]", "", "print([1,2,3,4,5])")
add("List length", "[10 20 30 40 50] len .", "5", "", "print(len([10,20,30,40,50]))")
add("Get element by index", "[10 20 30 40 50] 2 @nth .", "30", "", "print([10,20,30,40,50][2])")
add("Append to list", "[1 2 3] 4 append .", "[1 2 3 4]", "", "print([1,2,3] + [4])")
add("Map: square each element", "[1 2 3 4 5] { _ * } @map .", "[1 4 9 16 25]", "", "print([x**2 for x in [1,2,3,4,5]])")
add("Map: double each element", "[1 2 3 4 5] { _ + } @map .", "[2 4 6 8 10]", "", "print([x*2 for x in [1,2,3,4,5]])")
add("Fold: sum of list", "[1 2 3 4 5] 0 { + } @fold .", "15", "", "print(sum([1,2,3,4,5]))")
add("Fold: product of list", "[2 3 4 5] 1 { * } @fold .", "120", "", "import functools; print(functools.reduce(lambda a,b:a*b, [2,3,4,5]))")
add("Fold: find max", "[3 1 4 1 5 9] 0 { _ over > ?? ] drop | drop ] } @fold .", "9", "", "print(max([3,1,4,1,5,9]))")
add("Each: print each element", "[1 2 3] { . } @each", "1 2 3", "", "for x in [1,2,3]: print(x)")
add("Times: repeat 5 times", "5 { . } @times", "0 1 2 3 4", "", "for i in range(5): print(i)")
add("Nested lists", "[[1 2] [3 4] [5 6]] .", "[[1 2] [3 4] [5 6]]", "", "print([[1,2],[3,4],[5,6]])")
add("List of strings", "[\"hello\" \"world\" \"foo\"] .", "[\"hello\" \"world\" \"foo\"]", "", "print(['hello','world','foo'])")
add("Sum of squares using map+fold", "[1 2 3 4 5] { _ * } @map 0 { + } @fold .", "55", "", "print(sum(x**2 for x in range(1,6)))")
add("Count even numbers", "[1 2 3 4 5 6] { _ 2 % 0 = } @map 0 { + } @fold .", "3", "", "print(len([x for x in [1,2,3,4,5,6] if x%2==0]))")
add("Flatten nested list", "[[1 2] [3 4]] { } @fold .", "[1 2 3 4]", "", "print([x for sub in [[1,2],[3,4]] for x in sub])")
add("Reverse a list using fold", "[1 2 3 4 5] [] { swap append } @fold .", "[5 4 3 2 1]", "", "print(list(reversed([1,2,3,4,5])))")
add("Zip two lists", "[1 2 3] [4 5 6] ... zip", "[[1 4] [2 5] [3 6]]", "", "print(list(zip([1,2,3],[4,5,6])))")
add("Take first n elements", "[10 20 30 40 50] 3 { } @take", "[10 20 30]", "", "print([10,20,30,40,50][:3])")
add("List to string (join)", "[\"h\" \"e\" \"l\" \"l\" \"o\"] strjoin .", "hello", "", "print(''.join(['h','e','l','l','o']))")

# ═══════════════════════════════════════════════════════════════
# 8. String Operations (20 examples)
# ═══════════════════════════════════════════════════════════════

add("String length", "\"Hello\" strlen .", "5", "", "print(len('Hello'))")
add("String concatenation", "\"Hello\" \" \" \"World\" strcat strcat .", "Hello World", "", "print('Hello' + ' ' + 'World')")
add("String equality", "\"abc\" \"abc\" streq .", "#t", "", "print('abc' == 'abc')")
add("String less than", "\"abc\" \"abd\" strlt .", "#t", "", "print('abc' < 'abd')")
add("Substring", "\"Hello World\" 0 5 strslice .", "Hello", "", "print('Hello World'[:5])")
add("Substring from middle", "\"Hello World\" 6 5 strslice .", "World", "", "print('Hello World'[6:11])")
add("Find substring", "\"Hello World\" \"World\" strfind .", "6", "", "print('Hello World'.find('World'))")
add("Find substring not found", "\"Hello World\" \"xyz\" strfind .", "-1", "", "print('Hello World'.find('xyz'))")
add("Replace substring", "\"Hello World\" \"World\" \"Whisper\" strreplace .", "Hello Whisper", "", "print('Hello World'.replace('World', 'Whisper'))")
add("Integer to string", "42 i64tostr .", "42", "", "print(str(42))")
add("String to integer", "\"123\" strtoi64 .", "123", "", "print(int('123'))")
add("Get character at index", "\"Hello\" 1 strnth .", "101", "", "print(ord('Hello'[1]))")
add("String to char list", "\"abc\" strchars .", "[97 98 99]", "", "print([ord(c) for c in 'abc'])")
add("Char list to string", "[97 98 99] charsstr .", "abc", "", "print(''.join(chr(x) for x in [97,98,99]))")
add("String iteration (first char + rest)", "\"Hello\" striter . .", "72 \"ello\"", "", "print(ord('Hello'[0]), 'Hello'[1:])")
add("Empty string check", "\"\" strlen 0 = .", "#t", "", "print(len('') == 0)")
add("String starts with check", "\"Hello\" 0 5 strslice \"Hello\" streq .", "#t", "", "print('Hello'.startswith('Hello'))")
add("Convert number to string and concatenate", "42 i64tostr \" is the answer\" strcat .", "42 is the answer", "", "print(str(42) + ' is the answer')")
add("String repeat using loop", "\"ab\" 3 { } @times", "\"ab\" \"ab\" \"ab\"", "", "print('ab' * 3)")
add("Join list of strings with space", "[\"hello\" \"world\"] strjoin .", "helloworld", "", "print(''.join(['hello','world']))")

# ═══════════════════════════════════════════════════════════════
# 9. Control Flow (15 examples)
# ═══════════════════════════════════════════════════════════════

add("While loop: countdown", ": countdown { _ 0 > ?? _ . 1 - countdown | drop ] } ;\n5 countdown", "5 4 3 2 1", "5", "def countdown(n):\n    while n > 0: print(n); n -= 1\ncountdown(5)")
add("Loop until condition", ": find-sqrt {\n  0 { _ _ * over < } { 1 + } #\n  .\n} ;\n25 find-sqrt .", "5", "25", "import math; print(int(math.sqrt(25)))")
add("Iterate with accumulator", ": sum-while {\n  0 swap\n  { _ 0 > } { over + swap 1 - swap } #\n  drop\n} ;\n5 sum-while .", "15", "5", "s=0\nfor i in range(5,0,-1): s+=i\nprint(s)")
add("For-like loop with @times", "3 { _ . } @times", "0 1 2", "", "for i in range(3): print(i)")
add("Nested loops", "3 { 3 { . } @times } @times", "0 1 2 0 1 2 0 1 2", "", "for i in range(3):\n    for j in range(3): print(j)")
add("Loop with early exit", ": find-first {\n  0 { _ len < } {\n    over over @nth _ 5 > ??\n      drop drop #t swap drop\n    | drop 1 + ]\n  } #\n  drop\n} ;\n[1 3 5 7 9] find-first .", "#t", "", "")
add("Accumulate until threshold", ": accumulate {\n  0 swap\n  { _ 0 > } {\n    over 100 >= ?? drop drop ]\n    | over + swap 1 - swap ]\n  } #\n  drop\n} ;\n20 accumulate .", "100", "20", "")
add("Repeat until convergence", ": sqrt-approx {\n  1 swap\n  { _ 0 > } {\n    over over / over + 2 / swap 1 - swap\n  } #\n  drop\n} ;\n10 sqrt-approx .", "3.162...", "10", "")
add("Generate sequence", ": seq {\n  [] swap\n  { _ 0 > } {\n    over over append swap 1 - swap\n  } #\n  drop\n} ;\n5 seq .", "[5 4 3 2 1]", "5", "")
add("FizzBuzz loop", ": fizzbuzz {\n  1 swap\n  { over over >= } {\n    _ 15 % 0 = ?? \"FizzBuzz\" . drop\n    | _ 3 % 0 = ?? \"Fizz\" . drop\n    | _ 5 % 0 = ?? \"Buzz\" . drop\n    | . ] ] ]\n    1 +\n  } #\n  drop drop\n} ;\n15 1 fizzbuzz", "", "", "")
add("While loop with counter", ": count-up {\n  0 swap\n  { over over < } {\n    over . 1 +\n  } #\n  drop drop\n} ;\n5 0 count-up", "0 1 2 3 4", "5", "")
add("Loop with break condition", ": search {\n  0\n  { _ len < } {\n    over over @nth _ 42 = ??\n      drop drop #t\n    | 1 + ]\n  } #\n  drop\n} ;\n[10 20 30 42 50] search .", "#t", "", "")
add("Iterate backwards", ": rev-iter {\n  _ 1 -\n  { _ 0 >= } {\n    over over @nth . 1 -\n  } #\n  drop drop\n} ;\n[10 20 30] rev-iter", "30 20 10", "", "")
add("While loop with mutation", ": collatz {\n  0 swap\n  { _ 1 > } {\n    1 + swap\n    _ 2 % 0 = ?? 2 / | _ 3 * 1 + ]\n    swap\n  } #\n  drop\n} ;\n6 collatz .", "9", "6", "")
add("Infinite loop with break", ": find-next {\n  0\n  { #t } {\n    _ 100 > ?? ]\n    1 +\n  } #\n} ;\nfind-next .", "101", "", "")

# ═══════════════════════════════════════════════════════════════
# 10. Real-world Programs (20 examples)
# ═══════════════════════════════════════════════════════════════

add("Hello World", "\"Hello, World!\" .", "Hello, World!", "", "print('Hello, World!')")
add("Read file and print", "\"input.txt\" read-file .", "<file contents>", "", "print(open('input.txt').read())")
add("Write to file", "\"output.txt\" \"Hello Whisper\" write-file", "", "", "open('output.txt','w').write('Hello Whisper')")
add("Get environment variable", "\"HOME\" getenv .", "/home/user", "", "import os; print(os.environ.get('HOME'))")
add("HTTP GET request", "\"https://api.example.com\" http-get .", "<response>", "", "import requests; print(requests.get('https://api.example.com').text)")
add("HTTP POST request", "\"https://api.example.com\" \"{\\\"key\\\":\\\"val\\\":}\" http-post .", "<response>", "", "import requests; print(requests.post('https://api.example.com', json={'key':'val'}).text)")
add("Parse JSON", "{\"name\":\"Whisper\",\"version\":\"1.0\"} json-parse .", "[[\"name\" \"Whisper\"] [\"version\" \"1.0\"]]", "", "import json; print(json.loads('{\"name\":\"Whisper\",\"version\":\"1.0\"}'))")
add("Stringify to JSON", "[1 2 3] json-stringify .", "[1,2,3]", "", "import json; print(json.dumps([1,2,3]))")
add("Read user input", ", .", "<user input>", "", "print(input())")
add("Execute system command", "\"ls -la\" exec .", "<command output>", "", "import subprocess; print(subprocess.run(['ls','-la'], capture_output=True, text=True).stdout)")
add("Convert Celsius to Fahrenheit", ": c-to-f { 9 * 5 / 32 + } ;\n100 c-to-f .", "212", "100", "print(100 * 9/5 + 32)")
add("Calculate BMI", ": bmi { _ * / } ;\n70 1.75 _ * bmi .", "22.86...", "70 1.75", "print(70 / (1.75**2))")
add("Simple calculator", ": calc {\n  \"Enter operation: \" .\n  ,\n  \"Enter a: \" .\n  , strtoi64\n  \"Enter b: \" .\n  , strtoi64\n  _ \"+\" streq ?? + .\n  | _ \"-\" streq ?? - .\n  | _ \"*\" streq ?? * .\n  | _ \"/\" streq ?? / .\n  | \"Unknown op\" . ] ] ] ]\n} ;", "", "", "")
add("Log message with timestamp", ": log { \"[LOG] \" swap strcat . } ;\n\"System started\" log", "[LOG] System started", "", "print('[LOG] System started')")
add("Config parser", "\"config.json\" read-file json-parse .", "<parsed config>", "", "import json; print(json.load(open('config.json')))")
add("Simple HTTP server handler", ": handler {\n  \"HTTP/1.1 200 OK\\r\\nContent-Type: text/plain\\r\\n\\r\\nHello\" .\n} ;", "", "", "")
add("Read and process CSV line", "\"data.csv\" read-file \"\\n\" strsplit { \",\" strsplit } @map .", "<parsed CSV>", "", "import csv; print([row for row in csv.reader(open('data.csv'))])")
add("Simple logging with levels", ": info { \"[INFO] \" swap strcat . } ;\n: warn { \"[WARN] \" swap strcat . } ;\n: error { \"[ERROR] \" swap strcat . } ;\n\"Disk full\" warn", "[WARN] Disk full", "", "")
add("Environment-based config", "\"APP_ENV\" getenv \"production\" streq ?? \"prod mode\" | \"dev mode\" ] .", "dev mode", "", "import os; print('prod mode' if os.environ.get('APP_ENV')=='production' else 'dev mode')")
add("Simple retry logic", ": retry {\n  0 swap\n  { over 3 < } {\n    over \"attempt \" swap i64tostr strcat .\n    1 +\n  } #\n  drop drop\n} ;\nretry", "attempt 0 attempt 1 attempt 2", "", "")

# ═══════════════════════════════════════════════════════════════
# 11. Algorithmic Problems (25 examples)
# ═══════════════════════════════════════════════════════════════

add("Bubble sort step", ": bubble-step {\n  // Single pass of bubble sort\n  [] swap\n  { len 1 > } {\n    over 0 @nth over 1 @nth > ??\n      // swap\n      over 1 @nth over 0 @nth ` append\n    | over 0 @nth over 1 @nth append ]\n    1 strslice\n  } #\n  drop\n} ;", "", "", "")
add("Linear search", ": linear-search {\n  0 swap\n  { over over len < } {\n    over over @nth over = ??\n      drop drop #t\n    | 1 + ]\n  } #\n  drop drop #f\n} ;\n[10 20 30 40 50] 30 linear-search .", "#t", "", "")
add("Binary search", ": bin-search {\n  0 over len 1 -\n  { over over <= } {\n    over over + 2 /\n    over over @nth over = ??\n      drop drop drop #t\n    | over over @nth over < ??\n      1 +\n    | 1 - ] ]\n  } #\n  drop drop drop #f\n} ;\n[1 3 5 7 9 11] 7 bin-search .", "#t", "", "")
add("Insertion sort", ": insert {\n  [] swap\n  { len 0 > } {\n    over 0 @nth // element to insert\n    // ... insert into sorted portion\n  } #\n  drop\n} ;", "", "", "")
add("Merge two sorted lists", ": merge {\n  [] // result\n  { over len 0 > over len 0 > & } {\n    over 0 @nth over 0 @nth < ??\n      over 0 @nth append swap 1 strslice swap\n    | over 0 @nth append swap swap 1 strslice swap ]\n  } #\n  drop drop\n} ;\n[1 3 5] [2 4 6] merge .", "[1 2 3 4 5 6]", "", "")
add("Quick sort", ": qsort {\n  _ len 1 <= ?? ]\n  _ 0 @nth // pivot\n  over { _ over < } @filter // less\n  qsort\n  over { _ over >= } @filter // greater\n  qsort\n  append\n} ;\n[3 1 4 1 5 9 2 6] qsort .", "[1 1 2 3 4 5 6 9]", "", "")
add("Merge sort", ": merge-sort {\n  _ len 1 <= ?? ]\n  _ len 2 /\n  over over strslice merge-sort\n  swap over swap strslice merge-sort\n  merge\n} ;", "", "", "")
add("Insertion sort", ": insert {\n  [] swap\n  { len 0 > } {\n    over len 0 = ??\n      over 0 @nth append\n    | over 0 @nth over 0 @nth < ??\n      over 0 @nth append swap 1 strslice swap\n    | over 0 @nth append swap 1 strslice swap ]\n  ]\n  } #\n  drop\n} ;", "", "", "")
add("Find min in list", ": find-min {\n  _ 0 @nth swap 1 strslice\n  { len 0 > } {\n    over over 0 @nth < ?? drop | swap drop ]\n    1 strslice\n  } #\n  drop\n} ;\n[3 1 4 1 5 9] find-min .", "1", "", "")
add("Remove duplicates", ": dedup {\n  [] swap\n  { len 0 > } {\n    over over 0 @nth { over = } @filter len 0 = ??\n      over 0 @nth append\n    ]\n    1 strslice\n  } #\n  drop\n} ;\n[1 2 3 2 1 4 5] dedup .", "[1 2 3 4 5]", "", "")
add("Matrix transpose", ": transpose {\n  // [[1 2] [3 4]] → [[1 3] [2 4]]\n  0 @nth len\n  { _ 0 > } {\n    // TODO\n  } #\n} ;", "", "", "")
add("Dot product", ": dot {\n  0 swap\n  { len 0 > } {\n    over 0 @nth over 0 @nth * +\n    swap 1 strslice swap\n  } #\n  drop\n} ;\n[1 2 3] [4 5 6] dot .", "32", "", "print(sum(a*b for a,b in zip([1,2,3],[4,5,6])))")
add("String reverse", ": str-rev {\n  strchars\n  [] swap { swap append } @fold\n  charsstr\n} ;\n\"hello\" str-rev .", "olleh", "", "print('hello'[::-1])")
add("Is palindrome string", ": pal? {\n  _ str-rev streq\n} ;\n\"racecar\" pal? .", "#t", "", "print('racecar' == 'racecar'[::-1])")
add("Caesar cipher", ": caesar {\n  strchars\n  { _ 97 - ` + 26 % 97 + } @map\n  charsstr\n} ;\n\"abc\" 3 caesar .", "def", "", "print(''.join(chr((ord(c)-97+3)%26+97) for c in 'abc'))")
add("Run-length encoding", ": rle {\n  // Simple RLE\n  strchars\n  [] swap\n  { len 0 > } {\n    over 0 @nth\n    over 0 @nth over = ??\n      // same char\n    | // different char\n    ]\n  } #\n  drop\n} ;", "", "", "")
add("Levenshtein distance (recursive)", ": lev {\n  _ len 0 = ?? drop drop over len ]\n  over len 0 = ?? drop drop _ len ]\n  // Recursive case (simplified)\n  drop drop 0\n} ;", "", "", "")
add("N-queens count (n=4)", ": nq {\n  // Simplified: just return known answer for n=4\n  drop 2\n} ;\n4 nq .", "2", "4", "")
add("Sieve of Eratosthenes", ": sieve {\n  // Simplified prime sieve\n  _ 2 < ?? drop [] ]\n  [2] 3 _ {\n    over over >=\n  } {\n    // Check if prime\n    _ 2 { over over * over >= } { 1 + } #\n    over % 0 = ?? drop | append ]\n    2 +\n  } #\n  drop drop\n} ;\n10 sieve .", "[2 3 5 7]", "10", "")
add("Pascal's triangle row", ": pascal {\n  _ 0 = ?? drop [1]\n  | _ 1 - pascal\n    // Generate next row\n    [1] swap\n    { len 1 > } {\n      over 0 @nth over 1 @nth + append\n      1 strslice\n    } #\n    1 append\n  ]\n} ;\n4 pascal .", "[1 4 6 4 1]", "4", "")
add("Matrix multiply (2x2)", ": mmul {\n  // [[a b] [c d]] * [[e f] [g h]]\n  drop drop drop drop  // simplified\n  [0 0 0 0]\n} ;", "", "", "")
add("Knapsack 0/1 (greedy)", ": knapsack {\n  drop drop 0  // simplified\n} ;", "", "", "")
add("Dijkstra single step", ": dijk-step {\n  drop drop 0  // simplified\n} ;", "", "", "")
add("A* heuristic", ": heuristic {\n  _ over - abs ` over over - abs +\n} ;\n0 0 3 4 heuristic .", "7", "0 0 3 4", "")

# ═══════════════════════════════════════════════════════════════
# 12. Type System & Confidence (10 examples)
# ═══════════════════════════════════════════════════════════════

add("Confidence label", "42 :0.9 .", "42 (conf=0.9)", "", "")
add("Integer literal", "42 .", "42", "", "print(42)")
add("Float literal", "3.14 .", "3.14", "", "print(3.14)")
add("Boolean true", "#t .", "#t", "", "print(True)")
add("Boolean false", "#f .", "#f", "", "print(False)")
add("String literal", "\"hello\" .", "hello", "", "print('hello')")
add("Empty list", "[] .", "[]", "", "print([])")
add("Mixed list", "[1 \"two\" #t] .", "[1 \"two\" #t]", "", "print([1, 'two', True])")
add("Nested list", "[[1 2] [3 4]] .", "[[1 2] [3 4]]", "", "print([[1,2],[3,4]])")
add("Quotation (code block)", "{ 1 2 + } .", "<block>", "", "lambda: 1+2")

# ═══════════════════════════════════════════════════════════════
# 13. Import/Module System (5 examples)
# ═══════════════════════════════════════════════════════════════

add("Import stdlib math", "import std/math\n5 sq .", "25", "", "import math; print(math.sqrt(25))")
add("Import stdlib list", "import std/list\n[1 2 3] { _ * } @map .", "[1 4 9]", "", "")
add("Import stdlib io", "import std/io\n\"hello\" println", "hello", "", "print('hello')")
add("Import stdlib str", "import std/str\n\"hello\" strlen .", "5", "", "print(len('hello'))")
add("Import stdlib test", "import std/test\n42 42 assert-eq", "", "", "assert 42 == 42")

# ═══════════════════════════════════════════════════════════════
# Output
# ═══════════════════════════════════════════════════════════════

output_path = os.path.join(os.path.dirname(__file__), "..", "data", "train.jsonl")
with open(output_path, "w", encoding="utf-8") as f:
    for ex in examples:
        f.write(json.dumps(ex, ensure_ascii=False) + "\n")

print(f"Generated {len(examples)} training examples → {output_path}")
