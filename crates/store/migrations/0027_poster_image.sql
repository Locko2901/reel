-- An image someone uploaded as the clip's poster (D254): the moment it was taken, which keys the
-- posters' addresses; its re-encoded copy is poster-source.jpg in the clip's folder, and every
-- version's poster is made from it. None while the poster is a frame of the video.
ALTER TABLE clip ADD COLUMN poster_image TEXT;
