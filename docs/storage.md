# Link previews from external storage

Discord and other chat apps play a link's preview video right in the
chat, but they give up fetching it after about nine seconds. From a home
upload, a 20 MB preview often comes too late, and the chat shows a plain
link instead. Turn on **Link previews from external storage** and reel
puts each link's preview video in S3-compatible storage behind a CDN. The
chat apps fetch it from there in a moment, whatever your upload is doing.

It is off by default. Cloudflare R2 is the recommended storage: its free
tier covers what reel puts there, and downloads from it cost nothing.

## What goes there, and what stays home

- **Goes there:** the preview video of every version a live link serves
  (the version a link is pinned to, or the clip's current one, and every
  clip of a collection's link). With [Made ahead](#made-ahead), the
  previews of clips you are likely to share go there too. Each is an
  object under `reel/` in the bucket, at an address with 128 random bits
  in it. Anyone who has the address can fetch it, just as anyone with the
  link can open the link.
- **Stays home:** the share pages, the clips themselves, their downloads
  and the web copies a link's page plays. The page a link opens is always
  your server's, and so is the address of its preview video. Your server
  sends the chat app on to the storage while the preview is there, and
  plays it itself when it is not, so a message posted long ago never shows
  a broken video.
- **Goes away:** a preview leaves the storage when no live link uses its
  version any more: the link revoked or expired, the clip in the trash,
  the version deleted. Its address answers nothing from then on, and a
  version that goes up again, made ahead, gets a new address. A CDN in
  front may keep a copy for up to ten minutes.

reel writes only under `reel/` and never reads, counts or deletes anything
else in the bucket. A bucket of reel's own is still the tidy choice.

## Set it up with Cloudflare R2

1. In Cloudflare's dashboard, open **R2** and turn it on. Cloudflare asks
   for a payment method once. The free tier (10 GB stored, no fee for
   downloads, a million writes and ten million reads a month) covers
   reel's use with the default [limit](#the-limit-and-what-stays) of 5 GB.
2. **Create a bucket**, for example `reel-previews`, with the location
   left automatic.
3. In the bucket's **Settings**, under **Custom Domains**, connect a domain
   on Cloudflare, such as `media.example.com`. That is the public address
   the chat apps fetch from. The bucket's `r2.dev` address works for a
   test, but Cloudflare limits its rate, so do not use it for real links.
4. Under **Manage API tokens**, create a token with **Object Read & Write**
   for this bucket alone. Copy its **Access Key ID**, its **Secret Access
   Key** (shown once) and the **S3 endpoint**,
   `https://ACCOUNT_ID.r2.cloudflarestorage.com`.
5. In reel, open Settings, **Sharing**, **External storage**. Turn on
   **Link previews from external storage** and fill in the rows that
   appear:

    | Row | What to put there |
    |---|---|
    | [S3 endpoint](configuration.md#s3-endpoint) | the S3 endpoint from step 4 |
    | [Region](configuration.md#region) | `auto` |
    | [Bucket](configuration.md#bucket) | `reel-previews` |
    | [Access key](configuration.md#access-key) | the Access Key ID |
    | [Secret key](configuration.md#secret-key) | the Secret Access Key; reel never shows it again |
    | [Public address](configuration.md#public-address) | `https://media.example.com` |

6. Press **Test**. reel writes a small file, reads it back over the
   public address and deletes it. When it worked, it says so; when it did
   not, it says which step failed.
7. In Cloudflare's dashboard, add a billing notification
   (**Notifications**) for the account's R2 use. reel sees only its own
   objects, and R2's free 10 GB are shared by every bucket of the account.

## Other services

Any S3-compatible storage with a public address works, among them
Backblaze B2, Wasabi, MinIO and AWS S3. Give reel the service's S3
endpoint and region, a key that may read and write the bucket, and the
address its files are served from to anyone (for B2, its friendly URL
ending in `/file/BUCKET`). Mind what each charges for downloads: R2
charges nothing, AWS charges for every download.

## A new link waits for its preview

A link goes on your clipboard only once its preview is in the storage, so
a chat app never finds it missing. Meanwhile the dialog's button reads
**Making the link: preview going up**, with how far it is, and the app's
one-key share says "the link preview's video goes up to the storage".
The wait is as long as the file takes at your upload speed: a 40 MB
preview at 35 Mbit/s takes about nine seconds. A preview
[made ahead](#made-ahead) is there already, and the link is instant.

The previews from storage can be larger than your server's, so they look
sharper: [Preview video size from
storage](configuration.md#preview-video-size-from-storage) is 40 MB by
default, against the server's 20. A version under that size, in H.264
with AAC, goes up as it is. Any other gets a copy at that size, made when
the clip comes in.

## The limit and what stays

[Limit](configuration.md#limit) (5 GB by default) is the most reel keeps
in the storage. reel counts it from its own records and checks it every
ten minutes against a listing of `reel/`. When something would pass the
limit, previews make room in this order, the last first:

1. previews of links made within the
   [Protected window](configuration.md#protected-window) (14 days by
   default): never removed for room;
2. previews of links that were posted or opened, the most recently active
   first;
3. previews of links never posted nor opened;
4. previews [made ahead](#made-ahead): edited clips, then new ones, then
   older ones.

A link whose preview is removed for room plays its preview from your
server again, as it did before the storage, so an old message still
plays. Only when the previews of protected links alone fill the limit
does a new link's preview come from your server, and Status says so.

Settings, **Sharing**, **External storage**, **In the storage** shows
what is there: the use against the limit by kind, and the links served
from your server and why. **Check now** looks again at once. Status has
the same line.

## Made ahead

[Made ahead](configuration.md#made-ahead) puts previews in the storage
before any link exists, so a link you make later is instant. Choose **for
edited clips**, or **for edited and new clips** (new for
[New for](configuration.md#new-for) days after they come in, 7 by
default). With the desktop app's **Keep the last** set, every clip it
sends keeps only its last seconds, which counts as an edit, so **for
edited clips** covers every replay you save. These previews are the first
to make room.

## When the storage cannot be reached

When the storage cannot be reached, an upload fails, or the limit leaves
a link out, the link's preview comes from your server as it did before,
and goes up again once it can. The **Shares** page and the clip's page say
where each link's preview comes from: "preview from storage", or "preview
from your server" with the reason.

With **Notifications** set up, the event [the external storage needs a
look](configuration.md#events) (on by default) tells you at 80 % and at
100 % of the limit, and when an upload or a delete fails.

## Discord keeps its preview

Discord, like most chat apps, fetches a link's preview once, when the link
is posted, and keeps it. A message posted before you turned the storage on
keeps the embed it had. Post the link again for one from the storage.

## Revoke links never shared

With one-key sharing, every replay you save gets a link, and most of them
you never send anywhere. [Revoke links never
shared](configuration.md#revoke-links-never-shared), off by default,
revokes a link that no chat app ever fetched and no one ever opened,
[Revoke after](configuration.md#revoke-after) days after it was made (30
by default), and its preview leaves the storage.

A link that was posted or opened even once is never revoked, however long
it stays quiet, so someone scrolling back through a chat never finds a
broken link. A link with an expiry of its own is left to that expiry.

The Shares page shows which links it would revoke next, where the expiry
would be: "never shared, revoked in 12 days". What it cannot see: a link
pasted where no preview is built (a plain text message, a document) and
never opened looks never shared.

## Turn it off

Turn the switch off and every link's preview comes from your server again.
The settings under it are kept for when you turn it back on. What reel put
in the bucket stays there: delete `reel/` in the bucket to empty it.
