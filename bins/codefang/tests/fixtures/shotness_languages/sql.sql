CREATE FUNCTION probe(x integer) RETURNS integer LANGUAGE SQL AS $$ SELECT x + 111 $$;
