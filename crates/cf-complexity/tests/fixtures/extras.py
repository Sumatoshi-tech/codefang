def outer(a, b):
    def inner(c):
        if c:
            return 1
        return 0

    if a:
        pass
    elif b:
        pass
    else:
        if a or b:
            pass
    while a:
        a -= 1
    return inner(a)
