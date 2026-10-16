# cyclomatic 8, cognitive 8
def py_decisions(a, b, xs):
    r = 0
    if a > 0 and b > 0:
        r += 1
    elif a < 0 or b < 0:
        r -= 1
    else:
        r = 2
    for x in xs:
        r += x
    while r > 100:
        r -= 1
    try:
        r = r / a
    except ZeroDivisionError:
        r = 0
    return r


# cyclomatic 4, cognitive 6, nesting 3
def py_nested(a, b, c):
    if a:
        if b:
            if c:
                return 1
    return 0


class K:
    # cyclomatic 2, cognitive 1
    def method(self, a):
        if a:
            return 1
        return 0
