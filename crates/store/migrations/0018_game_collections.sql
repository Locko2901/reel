-- A game is a collection by itself: one row per game (its name in small letters), kept while
-- the game has no clips so that its link keeps its token and comes back with the game. Its
-- clips are the clips with that game, never rows in collection_clip.
ALTER TABLE collection ADD COLUMN game TEXT;
CREATE UNIQUE INDEX collection_by_game ON collection(game) WHERE game IS NOT NULL;
