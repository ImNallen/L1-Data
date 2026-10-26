CREATE SCHEMA "Weird ""S""";
CREATE SCHEMA app;
CREATE SCHEMA "order";
CREATE COLLATION app."my coll" (provider = icu, locale = 'sv-SE');
CREATE DOMAIN app.posint AS integer CHECK (VALUE > 0);
CREATE TYPE app.mood AS ENUM ('ok', 'sad');
CREATE TYPE app.pet AS (id int, kind text, name text);
CREATE FOREIGN DATA WRAPPER hostile_fdw;
CREATE SERVER hostile_server FOREIGN DATA WRAPPER hostile_fdw;
