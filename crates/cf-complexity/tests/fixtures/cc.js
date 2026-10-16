// cyclomatic 11, cognitive 10
function jsDecisions(a, b, xs) {
  let r = 0;
  if (a > 0 && b > 0) { r++; }
  else if (a < 0 || b < 0) { r--; }
  else { r = 2; }
  for (const x of xs) { r += x; }
  while (r > 100) { r--; }
  const t = a ? 1 : 2;
  try { r = r / a; } catch (e) { r = 0; }
  switch (r) { case 1: r = 10; break; case 2: r = 20; break; default: r = 0; }
  return r + t;
}

// cyclomatic 4, cognitive 6, nesting 3
function jsNested(a, b, c) {
  if (a) {
    if (b) {
      if (c) {
        return 1;
      }
    }
  }
  return 0;
}

// arrow fn with if: cyclomatic 2
const arrow = (x) => { if (x) { return 1; } return 0; };
