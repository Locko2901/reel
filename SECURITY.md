# Security

## Reporting a vulnerability

Please report it privately, not in a public issue: open the repository's **Security** tab
and choose **Report a vulnerability**
([direct link](https://github.com/Locko2901/reel/security/advisories/new)). GitHub keeps the
report between you and the maintainer until a fix is out.

Say what you found, how to reproduce it, and which version you ran (Settings, Status shows
the server's; the desktop app shows its own under This app, Startup, About). Reports are
answered as soon as possible. Once a fix is released, the advisory is published with credit
to you, unless you would rather stay unnamed.

## What is supported

Fixes go into the newest release only. The server's image (`ghcr.io/locko2901/reel:latest`)
and the desktop app both move to it: a paired app updates through its server, and a standalone
app says when a newer version is out.

## What counts

Anything that lets someone see, change or delete clips, links or settings they should not,
sign in as someone else, reach files outside reel's folders, or run code on the server or on a
PC with the app. Also a link that keeps playing after it was revoked or expired.

Not a vulnerability: a public share link that plays for whoever has it (that is what it is
for), and the installers' SmartScreen warning (they carry no Authenticode signature; their
updates are signed for the app's updater).
