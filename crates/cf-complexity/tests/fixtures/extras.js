function outer(a) {
  function inner(b) {
    if (b) { return 1; }
    return 0;
  }
  const g = function (x) { return x ? 1 : 2; };
  if (a) {
    if (a > 1) { return inner(a); }
  } else {
    if (a < 0) { return g(a); }
  }
  do { a--; } while (a > 0);
  loop: for (const k of [1]) { if (k) { break loop; } }
  return [1].map(x => x > 0 ? 1 : 2).length;
}
