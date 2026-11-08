defmodule Box do
 defp private_probe(x) do
  x + 111
 end
 defmacro macro_probe(x) do
  111
 end
 defmacrop private_macro(x) do
  111
 end
 def zero_probe do
  111
 end
end
