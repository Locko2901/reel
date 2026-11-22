# Sharing

**Sharing** holds what a link's preview and its page play, link previews from external storage,
the copies under a size a clip page offers, and the defaults a new link starts with. These
settings are kept on the server, the same for every PC and browser, and only an admin sees them.
A setting given in `config.toml` or the environment shows read only here, with where it came
from ([Configuration](../configuration.md)). [Sharing and links](../sharing.md) tells the story
of links and copies, and [Link previews from external storage](../storage.md) that of the
storage.

In the Settings list this page is under **Server**. In the desktop app's standalone library it
is called **Copies under a size**, under **Library**, and holds that group alone, **Default
limit** and **Limit list**: there they are the settings of the library on this PC. Links need a
server, and so does the rest of this page.

## Link preview {#link-preview}

### Preview video size from the server {#share-embed-mb}

The most a link's preview video may weigh when this server sends it: 20 MB by default, 5 to 95.
The preview video is what a chat's preview plays in the message (the page's `og:video`). A
version that is H.264 with AAC and under this size is sent as it is; any other gets a smaller
copy for it, at most 1080p, made right after the clip comes in and after every cut
([What the chat and the page play](../sharing.md#what-plays-where)).

Chat apps give up fetching a preview after a few seconds, so the file has to arrive within about
five seconds over your upload: 20 MB suits 30 to 35 Mbit/s up. Lower it on a slower upload,
raise it on a faster one. The previews made already keep their size until you press
[Remake the preview videos](storage.md#remake-embeds); a new version gets one at the new size.
With [link previews from external storage](#external-on) on, the links' previews come from the
storage instead, at [a size of their own](#external-embed-mb).

In `config.toml`: `embed_mb` under `[share]`; environment: `REEL_EMBED_MB`; every value it takes:
[Configuration](../configuration.md#preview-video-size-from-the-server).

## External storage {#external-storage}

Link previews from an S3-compatible storage behind a CDN.
[Set it up with Cloudflare R2](../storage.md#set-it-up-with-cloudflare-r2) goes through these
rows step by step. The rows under the switch show while it is on, and keep their values while it
is off.

### Link previews from external storage {#external-on}

Puts each link's preview video in an S3-compatible storage, so chat apps fetch it from there in a
moment, whatever your upload is doing. Discord and other chat apps give up on a preview video
after about nine seconds, and from a home upload a large one often comes too late. Cloudflare R2
is the recommended storage: its free tier covers reel's use with the default
[Limit](#external-limit-gb). Backblaze B2, Wasabi, MinIO and AWS S3 work too
([Other services](../storage.md#other-services)).

Only the preview videos go there. The pages, the clips, their downloads and the web copies stay
on your server, and so does the address of a link and of its preview
([What goes there, and what stays home](../storage.md#what-goes-there-and-what-stays-home)). A
new link waits for its preview's upload before it is copied, unless the preview went up ahead
([A new link waits for its preview](../storage.md#a-new-link-waits-for-its-preview)). A message
posted before keeps the embed it had
([Discord keeps its preview](../storage.md#discord-keeps-its-preview)).

Off by default. Off again, every link's preview comes from your server, and what reel put in the
bucket stays there ([Turn it off](../storage.md#turn-it-off)).

In `config.toml`: `on` under `[external]`; environment: `REEL_EXTERNAL`; every value it takes:
[Configuration](../configuration.md#link-previews-from-external-storage).

### S3 endpoint {#external-endpoint}

The storage service's S3 address, where reel writes and deletes its files: for R2
`https://ACCOUNT_ID.r2.cloudflarestorage.com`, with your account's id, as the bucket's settings
page shows it. It starts with `https://` or `http://`, and a slash at its end is dropped. It is
not the address chat apps fetch from: that is the [Public address](#external-public-url).

In `config.toml`: `endpoint` under `[external]`; environment: `REEL_EXTERNAL_ENDPOINT`; every
value it takes: [Configuration](../configuration.md#s3-endpoint).

### Region {#external-region}

The bucket's region: `auto`, the default, for R2 and MinIO; the bucket's own region on the other
services.

In `config.toml`: `region` under `[external]`; environment: `REEL_EXTERNAL_REGION`; every value
it takes: [Configuration](../configuration.md#region).

### Bucket {#external-bucket}

The bucket's name, such as `reel-previews`. A bucket of reel's own is the tidy choice: reel
writes only under `reel/` in it, and never reads, counts or deletes anything it did not write.

In `config.toml`: `bucket` under `[external]`; environment: `REEL_EXTERNAL_BUCKET`; every value
it takes: [Configuration](../configuration.md#bucket).

### Access key {#external-access-key}

The id of the token reel signs in to the storage with. Make a token that may read and write
this bucket and nothing else: on R2, **Object Read & Write** for this bucket alone, and its
**Access Key ID** goes here.

In `config.toml`: `access_key` under `[external]`; environment: `REEL_EXTERNAL_ACCESS_KEY`;
every value it takes: [Configuration](../configuration.md#access-key).

### Secret key {#external-secret-key}

The secret of that token. It is written and never shown again: the row says only whether it is
set. The exports of the database and of `config.toml` leave it blank, so after a restore from
them, enter it again ([Restore a server](../backup.md#restore-a-server)).

In `config.toml`: `secret_key` under `[external]`; environment: `REEL_EXTERNAL_SECRET_KEY`;
every value it takes: [Configuration](../configuration.md#secret-key).

### Public address {#external-public-url}

Where anyone can fetch the bucket's files: the address chat apps load the previews from. On R2,
a custom domain connected to the bucket, such as `https://media.example.com`. R2's `r2.dev`
address works for a test, but Cloudflare limits its rate, so give real links a custom domain;
[Test the storage](#external-test) reminds you. For B2, its friendly URL ending in
`/file/BUCKET`.

In `config.toml`: `public_url` under `[external]`; environment: `REEL_EXTERNAL_PUBLIC_URL`;
every value it takes: [Configuration](../configuration.md#public-address).

### Preview video size from storage {#external-embed-mb}

The most a link's preview video in the storage may weigh: 40 MB by default, 5 to 200. The storage
does not wait on your upload when a chat fetches from it, so its previews can be larger, and
sharper, than [the server's](#share-embed-mb), and chat apps still get them in time. A version
under this size that is H.264 with AAC goes up as it is; any other gets a copy at this size,
made when the clip comes in.

The size also sets how long a new link waits for its preview to go up: a 40 MB preview at 35
Mbit/s takes about nine seconds
([A new link waits for its preview](../storage.md#a-new-link-waits-for-its-preview)).

In `config.toml`: `embed_mb` under `[external]`; environment: `REEL_EXTERNAL_EMBED_MB`; every
value it takes: [Configuration](../configuration.md#preview-video-size-from-storage).

### Limit {#external-limit-gb}

The most reel keeps in the storage, counted from what it wrote: 5 GB by default, 1 to 10000.
R2's free 10 GB are shared by every bucket of the account, so with other buckets there, keep this
under what they leave. reel checks it every ten minutes against a listing of `reel/`.

When something would pass the limit, the previews made ahead go first, then the links'
previews, the least used first; a link inside the [Protected window](#external-protect-days)
keeps its own. A link whose preview leaves plays it from your server again
([The limit and what stays](../storage.md#the-limit-and-what-stays)). With Notifications set up,
the event "the external storage needs a look" tells you at 80 and at 100 % of it
([Events](notifications.md#notify-events)).

In `config.toml`: `limit_gb` under `[external]`; environment: `REEL_EXTERNAL_LIMIT_GB`; every
value it takes: [Configuration](../configuration.md#limit).

### Made ahead {#external-ahead}

Previews that go up before any link exists, so a link you make later is instant:

- **none**, the default: a preview goes up when a link needs it.
- **for edited clips**: the preview of every clip whose current version is a cut. With the
  desktop app's **Keep the last** set, every clip it sends is one.
- **for edited and new clips**: those, and every clip during its first
  [New for](#external-new-days) days.

Previews made ahead are the first to make room at the [Limit](#external-limit-gb)
([Made ahead](../storage.md#made-ahead)).

In `config.toml`: `ahead` under `[external]`; environment: `REEL_EXTERNAL_AHEAD`; every value it
takes: [Configuration](../configuration.md#made-ahead).

### New for {#external-new-days}

How long a clip counts as new after it comes in, for **Made ahead** set to **for edited and new
clips**: 7 days by default, 1 to 365. Once its days are over, no preview goes up ahead for it any
more; one that is there already stays while there is room, and is among the first to make room.

In `config.toml`: `new_days` under `[external]`; environment: `REEL_EXTERNAL_NEW_DAYS`; every
value it takes: [Configuration](../configuration.md#new-for).

### Protected window {#external-protect-days}

How long a link's preview stays in the storage after the link is made, whatever the order at the
[Limit](#external-limit-gb): 14 days by default, 0 to 365; 0 protects none. When the protected
previews alone fill the limit, a new link's preview comes from your server, and Status says so.

In `config.toml`: `protect_days` under `[external]`; environment: `REEL_EXTERNAL_PROTECT_DAYS`;
every value it takes: [Configuration](../configuration.md#protected-window).

### Revoke links never shared {#share-cleanup}

Revokes a link that no chat app ever fetched and no one ever opened, once it is
[Revoke after](#share-cleanup-days) days old, and its preview leaves the storage. With one-key
sharing every replay you save gets a link, and most are never sent anywhere; this takes their
previews out of the storage.

A link posted or opened even once is never revoked, however quiet it goes, so an old chat never
shows a broken link; nor is a link with an expiry of its own. A link pasted where no preview is
built (a plain text message, a document) and never opened looks never shared. The Shares page
shows which links it would revoke next. Off by default; it acts only while link previews from
external storage are on ([Revoke links never shared](../storage.md#revoke-links-never-shared)).

In `config.toml`: `cleanup` under `[share]`; environment: `REEL_LINK_CLEANUP`; every value it
takes: [Configuration](../configuration.md#revoke-links-never-shared).

### Revoke after {#share-cleanup-days}

How old, in days, a link never shared gets before it is revoked: 30 by default, 1 to 365. It
shows while **Revoke links never shared** is on.

In `config.toml`: `cleanup_days` under `[share]`; environment: `REEL_LINK_CLEANUP_DAYS`; every
value it takes: [Configuration](../configuration.md#revoke-after).

### In the storage {#external-check}

The line beside the row says what the storage holds, as Status says it: the use against the
limit ("1.2 GB of 5.0 GB (24 %)"), then the previews by kind, shared, edited, new and old, each
with its size and count ("shared 1.0 GB in 12"); how many links' previews come from your server,
and why (the limit reached, the storage out of reach, an upload that failed); the previews going
up; the last failure; and anything under `reel/` that reel has no record of, which it leaves
alone. It says "not checked yet" before the first check, and "cannot be used" with the reason
when a check fails.

**Check now** looks again at once, as the check every ten minutes does, and shows the line when
it is done. The row shows while the switch is on.

### Test the storage {#external-test}

**Test** writes a small file to the bucket with the settings in force, reads it back over the
[Public address](#external-public-url) and deletes it; the answer shows beside the button. When
it works, it says so, and reminds you when the address is an `r2.dev` one. When it does not, it
says which step failed: the write (the endpoint, the keys, the bucket), the read back (the
public address, or a bucket nobody may read from), or the delete. Run it after any change to the
rows above. The row shows while the switch is on.

## Link page {#link-page}

### Web copy bitrate {#share-web-mbit}

The rate of the copy a link's page plays in a browser, video and sound together: 10 Mbit/s by
default, 2 to 50. Everyone watching shares your home upload: at about 30 Mbit/s up, 10 Mbit/s
carries two or three viewers at once. The copy is made when a link to the version is made, or
when its page is first opened without one; until it is ready, the page plays the preview video
and moves over by itself ([What the chat and the page play](../sharing.md#what-plays-where)). A
version already under this rate, H.264 with AAC and no taller than
[Web copy height](#share-web-max-height), plays as it is.

After a change, housekeeping removes the copies made at the old rate within 10 minutes, and each
is made again on its page's next view.

In `config.toml`: `web_mbit` under `[share]`; environment: `REEL_WEB_MBIT`; every value it takes:
[Configuration](../configuration.md#web-copy-bitrate).

### Web copy height {#share-web-max-height}

The tallest a web copy is: **1440p** by default, or **720p**, **1080p** or **2160p**. A shorter
version keeps its own height. After a change, the copies made for the old height go within 10
minutes, and each is made again on its page's next view.

In `config.toml`: `web_max_height` under `[share]`; environment: `REEL_WEB_MAX_HEIGHT`; every
value it takes: [Configuration](../configuration.md#web-copy-height).

## Copies under a size {#copies-under-a-size}

The copies a clip page's **Send** offers, each made to fit a chat's upload limit
([Send the file itself](../sharing.md#send-the-file-itself)). How much disk they may take is
[Copies under a size](storage.md#share-discord-cache-gb) under Library and storage; how they are
encoded, [Audio for copies](ingest.md#media-copy-audio),
[x264 preset for copies](ingest.md#media-x264-preset) and
[Copies on the GPU](ingest.md#media-copies-on-hardware) under Ingest.

### Default limit {#share-discord-limit-mb}

The limit of a copy under a size asked for without naming one: the clip's `discord.mp4` address
with no `?limit=` at its end. The clip page's **Send** always names its limit, so the default
counts only for such an address of your own. It has to be one of the
[Limit list](#share-discord-limits-mb). Unless you set it, it is 20, Discord's limit without
Nitro, or the largest limit under 20 when the list has no 20, else the smallest.

To take out of the list the limit that is the default, choose another **Default limit** first:
a list without the default limit is refused.

In `config.toml`: `discord_limit_mb` under `[share]`; environment: `REEL_DISCORD_LIMIT_MB`; every
value it takes: [Configuration](../configuration.md#default-limit).

### Limit list {#share-discord-limits-mb}

The limits **Send** offers, in MB: `10, 20, 50, 100, 500` by default. Chat apps' upload limits
are the point of them: Discord takes 20 MB without Nitro, 50 with Nitro Basic and 500 with Nitro,
and some apps still stop at 10. **Send** lists the version as it is first, then a copy under each
limit the version does not fit under, the largest first; under a limit it fits already, it
offers no copy.

Give 1 to 8 limits, each from 1 to 500 MB, separated by commas; they are sorted, and a limit
given twice shows once. Each copy is made once per version and limit, and kept.

In `config.toml`: `discord_limits_mb` under `[share]`; environment: `REEL_DISCORD_LIMITS_MB`;
every value it takes: [Configuration](../configuration.md#limit-list).

## New links {#new-links}

### Allow the download {#share-download-default}

Whether a new link's page offers the full file, with a **Download** button: on by default. It is
where the switch in the **New share link** dialog starts, and what a link made in one step takes
as it is: **Copy link** in a clip's menu, **Copy links** in the Select bar, and the desktop app's
**Save and share** and **Share the last clip**. A collection's link takes it too, when it is
made. A link keeps the choice it is made with, so a change counts for the links made afterwards
([Expiry, download and revoke](../sharing.md#expiry-download-and-revoke)).

In `config.toml`: `download_default` under `[share]`; environment: `REEL_SHARE_DOWNLOAD_DEFAULT`;
every value it takes: [Configuration](../configuration.md#allow-the-download).

### Expire {#share-expiry-default}

What a new link's expiry starts on in the dialog: **never** by default, or **after 1 day**,
**after 7 days** or **after 30 days**. A link made in one step, as above, takes it as it is. A
link keeps its expiry, so a change counts for the links made afterwards; the link's page says
when it stops.

In `config.toml`: `expiry_default` under `[share]`; environment: `REEL_SHARE_EXPIRY_DEFAULT`;
every value it takes: [Configuration](../configuration.md#expire).
