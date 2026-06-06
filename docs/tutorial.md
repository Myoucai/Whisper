# Whisper Tutorial

## Hello World

```whisper
"Hello, World!" .
```

Only 2 tokens. Compare with Python's 6 tokens: `print("Hello, World!")`.

## Arithmetic (Postfix)

```
3 4 + .        # 7
10 3 - .       # 7
5 6 * .        # 30
```

## Conditionals

```
42 100 > ??BIG|SMALL] .     # "SMALL"
0 5 - 0 < ??NEGATIVE|POSITIVE] .   # "NEGATIVE"
```

## Word Definitions

```
: sq { _ * } ;
: cube { _ sq * } ;

5 sq .      # 25
3 cube .    # 27
```

## Fibonacci

```
: fib {
    _ 1 >
    ??_ 1 - fib _ 2 - fib +
    |_
    ]
} ;

10 fib .    # 55
```

## List Operations

```
[1 2 3 4 5] { sq } @map .     # [1 4 9 16 25]
[1 2 3 4 5] 0 { + } @fold .   # 15
```

## Confidence

```
{ "cat" } :0.93       # Value "cat" with 93% confidence
{ "dog" } { "cat" } ?|  # Probabilistic choice
```
