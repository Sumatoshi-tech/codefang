defmodule Box do
 def guarded(x) when is_integer(x) do
  111
 end
 defp private_guarded(x) when is_integer(x) do
  111
 end
 defmacro macro_guarded(x) when is_integer(x) do
  111
 end
 defmacrop private_macro_guarded(x) when is_integer(x) do
  111
 end
end
