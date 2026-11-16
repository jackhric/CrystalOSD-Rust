import sys
p, old, new = sys.argv[1], sys.argv[2], sys.argv[3]
s = open(p).read()
if s.count(old) < 1: print("NOT FOUND:", old); sys.exit(1)
open(p, 'w').write(s.replace(old, new, 1))
