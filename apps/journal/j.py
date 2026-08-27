
import sys
cmd = sys.argv[1]
with open("journal.txt", encoding="utf-8") as f:
    lines = f.read().split("\n")
if cmd == "list":
    for l in lines:
        if l: print(l)
else:
    q = sys.argv[2]
    for l in lines:
        if q in l: print(l)
