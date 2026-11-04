-- The library cap after the update that lets it act on a server's clips (D262). On an install
-- that already has clips it waits: a cap set long ago, which never moved anything there, does not
-- send the oldest clips to the trash on the first start. It acts once an admin saves the cap, the
-- cap changes, or it would move nothing. `told`: the notification about it went; `cap_gb`: the
-- cap it waited at.
CREATE TABLE cap_hold (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    told INTEGER NOT NULL DEFAULT 0,
    cap_gb INTEGER
);
INSERT INTO cap_hold (id) SELECT 1 WHERE EXISTS (SELECT 1 FROM clip);
